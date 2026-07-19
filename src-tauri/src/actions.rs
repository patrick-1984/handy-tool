use crate::TranscriptionCoordinator;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::apple_intelligence;
use crate::audio_feedback::{SoundType, play_feedback_sound, play_feedback_sound_blocking};
use crate::audio_toolkit::ClosedChunk;
use crate::managers::audio::AudioRecordingManager;
use crate::managers::history::HistoryManager;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::{
    APPLE_INTELLIGENCE_PROVIDER_ID, AppSettings, TranscriptionMode, get_settings,
};
use crate::shortcut;
use crate::tray::{TrayIconState, change_tray_icon};
use crate::utils::{
    self, show_processing_overlay, show_recording_overlay, show_transcribing_overlay,
};
use ferrous_opencc::{OpenCC, config::BuiltinConfig};
use log::{debug, error, info, warn};
use once_cell::sync::Lazy;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::AppHandle;
use tauri::{Emitter, Manager};

/// Payload for live transcription chunk events.
#[derive(Clone, Serialize)]
struct LiveTranscriptionChunk {
    index: usize,
    text: String,
    is_final: bool,
}

/// Drop guard that notifies the [`TranscriptionCoordinator`] when the
/// transcription pipeline finishes — whether it completes normally or panics.
struct FinishGuard(AppHandle);
impl Drop for FinishGuard {
    fn drop(&mut self) {
        if let Some(c) = self.0.try_state::<TranscriptionCoordinator>() {
            c.notify_processing_finished();
        }
    }
}

// Shortcut Action Trait
pub trait ShortcutAction: Send + Sync {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
}

// Transcribe Action
struct TranscribeAction {
    post_process: bool,
}

/// Field name for structured output JSON schema
const TRANSCRIPTION_FIELD: &str = "transcription";

/// Minimum number of audio samples (at 16 kHz) worth saving to history.
/// Below this threshold we treat the recording as an accidental tap.
const MIN_SAMPLES_TO_SAVE: usize = 16_000; // ≈ 1 second

/// Shared handle to the latest live transcription text, set in start(), read in stop().
static LIVE_TEXT: Lazy<Mutex<Option<Arc<Mutex<String>>>>> = Lazy::new(|| Mutex::new(None));

/// Flag indicating a segment transcription is in flight, used to avoid engine lock races.
static SEGMENT_BUSY: Lazy<Mutex<Option<Arc<AtomicBool>>>> = Lazy::new(|| Mutex::new(None));

/// Recording-start unix timestamp (seconds). Set in `start()`, read in `stop()`
/// to locate the recorder's glued `handy-{ts}.opus` for the history entry.
static RECORDING_TS: AtomicU64 = AtomicU64::new(0);

/// Per-recording state for the chunked (default Post-Recording) pipeline. Each
/// chunk is transcribed in the background as it closes; transcripts are joined
/// in index order on stop. Completion order is irrelevant (the BTreeMap orders
/// by chunk index).
struct ChunkedSession {
    ts: u64,
    transcripts: Mutex<BTreeMap<usize, Option<String>>>,
    closed_count: AtomicUsize,
    done_count: AtomicUsize,
    /// Total 16 kHz samples across all chunks, for the recording duration.
    total_samples: AtomicU64,
    /// Set when the recording is cancelled: queued chunk workers skip their
    /// (serialized, potentially expensive) transcription instead of burning
    /// the engine for a take nobody wants.
    abandoned: AtomicBool,
    /// When true (OpenRouter engine), chunks are NOT transcribed as they close;
    /// their PCM is buffered and the whole recording is sent in ONE request on
    /// stop — so nothing goes over the network mid-recording (the on-disk Opus
    /// chunks are still written for crash safety).
    deferred: bool,
    /// Buffered per-chunk PCM (only used when `deferred`).
    pcm: Mutex<BTreeMap<usize, Vec<f32>>>,
}

impl ChunkedSession {
    fn new(ts: u64, deferred: bool) -> Self {
        Self {
            ts,
            transcripts: Mutex::new(BTreeMap::new()),
            closed_count: AtomicUsize::new(0),
            done_count: AtomicUsize::new(0),
            total_samples: AtomicU64::new(0),
            abandoned: AtomicBool::new(false),
            deferred,
            pcm: Mutex::new(BTreeMap::new()),
        }
    }

    /// Concatenate buffered chunk PCM in index order (deferred mode).
    fn assemble_pcm(&self) -> Vec<f32> {
        let map = self.pcm.lock().unwrap();
        map.values().flat_map(|v| v.iter().copied()).collect()
    }

