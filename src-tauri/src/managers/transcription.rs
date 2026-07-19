use crate::audio_toolkit::{apply_custom_words, filter_transcription_output, pad_trailing_silence};
use crate::managers::model::{EngineType, ModelManager};
use crate::settings::{ModelUnloadTimeout, get_settings, normalize_language_for_engine};
use anyhow::Result;
use log::{debug, error, info, warn};
use serde::Serialize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter};
use transcribe_rs::{
    TranscriptionEngine,
    engines::{
        moonshine::{
            ModelVariant, MoonshineEngine, MoonshineModelParams, MoonshineStreamingEngine,
            StreamingModelParams,
        },
        parakeet::{
            ParakeetEngine, ParakeetInferenceParams, ParakeetModelParams, TimestampGranularity,
        },
        sense_voice::{
            Language as SenseVoiceLanguage, SenseVoiceEngine, SenseVoiceInferenceParams,
            SenseVoiceModelParams,
        },
        whisper::{WhisperEngine, WhisperInferenceParams},
    },
};

/// 1 s of digital silence @ 16 kHz appended before decoding for engines that
/// need trailing acoustic context to emit their final tokens (Parakeet,
/// Moonshine, SenseVoice — see `pad_trailing_silence`). Without it, the tail
/// segment cut the instant the user stops recording loses its last word(s).
/// Whisper-family engines are excluded: they decode the whole window fine and
/// trailing silence only invites end-of-audio hallucinations.
const TRAILING_SILENCE_PAD_SAMPLES: usize = 16_000;

#[derive(Clone, Debug, Serialize)]
pub struct ModelStateEvent {
    pub event_type: String,
    pub model_id: Option<String>,
    pub model_name: Option<String>,
    pub error: Option<String>,
}

enum LoadedEngine {
    Whisper(WhisperEngine),
    Parakeet(ParakeetEngine),
    Moonshine(MoonshineEngine),
    MoonshineStreaming(MoonshineStreamingEngine),
    SenseVoice(SenseVoiceEngine),
    #[cfg(not(target_os = "macos"))]
    FlmWhisper,
    ApiWhisper,
    OpenRouterWhisper,
}

/// The loaded engine AND the model ID it was loaded for, behind ONE mutex
/// (adversarial-review finding 8b). These used to live in two SEPARATE
/// `Mutex`es, updated in two separate critical sections by
/// `load_model`/`unload_model`. A racing reader that locked them one at a
/// time — e.g. "is anything loaded" followed by "for which model" — could
/// observe an inconsistent pairing (engine-without-id mid-load,
/// id-without-engine mid-unload) even though the writer always intended the
/// pair to change together: two separately-locked fields simply can't be
/// read as a consistent pair no matter how carefully the WRITER orders its
/// own two critical sections. Combining them under one lock makes "engine
/// and model_id always change together" an actual invariant instead of a
/// hopeful convention.
struct EngineState {
    engine: Option<LoadedEngine>,
    model_id: Option<String>,
    /// Bumped on EVERY state mutation (install, unload, panic-clear). The
    /// transcribe() take-out records it; put-back and panic-cleanup compare
    /// against it instead of `model_id` — a REPLACEMENT engine that happens
    /// to carry the same model id is a different instance and must never be
    /// clobbered by the old instance's cleanup (Codex 0410 pass-3 finding 8).
    instance: u64,
}

#[derive(Clone)]
pub struct TranscriptionManager {
    engine: Arc<Mutex<EngineState>>,
    model_manager: Arc<ModelManager>,
    app_handle: AppHandle,
    last_activity: Arc<AtomicU64>,
    shutdown_signal: Arc<AtomicBool>,
    watcher_handle: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
    is_loading: Arc<Mutex<bool>>,
    loading_condvar: Arc<Condvar>,
    /// Flag to prevent model unload during live/progressive transcription.
    is_live_transcribing: Arc<AtomicBool>,
    /// Same protection for the Translator's folder-batch jobs — a separate
    /// flag so the batch worker never fights the live pipeline's writes.
    is_batch_transcribing: Arc<AtomicBool>,
    #[cfg(not(target_os = "macos"))]
    flm_manager: Arc<Mutex<Option<crate::managers::flm::FlmManager>>>,
}

