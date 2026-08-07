use crate::settings::{get_settings, write_settings};
use anyhow::Result;
use flate2::read::GzDecoder;
use futures_util::StreamExt;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tar::Archive;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

/// Wakeable cancellation handle for an in-flight model download.
///
/// The `AtomicBool` is the source of truth for "was this cancelled"; the
/// `Notify` lets `cancel()` immediately wake a download task that is parked on
/// the next network chunk (or inside its stall window) instead of it only
/// noticing after another chunk finally arrives. `notify_one()` stores a permit
/// even if no waiter is currently parked, so a cancel that races the task
/// between `select!` iterations is not lost — the next `cancelled().await`
/// returns at once.
struct DownloadCancel {
    cancelled: AtomicBool,
    notify: Notify,
}

impl DownloadCancel {
    fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.notify.notify_one();
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Resolves as soon as cancellation has been requested. Safe to call in a
    /// `select!` arm every loop iteration.
    async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        self.notify.notified().await;
    }
}

/// One in-flight download attempt for a model. `generation` is a process-unique
/// id (from `ModelManager::download_seq`) so cleanup, cancellation, and the
/// terminal event a task emits can be tied to THIS attempt and never to a newer
/// retry that has since replaced it in `active_downloads`.
struct DownloadAttempt {
    generation: u64,
    cancel: Arc<DownloadCancel>,
}

/// How the streaming loop ended (distinct from an error return).
enum DownloadOutcome {
    /// Server sent all bytes (natural EOF).
    Completed,
    /// A cancel was observed; the partial file is left intact for resume.
    Cancelled,
}

/// Stream `stream` into `file`, racing each chunk against `cancel` and a stall
/// timeout in a biased `select!`. Returns `Cancelled` the instant a cancel is
/// seen (even while parked on the network or inside the stall window),
/// `Completed` on natural EOF, or an error if the transfer stalls or a chunk
/// errors. Deliberately free of any `ModelManager`/`AppHandle` dependency so it
/// can be unit-tested against a real stalled HTTP server; the caller supplies
/// `on_progress` to surface bytes written (the app emits Tauri events there).
async fn stream_to_file_with_cancel<S, B>(
    stream: &mut S,
    file: &mut std::fs::File,
    cancel: &DownloadCancel,
    stall_timeout: Duration,
    mut on_progress: impl FnMut(usize),
) -> Result<DownloadOutcome>
where
    S: futures_util::Stream<Item = reqwest::Result<B>> + Unpin,
    B: AsRef<[u8]>,
{
    loop {
        let chunk = tokio::select! {
            // Bias toward cancellation so a cancel that arrives mid-stall is
            // honored before we consider the network at all.
            biased;

            _ = cancel.cancelled() => {
                return Ok(DownloadOutcome::Cancelled);
            }

            result = tokio::time::timeout(stall_timeout, stream.next()) => {
                match result {
                    Ok(Some(chunk)) => chunk,
                    Ok(None) => return Ok(DownloadOutcome::Completed),
                    Err(_) => {
                        // A cancel can land exactly as the stall fires; honor it
                        // over reporting a stall so the caller keeps the partial.
                        if cancel.is_cancelled() {
                            return Ok(DownloadOutcome::Cancelled);
                        }
                        return Err(anyhow::anyhow!(
                            "Download stalled: no data received for {}s",
                            stall_timeout.as_secs()
                        ));
                    }
                }
            }
        };

        let chunk = chunk?;
        let bytes = chunk.as_ref();
        file.write_all(bytes)?;
        on_progress(bytes.len());
    }
}

/// RAII guard that clears a download attempt's transient state (its
/// `is_downloading` flag and its `active_downloads` entry) on EVERY early exit
/// from `download_model` after registration — cancellation, stall, stream
/// error, size mismatch, or any extraction/finalize error — unless disarmed on
/// success. Cleanup is scoped to the attempt's `generation`, so an old
/// cancelled attempt that drops AFTER a retry replaced the map entry never
/// deletes the retry's handle or clears its state.
struct RegistrationGuard<'a> {
    manager: &'a ModelManager,
    model_id: &'a str,
    generation: u64,
    armed: bool,
}

impl<'a> RegistrationGuard<'a> {
    fn new(manager: &'a ModelManager, model_id: &'a str, generation: u64) -> Self {
        Self {
            manager,
            model_id,
            generation,
            armed: true,
        }
    }

    /// Disarm on the success path so completion state (is_downloaded = true) is
    /// not touched by the guard.
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for RegistrationGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            // An armed drop means a genuine error `?`-exit (the cancel,
            // complete, and already-complete paths all disarm first). Clear this
            // attempt's state, then emit a generation-tagged terminal failure
            // event so the frontend clears exactly this attempt's UI state (and
            // never a newer retry's).
            self.manager
                .clear_download_registration(self.model_id, self.generation);
            let _ = self.manager.app_handle.emit(
                "model-download-error",
                DownloadEvent {
                    model_id: self.model_id.to_string(),
                    generation: self.generation,
                    error: Some("Model download failed".to_string()),
                },
            );
        }
    }
}

/// RAII guard for the extraction phase. On ANY early exit during extraction
/// (create_dir / open / unpack / read_dir / rename / remove errors), unless
/// disarmed after a successful extraction, it removes the model from the backend
/// `extracting_models` set. It deliberately does NOT emit a frontend event: an
/// extraction error also `?`-returns through the still-armed `RegistrationGuard`,
/// whose single `model-download-error` event is the ONE terminal failure event
/// for the attempt (its frontend handler clears extractingModels too). Emitting
/// here as well would double-report one attempt's failure.
struct ExtractingGuard<'a> {
    manager: &'a ModelManager,
    model_id: &'a str,
    armed: bool,
}

impl<'a> ExtractingGuard<'a> {
    fn new(manager: &'a ModelManager, model_id: &'a str) -> Self {
        Self {
            manager,
            model_id,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ExtractingGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            if let Ok(mut extracting) = self.manager.extracting_models.lock() {
                extracting.remove(self.model_id);
            }
        }
    }
}