    /// Join the per-chunk transcripts in chunk-index order, skipping chunks that
    /// produced no text (silent, failed, or timed out).
    fn assemble(&self) -> String {
        let map = self.transcripts.lock().unwrap();
        map.values()
            .filter_map(|o| o.as_deref())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

static CHUNKED_SESSION: Lazy<Mutex<Option<Arc<ChunkedSession>>>> = Lazy::new(|| Mutex::new(None));

/// Pipeline decisions made in `start()` and consumed by `stop()`, so a
/// mid-recording settings/model change can never route `stop()` down a path
/// `start()` didn't set up (which silently discarded the take one way and
/// leaked a stale chunk callback the other).
#[derive(Clone, Copy)]
struct RecordingPlan {
    live: bool,
    chunked: bool,
    crash_safe: bool,
}

static RECORDING_PLAN: Lazy<Mutex<Option<RecordingPlan>>> = Lazy::new(|| Mutex::new(None));

/// Chunk transcriptions run strictly one at a time. The engine is a single
/// serial resource and a concurrent `transcribe()` fails fast (the engine is
/// taken out of its mutex while in use), which used to leave a permanent
/// silent hole in the assembled text whenever chunks closed faster than they
/// transcribed. `pub(crate)` because EVERY local engine call in the app must
/// serialize through this lock now that the Translator's batch worker also
/// uses the engine — a waiter blocks for at most one segment (seconds).
pub(crate) static CHUNK_TRANSCRIBE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

/// A failed paste must never silently swallow the take: park the text on the
/// clipboard (best effort) and tell the user via a global toast. The text is
/// also in History, but the toast is what stops the "where did my words go"
/// confusion in the moment.
fn report_paste_failure(app: &AppHandle, text: &str, err: &str) {
    error!("Failed to paste transcription: {}", err);
    // park_text bumps the paste generation first — a pending delayed
    // clipboard-restore must never overwrite the parked transcription.
    let parked = crate::clipboard::park_text(app, text);
    let _ = app.emit(
        "paste-failed",
        serde_json::json!({ "error": err, "parked": parked }),
    );
}

/// Compute the unix-second timestamp for a new recording.
fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Save a transcription to history. When crash-safe (chunked Opus) recording is
/// on, the recorder has already written `handy-{ts}.opus`, so we point the
/// history row at it; otherwise we write a WAV from the in-memory samples.
#[allow(clippy::too_many_arguments)]
async fn save_history(
    hm: &HistoryManager,
    crash_safe: bool,
    ts: u64,
    samples: Vec<f32>,
    text: String,
    post_processed: Option<String>,
    prompt: Option<String>,
    cost_usd: Option<f64>,
    duration_seconds: Option<f64>,
    model_used: Option<String>,
) {
    let res = if crash_safe {
        hm.save_transcription_with_file(
            format!("handy-{}.opus", ts),
            ts as i64,
            text,
            post_processed,
            prompt,
            cost_usd,
            duration_seconds,
            model_used,
        )
        .await
    } else {
        hm.save_transcription(
            samples,
            text,
            post_processed,
            prompt,
            cost_usd,
            duration_seconds,
            model_used,
        )
        .await
    };
    if let Err(e) = res {
        error!("Failed to save transcription to history: {}", e);
    }
}

/// 16 kHz sample count → duration in seconds.
fn samples_to_seconds(n: usize) -> f64 {
    n as f64 / 16_000.0
}

/// Human label of the transcription engine/model in use, for the history entry
/// (e.g. "Whisper Large — local", "openai/whisper-large-v3 — OpenRouter").
fn model_label(app: &AppHandle) -> Option<String> {
    let settings = get_settings(app);
    let id = settings.selected_model.clone();
    if id.is_empty() {
        return None;
    }
    let label = match id.as_str() {
        "openrouter-transcription" => {
            let m = settings.openrouter_transcription_model.trim();
            let m = if m.is_empty() {
                "openai/whisper-large-v3"
            } else {
                m
            };
            format!("{} — OpenRouter", m)
        }
        "api-whisper" => {
            let m = settings.api_transcription_model.trim();
            if m.is_empty() {
                "API".to_string()
            } else {
                format!("{} — API", m)
            }
        }
        other => {
            let name = app
                .try_state::<Arc<crate::managers::model::ModelManager>>()
                .and_then(|mm| mm.get_model_info(other).map(|mi| mi.name))
                .unwrap_or_else(|| other.to_string());
            format!("{} — local", name)
        }
    };
    Some(label)
}

/// Tear down any active chunked-recording session (used on cancel). Clears the
/// chunk callback, drops the session state, and re-enables model unloading.
/// In-flight chunk transcription threads keep their own `Arc` and finish
/// harmlessly, writing into a map no one reads.
pub fn end_chunked_session(app: &AppHandle) {
    if let Some(rm) = app.try_state::<Arc<AudioRecordingManager>>() {
        rm.clear_on_chunk_callback();
        // Also clear the live-mode segment callback (cancel path): otherwise a
        // cancelled live session keeps feeding segments into stale state.
        rm.clear_segment_callback();
    }
    if let Some(tm) = app.try_state::<Arc<TranscriptionManager>>() {
        tm.set_live_transcribing(false);
    }
    if let Ok(mut g) = CHUNKED_SESSION.lock() {
        if let Some(session) = g.as_ref() {
            session.abandoned.store(true, Ordering::SeqCst);
        }
        *g = None;
    }
    // A cancelled recording's pipeline plan must not leak into a later stop().
    if let Ok(mut g) = RECORDING_PLAN.lock() {
        *g = None;
    }
    // Drop live-mode leftovers so a later stop() can't pick up stale text from
    // a cancelled recording (e.g. cancel in Live mode, then switch engines).
    if let Ok(mut g) = LIVE_TEXT.lock() {
        *g = None;
    }
    if let Ok(mut g) = SEGMENT_BUSY.lock() {
        *g = None;
    }
}

/// Strip invisible Unicode characters that some LLMs may insert
fn strip_invisible_chars(s: &str) -> String {
    s.replace(['\u{200B}', '\u{200C}', '\u{200D}', '\u{FEFF}'], "")
}

/// Strip `<think>...</think>` blocks that thinking models (e.g. Qwen3) may prepend.
fn strip_thinking_tags(s: &str) -> String {
    let mut result = s.to_string();
    while let Some(start) = result.find("<think>") {
        if let Some(end) = result.find("</think>") {
            let end_pos = end + "</think>".len();
            result = format!("{}{}", &result[..start], &result[end_pos..]);
        } else {
            // Unclosed <think> tag — strip from <think> to end
            result = result[..start].to_string();
            break;
        }
    }
    result.trim().to_string()
}

/// Build a system prompt from the user's prompt template.
/// Removes `${output}` placeholder since the transcription is sent as the user message.
fn build_system_prompt(prompt_template: &str) -> String {
    prompt_template.replace("${output}", "").trim().to_string()
}

async fn post_process_transcription(settings: &AppSettings, transcription: &str) -> Option<String> {
    let pp_start = Instant::now();

    let provider = match settings.active_post_process_provider().cloned() {
        Some(provider) => provider,
        None => {
            info!("Post-process: skipped — no provider selected");
            return None;
        }
    };

    let model = provider.model.clone();

    if model.trim().is_empty() {
        info!(
            "Post-process: skipped — provider '{}' has no model configured",
            provider.id
        );
        return None;
    }

    let selected_prompt_id = match &settings.post_process_selected_prompt_id {
        Some(id) => id.clone(),
        None => {
            info!("Post-process: skipped — no prompt selected");
            return None;
        }
    };

    let prompt = match settings
        .post_process_prompts
        .iter()
        .find(|prompt| prompt.id == selected_prompt_id)
    {
        Some(prompt) => prompt.prompt.clone(),
        None => {
            info!(
                "Post-process: skipped — prompt '{}' not found",
                selected_prompt_id
            );
            return None;
        }
    };

    if prompt.trim().is_empty() {
        info!("Post-process: skipped — selected prompt is empty");
        return None;
    }

    info!(
        "Post-process: starting (provider: {}, model: {}, base_url: {}, structured: {}, disable_thinking: {}, input: {} chars)",
        provider.id,
        model,
        provider.base_url,
        provider.supports_structured_output,
        settings.post_process_disable_thinking,
        transcription.len()
    );

    let api_key = provider.api_key.clone();
    let temperature = Some(settings.post_process_temperature);

    if provider.supports_structured_output {
        info!(
            "Post-process: using structured output mode for provider '{}'",
            provider.id
        );

        let system_prompt = build_system_prompt(&prompt);
        let user_content = transcription.to_string();

        // Handle Apple Intelligence separately since it uses native Swift APIs
        if provider.id == APPLE_INTELLIGENCE_PROVIDER_ID {
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            {
                if !apple_intelligence::check_apple_intelligence_availability() {
                    debug!(
                        "Apple Intelligence selected but not currently available on this device"
                    );
                    return None;
                }

                let token_limit = model.trim().parse::<i32>().unwrap_or(0);
                return match apple_intelligence::process_text_with_system_prompt(
                    &system_prompt,
                    &user_content,
                    token_limit,
                ) {
                    Ok(result) => {
                        if result.trim().is_empty() {
                            debug!("Apple Intelligence returned an empty response");
                            None
                        } else {
                            let result = strip_invisible_chars(&result);
                            debug!(
                                "Apple Intelligence post-processing succeeded. Output length: {} chars",
                                result.len()
                            );
                            Some(result)
                        }
                    }
                    Err(err) => {
                        error!("Apple Intelligence post-processing failed: {}", err);
                        None
                    }
                };
            }

            #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
            {
                debug!("Apple Intelligence provider selected on unsupported platform");
                return None;
            }
        }

        // Define JSON schema for transcription output
        let json_schema = serde_json::json!({
            "type": "object",
            "properties": {
                (TRANSCRIPTION_FIELD): {
                    "type": "string",
                    "description": "The cleaned and processed transcription text"
                }
            },
            "required": [TRANSCRIPTION_FIELD],
            "additionalProperties": false
        });

        match crate::llm_client::send_chat_completion_with_schema(
            &provider,
            api_key.clone(),
            &model,
            user_content,
            Some(system_prompt),
            Some(json_schema),
            settings.post_process_disable_thinking,
            temperature,
        )
        .await
        {
            Ok(Some(content)) => {
                info!(
                    "Post-process: structured output response received ({} chars) in {}ms",
                    content.len(),
                    pp_start.elapsed().as_millis()
                );
                // Parse the JSON response to extract the transcription field
                match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(json) => {
                        if let Some(transcription_value) =
                            json.get(TRANSCRIPTION_FIELD).and_then(|t| t.as_str())
                        {
                            let result =
                                strip_thinking_tags(&strip_invisible_chars(transcription_value));
                            info!(
                                "Post-process: structured output succeeded (provider: {}, output: {} chars, total: {}ms)",
                                provider.id,
                                result.len(),
                                pp_start.elapsed().as_millis()
                            );
                            return Some(result);
                        } else {
                            error!(
                                "Post-process: structured output response missing '{}' field, raw: {}",
                                TRANSCRIPTION_FIELD,
                                &content[..content.len().min(500)]
                            );
                            return Some(strip_thinking_tags(&strip_invisible_chars(&content)));
                        }
                    }
                    Err(e) => {
                        error!(
                            "Post-process: failed to parse structured output JSON: {}. Raw: {}",
                            e,
                            &content[..content.len().min(500)]
                        );
                        return Some(strip_thinking_tags(&strip_invisible_chars(&content)));
                    }
                }
            }
            Ok(None) => {
                error!("Post-process: LLM API returned no content (structured mode)");
                return None;
            }
            Err(e) => {
                warn!(
                    "Post-process: structured output failed for provider '{}' in {}ms: {}. Falling back to legacy mode.",
                    provider.id,
                    pp_start.elapsed().as_millis(),
                    e
                );
                // Fall through to legacy mode below
            }
        }
    }

    // Legacy mode: Replace ${output} variable in the prompt with the actual text
    info!("Post-process: falling back to legacy mode (no structured output)");
    let legacy_start = Instant::now();
    let processed_prompt = prompt.replace("${output}", transcription);
    info!(
        "Post-process: legacy prompt {} chars",
        processed_prompt.len()
    );

    match crate::llm_client::send_chat_completion(
        &provider,
        api_key,
        &model,
        processed_prompt,
        settings.post_process_disable_thinking,
        temperature,
    )
    .await
    {
        Ok(Some(content)) => {
            let content = strip_thinking_tags(&strip_invisible_chars(&content));
            info!(
                "Post-process: legacy mode succeeded (provider: {}, output: {} chars, legacy: {}ms, total: {}ms)",
                provider.id,
                content.len(),
                legacy_start.elapsed().as_millis(),
                pp_start.elapsed().as_millis()
            );
            Some(content)
        }
        Ok(None) => {
            error!("Post-process: LLM API returned no content (legacy mode)");
            None
        }
        Err(e) => {
            error!(
                "Post-process: legacy mode failed (provider: {}, {}ms): {}",
                provider.id,
                pp_start.elapsed().as_millis(),
                e
            );
            None
        }
    }
}

async fn maybe_convert_chinese_variant(
    settings: &AppSettings,
    transcription: &str,
) -> Option<String> {
    // Check if language is set to Simplified or Traditional Chinese
    let is_simplified = settings.selected_language == "zh-Hans";
    let is_traditional = settings.selected_language == "zh-Hant";

    if !is_simplified && !is_traditional {
        debug!("selected_language is not Simplified or Traditional Chinese; skipping translation");
        return None;
    }

    debug!(
        "Starting Chinese translation using OpenCC for language: {}",
        settings.selected_language
    );

    // Use OpenCC to convert based on selected language. Conversions are
    // CHARACTER-level on purpose: the "p" (phrase) variants additionally
    // rewrite regional vocabulary (e.g. 軟體/软件), which changes what the
    // user actually said — too aggressive for a transcription tool.
    let config = if is_simplified {
        // Traditional Chinese -> Simplified Chinese
        BuiltinConfig::Tw2s
    } else {
        // Simplified Chinese -> Traditional Chinese
        BuiltinConfig::S2tw
    };

    match OpenCC::from_config(config) {
        Ok(converter) => {
            let converted = converter.convert(transcription);
            debug!(
                "OpenCC translation completed. Input length: {}, Output length: {}",
                transcription.len(),
                converted.len()
            );
            Some(converted)
        }
        Err(e) => {
            error!(
                "Failed to initialize OpenCC converter: {}. Falling back to original transcription.",
                e
            );
            None
        }
    }
}

impl ShortcutAction for TranscribeAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let start_time = Instant::now();
        debug!("TranscribeAction::start called for binding: {}", binding_id);