impl TranscriptionManager {
    pub fn new(app_handle: &AppHandle, model_manager: Arc<ModelManager>) -> Result<Self> {
        let manager = Self {
            engine: Arc::new(Mutex::new(EngineState {
                engine: None,
                model_id: None,
                instance: 0,
            })),
            model_manager,
            app_handle: app_handle.clone(),
            last_activity: Arc::new(AtomicU64::new(
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            )),
            shutdown_signal: Arc::new(AtomicBool::new(false)),
            watcher_handle: Arc::new(Mutex::new(None)),
            is_loading: Arc::new(Mutex::new(false)),
            loading_condvar: Arc::new(Condvar::new()),
            is_live_transcribing: Arc::new(AtomicBool::new(false)),
            is_batch_transcribing: Arc::new(AtomicBool::new(false)),
            #[cfg(not(target_os = "macos"))]
            flm_manager: Arc::new(Mutex::new(None)),
        };

        // Start the idle watcher
        {
            let app_handle_cloned = app_handle.clone();
            let manager_cloned = manager.clone();
            let shutdown_signal = manager.shutdown_signal.clone();
            let handle = thread::spawn(move || {
                while !shutdown_signal.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_secs(10)); // Check every 10 seconds

                    // Check shutdown signal again after sleep
                    if shutdown_signal.load(Ordering::Relaxed) {
                        break;
                    }

                    let settings = get_settings(&app_handle_cloned);
                    let timeout_seconds = settings
                        .model_unload_timeout
                        .to_seconds(settings.model_unload_custom_seconds);

                    if let Some(limit_seconds) = timeout_seconds {
                        // Skip polling-based unloading for immediate timeout since it's handled directly in transcribe()
                        if settings.model_unload_timeout == ModelUnloadTimeout::Immediately {
                            continue;
                        }

                        let last = manager_cloned.last_activity.load(Ordering::Relaxed);
                        let now_ms = SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as u64;

                        // Never unload while a live/chunked recording is feeding
                        // the engine — a short Custom timeout must not yank the
                        // model out from under an in-progress take. Ditto for a
                        // Translator batch job mid-file.
                        if manager_cloned.is_live_transcribing.load(Ordering::Relaxed)
                            || manager_cloned.is_batch_transcribing.load(Ordering::Relaxed)
                        {
                            continue;
                        }

                        if now_ms.saturating_sub(last) > limit_seconds * 1000 {
                            // idle -> unload
                            if manager_cloned.is_model_loaded() {
                                let unload_start = std::time::Instant::now();
                                debug!("Starting to unload model due to inactivity");

                                if let Ok(()) = manager_cloned.unload_model() {
                                    let _ = app_handle_cloned.emit(
                                        "model-state-changed",
                                        ModelStateEvent {
                                            event_type: "unloaded".to_string(),
                                            model_id: None,
                                            model_name: None,
                                            error: None,
                                        },
                                    );
                                    let unload_duration = unload_start.elapsed();
                                    debug!(
                                        "Model unloaded due to inactivity (took {}ms)",
                                        unload_duration.as_millis()
                                    );
                                }
                            }
                        }
                    }
                }
                debug!("Idle watcher thread shutting down gracefully");
            });
            *manager.watcher_handle.lock().unwrap() = Some(handle);
        }

        Ok(manager)
    }

    /// Reject a dispatch when the loaded model does not match the caller's
    /// expectation (pass-4 finding 8b): EVERY dispatch route — external
    /// (FLM/API/OpenRouter, which return before the local take-out) and the
    /// local engine path — must validate against the SAME guard it dispatches
    /// under, because selecting a model persists `selected_model` before the
    /// async load installs it. Empty `expected` = no expectation.
    fn ensure_expected_model(state: &EngineState, expected: &str) -> Result<()> {
        if expected.is_empty() {
            return Ok(());
        }
        match state.model_id.as_deref() {
            Some(loaded) if loaded != expected => Err(anyhow::anyhow!(
                "Loaded model '{}' does not match the expected model '{}' \
                 (it changed mid-take) — please retry.",
                loaded,
                expected
            )),
            _ => Ok(()),
        }
    }

    /// Lock the engine mutex, recovering from poison if a previous transcription panicked.
    fn lock_engine(&self) -> MutexGuard<'_, EngineState> {
        self.engine.lock().unwrap_or_else(|poisoned| {
            warn!("Engine mutex was poisoned by a previous panic, recovering");
            poisoned.into_inner()
        })
    }

    pub fn is_model_loaded(&self) -> bool {
        let state = self.lock_engine();
        state.engine.is_some()
    }

    pub fn unload_model(&self) -> Result<()> {
        let unload_start = std::time::Instant::now();
        debug!("Starting to unload model");

        {
            let mut state = self.lock_engine();
            if let Some(ref mut loaded_engine) = state.engine {
                match loaded_engine {
                    LoadedEngine::Whisper(e) => e.unload_model(),
                    LoadedEngine::Parakeet(e) => e.unload_model(),
                    LoadedEngine::Moonshine(e) => e.unload_model(),
                    LoadedEngine::MoonshineStreaming(e) => e.unload_model(),
                    LoadedEngine::SenseVoice(e) => e.unload_model(),
                    #[cfg(not(target_os = "macos"))]
                    LoadedEngine::FlmWhisper => {
                        // Stop the FLM subprocess
                        if let Ok(mut flm_guard) = self.flm_manager.lock() {
                            if let Some(ref mut flm) = *flm_guard {
                                flm.stop();
                            }
                            *flm_guard = None;
                        }
                    }
                    LoadedEngine::ApiWhisper => {
                        // No resources to release — stateless HTTP client
                    }
                    LoadedEngine::OpenRouterWhisper => {
                        // No resources to release — stateless HTTP client
                    }
                }
            }
            // Finding 8(b): clear the engine AND its model_id together, under
            // the SAME lock acquisition — never in two separate critical
            // sections (the old two-mutex design let a racing reader observe
            // engine-without-id or id-without-engine).
            state.engine = None; // Drop the engine to free memory
            state.model_id = None;
            state.instance += 1;
        }

        // Emit unloaded event
        let _ = self.app_handle.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "unloaded".to_string(),
                model_id: None,
                model_name: None,
                error: None,
            },
        );

        let unload_duration = unload_start.elapsed();
        debug!(
            "Model unloaded manually (took {}ms)",
            unload_duration.as_millis()
        );
        Ok(())
    }

    /// Unloads the model immediately if the setting is enabled and the model is loaded.
    /// Suppressed while live transcription is active to avoid dropping the model between chunks.
    pub fn maybe_unload_immediately(&self, context: &str) {
        if self.is_live_transcribing.load(Ordering::Relaxed) {
            debug!("Skipping immediate unload during live transcription");
            return;
        }
        if self.is_batch_transcribing.load(Ordering::Relaxed) {
            debug!("Skipping immediate unload during a Translator batch job");
            return;
        }
        let settings = get_settings(&self.app_handle);
        if settings.model_unload_timeout == ModelUnloadTimeout::Immediately
            && self.is_model_loaded()
        {
            info!("Immediately unloading model after {}", context);
            if let Err(e) = self.unload_model() {
                warn!("Failed to immediately unload model: {}", e);
            }
        }
    }

    pub fn set_live_transcribing(&self, active: bool) {
        self.is_live_transcribing.store(active, Ordering::Relaxed);
    }

    /// Translator batch jobs hold this across their whole file (all segments)
    /// so an "Immediately"/short unload timeout can't drop the model between
    /// batch segments.
    pub fn set_batch_transcribing(&self, active: bool) {
        self.is_batch_transcribing.store(active, Ordering::Relaxed);
    }

    pub fn load_model(&self, model_id: &str) -> Result<()> {
        let load_start = std::time::Instant::now();
        debug!("Starting to load model: {}", model_id);

        // Emit loading started event
        let _ = self.app_handle.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "loading_started".to_string(),
                model_id: Some(model_id.to_string()),
                model_name: None,
                error: None,
            },
        );

        let model_info = self
            .model_manager
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

        if !model_info.is_downloaded {
            let error_msg = "Model not downloaded";
            let _ = self.app_handle.emit(
                "model-state-changed",
                ModelStateEvent {
                    event_type: "loading_failed".to_string(),
                    model_id: Some(model_id.to_string()),
                    model_name: Some(model_info.name.clone()),
                    error: Some(error_msg.to_string()),
                },
            );
            return Err(anyhow::anyhow!(error_msg));
        }

        // External engines (FLM subprocess, HTTP APIs) have no artifact in the
        // models dir — probing would always fail (FLM's "whisper-v3:turbo"
        // filename is not even a valid Windows path) and made the FLM arm
        // below unreachable. Their arms never read the path.
        let model_path = if model_info.engine_type.is_external() {
            std::path::PathBuf::new()
        } else {
            self.model_manager.get_model_path(model_id)?
        };

        // Create appropriate engine based on model type
        let loaded_engine = match model_info.engine_type {
            EngineType::Whisper => {
                let mut engine = WhisperEngine::new();
                engine.load_model(&model_path).map_err(|e| {
                    let error_msg = format!("Failed to load whisper model {}: {}", model_id, e);
                    let _ = self.app_handle.emit(
                        "model-state-changed",
                        ModelStateEvent {
                            event_type: "loading_failed".to_string(),
                            model_id: Some(model_id.to_string()),
                            model_name: Some(model_info.name.clone()),
                            error: Some(error_msg.clone()),
                        },
                    );
                    anyhow::anyhow!(error_msg)
                })?;
                LoadedEngine::Whisper(engine)
            }
            EngineType::Parakeet => {
                let mut engine = ParakeetEngine::new();
                engine
                    .load_model_with_params(&model_path, ParakeetModelParams::int8())
                    .map_err(|e| {
                        let error_msg =
                            format!("Failed to load parakeet model {}: {}", model_id, e);
                        let _ = self.app_handle.emit(
                            "model-state-changed",
                            ModelStateEvent {
                                event_type: "loading_failed".to_string(),
                                model_id: Some(model_id.to_string()),
                                model_name: Some(model_info.name.clone()),
                                error: Some(error_msg.clone()),
                            },
                        );
                        anyhow::anyhow!(error_msg)
                    })?;
                LoadedEngine::Parakeet(engine)
            }
            EngineType::Moonshine => {
                let mut engine = MoonshineEngine::new();
                engine
                    .load_model_with_params(
                        &model_path,
                        MoonshineModelParams::variant(ModelVariant::Base),
                    )
                    .map_err(|e| {
                        let error_msg =
                            format!("Failed to load moonshine model {}: {}", model_id, e);
                        let _ = self.app_handle.emit(
                            "model-state-changed",
                            ModelStateEvent {
                                event_type: "loading_failed".to_string(),
                                model_id: Some(model_id.to_string()),
                                model_name: Some(model_info.name.clone()),
                                error: Some(error_msg.clone()),
                            },
                        );
                        anyhow::anyhow!(error_msg)
                    })?;
                LoadedEngine::Moonshine(engine)
            }
            EngineType::MoonshineStreaming => {
                let mut engine = MoonshineStreamingEngine::new();
                engine
                    .load_model_with_params(&model_path, StreamingModelParams::default())
                    .map_err(|e| {
                        let error_msg = format!(
                            "Failed to load moonshine streaming model {}: {}",
                            model_id, e
                        );
                        let _ = self.app_handle.emit(
                            "model-state-changed",
                            ModelStateEvent {
                                event_type: "loading_failed".to_string(),
                                model_id: Some(model_id.to_string()),
                                model_name: Some(model_info.name.clone()),
                                error: Some(error_msg.clone()),
                            },
                        );
                        anyhow::anyhow!(error_msg)
                    })?;
                LoadedEngine::MoonshineStreaming(engine)
            }
            EngineType::SenseVoice => {
                let mut engine = SenseVoiceEngine::new();
                engine
                    .load_model_with_params(&model_path, SenseVoiceModelParams::int8())
                    .map_err(|e| {
                        let error_msg =
                            format!("Failed to load SenseVoice model {}: {}", model_id, e);
                        let _ = self.app_handle.emit(
                            "model-state-changed",
                            ModelStateEvent {
                                event_type: "loading_failed".to_string(),
                                model_id: Some(model_id.to_string()),
                                model_name: Some(model_info.name.clone()),
                                error: Some(error_msg.clone()),
                            },
                        );
                        anyhow::anyhow!(error_msg)
                    })?;
                LoadedEngine::SenseVoice(engine)
            }
            #[cfg(not(target_os = "macos"))]
            EngineType::FlmWhisper => {
                use crate::managers::flm::FlmManager;
                let flm_model_name = if model_info.filename.is_empty() {
                    "whisper-v3:turbo"
                } else {
                    &model_info.filename
                };
                let flm = FlmManager::start_serve(flm_model_name).map_err(|e| {
                    let error_msg = format!("Failed to start FLM server: {}", e);
                    let _ = self.app_handle.emit(
                        "model-state-changed",
                        ModelStateEvent {
                            event_type: "loading_failed".to_string(),
                            model_id: Some(model_id.to_string()),
                            model_name: Some(model_info.name.clone()),
                            error: Some(error_msg.clone()),
                        },
                    );
                    anyhow::anyhow!(error_msg)
                })?;
                *self.flm_manager.lock().unwrap() = Some(flm);
                LoadedEngine::FlmWhisper
            }
            EngineType::ApiWhisper => {
                // Validate that a URL is configured
                let settings = get_settings(&self.app_handle);
                if settings.api_transcription_url.trim().is_empty() {
                    let error_msg =
                        "API Transcription URL is not configured. Set it in Advanced settings."
                            .to_string();
                    let _ = self.app_handle.emit(
                        "model-state-changed",
                        ModelStateEvent {
                            event_type: "loading_failed".to_string(),
                            model_id: Some(model_id.to_string()),
                            model_name: Some(model_info.name.clone()),
                            error: Some(error_msg.clone()),
                        },
                    );
                    return Err(anyhow::anyhow!(error_msg));
                }
                info!(
                    "API Whisper engine selected (url: {})",
                    settings.api_transcription_url
                );
                LoadedEngine::ApiWhisper
            }
            EngineType::OpenRouterWhisper => {
                // Require a provider WITH an API key. The model is optional — it
                // defaults to a known-good Whisper id at request time.
                let settings = get_settings(&self.app_handle);
                let has_key = settings
                    .llm_provider(&settings.openrouter_transcription_provider_ref)
                    .map(|p| !p.api_key.trim().is_empty())
                    .unwrap_or(false);
                if !has_key {
                    let error_msg = "OpenRouter transcription needs a provider with an API key. Pick an OpenRouter provider (that has a key) in Advanced > Transcription.".to_string();
                    let _ = self.app_handle.emit(
                        "model-state-changed",
                        ModelStateEvent {
                            event_type: "loading_failed".to_string(),
                            model_id: Some(model_id.to_string()),
                            model_name: Some(model_info.name.clone()),
                            error: Some(error_msg.clone()),
                        },
                    );
                    return Err(anyhow::anyhow!(error_msg));
                }
                info!(
                    "OpenRouter transcription engine selected (model: {})",
                    settings.openrouter_transcription_model
                );
                LoadedEngine::OpenRouterWhisper
            }
        };

        // Finding 8(b): update the engine AND its model ID together, under
        // the SAME lock acquisition — never in two separate critical
        // sections, so a racing observer can never see engine-without-id or
        // id-without-engine.
        {
            let mut state = self.lock_engine();
            state.engine = Some(loaded_engine);
            state.model_id = Some(model_id.to_string());
            state.instance += 1;
        }

        // Emit loading completed event
        let _ = self.app_handle.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "loading_completed".to_string(),
                model_id: Some(model_id.to_string()),
                model_name: Some(model_info.name.clone()),
                error: None,
            },
        );

        let load_duration = load_start.elapsed();
        debug!(
            "Successfully loaded transcription model: {} (took {}ms)",
            model_id,
            load_duration.as_millis()
        );
        Ok(())
    }

    /// Kicks off the model loading in a background thread if it's not already loaded
    pub fn initiate_model_load(&self) {
        let mut is_loading = self.is_loading.lock().unwrap();
        if *is_loading {
            return;
        }

        let selected_model = get_settings(&self.app_handle).selected_model;

        // T-113/finding 8: non-blocking preflight. `is_model_loaded()` locks
        // the ENGINE mutex, and `unload_model()` holds that SAME mutex while
        // releasing engine resources (can take real time for some engines) —
        // blocking here on the recording-START path could stall behind an
        // in-flight unload, directly delaying capture (this is also why
        // `TranscribeAction::start()` now calls this AFTER the mic-open
        // attempt, not before — see actions.rs). On contention, assume "not
        // loaded" and let `load_model()` below sort it out: it fully
        // (re)loads regardless of what was there before, so a wrong
        // assumption here costs at most one redundant-but-correct load —
        // never a missed load, and never a stall on the start path.
        //
        // Finding 8(a): a loaded engine ALONE is not proof the right model is
        // in it — compare the loaded model's ID against the currently
        // SELECTED model before skipping. The Translator's folder-batch
        // worker can temporarily load an OVERRIDE model into this same
        // shared engine slot (see `managers/translator.rs`); if this
        // preflight only checked "is anything loaded" it would see the
        // Translator's override engine, wrongly conclude the dictation model
        // was already ready, and skip loading it — dictation would then
        // transcribe through the WRONG model.
        match self.engine.try_lock() {
            Ok(guard) => {
                if guard.engine.is_some() {
                    if guard.model_id.as_deref() == Some(selected_model.as_str()) {
                        return; // already loaded with the right model — nothing to do
                    }
                    debug!(
                        "Engine loaded for a different model than selected ({:?} vs {}) — reloading",
                        guard.model_id, selected_model
                    );
                }
            }
            Err(_) => {
                debug!(
                    "Engine mutex busy during load preflight (likely an in-flight unload) — proceeding to load without blocking"
                );
            }
        }

        *is_loading = true;
        let self_clone = self.clone();
        thread::spawn(move || {
            // Re-fetch settings fresh inside the thread (rather than reusing
            // the `selected_model` snapshot above, which was only taken for
            // the preflight comparison) in case they changed in the gap
            // between this call and the thread actually starting.
            let settings = get_settings(&self_clone.app_handle);
            if let Err(e) = self_clone.load_model(&settings.selected_model) {
                error!("Failed to load model: {}", e);
            }
            let mut is_loading = self_clone.is_loading.lock().unwrap();
            *is_loading = false;
            self_clone.loading_condvar.notify_all();
        });
    }

    pub fn get_current_model(&self) -> Option<String> {
        let state = self.lock_engine();
        state.model_id.clone()
    }

    /// Transcribe expecting the currently SELECTED model (the dictation
    /// contract). Callers with a different expectation — the Translator's
    /// batch override — must use [`Self::transcribe_expecting`].
    pub fn transcribe(&self, audio: Vec<f32>) -> Result<String> {
        let expected = get_settings(&self.app_handle).selected_model;
        self.transcribe_expecting(&expected, audio)
    }

    /// Transcribe, revalidating at ACTUAL inference time that the loaded
    /// local engine still carries `expected_model` (pass-3 finding 8a): the
    /// engine slot is shared and a concurrent load can swap models between a
    /// caller's preflight and the take-out below. Empty `expected_model`
    /// skips the check (no expectation).
    pub fn transcribe_expecting(&self, expected_model: &str, audio: Vec<f32>) -> Result<String> {
        // Update last activity timestamp
        self.last_activity.store(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            Ordering::Relaxed,
        );

        let st = std::time::Instant::now();

        debug!("Audio vector length: {}", audio.len());

        if audio.is_empty() {
            debug!("Empty audio vector");
            self.maybe_unload_immediately("empty audio");
            return Ok(String::new());
        }

        // Check if model is loaded, if not try to load it
        {
            // If the model is loading, wait for it — but BOUNDED. FLM's first
            // use can spend minutes downloading its model inside load_model();
            // an unbounded wait here froze recording stops ("transcription
            // cannot be stopped"). 60 s must cover a COLD local-model load at
            // stop time (a 1.6 GB Whisper started loading in the background at
            // recording start can legitimately need tens of seconds — a shorter
            // bound silently produced empty takes); a stuck FLM start is capped
            // at one such wait thanks to its failure cooldown.
            let mut is_loading = self.is_loading.lock().unwrap();
            let wait_start = std::time::Instant::now();
            while *is_loading {
                let remaining = Duration::from_secs(60).saturating_sub(wait_start.elapsed());
                if remaining.is_zero() {
                    return Err(anyhow::anyhow!(
                        "The model is still loading (this can take a while on first \
                         use, e.g. FLM downloading its model). Try again shortly."
                    ));
                }
                let (guard, _timeout) = self
                    .loading_condvar
                    .wait_timeout(is_loading, remaining)
                    .unwrap();
                is_loading = guard;
            }

            let engine_guard = self.lock_engine();
            if engine_guard.engine.is_none() {
                return Err(anyhow::anyhow!("Model is not loaded for transcription."));
            }
        }

        // Get current settings for configuration
        let settings = get_settings(&self.app_handle);

        // Only honor "translate to English" when the active model actually
        // supports translation. This guards against a stale global flag left
        // over from a translation-capable model (e.g. user enabled translate on
        // Whisper, then switched to a model that can't translate).
        let effective_translate = settings.translate_to_english
            && self
                .get_current_model()
                .and_then(|id| self.model_manager.get_model_info(&id))
                .map(|m| m.supports_translation)
                .unwrap_or(false);

        // If FLM engine is active, delegate transcription to the subprocess
        #[cfg(not(target_os = "macos"))]
        {
            let engine_guard = self.lock_engine();
            if matches!(&engine_guard.engine, Some(LoadedEngine::FlmWhisper)) {
                Self::ensure_expected_model(&engine_guard, expected_model)?;
                drop(engine_guard);
                let flm_guard = self.flm_manager.lock().unwrap();
                if let Some(ref flm) = *flm_guard {
                    let language = normalize_language_for_engine(&settings.selected_language)
                        .or_else(|| Some("en".to_string()));
                    let raw_text =
                        flm.transcribe(audio, language.as_deref(), effective_translate)?;
                    let corrected = if !settings.custom_words.is_empty() {
                        apply_custom_words(
                            &raw_text,
                            &settings.custom_words,
                            settings.word_correction_threshold,
                        )
                    } else {
                        raw_text
                    };
                    let filtered = filter_transcription_output(&corrected);
                    self.maybe_unload_immediately("FLM transcription");
                    return Ok(filtered);
                }
            }
        }

        // If API Whisper engine is active, POST to the configured endpoint
        {
            let engine_guard = self.lock_engine();
            if matches!(&engine_guard.engine, Some(LoadedEngine::ApiWhisper)) {
                Self::ensure_expected_model(&engine_guard, expected_model)?;
                drop(engine_guard);
                let language = normalize_language_for_engine(&settings.selected_language)
                    .or_else(|| Some("en".to_string()));
                let raw_text = crate::managers::api_transcription::transcribe(
                    &settings.api_transcription_url,
                    &settings.api_transcription_key,
                    &settings.api_transcription_model,
                    audio,
                    language.as_deref(),
                    effective_translate,
                )?;
                let corrected = if !settings.custom_words.is_empty() {
                    apply_custom_words(
                        &raw_text,
                        &settings.custom_words,
                        settings.word_correction_threshold,
                    )
                } else {
                    raw_text
                };
                let filtered = filter_transcription_output(&corrected);
                self.maybe_unload_immediately("API transcription");
                return Ok(filtered);
            }
        }

        // If OpenRouter transcription is active, POST base64 audio (JSON). This
        // engine runs per segment/chunk, so it works in live and chunked modes.
        {
            let engine_guard = self.lock_engine();
            if matches!(&engine_guard.engine, Some(LoadedEngine::OpenRouterWhisper)) {
                Self::ensure_expected_model(&engine_guard, expected_model)?;
                drop(engine_guard);
                // Require the configured provider to exist AND carry a key — never
                // send an unauthenticated request (which just 401s silently).
                let provider = settings
                    .llm_provider(&settings.openrouter_transcription_provider_ref)
                    .filter(|p| !p.api_key.trim().is_empty())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "OpenRouter transcription needs a provider with an API key. \
                             Pick an OpenRouter provider (with a key) in Advanced > Transcription."
                        )
                    })?;
                let base_url = provider.base_url.clone();
                let api_key = provider.api_key.clone();
                // Default to a known-good Whisper STT id when the user hasn't set one.
                let model = {
                    let m = settings.openrouter_transcription_model.trim();
                    if m.is_empty() {
                        "openai/whisper-large-v3"
                    } else {
                        m
                    }
                };
                // Keep None ("auto") — the OpenRouter STT endpoint auto-detects.
                let language = normalize_language_for_engine(&settings.selected_language);
                let raw_text = crate::managers::openrouter_transcription::transcribe(
                    &base_url,
                    &api_key,
                    model,
                    &audio,
                    language.as_deref(),
                    settings.openrouter_transcription_route,
                    settings.openrouter_transcription_audio_format,
                )?;
                let corrected = if !settings.custom_words.is_empty() {
                    apply_custom_words(
                        &raw_text,
                        &settings.custom_words,
                        settings.word_correction_threshold,
                    )
                } else {
                    raw_text
                };
                let filtered = filter_transcription_output(&corrected);
                self.maybe_unload_immediately("OpenRouter transcription");
                return Ok(filtered);
            }
        }

        // Perform transcription with the appropriate engine.
        // We use catch_unwind to prevent engine panics from poisoning the mutex,
        // which would make the app hang indefinitely on subsequent operations.
        let result = {
            let mut engine_guard = self.lock_engine();

            // Take the engine out so we own it during transcription.
            // If the engine panics, we simply don't put it back (effectively unloading it)
            // instead of poisoning the mutex.
            let mut engine = match engine_guard.engine.take() {
                Some(e) => e,
                None => {
                    return Err(anyhow::anyhow!(
                        "Model failed to load after auto-load attempt. Please check your model settings."
                    ));
                }
            };
            // Identity of the engine we took — a concurrent load/unload can
            // legitimately change the loaded model while inference runs (the
            // engine lives outside its mutex during the call). Read straight
            // off the guard we already hold — `get_current_model()` locks
            // this SAME combined mutex (finding 8b) and would deadlock here.
            let taken_model_id = engine_guard.model_id.clone();
            let taken_instance = engine_guard.instance;

            // Revalidate the EXPECTED model at actual inference time (pass-3
            // finding 8a): the preflight comparison happens long before this
            // point, and the shared engine slot can legitimately swap models
            // in the gap (dictation vs the Translator's batch override). A
            // mismatch puts the engine straight back (we still hold the
            // guard) and errors instead of silently transcribing through the
            // wrong model.
            if !expected_model.is_empty() {
                if let Some(loaded) = taken_model_id.as_deref() {
                    if loaded != expected_model {
                        engine_guard.engine = Some(engine);
                        return Err(anyhow::anyhow!(
                            "Loaded model '{}' does not match the expected model '{}' \
                             (it changed mid-take) — please retry.",
                            loaded,
                            expected_model
                        ));
                    }
                }
            }

            // Release the lock before transcribing — no mutex held during the engine call
            drop(engine_guard);

            // Pad trailing silence for engines that drop final tokens when the
            // audio ends abruptly (tail segment at stop, ~45 s hard cuts, live
            // 3 s timer cuts). Whisper gets its audio untouched.
            let audio = match &engine {
                LoadedEngine::Whisper(_) => audio,
                _ => {
                    debug!(
                        "Padding {} samples of trailing silence for non-Whisper engine",
                        TRAILING_SILENCE_PAD_SAMPLES
                    );
                    pad_trailing_silence(audio, TRAILING_SILENCE_PAD_SAMPLES)
                }
            };

            let transcribe_result = catch_unwind(AssertUnwindSafe(
                || -> Result<transcribe_rs::TranscriptionResult> {
                    match &mut engine {
                        LoadedEngine::Whisper(whisper_engine) => {
                            let params = WhisperInferenceParams {
                                language: normalize_language_for_engine(
                                    &settings.selected_language,
                                ),
                                translate: effective_translate,
                                ..Default::default()
                            };

                            whisper_engine
                                .transcribe_samples(audio, Some(params))
                                .map_err(|e| anyhow::anyhow!("Whisper transcription failed: {}", e))
                        }
                        LoadedEngine::Parakeet(parakeet_engine) => {
                            let params = ParakeetInferenceParams {
                                timestamp_granularity: TimestampGranularity::Segment,
                                ..Default::default()
                            };
                            parakeet_engine
                                .transcribe_samples(audio, Some(params))
                                .map_err(|e| {
                                    anyhow::anyhow!("Parakeet transcription failed: {}", e)
                                })
                        }
                        LoadedEngine::Moonshine(moonshine_engine) => moonshine_engine
                            .transcribe_samples(audio, None)
                            .map_err(|e| anyhow::anyhow!("Moonshine transcription failed: {}", e)),
                        LoadedEngine::MoonshineStreaming(streaming_engine) => streaming_engine
                            .transcribe_samples(audio, None)
                            .map_err(|e| {
                                anyhow::anyhow!("Moonshine streaming transcription failed: {}", e)
                            }),
                        LoadedEngine::SenseVoice(sense_voice_engine) => {
                            let language = match settings.selected_language.as_str() {
                                "zh" | "zh-Hans" | "zh-Hant" => SenseVoiceLanguage::Chinese,
                                "en" => SenseVoiceLanguage::English,
                                "ja" => SenseVoiceLanguage::Japanese,
                                "ko" => SenseVoiceLanguage::Korean,
                                "yue" => SenseVoiceLanguage::Cantonese,
                                _ => SenseVoiceLanguage::Auto,
                            };
                            let params = SenseVoiceInferenceParams {
                                language,
                                use_itn: true,
                            };
                            sense_voice_engine
                                .transcribe_samples(audio, Some(params))
                                .map_err(|e| {
                                    anyhow::anyhow!("SenseVoice transcription failed: {}", e)
                                })
                        }
                        #[cfg(not(target_os = "macos"))]
                        LoadedEngine::FlmWhisper => {
                            // FLM transcription is handled earlier via flm_manager; this arm
                            // should never be reached.
                            Err(anyhow::anyhow!(
                                "FlmWhisper engine should use FLM manager path"
                            ))
                        }
                        LoadedEngine::ApiWhisper => {
                            // API transcription is handled earlier; this arm should never be reached.
                            Err(anyhow::anyhow!("ApiWhisper engine should use API path"))
                        }
                        LoadedEngine::OpenRouterWhisper => {
                            // Handled earlier via the OpenRouter path; never reached.
                            Err(anyhow::anyhow!(
                                "OpenRouterWhisper engine should use the OpenRouter path"
                            ))
                        }
                    }
                },
            ));

            match transcribe_result {
                Ok(inner_result) => {
                    // Success or normal error — put the engine back, UNLESS a
                    // concurrent load/unload mutated the state while we were
                    // transcribing. The INSTANCE counter (not model_id) is the
                    // identity: every install/unload bumps it, so a
                    // replacement engine carrying the same model id — or a
                    // manual unload — reads as a different instance and is
                    // never clobbered (pass-3 finding 8). Take-out itself
                    // doesn't bump, so an unchanged instance means untouched.
                    let mut engine_guard = self.lock_engine();
                    if engine_guard.engine.is_none() && engine_guard.instance == taken_instance {
                        engine_guard.engine = Some(engine);
                    } else {
                        info!(
                            "Loaded model changed during transcription; dropping the \
                             previous engine instead of restoring it"
                        );
                        drop(engine_guard);
                        drop(engine);
                    }
                    inner_result?
                }
                Err(panic_payload) => {
                    // Engine panicked — do NOT put it back (it's in an unknown state).
                    // The engine is dropped here, effectively unloading it.
                    let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "unknown panic".to_string()
                    };
                    error!("Transcription engine panicked: {}", panic_msg);

                    // Clear the state so the model reloads on next attempt —
                    // but ONLY if OUR instance is still current (pass-3
                    // finding 8): a concurrent load may have installed a
                    // replacement engine, possibly with the SAME model id, and
                    // the panicked instance's cleanup must never tear that
                    // down. The "unloaded" event fires only when this cleanup
                    // actually cleared the state — if a replacement survived,
                    // nothing was unloaded from the app's point of view.
                    // `!cleared` alone doesn't say WHAT superseded us — a
                    // newer load (replacement engine alive) and a concurrent
                    // unload (slot empty) both bump the instance. Capture the
                    // distinction under the same lock so the diagnostics can
                    // tell the truth for both cases.
                    let (cleared, replacement_alive) = {
                        let mut state = self.engine.lock().unwrap_or_else(|e| e.into_inner());
                        if state.instance == taken_instance {
                            state.engine = None;
                            state.model_id = None;
                            state.instance += 1;
                            (true, false)
                        } else {
                            (false, state.engine.is_some())
                        }
                    };

                    if cleared {
                        let _ = self.app_handle.emit(
                            "model-state-changed",
                            ModelStateEvent {
                                event_type: "unloaded".to_string(),
                                model_id: None,
                                model_name: None,
                                error: Some(format!("Engine panicked: {}", panic_msg)),
                            },
                        );
                    } else if replacement_alive {
                        info!(
                            "Panicked engine instance was already replaced by a newer load — \
                             leaving the replacement untouched"
                        );
                    } else {
                        info!(
                            "Panicked engine instance was already unloaded concurrently — \
                             nothing to clean up"
                        );
                    }

                    return Err(anyhow::anyhow!(
                        "Transcription engine panicked: {}. {}",
                        panic_msg,
                        if cleared {
                            "The model has been unloaded and will reload on next attempt."
                        } else if replacement_alive {
                            "A newer model load already replaced it; the replacement is untouched."
                        } else {
                            "It was already unloaded concurrently; nothing further was changed."
                        }
                    ));
                }
            }
        };

        // Apply word correction if custom words are configured
        let corrected_result = if !settings.custom_words.is_empty() {
            apply_custom_words(
                &result.text,
                &settings.custom_words,
                settings.word_correction_threshold,
            )
        } else {
            result.text
        };

        // Filter out filler words and hallucinations
        let filtered_result = filter_transcription_output(&corrected_result);

        let et = std::time::Instant::now();
        let translation_note = if effective_translate {
            " (translated)"
        } else {
            ""
        };
        info!(
            "Transcription completed in {}ms{}",
            (et - st).as_millis(),
            translation_note
        );

        let final_result = filtered_result;

        if final_result.is_empty() {
            info!("Transcription result is empty");
        } else {
            // Length at info; CONTENT only at debug — release file logs are
            // info-level and dictated text can be sensitive.
            info!("Transcription result: {} chars", final_result.len());
            debug!("Transcription result: {}", final_result);
        }

        self.maybe_unload_immediately("transcription");

        Ok(final_result)
    }
}

impl Drop for TranscriptionManager {
    fn drop(&mut self) {
        debug!("Shutting down TranscriptionManager");

        // Signal the watcher thread to shutdown
        self.shutdown_signal.store(true, Ordering::Relaxed);

        // Wait for the thread to finish gracefully
        if let Some(handle) = self.watcher_handle.lock().unwrap().take() {
            if let Err(e) = handle.join() {
                warn!("Failed to join idle watcher thread: {:?}", e);
            } else {
                debug!("Idle watcher thread joined successfully");
            }
        }
    }
}