/// Best-effort removal of a path regardless of whether it is a file or a
/// directory. Used before committing a downloaded model so a wrong-type
/// artifact sitting at the destination (e.g. a directory where a file model
/// belongs, or vice versa) is cleared and the download can self-heal on retry.
fn remove_path_any_type(path: &Path) {
    if path.is_dir() {
        let _ = fs::remove_dir_all(path);
    } else if path.exists() {
        let _ = fs::remove_file(path);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub enum EngineType {
    Whisper,
    Parakeet,
    Moonshine,
    MoonshineStreaming,
    SenseVoice,
    #[cfg(not(target_os = "macos"))]
    FlmWhisper,
    ApiWhisper,
    /// OpenRouter transcription (JSON + base64 audio via the dedicated STT
    /// endpoint or chat `input_audio`). Unlike ApiWhisper it participates in
    /// the live/chunked per-segment pipeline.
    OpenRouterWhisper,
}

impl EngineType {
    /// Engines with no model artifact in Handy's models dir: FLM manages its
    /// own model cache (and its `filename` like "whisper-v3:turbo" is not even
    /// a valid Windows path), API/OpenRouter are remote endpoints. For these
    /// the registry's `is_downloaded` is authoritative — a filesystem probe
    /// must never overwrite it.
    pub fn is_external(&self) -> bool {
        match self {
            #[cfg(not(target_os = "macos"))]
            EngineType::FlmWhisper => true,
            EngineType::ApiWhisper | EngineType::OpenRouterWhisper => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub filename: String,
    pub url: Option<String>,
    pub size_mb: u64,
    pub is_downloaded: bool,
    pub is_downloading: bool,
    pub partial_size: u64,
    pub is_directory: bool,
    pub engine_type: EngineType,
    pub accuracy_score: f32,        // 0.0 to 1.0, higher is more accurate
    pub speed_score: f32,           // 0.0 to 1.0, higher is faster
    pub supports_translation: bool, // Whether the model supports translating to English
    pub is_recommended: bool,       // Whether this is the recommended model for new users
    pub supported_languages: Vec<String>, // Languages this model can transcribe
    pub is_custom: bool,            // Whether this is a user-provided custom model
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DownloadProgress {
    pub model_id: String,
    /// Per-attempt id. The frontend tracks the newest generation seen per model
    /// and ignores events from a superseded (older) attempt, so a stale event
    /// for a cancelled attempt cannot clobber a fresh retry of the same model.
    pub generation: u64,
    pub downloaded: u64,
    pub total: u64,
    pub percentage: f64,
}

/// Payload for terminal / lifecycle download events (complete, cancelled,
/// extraction-started/completed/failed). Carries the attempt `generation` so
/// the frontend can drop events belonging to a superseded attempt.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DownloadEvent {
    pub model_id: String,
    pub generation: u64,
    /// Present only on failure events (e.g. extraction-failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub struct ModelManager {
    app_handle: AppHandle,
    models_dir: PathBuf,
    available_models: Mutex<HashMap<String, ModelInfo>>,
    /// One entry per in-flight download, keyed by model id. Single-owner: a
    /// second concurrent `download_model` for the same id is rejected while an
    /// entry exists. Each entry carries a unique `generation` for
    /// attempt-scoped cleanup.
    active_downloads: Arc<Mutex<HashMap<String, DownloadAttempt>>>,
    /// Monotonic source of per-attempt generations.
    download_seq: AtomicU64,
    /// Per-model refresh revision, bumped on EVERY lifecycle transition that
    /// changes a model's is_downloaded/is_downloading — ATOMICALLY with that
    /// change (always mutated while holding `available_models`, as a strict
    /// inner lock). `update_download_status` snapshots a model's revision when
    /// it probes the disk and commits the probed value only if the revision is
    /// unchanged at commit time; otherwise a lifecycle transition happened
    /// during the probe and the stale disk snapshot is skipped. Closes the T-222
    /// probe→commit window (e.g. a download completing during a delete_model
    /// refresh). ALWAYS lock this AFTER available_models, never alone.
    refresh_revisions: Mutex<HashMap<String, u64>>,
    extracting_models: Arc<Mutex<HashSet<String>>>,
}

impl ModelManager {
    pub fn new(app_handle: &AppHandle) -> Result<Self> {
        // Create models directory in app data
        let models_dir = crate::portable::resolve_app_data_dir(app_handle)
            .map_err(|e| anyhow::anyhow!(e))?
            .join("models");

        if !models_dir.exists() {
            fs::create_dir_all(&models_dir)?;
        }

        let mut available_models = HashMap::new();

        // Whisper supported languages (99 languages from tokenizer)
        // Including zh-Hans and zh-Hant variants to match frontend language codes
        let whisper_languages: Vec<String> = vec![
            "en", "zh", "zh-Hans", "zh-Hant", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl",
            "ca", "nl", "ar", "sv", "it", "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs",
            "ro", "da", "hu", "ta", "no", "th", "ur", "hr", "bg", "lt", "la", "mi", "ml", "cy",
            "sk", "te", "fa", "lv", "bn", "sr", "az", "sl", "kn", "et", "mk", "br", "eu", "is",
            "hy", "ne", "mn", "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si", "km", "sn", "yo",
            "so", "af", "oc", "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo", "ht",
            "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt", "haw", "ln",
            "ha", "ba", "jw", "su", "yue",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        // TODO this should be read from a JSON file or something..
        available_models.insert(
            "small".to_string(),
            ModelInfo {
                id: "small".to_string(),
                name: "Whisper Small".to_string(),
                description: "Fast and fairly accurate.".to_string(),
                filename: "ggml-small.bin".to_string(),
                url: Some("https://blob.handy.computer/ggml-small.bin".to_string()),
                size_mb: 487,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.60,
                speed_score: 0.85,
                supports_translation: true,
                is_recommended: false,
                supported_languages: whisper_languages.clone(),
                is_custom: false,
            },
        );

        // Add downloadable models
        available_models.insert(
            "medium".to_string(),
            ModelInfo {
                id: "medium".to_string(),
                name: "Whisper Medium".to_string(),
                description: "Good accuracy, medium speed".to_string(),
                filename: "whisper-medium-q4_1.bin".to_string(),
                url: Some("https://blob.handy.computer/whisper-medium-q4_1.bin".to_string()),
                size_mb: 492, // Approximate size
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.75,
                speed_score: 0.60,
                supports_translation: true,
                is_recommended: false,
                supported_languages: whisper_languages.clone(),
                is_custom: false,
            },
        );

        available_models.insert(
            "turbo".to_string(),
            ModelInfo {
                id: "turbo".to_string(),
                name: "Whisper Large V3 Turbo".to_string(),
                description: "Balanced accuracy and speed (large-v3-turbo).".to_string(),
                filename: "ggml-large-v3-turbo.bin".to_string(),
                url: Some("https://blob.handy.computer/ggml-large-v3-turbo.bin".to_string()),
                size_mb: 1600, // Approximate size
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.80,
                speed_score: 0.40,
                supports_translation: false, // Turbo doesn't support translation
                is_recommended: false,
                supported_languages: whisper_languages.clone(),
                is_custom: false,
            },
        );

        available_models.insert(
            "large".to_string(),
            ModelInfo {
                id: "large".to_string(),
                name: "Whisper Large V3".to_string(),
                description:
                    "Full large-v3 (q5 quant). Best accuracy, slower; supports translation."
                        .to_string(),
                filename: "ggml-large-v3-q5_0.bin".to_string(),
                url: Some("https://blob.handy.computer/ggml-large-v3-q5_0.bin".to_string()),
                size_mb: 1100, // Approximate size
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.85,
                speed_score: 0.30,
                supports_translation: true,
                is_recommended: false,
                supported_languages: whisper_languages.clone(),
                is_custom: false,
            },
        );

        available_models.insert(
            "breeze-asr".to_string(),
            ModelInfo {
                id: "breeze-asr".to_string(),
                name: "Breeze ASR".to_string(),
                description: "Optimized for Taiwanese Mandarin. Code-switching support."
                    .to_string(),
                filename: "breeze-asr-q5_k.bin".to_string(),
                url: Some("https://blob.handy.computer/breeze-asr-q5_k.bin".to_string()),
                size_mb: 1080,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.85,
                speed_score: 0.35,
                supports_translation: false,
                is_recommended: false,
                supported_languages: whisper_languages.clone(),
                is_custom: false,
            },
        );

        // Add NVIDIA Parakeet models (directory-based)
        available_models.insert(
            "parakeet-tdt-0.6b-v2".to_string(),
            ModelInfo {
                id: "parakeet-tdt-0.6b-v2".to_string(),
                name: "Parakeet V2".to_string(),
                description: "English only. The best model for English speakers.".to_string(),
                filename: "parakeet-tdt-0.6b-v2-int8".to_string(), // Directory name
                url: Some("https://blob.handy.computer/parakeet-v2-int8.tar.gz".to_string()),
                size_mb: 473, // Approximate size for int8 quantized model
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::Parakeet,
                accuracy_score: 0.85,
                speed_score: 0.85,
                supports_translation: false,
                is_recommended: false,
                supported_languages: vec!["en".to_string()],
                is_custom: false,
            },
        );

        // Parakeet V3 supported languages (25 EU languages + Russian/Ukrainian):
        // bg, hr, cs, da, nl, en, et, fi, fr, de, el, hu, it, lv, lt, mt, pl, pt, ro, sk, sl, es, sv, ru, uk
        let parakeet_v3_languages: Vec<String> = vec![
            "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv",
            "lt", "mt", "pl", "pt", "ro", "sk", "sl", "es", "sv", "ru", "uk",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        available_models.insert(
            "parakeet-tdt-0.6b-v3".to_string(),
            ModelInfo {
                id: "parakeet-tdt-0.6b-v3".to_string(),
                name: "Parakeet V3".to_string(),
                description: "Fast and accurate. Supports 25 European languages.".to_string(),
                filename: "parakeet-tdt-0.6b-v3-int8".to_string(), // Directory name
                url: Some("https://blob.handy.computer/parakeet-v3-int8.tar.gz".to_string()),
                size_mb: 478, // Approximate size for int8 quantized model
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::Parakeet,
                accuracy_score: 0.80,
                speed_score: 0.85,
                supports_translation: false,
                is_recommended: true,
                supported_languages: parakeet_v3_languages,
                is_custom: false,
            },
        );

        available_models.insert(
            "moonshine-base".to_string(),
            ModelInfo {
                id: "moonshine-base".to_string(),
                name: "Moonshine Base".to_string(),
                description: "Very fast, English only. Handles accents well.".to_string(),
                filename: "moonshine-base".to_string(),
                url: Some("https://blob.handy.computer/moonshine-base.tar.gz".to_string()),
                size_mb: 58,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::Moonshine,
                accuracy_score: 0.70,
                speed_score: 0.90,
                supports_translation: false,
                is_recommended: false,
                supported_languages: vec!["en".to_string()],
                is_custom: false,
            },
        );

        available_models.insert(
            "moonshine-tiny-streaming-en".to_string(),
            ModelInfo {
                id: "moonshine-tiny-streaming-en".to_string(),
                name: "Moonshine V2 Tiny".to_string(),
                description: "Ultra-fast, English only".to_string(),
                filename: "moonshine-tiny-streaming-en".to_string(),
                url: Some(
                    "https://blob.handy.computer/moonshine-tiny-streaming-en.tar.gz".to_string(),
                ),
                size_mb: 31,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::MoonshineStreaming,
                accuracy_score: 0.55,
                speed_score: 0.95,
                supports_translation: false,
                is_recommended: false,
                supported_languages: vec!["en".to_string()],
                is_custom: false,
            },
        );

        available_models.insert(
            "moonshine-small-streaming-en".to_string(),
            ModelInfo {
                id: "moonshine-small-streaming-en".to_string(),
                name: "Moonshine V2 Small".to_string(),
                description: "Fast, English only. Good balance of speed and accuracy.".to_string(),
                filename: "moonshine-small-streaming-en".to_string(),
                url: Some(
                    "https://blob.handy.computer/moonshine-small-streaming-en.tar.gz".to_string(),
                ),
                size_mb: 100,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::MoonshineStreaming,
                accuracy_score: 0.65,
                speed_score: 0.90,
                supports_translation: false,
                is_recommended: false,
                supported_languages: vec!["en".to_string()],
                is_custom: false,
            },
        );

        available_models.insert(
            "moonshine-medium-streaming-en".to_string(),
            ModelInfo {
                id: "moonshine-medium-streaming-en".to_string(),
                name: "Moonshine V2 Medium".to_string(),
                description: "English only. High quality.".to_string(),
                filename: "moonshine-medium-streaming-en".to_string(),
                url: Some(
                    "https://blob.handy.computer/moonshine-medium-streaming-en.tar.gz".to_string(),
                ),
                size_mb: 192,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::MoonshineStreaming,
                accuracy_score: 0.75,
                speed_score: 0.80,
                supports_translation: false,
                is_recommended: false,
                supported_languages: vec!["en".to_string()],
                is_custom: false,
            },
        );

        // SenseVoice supported languages
        let sense_voice_languages: Vec<String> =
            vec!["zh", "zh-Hans", "zh-Hant", "en", "yue", "ja", "ko"]
                .into_iter()
                .map(String::from)
                .collect();

        available_models.insert(
            "sense-voice-int8".to_string(),
            ModelInfo {
                id: "sense-voice-int8".to_string(),
                name: "SenseVoice".to_string(),
                description: "Very fast. Chinese, English, Japanese, Korean, Cantonese."
                    .to_string(),
                filename: "sense-voice-int8".to_string(),
                url: Some("https://blob.handy.computer/sense-voice-int8.tar.gz".to_string()),
                size_mb: 160,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: true,
                engine_type: EngineType::SenseVoice,
                accuracy_score: 0.65,
                speed_score: 0.95,
                supports_translation: false,
                is_recommended: false,
                supported_languages: sense_voice_languages,
                is_custom: false,
            },
        );

        // FLM (FastFlowLM) — NPU-accelerated Whisper (Windows/Linux only)
        // FLM auto-downloads missing models on first use.
        #[cfg(not(target_os = "macos"))]
        {
            use crate::managers::flm::FlmManager;
            let flm_available = FlmManager::detect_flm().is_some();
            log::info!("FLM detection result: available={}", flm_available);

            if flm_available {
                available_models.insert(
                    "flm-whisper-v3-turbo".to_string(),
                    ModelInfo {
                        id: "flm-whisper-v3-turbo".to_string(),
                        name: "FLM Whisper V3 Turbo (NPU)".to_string(),
                        description: "NPU-accelerated via FLM. Auto-downloads on first use."
                            .to_string(),
                        filename: "whisper-v3:turbo".to_string(),
                        url: None,
                        size_mb: 0,
                        is_downloaded: true,
                        is_downloading: false,
                        partial_size: 0,
                        is_directory: false,
                        engine_type: EngineType::FlmWhisper,
                        accuracy_score: 0.85,
                        speed_score: 0.95,
                        // whisper-v3-turbo was trained without the translate
                        // objective, so it cannot translate to English.
                        supports_translation: false,
                        is_recommended: false,
                        supported_languages: whisper_languages.clone(),
                        is_custom: false,
                    },
                );
            }
        }

        // OpenAI-compatible transcription API — always available, user configures URL
        available_models.insert(
            "api-whisper".to_string(),
            ModelInfo {
                id: "api-whisper".to_string(),
                name: "API Transcription (OpenAI-compatible)".to_string(),
                description: "Use any OpenAI-compatible speech-to-text API. Configure the URL in Advanced settings.".to_string(),
                filename: String::new(),
                url: None,
                size_mb: 0,
                is_downloaded: true,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::ApiWhisper,
                accuracy_score: 0.90,
                speed_score: 0.80,
                supports_translation: true,
                is_recommended: false,
                supported_languages: whisper_languages.clone(),
                is_custom: false,
            },
        );

        // OpenRouter transcription — JSON + base64 audio (Whisper/Gemini/GPT-4o
        // audio models). Unlike api-whisper it supports live/chunked segments.
        available_models.insert(
            "openrouter-transcription".to_string(),
            ModelInfo {
                id: "openrouter-transcription".to_string(),
                name: "OpenRouter Transcription".to_string(),
                description: "Transcribe with OpenRouter models (Whisper, Gemini, GPT-4o-audio). Configure in Advanced > Providers.".to_string(),
                filename: String::new(),
                url: None,
                size_mb: 0,
                is_downloaded: true,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::OpenRouterWhisper,
                accuracy_score: 0.92,
                speed_score: 0.75,
                supports_translation: false,
                is_recommended: false,
                supported_languages: whisper_languages.clone(),
                is_custom: false,
            },
        );

        // Auto-discover custom Whisper models (.bin files) in the models directory
        if let Err(e) = Self::discover_custom_whisper_models(&models_dir, &mut available_models) {
            warn!("Failed to discover custom models: {}", e);
        }

        let manager = Self {
            app_handle: app_handle.clone(),
            models_dir,
            available_models: Mutex::new(available_models),
            active_downloads: Arc::new(Mutex::new(HashMap::new())),
            download_seq: AtomicU64::new(0),
            refresh_revisions: Mutex::new(HashMap::new()),
            extracting_models: Arc::new(Mutex::new(HashSet::new())),
        };

        // Migrate any bundled models to user directory
        manager.migrate_bundled_models()?;

        // Check which models are already downloaded. Startup is the only place
        // that cleans up leftover .extracting dirs (no downloads are in flight
        // yet, so the cleanup can't race a live extraction — T-222 item ii).
        manager.update_download_status(true)?;

        // Auto-select a model if none is currently selected
        manager.auto_select_model_if_needed()?;

        Ok(manager)
    }

    pub fn get_available_models(&self) -> Vec<ModelInfo> {
        let models = self.available_models.lock().unwrap();
        let mut result: Vec<ModelInfo> = models.values().cloned().collect();
        #[cfg(not(target_os = "macos"))]
        result.retain(|model| {
            model.id != "flm-whisper-v3-turbo"
                || crate::managers::flm::FlmManager::detect_flm().is_some()
        });
        // Inject the configured URL into the API Whisper model description
        let settings = crate::settings::get_settings(&self.app_handle);
        for model in &mut result {
            if model.id == "api-whisper" {
                let url = &settings.api_transcription_url;
                if url.is_empty() {
                    model.description =
                        "OpenAI-compatible API. Configure the URL in Advanced settings."
                            .to_string();
                } else {
                    model.description = format!("OpenAI-compatible API: {}", url);
                }
            }
        }
        result
    }

    pub fn get_model_info(&self, model_id: &str) -> Option<ModelInfo> {
        #[cfg(not(target_os = "macos"))]
        if model_id == "flm-whisper-v3-turbo"
            && crate::managers::flm::FlmManager::detect_flm().is_none()
        {
            return None;
        }
        let models = self.available_models.lock().unwrap();
        let mut info = models.get(model_id).cloned();
        if model_id == "api-whisper" {
            if let Some(ref mut model) = info {
                let settings = crate::settings::get_settings(&self.app_handle);
                let url = &settings.api_transcription_url;
                if url.is_empty() {
                    model.description =
                        "OpenAI-compatible API. Configure the URL in Advanced settings."
                            .to_string();
                } else {
                    model.description = format!("OpenAI-compatible API: {}", url);
                }
            }
        }
        info
    }

    fn migrate_bundled_models(&self) -> Result<()> {
        // Check for bundled models and copy them to user directory
        let bundled_models = ["ggml-small.bin"]; // Add other bundled models here if any

        for filename in &bundled_models {
            let bundled_path = self.app_handle.path().resolve(
                &format!("resources/models/{}", filename),
                tauri::path::BaseDirectory::Resource,
            );

            if let Ok(bundled_path) = bundled_path {
                if bundled_path.exists() {
                    let user_path = self.models_dir.join(filename);

                    // Only copy if user doesn't already have the model
                    if !user_path.exists() {
                        info!("Migrating bundled model {} to user directory", filename);
                        fs::copy(&bundled_path, &user_path)?;
                        info!("Successfully migrated {}", filename);
                    }
                }
            }
        }

        Ok(())
    }

    /// Refresh each local model's on-disk status.
    ///
    /// `cleanup_stale_extractions`: when true (startup only — no downloads are
    /// in flight then), remove leftover `<filename>.extracting` directories from
    /// interrupted extractions. When false (e.g. from `delete_model`, which can
    /// run while another model is extracting), the cleanup is skipped so it can
    /// never race and delete a live extraction's temp directory (T-222 item ii).
    fn update_download_status(&self, cleanup_stale_extractions: bool) -> Result<()> {
        // Two-phase to keep filesystem I/O OUT of any critical section, so a
        // refresh never blocks cancel_download / admission (which would regress
        // T-205 responsiveness).

        // Phase 0: snapshot model identity AND each model's refresh revision
        // (brief available_models -> refresh_revisions lock, no fs). The
        // revision snapshot lets phase 2 detect a lifecycle transition that
        // happened while we were probing the disk (T-222 item i).
        struct Probe {
            id: String,
            filename: String,
            is_directory: bool,
            revision: u64,
        }
        let probes: Vec<Probe> = {
            let models = self.available_models.lock().unwrap();
            let revs = self.refresh_revisions.lock().ok();
            models
                .values()
                .filter(|m| !m.engine_type.is_external())
                .map(|m| Probe {
                    id: m.id.clone(),
                    filename: m.filename.clone(),
                    is_directory: m.is_directory,
                    revision: revs
                        .as_ref()
                        .and_then(|r| r.get(&m.id).copied())
                        .unwrap_or(0),
                })
                .collect()
        };

        // Phase 1: filesystem probes (+ optional leftover .extracting cleanup)
        // with NO manager locks held. is_file()/is_dir() (not exists()) so a
        // wrong-type artifact is not reported as downloaded.
        let mut results: HashMap<String, (bool, u64)> = HashMap::new();
        for p in &probes {
            let model_path = self.models_dir.join(&p.filename);
            let partial_path = self.models_dir.join(format!("{}.partial", &p.filename));
            if p.is_directory && cleanup_stale_extractions {
                let extracting_path = self.models_dir.join(format!("{}.extracting", &p.filename));
                let is_currently_extracting = self
                    .extracting_models
                    .lock()
                    .map(|e| e.contains(&p.id))
                    .unwrap_or(false);
                if extracting_path.exists() && !is_currently_extracting {
                    warn!("Cleaning up interrupted extraction for model: {}", p.id);
                    let _ = fs::remove_dir_all(&extracting_path);
                }
            }
            // Match get_model_path's definition of "usable" EXACTLY: correct
            // artifact type AND no leftover .partial. Otherwise a model with a
            // lingering archive would be reported downloaded here yet rejected
            // by get_model_path at load time.
            let partial_exists = partial_path.exists();
            let artifact_ok = if p.is_directory {
                model_path.is_dir()
            } else {
                model_path.is_file()
            };
            let is_downloaded = artifact_ok && !partial_exists;
            let partial_size = if partial_exists {
                partial_path.metadata().map(|m| m.len()).unwrap_or(0)
            } else {
                0
            };
            results.insert(p.id.clone(), (is_downloaded, partial_size));
        }

        // Phase 2: short commit under available_models (+ refresh_revisions as a
        // strict inner lock). This refresh writes ONLY disk-derived facts
        // (is_downloaded, partial_size) and deliberately NEVER touches
        // is_downloading — owned solely by the download lifecycle.
        //
        // A model that is CURRENTLY downloading owns its is_downloaded /
        // partial_size, so skip active models. AND, to close the probe->commit
        // window, commit a probed value only if the model's refresh revision is
        // UNCHANGED since phase 0 — if a lifecycle transition (e.g. a download
        // completing during a delete_model refresh) bumped it, the disk snapshot
        // is stale, so skip it (its own transition already set the truth).
        let snapshot_revs: HashMap<&str, u64> =
            probes.iter().map(|p| (p.id.as_str(), p.revision)).collect();
        let active_ids: HashSet<String> = match self.active_downloads.lock() {
            Ok(active) => active.keys().cloned().collect(),
            Err(_) => HashSet::new(),
        };
        let mut models = self.available_models.lock().unwrap();
        let current_revs = self.refresh_revisions.lock().ok();
        for model in models.values_mut() {
            if model.engine_type.is_external() {
                continue;
            }
            if active_ids.contains(&model.id) {
                continue;
            }
            // Skip if a transition bumped the revision during our probe.
            let snap = snapshot_revs.get(model.id.as_str()).copied();
            let now = current_revs
                .as_ref()
                .and_then(|r| r.get(&model.id).copied())
                .unwrap_or(0);
            if snap != Some(now) {
                continue;
            }
            if let Some(&(is_downloaded, partial_size)) = results.get(&model.id) {
                model.is_downloaded = is_downloaded;
                model.partial_size = partial_size;
            }
        }

        Ok(())
    }

    fn auto_select_model_if_needed(&self) -> Result<()> {
        let mut settings = get_settings(&self.app_handle);

        // Clear stale selection: selected model is set but doesn't exist
        // in available_models (e.g. deleted custom model file)
        if !settings.selected_model.is_empty() {
            let models = self.available_models.lock().unwrap();
            let exists = models.contains_key(&settings.selected_model);
            drop(models);

            if !exists {
                info!(
                    "Selected model '{}' not found in available models, clearing selection",
                    settings.selected_model
                );
                settings.selected_model = String::new();
                write_settings(&self.app_handle, settings.clone());
            }
        }

        // If no model is selected, pick the first downloaded one
        if settings.selected_model.is_empty() {
            // Find the first available (downloaded) LOCAL model. External
            // engines are always "downloaded" but need user configuration
            // (API URL/key) or extra software (FLM) — never auto-select them.
            let models = self.available_models.lock().unwrap();
            if let Some(available_model) = models
                .values()
                .find(|model| model.is_downloaded && !model.engine_type.is_external())
            {
                info!(
                    "Auto-selecting model: {} ({})",
                    available_model.id, available_model.name
                );

                // Update settings with the selected model
                let mut updated_settings = settings;
                updated_settings.selected_model = available_model.id.clone();
                write_settings(&self.app_handle, updated_settings);

                info!("Successfully auto-selected model: {}", available_model.id);
            }
        }

        Ok(())
    }

    /// Discover custom Whisper models (.bin files) in the models directory.
    /// Skips files that match predefined model filenames.
    fn discover_custom_whisper_models(
        models_dir: &Path,
        available_models: &mut HashMap<String, ModelInfo>,
    ) -> Result<()> {
        if !models_dir.exists() {
            return Ok(());
        }

        // Collect filenames of predefined Whisper file-based models to skip
        let predefined_filenames: HashSet<String> = available_models
            .values()
            .filter(|m| matches!(m.engine_type, EngineType::Whisper) && !m.is_directory)
            .map(|m| m.filename.clone())
            .collect();

        // Scan models directory for .bin files
        for entry in fs::read_dir(models_dir)? {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    warn!("Failed to read directory entry: {}", e);
                    continue;
                }
            };

            let path = entry.path();

            // Only process .bin files (not directories)
            if !path.is_file() {
                continue;
            }

            let filename = match path.file_name().and_then(|s| s.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            // Skip hidden files
            if filename.starts_with('.') {
                continue;
            }

            // Only process .bin files (Whisper GGML format).
            // This also excludes .partial downloads (e.g., "model.bin.partial").
            // If we add discovery for other formats, add a .partial check before this filter.
            if !filename.ends_with(".bin") {
                continue;
            }

            // Skip predefined model files
            if predefined_filenames.contains(&filename) {
                continue;
            }

            // Generate model ID from filename (remove .bin extension)
            let model_id = filename.trim_end_matches(".bin").to_string();

            // Skip if model ID already exists (shouldn't happen, but be safe)
            if available_models.contains_key(&model_id) {
                continue;
            }

            // Generate display name: replace - and _ with space, capitalize words
            let display_name = model_id
                .replace(['-', '_'], " ")
                .split_whitespace()
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");

            // Get file size in MB
            let size_mb = match path.metadata() {
                Ok(meta) => meta.len() / (1024 * 1024),
                Err(e) => {
                    warn!("Failed to get metadata for {}: {}", filename, e);
                    0
                }
            };

            info!(
                "Discovered custom Whisper model: {} ({}, {} MB)",
                model_id, filename, size_mb
            );

            available_models.insert(
                model_id.clone(),
                ModelInfo {
                    id: model_id,
                    name: display_name,
                    description: "Not officially supported".to_string(),
                    filename,
                    url: None, // Custom models have no download URL
                    size_mb,
                    is_downloaded: true, // Already present on disk
                    is_downloading: false,
                    partial_size: 0,
                    is_directory: false,
                    engine_type: EngineType::Whisper,
                    accuracy_score: 0.0, // Sentinel: UI hides score bars when both are 0
                    speed_score: 0.0,
                    supports_translation: false,
                    is_recommended: false,
                    supported_languages: vec![],
                    is_custom: true,
                },
            );
        }

        Ok(())
    }

    pub async fn download_model(&self, model_id: &str) -> Result<()> {
        // Admit this attempt as the SOLE owner of this model's download as the
        // very FIRST thing — before even the available_models lookup — and
        // register its wakeable cancellation handle up front, so a cancel that
        // races the very start of the download always finds a handle (no
        // pre-admission window). A second concurrent download of the same model
        // is rejected here (no two writers to the same .partial). The error is
        // prefixed ALREADY_DOWNLOADING so the frontend can tell "another attempt
        // owns this" apart from a genuine failure and NOT clear that attempt's
        // UI state.
        let cancel = Arc::new(DownloadCancel::new());
        let generation = self
            .download_seq
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_add(1);
        {
            let mut active = self.active_downloads.lock().unwrap();
            if active.contains_key(model_id) {
                return Err(anyhow::anyhow!(
                    "ALREADY_DOWNLOADING: a download is already in progress for model: {}",
                    model_id
                ));
            }
            active.insert(
                model_id.to_string(),
                DownloadAttempt {
                    generation,
                    cancel: cancel.clone(),
                },
            );
        }

        // From here on, any early exit (`?`, explicit return, cancel, stall)
        // clears is_downloading + this attempt's registration exactly once via
        // the guard, unless disarmed on success. Cleanup is generation-scoped so
        // a superseding retry is never disturbed.
        let mut cleanup_guard = RegistrationGuard::new(self, model_id, generation);

        // Now resolve model info (guarded: any early return runs cleanup).
        let model_info = {
            let models = self.available_models.lock().unwrap();
            models.get(model_id).cloned()
        };
        let model_info =
            model_info.ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;
        let url = model_info
            .url
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No download URL for model"))?;
        let model_path = self.models_dir.join(&model_info.filename);
        let partial_path = self
            .models_dir
            .join(format!("{}.partial", &model_info.filename));

        // Mark downloading and immediately publish this attempt's generation to
        // the frontend (a zero-byte progress event) so any straggler event from
        // an older attempt of the same model is recognized as stale right away.
        if let Ok(mut models) = self.available_models.lock() {
            if let Some(model) = models.get_mut(model_id) {
                model.is_downloading = true;
            }
            self.bump_refresh_revision(model_id);
        }

        // Already complete: shortcut ONLY if the on-disk artifact is actually
        // usable (correct type) AND any leftover partial is gone — otherwise a
        // "complete" event would be emitted for a model that get_model_path
        // still rejects (partial present) or that is the wrong type. If we can't
        // make it usable, fall through and (re)download.
        let artifact_usable = if model_info.is_directory {
            model_path.is_dir()
        } else {
            model_path.is_file()
        };
        if artifact_usable {
            let partial_cleared = if partial_path.exists() {
                fs::remove_file(&partial_path).is_ok()
            } else {
                true
            };
            if partial_cleared {
                if let Ok(mut models) = self.available_models.lock() {
                    if let Some(model) = models.get_mut(model_id) {
                        model.is_downloaded = true;
                        model.is_downloading = false;
                        model.partial_size = 0;
                    }
                    self.bump_refresh_revision(model_id);
                }
                self.clear_download_registration(model_id, generation);
                cleanup_guard.disarm();
                self.emit_download_complete(model_id, generation);
                return Ok(());
            }
            warn!(
                "Model {} looks complete but its stale .partial could not be removed; re-downloading",
                model_id
            );
        }

        // Check if we have a partial download to resume
        let mut resume_from = if partial_path.exists() {
            let size = partial_path.metadata()?.len();
            info!("Resuming download of model {} from byte {}", model_id, size);
            size
        } else {
            info!("Starting fresh download of model {} from {}", model_id, url);
            0
        };

        // Publish the new attempt's generation to the frontend now (before the
        // blocking HTTP prep) via an initial progress event. It carries
        // `resume_from` as the already-downloaded baseline so the frontend's
        // speed calculation doesn't count previously-downloaded bytes as a
        // burst of new transfer on the first real chunk.
        let _ = self.app_handle.emit(
            "model-download-progress",
            &DownloadProgress {
                model_id: model_id.to_string(),
                generation,
                downloaded: resume_from,
                total: 0,
                percentage: 0.0,
            },
        );

        // Create HTTP client with range request for resuming. Connect timeout
        // only — a total-request deadline would kill large model downloads.
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .build()?;
        let mut request = client.get(&url);

        if resume_from > 0 {
            request = request.header("Range", format!("bytes={}-", resume_from));
        }

        // Race the request against cancellation too — connect_timeout bounds it,
        // but a cancel should not have to wait out even that.
        let mut response = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                cleanup_guard.disarm();
                self.emit_download_cancelled(model_id, generation);
                return Ok(());
            }
            r = request.send() => r?,
        };

        // If we tried to resume but server returned 200 (not 206 Partial Content),
        // the server doesn't support range requests. Delete partial file and restart
        // fresh to avoid file corruption (appending full file to partial).
        if resume_from > 0 && response.status() == reqwest::StatusCode::OK {
            warn!(
                "Server doesn't support range requests for model {}, restarting download",
                model_id
            );
            drop(response);
            let _ = fs::remove_file(&partial_path);

            // Reset resume_from since we're starting fresh
            resume_from = 0;

            // Restart download without range header
            response = tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    cleanup_guard.disarm();
                    self.emit_download_cancelled(model_id, generation);
                    return Ok(());
                }
                r = client.get(&url).send() => r?,
            };
        }

        // Check for success or partial content status
        if !response.status().is_success()
            && response.status() != reqwest::StatusCode::PARTIAL_CONTENT
        {
            // cleanup_guard clears is_downloading + registration on return.
            return Err(anyhow::anyhow!(
                "Failed to download model: HTTP {}",
                response.status()
            ));
        }

        let total_size = if resume_from > 0 {
            // For resumed downloads, add the resume point to content length
            resume_from + response.content_length().unwrap_or(0)
        } else {
            response.content_length().unwrap_or(0)
        };

        let mut downloaded = resume_from;
        // Box::pin so the stream is `Unpin` for the helper's `select!`.
        let mut stream = Box::pin(response.bytes_stream());

        // Open file for appending if resuming, or create new if starting fresh
        let mut file = if resume_from > 0 {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&partial_path)?
        } else {
            std::fs::File::create(&partial_path)?
        };

        // Emit initial progress
        let initial_progress = DownloadProgress {
            model_id: model_id.to_string(),
            generation,
            downloaded,
            total: total_size,
            percentage: if total_size > 0 {
                (downloaded as f64 / total_size as f64) * 100.0
            } else {
                0.0
            },
        };
        let _ = self
            .app_handle
            .emit("model-download-progress", &initial_progress);

        // Throttle progress events to max 10/sec (100ms intervals)
        let mut last_emit = Instant::now();
        let throttle_duration = Duration::from_millis(100);

        // Abort if the server stops sending bytes for this long; the partial
        // file is kept on disk so the download stays resumable.
        const STALL_TIMEOUT: Duration = Duration::from_secs(60);

        // Stream to disk, racing each chunk against the wakeable cancel handle
        // and the stall timeout (see stream_to_file_with_cancel). A cancel
        // interrupts even a stalled read immediately. Progress is emitted from
        // the callback (throttled). On a stream error the `?` propagates and
        // cleanup_guard clears state.
        let outcome =
            stream_to_file_with_cancel(&mut stream, &mut file, &cancel, STALL_TIMEOUT, |n| {
                downloaded += n as u64;
                if last_emit.elapsed() >= throttle_duration {
                    let progress = DownloadProgress {
                        model_id: model_id.to_string(),
                        generation,
                        downloaded,
                        total: total_size,
                        percentage: if total_size > 0 {
                            (downloaded as f64 / total_size as f64) * 100.0
                        } else {
                            0.0
                        },
                    };
                    let _ = self.app_handle.emit("model-download-progress", &progress);
                    last_emit = Instant::now();
                }
            })
            .await?;

        if let DownloadOutcome::Cancelled = outcome {
            // Close the file, keep the partial file for resume, and emit the one
            // terminal event for this attempt (emit_download_cancelled clears
            // state first). Disarm so the guard doesn't re-run the clear.
            drop(file);
            cleanup_guard.disarm();
            self.emit_download_cancelled(model_id, generation);
            return Ok(());
        }

        // Emit final progress to ensure 100% is shown
        let final_progress = DownloadProgress {
            model_id: model_id.to_string(),
            generation,
            downloaded,
            total: total_size,
            percentage: if total_size > 0 {
                (downloaded as f64 / total_size as f64) * 100.0
            } else {
                100.0
            },
        };
        let _ = self
            .app_handle
            .emit("model-download-progress", &final_progress);

        file.flush()?;
        drop(file); // Ensure file is closed before moving

        // Verify downloaded file size matches expected size
        if total_size > 0 {
            let actual_size = partial_path.metadata()?.len();
            if actual_size != total_size {
                // Incomplete/corrupted: delete the partial so the next attempt
                // restarts cleanly. cleanup_guard clears download state.
                let _ = fs::remove_file(&partial_path);
                return Err(anyhow::anyhow!(
                    "Download incomplete: expected {} bytes, got {} bytes",
                    total_size,
                    actual_size
                ));
            }
        }

        // Honor a cancel that landed during the final chunks BEFORE we commit
        // the download (extract or rename into place). Past this point the bytes
        // are all on disk, so a later cancel loses the race and the download
        // completes — but there is only ever one terminal event per attempt.
        if cancel.is_cancelled() {
            cleanup_guard.disarm();
            self.emit_download_cancelled(model_id, generation);
            return Ok(());
        }

        // Handle directory-based models (extract tar.gz) vs file-based models
        if model_info.is_directory {
            // Track that this model is being extracted
            {
                let mut extracting = self.extracting_models.lock().unwrap();
                extracting.insert(model_id.to_string());
            }
            // Clears the backend extracting_models set on ANY early exit during
            // extraction, disarmed on success. It does NOT emit — an extraction
            // error also unwinds through the armed RegistrationGuard, whose
            // single model-download-error is the one terminal failure event.
            let mut extracting_guard = ExtractingGuard::new(self, model_id);

            // Emit extraction started event
            let _ = self.app_handle.emit(
                "model-extraction-started",
                DownloadEvent {
                    model_id: model_id.to_string(),
                    generation,
                    error: None,
                },
            );
            info!("Extracting archive for directory-based model: {}", model_id);

            // Use a temporary extraction directory to ensure atomic operations
            let temp_extract_dir = self
                .models_dir
                .join(format!("{}.extracting", &model_info.filename));
            let final_model_dir = self.models_dir.join(&model_info.filename);

            // Clean up any previous incomplete extraction
            if temp_extract_dir.exists() {
                let _ = fs::remove_dir_all(&temp_extract_dir);
            }

            // Create temporary extraction directory
            fs::create_dir_all(&temp_extract_dir)?;

            // Open the downloaded tar.gz file
            let tar_gz = File::open(&partial_path)?;
            let tar = GzDecoder::new(tar_gz);
            let mut archive = Archive::new(tar);

            // Extract to the temporary directory first. On failure, clean up the
            // temp dir and propagate; extracting_guard clears the extracting set
            // and the armed RegistrationGuard emits the single terminal
            // model-download-error event.
            archive.unpack(&temp_extract_dir).map_err(|e| {
                let error_msg = format!("Failed to extract archive: {}", e);
                let _ = fs::remove_dir_all(&temp_extract_dir);
                anyhow::anyhow!(error_msg)
            })?;

            // Find the actual extracted directory (archive might have a nested structure)
            let extracted_dirs: Vec<_> = fs::read_dir(&temp_extract_dir)?
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
                .collect();

            if extracted_dirs.len() == 1 {
                // Single directory extracted, move it to the final location
                let source_dir = extracted_dirs[0].path();
                // Clear any existing artifact at the destination (file OR dir).
                remove_path_any_type(&final_model_dir);
                fs::rename(&source_dir, &final_model_dir)?;
                // Clean up temp directory
                let _ = fs::remove_dir_all(&temp_extract_dir);
            } else {
                // Multiple items or no directories, rename the temp directory itself
                remove_path_any_type(&final_model_dir);
                fs::rename(&temp_extract_dir, &final_model_dir)?;
            }

            info!("Successfully extracted archive for model: {}", model_id);
            // Extraction succeeded — remove from the extracting set exactly once
            // and disarm the guard so it doesn't remove it again.
            {
                let mut extracting = self.extracting_models.lock().unwrap();
                extracting.remove(model_id);
            }
            extracting_guard.disarm();

            // Remove the downloaded tar.gz BEFORE announcing completion. If it
            // lingers, get_model_path would reject the (otherwise extracted)
            // model, so treat a removal failure as an error rather than emit a
            // false completion — the next attempt hits the already-complete path
            // and retries the removal. Emitting model-extraction-completed only
            // after the archive is gone means no lifecycle event ever reports a
            // model that get_model_path would still reject.
            if partial_path.exists() {
                if let Err(e) = fs::remove_file(&partial_path) {
                    return Err(anyhow::anyhow!(
                        "Extracted model {} but could not remove its archive {}: {}",
                        model_id,
                        partial_path.display(),
                        e
                    ));
                }
            }

            // Emit extraction completed event (now that the model is finalized).
            let _ = self.app_handle.emit(
                "model-extraction-completed",
                DownloadEvent {
                    model_id: model_id.to_string(),
                    generation,
                    error: None,
                },
            );
        } else {
            // Move partial file to final location for file-based models. Clear
            // any wrong-type artifact (e.g. a directory) at the destination
            // first so the rename can't fail on it.
            remove_path_any_type(&model_path);
            fs::rename(&partial_path, &model_path)?;
        }

        // Update download status
        {
            let mut models = self.available_models.lock().unwrap();
            if let Some(model) = models.get_mut(model_id) {
                model.is_downloading = false;
                model.is_downloaded = true;
                model.partial_size = 0;
            }
            self.bump_refresh_revision(model_id);
        }

        // Drop this attempt's registration on success (generation-scoped). This
        // only removes the active_downloads entry and sets is_downloading=false
        // (already false here); it never touches the is_downloaded=true set
        // above. Then disarm the guard so it doesn't run again on scope exit.
        self.clear_download_registration(model_id, generation);
        cleanup_guard.disarm();

        // Emit the single terminal completion event for this attempt.
        self.emit_download_complete(model_id, generation);

        info!(
            "Successfully downloaded model {} to {:?}",
            model_id, model_path
        );

        Ok(())
    }

    pub fn delete_model(&self, model_id: &str) -> Result<()> {
        debug!("ModelManager: delete_model called for: {}", model_id);

        let model_info = {
            let models = self.available_models.lock().unwrap();
            models.get(model_id).cloned()
        };

        let model_info =
            model_info.ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

        debug!("ModelManager: Found model info: {:?}", model_info);

        let model_path = self.models_dir.join(&model_info.filename);
        let partial_path = self
            .models_dir
            .join(format!("{}.partial", &model_info.filename));
        debug!("ModelManager: Model path: {:?}", model_path);
        debug!("ModelManager: Partial path: {:?}", partial_path);

        let mut deleted_something = false;

        if model_info.is_directory {
            // Delete complete model directory if it exists
            if model_path.exists() && model_path.is_dir() {
                info!("Deleting model directory at: {:?}", model_path);
                fs::remove_dir_all(&model_path)?;
                info!("Model directory deleted successfully");
                deleted_something = true;
            }
        } else {
            // Delete complete model file if it exists
            if model_path.exists() {
                info!("Deleting model file at: {:?}", model_path);
                fs::remove_file(&model_path)?;
                info!("Model file deleted successfully");
                deleted_something = true;
            }
        }

        // Delete partial file if it exists (same for both types)
        if partial_path.exists() {
            info!("Deleting partial file at: {:?}", partial_path);
            fs::remove_file(&partial_path)?;
            info!("Partial file deleted successfully");
            deleted_something = true;
        }

        if !deleted_something {
            return Err(anyhow::anyhow!("No model files found to delete"));
        }

        // Custom models should be removed from the list entirely since they
        // have no download URL and can't be re-downloaded
        if model_info.is_custom {
            let mut models = self.available_models.lock().unwrap();
            models.remove(model_id);
            debug!("ModelManager: removed custom model from available models");
        } else {
            // Update download status (marks predefined models as not
            // downloaded). Do NOT clean up .extracting dirs here — a delete can
            // run while another model is extracting (T-222 item ii).
            self.update_download_status(false)?;
            debug!("ModelManager: download status updated");
        }

        // Emit event to notify UI
        let _ = self.app_handle.emit("model-deleted", model_id);

        Ok(())
    }

    pub fn get_model_path(&self, model_id: &str) -> Result<PathBuf> {
        let model_info = self
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

        if !model_info.is_downloaded {
            return Err(anyhow::anyhow!("Model not available: {}", model_id));
        }

        // Ensure we don't return partial files/directories
        if model_info.is_downloading {
            return Err(anyhow::anyhow!(
                "Model is currently downloading: {}",
                model_id
            ));
        }

        let model_path = self.models_dir.join(&model_info.filename);
        let partial_path = self
            .models_dir
            .join(format!("{}.partial", &model_info.filename));

        if model_info.is_directory {
            // For directory-based models, ensure the directory exists and is complete
            if model_path.exists() && model_path.is_dir() && !partial_path.exists() {
                Ok(model_path)
            } else {
                Err(anyhow::anyhow!(
                    "Complete model directory not found: {}",
                    model_id
                ))
            }
        } else {
            // For file-based models: require an actual file (not a dir of the
            // same name) and no leftover partial.
            if model_path.is_file() && !partial_path.exists() {
                Ok(model_path)
            } else {
                Err(anyhow::anyhow!(
                    "Complete model file not found: {}",
                    model_id
                ))
            }
        }
    }

    /// Bump a model's refresh revision. MUST be called while already holding
    /// `available_models` (refresh_revisions is a strict inner lock), so the
    /// revision bump is atomic with the is_downloaded/is_downloading change it
    /// accompanies. This is what lets `update_download_status` detect that a
    /// lifecycle transition happened during its disk probe and skip the stale
    /// commit. Poison-safe.
    fn bump_refresh_revision(&self, model_id: &str) {
        if let Ok(mut revs) = self.refresh_revisions.lock() {
            *revs.entry(model_id.to_string()).or_insert(0) += 1;
        }
    }

    /// Clear a download attempt's transient state: mark the model
    /// not-downloading and drop its `active_downloads` entry — but ONLY if the
    /// entry still belongs to `generation`. If a newer attempt has replaced it,
    /// leave that attempt's state untouched. Poison-safe. Lock order here:
    /// active_downloads outer, available_models inner, refresh_revisions
    /// innermost (a strict subset of the global active -> available ->
    /// refresh_revisions order).
    fn clear_download_registration(&self, model_id: &str, generation: u64) {
        if let Ok(mut active) = self.active_downloads.lock() {
            match active.get(model_id) {
                Some(attempt) if attempt.generation == generation => {
                    active.remove(model_id);
                    if let Ok(mut models) = self.available_models.lock() {
                        if let Some(model) = models.get_mut(model_id) {
                            model.is_downloading = false;
                        }
                        // Atomic with the is_downloading change above.
                        self.bump_refresh_revision(model_id);
                    }
                }
                // A superseding retry (or nothing) owns the slot now — don't
                // clear its handle or its downloading flag.
                _ => {}
            }
        }
    }

    /// Emit the single terminal "cancelled" event for a download attempt AFTER
    /// clearing this attempt's backend state (generation-scoped), so the
    /// terminal event never precedes terminal state. Only the owning task calls
    /// this, so a cancellation is reported exactly once and never alongside a
    /// completion event.
    fn emit_download_cancelled(&self, model_id: &str, generation: u64) {
        self.clear_download_registration(model_id, generation);
        info!("Download cancelled for: {}", model_id);
        let _ = self.app_handle.emit(
            "model-download-cancelled",
            DownloadEvent {
                model_id: model_id.to_string(),
                generation,
                error: None,
            },
        );
    }

    /// Emit the single terminal "complete" event for a download attempt.
    fn emit_download_complete(&self, model_id: &str, generation: u64) {
        let _ = self.app_handle.emit(
            "model-download-complete",
            DownloadEvent {
                model_id: model_id.to_string(),
                generation,
                error: None,
            },
        );
    }

    pub fn cancel_download(&self, model_id: &str) -> Result<()> {
        debug!("ModelManager: cancel_download called for: {}", model_id);

        // Only wake the in-flight task so it aborts immediately (even parked on
        // the next chunk or inside its stall window) and cleans up + emits its
        // own terminal event. We deliberately do NOT flip is_downloading here,
        // do NOT call update_download_status (which would stomp EVERY model's
        // flag), and do NOT emit the cancelled event (the task owns it) — so a
        // cancel of model A can't disturb model B, and no attempt gets two
        // terminal events. The wakeable handle makes the task's cleanup prompt.
        let active = self
            .active_downloads
            .lock()
            .map_err(|_| anyhow::anyhow!("active_downloads mutex poisoned"))?;
        if let Some(attempt) = active.get(model_id) {
            attempt.cancel.cancel();
            info!("Cancellation requested for: {}", model_id);
        } else {
            warn!("No active download found for: {}", model_id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_discover_custom_whisper_models() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().to_path_buf();

        // Create test .bin files
        let mut custom_file = File::create(models_dir.join("my-custom-model.bin")).unwrap();
        custom_file.write_all(b"fake model data").unwrap();

        let mut another_file = File::create(models_dir.join("whisper_medical_v2.bin")).unwrap();
        another_file.write_all(b"another fake model").unwrap();

        // Create files that should be ignored
        File::create(models_dir.join(".hidden-model.bin")).unwrap(); // Hidden file
        File::create(models_dir.join("readme.txt")).unwrap(); // Non-.bin file
        File::create(models_dir.join("ggml-small.bin")).unwrap(); // Predefined filename
        fs::create_dir(models_dir.join("some-directory.bin")).unwrap(); // Directory

        // Set up available_models with a predefined Whisper model
        let mut models = HashMap::new();
        models.insert(
            "small".to_string(),
            ModelInfo {
                id: "small".to_string(),
                name: "Whisper Small".to_string(),
                description: "Test".to_string(),
                filename: "ggml-small.bin".to_string(),
                url: Some("https://example.com".to_string()),
                size_mb: 100,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.5,
                speed_score: 0.5,
                supports_translation: true,
                is_recommended: false,
                supported_languages: vec!["en".to_string()],
                is_custom: false,
            },
        );

        // Discover custom models
        ModelManager::discover_custom_whisper_models(&models_dir, &mut models).unwrap();

        // Should have discovered 2 custom models (my-custom-model and whisper_medical_v2)
        assert!(models.contains_key("my-custom-model"));
        assert!(models.contains_key("whisper_medical_v2"));

        // Verify custom model properties
        let custom = models.get("my-custom-model").unwrap();
        assert_eq!(custom.name, "My Custom Model");
        assert_eq!(custom.filename, "my-custom-model.bin");
        assert!(custom.url.is_none()); // Custom models have no URL
        assert!(custom.is_downloaded);
        assert!(custom.is_custom);
        assert_eq!(custom.accuracy_score, 0.0);
        assert_eq!(custom.speed_score, 0.0);
        assert!(custom.supported_languages.is_empty());

        // Verify underscore handling
        let medical = models.get("whisper_medical_v2").unwrap();
        assert_eq!(medical.name, "Whisper Medical V2");

        // Should NOT have discovered hidden, non-.bin, predefined, or directories
        assert!(!models.contains_key(".hidden-model"));
        assert!(!models.contains_key("readme"));
        assert!(!models.contains_key("some-directory"));
    }

    #[test]
    fn test_discover_custom_models_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().to_path_buf();

        let mut models = HashMap::new();
        let count_before = models.len();

        ModelManager::discover_custom_whisper_models(&models_dir, &mut models).unwrap();

        // No new models should be added
        assert_eq!(models.len(), count_before);
    }

    #[test]
    fn test_discover_custom_models_nonexistent_dir() {
        let models_dir = PathBuf::from("/nonexistent/path/that/does/not/exist");

        let mut models = HashMap::new();
        let count_before = models.len();

        // Should not error, just return Ok
        let result = ModelManager::discover_custom_whisper_models(&models_dir, &mut models);
        assert!(result.is_ok());
        assert_eq!(models.len(), count_before);
    }
}

#[cfg(test)]
mod download_cancel_tests {
    use super::{DownloadCancel, DownloadOutcome, stream_to_file_with_cancel};
    use std::io::{Read, Write};
    use std::sync::Arc;
    use std::time::Duration;

    /// The core T-205 property: a cancel must wake a task parked on the next
    /// chunk immediately — it must NOT wait for the (60s) stall timeout. Here a
    /// 60s sleep stands in for "blocked on the network"; the test fails via its
    /// own 1s deadline if `cancelled()` doesn't wake promptly.
    #[tokio::test]
    async fn cancel_wakes_a_blocked_wait_without_waiting_for_the_stall_timeout() {
        let cancel = Arc::new(DownloadCancel::new());
        let waiter_cancel = cancel.clone();
        let waiter = tokio::spawn(async move {
            tokio::select! {
                biased;
                _ = waiter_cancel.cancelled() => "cancelled",
                _ = tokio::time::sleep(Duration::from_secs(60)) => "stalled",
            }
        });

        // Let the waiter park on notify, then request cancellation.
        tokio::time::sleep(Duration::from_millis(20)).await;
        cancel.cancel();

        let outcome = tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("cancelled() did not wake within 1s of cancel()")
            .expect("waiter task panicked");
        assert_eq!(outcome, "cancelled");
        assert!(cancel.is_cancelled());
    }

    /// `cancelled()` on an already-cancelled handle must return immediately.
    #[tokio::test]
    async fn cancelled_returns_immediately_when_already_cancelled() {
        let cancel = DownloadCancel::new();
        cancel.cancel();
        tokio::time::timeout(Duration::from_millis(100), cancel.cancelled())
            .await
            .expect("cancelled() blocked despite a prior cancel()");
    }

    /// A cancel that fires BEFORE the task parks (i.e. between `select!`
    /// iterations, with no waiter registered) must not be lost — `notify_one`
    /// stores a permit so the next `cancelled()` still resolves promptly.
    #[tokio::test]
    async fn cancel_before_wait_is_not_lost() {
        let cancel = Arc::new(DownloadCancel::new());
        cancel.cancel(); // no waiter parked yet

        let waiter_cancel = cancel.clone();
        let outcome = tokio::time::timeout(Duration::from_secs(1), async move {
            tokio::select! {
                biased;
                _ = waiter_cancel.cancelled() => "cancelled",
                _ = tokio::time::sleep(Duration::from_secs(60)) => "stalled",
            }
        })
        .await
        .expect("a pre-wait cancel was lost; cancelled() never resolved");
        assert_eq!(outcome, "cancelled");
    }

    /// Acceptance #4: a real HTTP server that sends headers + a few bytes then
    /// STALLS. A cancel must interrupt the parked read promptly (well under the
    /// stall timeout, which is set to 10 minutes here so only the cancel can end
    /// it) and the partial file must be preserved for resume.
    #[tokio::test]
    async fn cancel_interrupts_a_stalled_http_stream_and_keeps_partial() {
        // Bind an ephemeral TCP server that speaks minimal HTTP/1.1, sends a few
        // bytes, then holds the connection open without sending more.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                let mut buf = [0u8; 2048];
                let _ = sock.read(&mut buf); // consume the request line/headers
                let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1048576\r\n\r\n");
                let _ = sock.write_all(&[0u8; 16]); // a few real bytes
                let _ = sock.flush();
                // Stall: keep the socket open but send nothing more.
                std::thread::sleep(Duration::from_secs(3));
            }
        });

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{}/", addr))
            .send()
            .await
            .expect("request to local stall server failed");
        let mut stream = Box::pin(resp.bytes_stream());

        let tmp = tempfile::TempDir::new().unwrap();
        let partial = tmp.path().join("model.partial");
        let mut file = std::fs::File::create(&partial).unwrap();

        let cancel = Arc::new(DownloadCancel::new());
        let canceller = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            canceller.cancel();
        });

        // Stall timeout is huge on purpose: only the cancel can end this
        // promptly. The 5s outer deadline fails the test if it doesn't.
        let outcome = tokio::time::timeout(
            Duration::from_secs(5),
            stream_to_file_with_cancel(
                &mut stream,
                &mut file,
                &cancel,
                Duration::from_secs(600),
                |_| {},
            ),
        )
        .await
        .expect("stream_to_file_with_cancel did not wake within 5s of cancel()")
        .expect("stream_to_file_with_cancel returned an error");

        assert!(
            matches!(outcome, DownloadOutcome::Cancelled),
            "expected Cancelled outcome"
        );

        drop(file);
        assert!(
            partial.exists(),
            "partial file must be preserved for resume"
        );

        let _ = server.join();
    }
}