        // Load model in the background
        let tm = app.state::<Arc<TranscriptionManager>>();
        tm.initiate_model_load();

        // Emit reset event to clear any previous live transcription session
        let _ = app.emit("live-transcription-reset", ());

        let binding_id = binding_id.to_string();
        change_tray_icon(app, TrayIconState::Recording);
        show_recording_overlay(app);

        let rm = app.state::<Arc<AudioRecordingManager>>();

        // Get the microphone mode to determine audio feedback timing
        let settings = get_settings(app);
        let is_always_on = settings.always_on_microphone;
        debug!("Microphone mode - always_on: {}", is_always_on);

        // Timestamp for this recording — shared with the recorder so chunk files
        // (handy-{ts}-chunk-N.opus) and the glued history file (handy-{ts}.opus)
        // agree, and read again in stop().
        let ts = now_ts();
        RECORDING_TS.store(ts, Ordering::SeqCst);
        // Start a fresh per-recording OpenRouter cost tally.
        crate::managers::openrouter_transcription::reset_session_cost();

        let mut recording_started = false;
        if is_always_on {
            // Always-on mode: Play audio feedback immediately, then apply mute after sound finishes
            debug!("Always-on mode: Playing audio feedback immediately");
            let rm_clone = Arc::clone(&rm);
            let app_clone = app.clone();
            // The blocking helper exits immediately if audio feedback is disabled,
            // so we can always reuse this thread to ensure mute happens right after playback.
            std::thread::spawn(move || {
                play_feedback_sound_blocking(&app_clone, SoundType::Start);
                // Only mute while a recording is actually active — a failed
                // start or a quick tap that already stopped must not leave the
                // system audio muted.
                if rm_clone.is_recording() {
                    rm_clone.apply_mute();
                }
            });

            recording_started = rm.try_start_recording(&binding_id, ts);
            debug!("Recording started: {}", recording_started);
        } else {
            // On-demand mode: Start recording first, then play audio feedback, then apply mute
            // This allows the microphone to be activated before playing the sound
            debug!("On-demand mode: Starting recording first, then audio feedback");
            let recording_start_time = Instant::now();
            if rm.try_start_recording(&binding_id, ts) {
                recording_started = true;
                debug!("Recording started in {:?}", recording_start_time.elapsed());
                // Small delay to ensure microphone stream is active
                let app_clone = app.clone();
                let rm_clone = Arc::clone(&rm);
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    debug!("Handling delayed audio feedback/mute sequence");
                    // Helper handles disabled audio feedback by returning early, so we reuse it
                    // to keep mute sequencing consistent in every mode.
                    play_feedback_sound_blocking(&app_clone, SoundType::Start);
                    // A quick tap can stop the recording before the sound ends;
                    // never mute after the recording is already over.
                    if rm_clone.is_recording() {
                        rm_clone.apply_mute();
                    }
                });
            } else {
                debug!("Failed to start recording");
            }
        }

        if !recording_started {
            // Roll back the optimistic Recording UI (set above, before the
            // start attempt) so a failed start doesn't leave the overlay and
            // tray stuck on Recording with no way to clear them. Also undo any
            // mute the always-on feedback thread may have applied.
            warn!(
                "Recording failed to start for '{}': rolling back UI",
                binding_id
            );
            rm.remove_mute();
            utils::hide_recording_overlay(app);
            change_tray_icon(app, TrayIconState::Idle);
        }

        if recording_started {
            let tm_seg = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
            // Reuse the settings snapshot read above — a second read here could
            // diverge from what the recorder was started with and skew the plan.
            let is_ptt_binding = binding_id == "transcribe_ptt";
            let is_live_mode = if is_ptt_binding {
                settings.transcription_mode_ptt == TranscriptionMode::Live
            } else {
                settings.transcription_mode == TranscriptionMode::Live
            };
            let is_api_model = settings.selected_model == "api-whisper";
            // OpenRouter transcription must never stream audio mid-recording (the
            // user may hit network drops). It uses the on-disk chunked recording
            // for crash safety, but transcribes the WHOLE recording in one request
            // on stop — so it's excluded from live streaming here and marked
            // `deferred` in the chunked session below.
            let is_openrouter = settings.selected_model == "openrouter-transcription";

            let plan_live = is_live_mode && !is_api_model && !is_openrouter;
            let plan_chunked = !plan_live && settings.crash_resilient_recording && !is_api_model;
            *RECORDING_PLAN.lock().unwrap() = Some(RecordingPlan {
                live: plan_live,
                chunked: plan_chunked,
                crash_safe: settings.crash_resilient_recording,
            });

            if plan_live {
                // Live mode: set up segment callback for progressive transcription
                let app_seg = app.clone();
                let chunk_index = Arc::new(AtomicUsize::new(0));
                tm_seg.set_live_transcribing(true);

                let transcribing = Arc::new(AtomicBool::new(false));

                // Store the live text handle and busy flag so stop() can access them
                let live_text = Arc::new(Mutex::new(String::new()));
                *LIVE_TEXT.lock().unwrap() = Some(Arc::clone(&live_text));
                *SEGMENT_BUSY.lock().unwrap() = Some(Arc::clone(&transcribing));

                rm.set_on_segment_callback(move |segment_samples| {
                    // Skip if a previous segment is still being transcribed
                    if transcribing.swap(true, Ordering::Relaxed) {
                        return;
                    }
                    let idx = chunk_index.fetch_add(1, Ordering::Relaxed);
                    let tm_inner = Arc::clone(&tm_seg);
                    let app_inner = app_seg.clone();
                    let busy = Arc::clone(&transcribing);
                    let live_text_inner = Arc::clone(&live_text);
                    std::thread::spawn(move || {
                        // Serialize with other engine users (Translator batch
                        // segments): wait briefly instead of failing fast, so
                        // live preview text is delayed — not dropped — when a
                        // batch segment holds the engine.
                        let result = {
                            let _serial = CHUNK_TRANSCRIBE_LOCK
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            tm_inner.transcribe(segment_samples)
                        };
                        match result {
                            Ok(text) => {
                                if !text.is_empty() {
                                    if let Ok(mut live) = live_text_inner.lock() {
                                        *live = text.clone();
                                    }
                                    let _ = app_inner.emit(
                                        "live-transcription-chunk",
                                        LiveTranscriptionChunk {
                                            index: idx,
                                            text,
                                            is_final: false,
                                        },
                                    );
                                }
                            }
                            Err(e) => {
                                debug!("Live segment transcription failed: {}", e);
                            }
                        }
                        busy.store(false, Ordering::Relaxed);
                    });
                });
            } else if plan_chunked {
                // Chunked (default Post-Recording) mode: transcribe each chunk in
                // the background as it closes, so a long recording is mostly
                // transcribed by the time the user stops. For OpenRouter (deferred)
                // we instead buffer each chunk's PCM and transcribe the whole
                // recording once on stop (no per-chunk network calls).
                tm_seg.set_live_transcribing(true);
                let session = Arc::new(ChunkedSession::new(ts, is_openrouter));
                *CHUNKED_SESSION.lock().unwrap() = Some(Arc::clone(&session));
                let tm_chunk = Arc::clone(&tm_seg);
                rm.set_on_chunk_callback(move |closed: ClosedChunk| {
                    // Reserve the slot before spawning so stop()'s wait can't see
                    // done_count >= closed_count prematurely.
                    if let Ok(mut map) = session.transcripts.lock() {
                        map.insert(closed.index, None);
                    }
                    session.closed_count.fetch_add(1, Ordering::SeqCst);
                    session
                        .total_samples
                        .fetch_add(closed.pcm.len() as u64, Ordering::SeqCst);

                    if session.deferred {
                        // Buffer the chunk PCM; the full recording is transcribed
                        // in one request on stop. No transcription thread here.
                        if let Ok(mut map) = session.pcm.lock() {
                            map.insert(closed.index, closed.pcm);
                        }
                        session.done_count.fetch_add(1, Ordering::SeqCst);
                        return;
                    }

                    let tm_inner = Arc::clone(&tm_chunk);
                    let session_inner = Arc::clone(&session);
                    let idx = closed.index;
                    let pcm = closed.pcm;
                    std::thread::spawn(move || {
                        // Serialize with other chunk threads (see
                        // CHUNK_TRANSCRIBE_LOCK); recover the guard even if a
                        // previous holder panicked.
                        let _serial = CHUNK_TRANSCRIBE_LOCK
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        // Cancelled sessions don't get engine time — a queued
                        // worker from an abandoned take must not delay the
                        // recording the user is making NOW.
                        if session_inner.abandoned.load(Ordering::SeqCst) {
                            session_inner.done_count.fetch_add(1, Ordering::SeqCst);
                            return;
                        }
                        let text = tm_inner.transcribe(pcm).unwrap_or_else(|e| {
                            debug!("Chunk {} transcription failed: {}", idx, e);
                            String::new()
                        });
                        if let Ok(mut map) = session_inner.transcripts.lock() {
                            map.insert(idx, Some(text));
                        }
                        session_inner.done_count.fetch_add(1, Ordering::SeqCst);
                    });
                });
            } else {
                // API mode, or crash-safe recording disabled: single-shot
                // transcription on stop (no per-chunk/live transcription).
                debug!(
                    "Single-shot transcription (live={}, api_model={}, crash_safe={})",
                    is_live_mode, is_api_model, settings.crash_resilient_recording
                );
            }

            // Dynamically register the cancel shortcut in a separate task to avoid deadlock
            shortcut::register_cancel_shortcut(app);
        }

        debug!(
            "TranscribeAction::start completed in {:?}",
            start_time.elapsed()
        );
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        // Unregister the cancel shortcut when transcription stops
        shortcut::unregister_cancel_shortcut(app);

        let stop_time = Instant::now();
        debug!("TranscribeAction::stop called for binding: {}", binding_id);

        // Clear the live segment callback. The chunk callback is cleared later
        // (in the chunked branch) after stop_recording() fires it for the final
        // chunk.
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
        rm.clear_segment_callback();
        let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());

        let ah = app.clone();
        let hm = Arc::clone(&app.state::<Arc<HistoryManager>>());

        change_tray_icon(app, TrayIconState::Transcribing);
        show_transcribing_overlay(app);

        // Unmute early so the stop sound (played inside the async task, AFTER
        // the recorder actually stops) is audible. Playing the beep here used
        // to race the recorder shutdown and get captured into the take.
        rm.remove_mute();

        let binding_id = binding_id.to_string(); // Clone binding_id for the async task
        let post_process = self.post_process;

        // Consume the pipeline plan captured by start() — the recording MUST be
        // finished the way it was set up, regardless of settings changes made
        // mid-recording. The settings-derived values below are only a defensive
        // fallback for a missing plan (should not happen: every successful
        // start() writes one).
        let settings_snapshot = get_settings(app);
        let is_ptt_binding = binding_id == "transcribe_ptt";
        let plan = RECORDING_PLAN.lock().ok().and_then(|mut g| g.take());
        let is_openrouter = settings_snapshot.selected_model == "openrouter-transcription";
        let fallback_live = !is_openrouter
            && if is_ptt_binding {
                settings_snapshot.transcription_mode_ptt == TranscriptionMode::Live
            } else {
                settings_snapshot.transcription_mode == TranscriptionMode::Live
            };
        let is_api_model = settings_snapshot.selected_model == "api-whisper";
        let use_live = plan.map(|p| p.live).unwrap_or(fallback_live);
        let crash_safe = plan
            .map(|p| p.crash_safe)
            .unwrap_or(settings_snapshot.crash_resilient_recording);
        let use_chunked = plan
            .map(|p| p.chunked)
            .unwrap_or(crash_safe && !use_live && !is_api_model);
        debug!(
            "Transcription pipeline: use_live={}, use_chunked={}, is_ptt={}, planned={}",
            use_live,
            use_chunked,
            is_ptt_binding,
            plan.is_some()
        );
        let ts = RECORDING_TS.load(Ordering::SeqCst);
        let chunked_session = CHUNKED_SESSION.lock().ok().and_then(|mut g| g.take());

        // Live-transcribing suppression (blocks Immediately-unload while
        // segments are in flight) is deliberately NOT cleared here: with
        // ModelUnloadTimeout::Immediately, clearing before the final full-audio
        // transcribe lets an in-flight segment's completion unload the model,
        // making the final pass fail ("Model is not loaded") and fall back to
        // stale live text. Every path below clears it after transcription
        // (chunked already did; single-shot/live do now).

        // Grab the busy flag and live text handles (non-blocking) so
        // the async task can wait for in-flight segments off the main thread.
        let busy_flag = SEGMENT_BUSY.lock().ok().and_then(|mut g| g.take());
        let live_text_handle = LIVE_TEXT.lock().ok().and_then(|mut g| g.take());

        // Take ownership of this take's deferred on-finish action NOW, while
        // the coordinator flow is still serialized — a global left armed
        // until the (queued) paste ran could cross take boundaries.
        let post_take_action = crate::anchor::take_post_take_action();

        tauri::async_runtime::spawn(async move {
            let _guard = FinishGuard(ah.clone());
            let binding_id = binding_id.clone(); // Clone for the inner async task
            debug!(
                "Starting async transcription task for binding: {}",
                binding_id
            );

            // ===== Chunked (default Post-Recording) path =====
            if use_chunked {
                // Stop the recorder: finalizes the final chunk (firing its
                // callback) and glues handy-{ts}.opus, then returns.
                let _ = rm.stop_recording(&binding_id);
                rm.clear_on_chunk_callback();
                // Mic is cold now — the stop beep can't leak into the take.
                play_feedback_sound(&ah, SoundType::Stop);

                let mut raw_text = String::new();
                let mut produced_audio = false;
                let mut total_samples: u64 = 0;
                if let Some(session) = chunked_session {
                    let total = session.closed_count.load(Ordering::SeqCst);
                    produced_audio = total > 0;
                    // Wait for every chunk transcription to finish. Each spawned
                    // task always increments done_count (transcribe() is
                    // panic-guarded), so this loop's normal exit is
                    // done_count == total — typically the moment transcription
                    // completes. The generous cap is only a deadlock backstop
                    // (the legacy non-chunked path blocked with no timeout at
                    // all); a final chunk is at most ~11 min of audio, which can
                    // take minutes to transcribe on a slow/CPU engine, so we must
                    // not give up early or the result is lost.
                    let wait_start = Instant::now();
                    while session.done_count.load(Ordering::SeqCst) < total
                        && wait_start.elapsed() < Duration::from_secs(15 * 60)
                    {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    let done = session.done_count.load(Ordering::SeqCst);
                    if done < total {
                        warn!(
                            "Chunk transcription wait hit the {}s backstop ({}/{} chunks done); saving partial result",
                            15 * 60,
                            done,
                            total
                        );
                    }
                    total_samples = session.total_samples.load(Ordering::SeqCst);
                    if session.deferred {
                        // Deferred (OpenRouter): all chunk PCM is now buffered —
                        // concatenate and transcribe the whole recording in ONE
                        // request. This is where the single network call happens.
                        let full = session.assemble_pcm();
                        if !full.is_empty() {
                            // Same serialization as every other engine call
                            // (harmless for the HTTP engine, required if a
                            // local engine ever runs deferred).
                            let _serial = CHUNK_TRANSCRIBE_LOCK
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            raw_text = tm.transcribe(full).unwrap_or_else(|e| {
                                error!("Deferred (OpenRouter) transcription failed: {}", e);
                                String::new()
                            });
                        }
                    } else {
                        raw_text = session.assemble();
                    }
                }

                // Done transcribing — allow the model to unload again.
                tm.set_live_transcribing(false);
                tm.maybe_unload_immediately("chunked recording complete");

                if !produced_audio {
                    debug!("Chunked recording produced no audio");
                    utils::hide_recording_overlay(&ah);
                    change_tray_icon(&ah, TrayIconState::Idle);
                    return;
                }

                let settings = get_settings(&ah);
                let mut text = raw_text.clone();
                let mut post_processed_text: Option<String> = None;
                let mut post_process_prompt: Option<String> = None;

                if !text.is_empty() {
                    let _ = ah.emit(
                        "live-transcription-chunk",
                        LiveTranscriptionChunk {
                            index: 0,
                            text: text.clone(),
                            is_final: true,
                        },
                    );

                    if let Some(converted) = maybe_convert_chinese_variant(&settings, &text).await {
                        text = converted;
                    }
                    if post_process {
                        show_processing_overlay(&ah);
                    }
                    let processed = if post_process {
                        post_process_transcription(&settings, &text).await
                    } else {
                        None
                    };
                    if let Some(processed_text) = processed {
                        post_processed_text = Some(processed_text.clone());
                        text = processed_text;
                        if let Some(prompt_id) = &settings.post_process_selected_prompt_id {
                            if let Some(prompt) = settings
                                .post_process_prompts
                                .iter()
                                .find(|p| &p.id == prompt_id)
                            {
                                post_process_prompt = Some(prompt.prompt.clone());
                            }
                        }
                    } else if text != raw_text {
                        post_processed_text = Some(text.clone());
                    }
                }

                // Save history pointing at the glued opus (kept even if text is
                // empty, so a long but quiet recording isn't lost). Accidental
                // taps — tiny AND textless — aren't worth a history row.
                let worth_saving =
                    total_samples as usize >= MIN_SAMPLES_TO_SAVE || !raw_text.is_empty();
                let hm_clone = Arc::clone(&hm);
                let history_text = raw_text.clone();
                let pp = post_processed_text.clone();
                let prompt = post_process_prompt.clone();
                let cost = crate::managers::openrouter_transcription::take_session_cost();
                let duration = Some(samples_to_seconds(total_samples as usize));
                let model = model_label(&ah);
                if worth_saving {
                    // Verify the glued artifact actually exists before pointing
                    // a history row at it — encode/glue/rename can fail, and a
                    // row referencing a missing file is worse than a text-only
                    // row.
                    let opus_ok = ah
                        .path()
                        .app_data_dir()
                        .map(|d| {
                            d.join("recordings")
                                .join(format!("handy-{}.opus", ts))
                                .exists()
                        })
                        .unwrap_or(false);
                    if !opus_ok {
                        warn!(
                            "Glued opus for ts {} missing — saving history text without audio",
                            ts
                        );
                    }
                    tauri::async_runtime::spawn(async move {
                        save_history(
                            &hm_clone,
                            opus_ok,
                            ts,
                            Vec::new(),
                            history_text,
                            pp,
                            prompt,
                            cost,
                            duration,
                            model,
                        )
                        .await;
                    });
                } else {
                    debug!("Chunked recording too short and empty — skipping history save");
                    // Without a history row, the crash-recovery scan would
                    // resurrect the glued opus on next launch — remove what
                    // this recording just wrote (handy-{ts} files only; the
                    // never-touch-user-files invariant holds).
                    if let Ok(dir) = ah.path().app_data_dir() {
                        let rec = dir.join("recordings");
                        let _ = std::fs::remove_file(rec.join(format!("handy-{}.opus", ts)));
                        let chunk_prefix = format!("handy-{}-chunk-", ts);
                        if let Ok(entries) = std::fs::read_dir(&rec) {
                            for entry in entries.flatten() {
                                if entry
                                    .file_name()
                                    .to_string_lossy()
                                    .starts_with(&chunk_prefix)
                                {
                                    let _ = std::fs::remove_file(entry.path());
                                }
                            }
                        }
                    }
                }

                if !text.is_empty() {
                    let ah_clone = ah.clone();
                    let is_ptt = binding_id == "transcribe_ptt";
                    let paste_text = text.clone();
                    let dispatch_park = text.clone();
                    ah.run_on_main_thread(move || {
                        let park_text = paste_text.clone();
                        match utils::paste(paste_text, ah_clone.clone(), is_ptt) {
                            Ok(()) => {
                                crate::anchor::run_post_take_action(&ah_clone, post_take_action)
                            }
                            Err(e) => report_paste_failure(&ah_clone, &park_text, &e),
                        }
                        utils::hide_recording_overlay(&ah_clone);
                        change_tray_icon(&ah_clone, TrayIconState::Idle);
                    })
                    .unwrap_or_else(|e| {
                        // The paste never ran — park the text so it isn't lost.
                        report_paste_failure(
                            &ah,
                            &dispatch_park,
                            &format!("main-thread dispatch failed: {e:?}"),
                        );
                        utils::hide_recording_overlay(&ah);
                        change_tray_icon(&ah, TrayIconState::Idle);
                    });
                } else {
                    utils::hide_recording_overlay(&ah);
                    change_tray_icon(&ah, TrayIconState::Idle);
                }
                return;
            }

            // ===== Live / API / legacy single-shot path =====
            // Stop the recorder FIRST so the mic goes cold at key release.
            // (This used to happen after the segment wait below, which kept the
            // mic recording through the wait and pasted the stop beep plus any
            // post-release speech.)
            let stop_recording_time = Instant::now();
            let samples_taken = rm.stop_recording(&binding_id);
            // Mic is cold now — the stop beep can't leak into the take.
            play_feedback_sound(&ah, SoundType::Stop);

            // Wait for any in-flight segment transcription to finish (off main
            // thread). This must be generous: transcribe() fails FAST (doesn't
            // queue) while the engine is busy, so giving up early would make the
            // final full-audio pass error and fall back to stale live text —
            // exactly the tail loss we're preventing. A late live segment
            // re-transcribes the whole recording, so it can legitimately take
            // minutes; the cap is only a deadlock backstop (chunked uses 15 min).
            if let Some(ref busy) = busy_flag {
                let wait_start = Instant::now();
                while busy.load(Ordering::Relaxed)
                    && wait_start.elapsed() < Duration::from_secs(15 * 60)
                {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                if busy.load(Ordering::Relaxed) {
                    warn!(
                        "In-flight live segment still busy after {:?}; final transcription may fall back to live text",
                        wait_start.elapsed()
                    );
                }
            }

            // Grab live text if in Live mode
            let live_transcription = if use_live {
                live_text_handle.and_then(|arc| {
                    let text = arc.lock().ok()?.clone();
                    if text.is_empty() { None } else { Some(text) }
                })
            } else {
                None
            };

            if let Some(samples) = samples_taken {
                debug!(
                    "Recording stopped and samples retrieved in {:?}, sample count: {}",
                    stop_recording_time.elapsed(),
                    samples.len()
                );
                // crash_safe history rows point at handy-{ts}.opus — verify the
                // artifact exists (finalize/glue can fail) and fall back to the
                // in-memory samples (WAV) when it doesn't.
                let crash_safe = crash_safe
                    && ah
                        .path()
                        .app_data_dir()
                        .map(|d| {
                            d.join("recordings")
                                .join(format!("handy-{}.opus", ts))
                                .exists()
                        })
                        .unwrap_or(false);

                let transcription_time = Instant::now();
                let samples_clone = samples.clone(); // Clone for history saving
                // The accidental-tap guard must ignore the trailing zero-pad
                // that stop_recording appends to sub-second recordings (real
                // audio is never exactly 0.0), or every tap passes the check.
                let effective_len = samples_clone.len()
                    - samples_clone
                        .iter()
                        .rev()
                        .take_while(|s| **s == 0.0)
                        .count();

                // Always transcribe the COMPLETE audio for the final text. The
                // accumulated live text is only a progressive preview — it can
                // miss the tail (audio after the last ≤3 s emit window, or a
                // whole final segment when the busy-skip dropped the last emit).
                // Live emits already re-transcribe the full audio each time, so
                // this final pass costs the same as one more live segment. The
                // live text remains a fallback if the final pass fails.
                //
                // Serialize with other engine users (chunk workers, Translator
                // batch segments): transcribe() fails fast when the engine is
                // busy, and this final pass must WAIT (≤ one segment), not
                // fail into the stale-live-text fallback. The guard is scoped
                // to the transcribe call only — never held across post-
                // processing or pasting.
                let transcription_result = {
                    let _serial = CHUNK_TRANSCRIBE_LOCK
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    match tm.transcribe(samples) {
                        Ok(text) => Ok(text),
                        Err(e) => {
                            if let Some(live) = live_transcription {
                                warn!(
                                    "Final transcription failed ({}); falling back to live text ({} chars)",
                                    e,
                                    live.len()
                                );
                                Ok(live)
                            } else {
                                Err(e)
                            }
                        }
                    }
                };
                // Final pass done — allow Immediately-unload again (deferred
                // from stop() start; see comment there).
                tm.set_live_transcribing(false);
                tm.maybe_unload_immediately("single-shot transcription complete");

                match transcription_result {
                    Ok(transcription) => {
                        debug!(
                            "Transcription completed in {:?}: '{}'",
                            transcription_time.elapsed(),
                            transcription
                        );
                        if !transcription.is_empty() {
                            // Emit final live transcription chunk
                            let _ = ah.emit(
                                "live-transcription-chunk",
                                LiveTranscriptionChunk {
                                    index: 0,
                                    text: transcription.clone(),
                                    is_final: true,
                                },
                            );

                            let settings = get_settings(&ah);
                            let mut final_text = transcription.clone();
                            let mut post_processed_text: Option<String> = None;
                            let mut post_process_prompt: Option<String> = None;

                            // First, check if Chinese variant conversion is needed
                            if let Some(converted_text) =
                                maybe_convert_chinese_variant(&settings, &transcription).await
                            {
                                final_text = converted_text;
                            }

                            // Then apply LLM post-processing if this is the post-process hotkey
                            // Uses final_text which may already have Chinese conversion applied
                            if post_process {
                                info!(
                                    "Post-process: hotkey active, starting post-processing pipeline"
                                );
                                show_processing_overlay(&ah);
                            }
                            let processed = if post_process {
                                post_process_transcription(&settings, &final_text).await
                            } else {
                                None
                            };
                            if let Some(ref processed_text) = processed {
                                info!(
                                    "Post-process: completed, output {} chars (was {} chars)",
                                    processed_text.len(),
                                    final_text.len()
                                );
                            } else if post_process {
                                info!("Post-process: returned None, using original transcription");
                            }
                            if let Some(processed_text) = processed {
                                post_processed_text = Some(processed_text.clone());
                                final_text = processed_text;

                                // Get the prompt that was used
                                if let Some(prompt_id) = &settings.post_process_selected_prompt_id {
                                    if let Some(prompt) = settings
                                        .post_process_prompts
                                        .iter()
                                        .find(|p| &p.id == prompt_id)
                                    {
                                        post_process_prompt = Some(prompt.prompt.clone());
                                    }
                                }
                            } else if final_text != transcription {
                                // Chinese conversion was applied but no LLM post-processing
                                post_processed_text = Some(final_text.clone());
                            }

                            // Save to history (glued opus when crash-safe, else WAV).
                            let hm_clone = Arc::clone(&hm);
                            let transcription_for_history = transcription.clone();
                            let cost =
                                crate::managers::openrouter_transcription::take_session_cost();
                            let duration = Some(samples_to_seconds(samples_clone.len()));
                            let model = model_label(&ah);
                            tauri::async_runtime::spawn(async move {
                                save_history(
                                    &hm_clone,
                                    crash_safe,
                                    ts,
                                    samples_clone,
                                    transcription_for_history,
                                    post_processed_text,
                                    post_process_prompt,
                                    cost,
                                    duration,
                                    model,
                                )
                                .await;
                            });

                            // Paste the final text (either processed or original)
                            let ah_clone = ah.clone();
                            let paste_time = Instant::now();
                            let is_ptt = binding_id == "transcribe_ptt";
                            let dispatch_park = final_text.clone();
                            ah.run_on_main_thread(move || {
                                let park_text = final_text.clone();
                                match utils::paste(final_text, ah_clone.clone(), is_ptt) {
                                    Ok(()) => {
                                        debug!(
                                            "Text pasted successfully in {:?}",
                                            paste_time.elapsed()
                                        );
                                        crate::anchor::run_post_take_action(
                                            &ah_clone,
                                            post_take_action,
                                        );
                                    }
                                    Err(e) => report_paste_failure(&ah_clone, &park_text, &e),
                                }
                                // Hide the overlay after transcription is complete
                                utils::hide_recording_overlay(&ah_clone);
                                change_tray_icon(&ah_clone, TrayIconState::Idle);
                            })
                            .unwrap_or_else(|e| {
                                // The paste never ran — park the text so it
                                // isn't lost.
                                report_paste_failure(
                                    &ah,
                                    &dispatch_park,
                                    &format!("main-thread dispatch failed: {e:?}"),
                                );
                                utils::hide_recording_overlay(&ah);
                                change_tray_icon(&ah, TrayIconState::Idle);
                            });
                        } else {
                            // Transcription returned empty (hallucinations filtered, filler-only, etc.)
                            // Still save the audio so long recordings aren't silently lost.
                            if effective_len >= MIN_SAMPLES_TO_SAVE {
                                let hm_clone = Arc::clone(&hm);
                                let cost =
                                    crate::managers::openrouter_transcription::take_session_cost();
                                let duration = Some(samples_to_seconds(samples_clone.len()));
                                let model = model_label(&ah);
                                tauri::async_runtime::spawn(async move {
                                    save_history(
                                        &hm_clone,
                                        crash_safe,
                                        ts,
                                        samples_clone,
                                        String::new(),
                                        None,
                                        None,
                                        cost,
                                        duration,
                                        model,
                                    )
                                    .await;
                                });
                            }
                            utils::hide_recording_overlay(&ah);
                            change_tray_icon(&ah, TrayIconState::Idle);
                        }
                    }
                    Err(err) => {
                        error!("Transcription failed: {}", err);
                        // Save the audio so recordings aren't lost on engine failure
                        // (e.g. Moonshine 64s limit, Whisper OOM, etc.)
                        if effective_len >= MIN_SAMPLES_TO_SAVE {
                            let hm_clone = Arc::clone(&hm);
                            let cost =
                                crate::managers::openrouter_transcription::take_session_cost();
                            let duration = Some(samples_to_seconds(samples_clone.len()));
                            let model = model_label(&ah);
                            tauri::async_runtime::spawn(async move {
                                save_history(
                                    &hm_clone,
                                    crash_safe,
                                    ts,
                                    samples_clone,
                                    String::new(),
                                    None,
                                    None,
                                    cost,
                                    duration,
                                    model,
                                )
                                .await;
                            });
                        }
                        utils::hide_recording_overlay(&ah);
                        change_tray_icon(&ah, TrayIconState::Idle);
                    }
                }
            } else {
                debug!("No samples retrieved from recording stop");
                tm.set_live_transcribing(false);
                utils::hide_recording_overlay(&ah);
                change_tray_icon(&ah, TrayIconState::Idle);
            }
        });

        debug!(
            "TranscribeAction::stop completed in {:?}",
            stop_time.elapsed()
        );
    }
}

// Cancel Action
struct CancelAction;

impl ShortcutAction for CancelAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        utils::cancel_current_operation(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        // Nothing to do on stop for cancel
    }
}

// Type Text Action: toggles the Keyboard Typer session
struct TypeTextAction;

impl ShortcutAction for TypeTextAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        crate::typing::toggle_from_shortcut(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        // No-op: typing is started/cancelled on press only
    }
}

// Test Action
struct TestAction;

impl ShortcutAction for TestAction {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Started - {} (App: {})", // Changed "Pressed" to "Started" for consistency
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Stopped - {} (App: {})", // Changed "Released" to "Stopped" for consistency
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }
}

// Static Action Map
/// Anchor & Deliver: capture the focused field as the delivery target.
struct AnchorSetAction;
impl ShortcutAction for AnchorSetAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        match crate::anchor::set_anchor(app) {
            Ok(status) => info!("Anchor set: {} ({})", status.app, status.control_class),
            Err(e) => warn!("Set anchor failed: {}", e),
        }
    }
    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

/// Anchor & Deliver: pure navigation to the anchored field (never pastes,
/// never consumes the anchor).
struct AnchorJumpAction;
impl ShortcutAction for AnchorJumpAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        if let Err(e) = crate::anchor::jump(app, crate::anchor::HOT) {
            warn!("Jump to anchor failed: {}", e);
        }
    }
    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

/// Jumper static slot: memorize the focused field into slot `.0`.
struct SetSlotAction(usize);
impl ShortcutAction for SetSlotAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        match crate::anchor::set_slot(app, self.0) {
            Ok(status) => info!("Jump slot {} set: {}", self.0, status.app),
            Err(e) => warn!("Set jump slot {} failed: {}", self.0, e),
        }
    }
    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

/// Jumper static slot: navigate to slot `.0` (never pastes).
struct JumpSlotAction(usize);
impl ShortcutAction for JumpSlotAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        if let Err(e) = crate::anchor::jump(app, self.0) {
            warn!("Jump to slot {} failed: {}", self.0, e);
        }
    }
    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {}
}

pub static ACTION_MAP: Lazy<HashMap<String, Arc<dyn ShortcutAction>>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert(
        "transcribe".to_string(),
        Arc::new(TranscribeAction {
            post_process: false,
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "transcribe_ptt".to_string(),
        Arc::new(TranscribeAction {
            post_process: false,
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "transcribe_with_post_process".to_string(),
        Arc::new(TranscribeAction { post_process: true }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "transcribe_and_submit".to_string(),
        Arc::new(TranscribeAction {
            post_process: false,
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "cancel".to_string(),
        Arc::new(CancelAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "type_text".to_string(),
        Arc::new(TypeTextAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "test".to_string(),
        Arc::new(TestAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "anchor_set".to_string(),
        Arc::new(AnchorSetAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "anchor_jump".to_string(),
        Arc::new(AnchorJumpAction) as Arc<dyn ShortcutAction>,
    );
    for i in 1..=4usize {
        map.insert(
            format!("jump_set_slot_{}", i),
            Arc::new(SetSlotAction(i)) as Arc<dyn ShortcutAction>,
        );
        map.insert(
            format!("jump_slot_{}", i),
            Arc::new(JumpSlotAction(i)) as Arc<dyn ShortcutAction>,
        );
    }
    map
});
