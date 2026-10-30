use crate::audio_toolkit::{apply_custom_words, filter_transcription_output, pad_trailing_silence};
use crate::managers::model::{EngineType, ModelManager};
use crate::settings::{
    AppSettings, ModelUnloadTimeout, get_settings, normalize_language_for_engine,
};
use anyhow::Result;
use log::{debug, error, info, warn};
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
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
        whisper::{WhisperEngine, WhisperInferenceParams, WhisperModelParams},
    },
};

/// 1 s of digital silence @ 16 kHz appended before decoding for engines that
/// need trailing acoustic context to emit their final tokens (Parakeet,
/// Moonshine, SenseVoice — see `pad_trailing_silence`). Without it, the tail
/// segment cut the instant the user stops recording loses its last word(s).
/// Whisper-family engines are excluded: they decode the whole window fine and
/// trailing silence only invites end-of-audio hallucinations.
const TRAILING_SILENCE_PAD_SAMPLES: usize = 16_000;

/// Serializes this app's only two entry points into ggml's Vulkan backend:
/// `commands::models::list_gpu_devices` (enumeration) and the Whisper GPU
/// model-load path below (adversarial review finding 4, T-212 follow-up).
///
/// `ggml-vulkan.cpp` guards its one-time backend init with an
/// *unsynchronized* global flag that it marks complete before the device
/// vectors it populates are fully written, so an enumeration call
/// overlapping an in-progress load can observe partial state or otherwise
/// data-race. `whisper-rs-local/src/vulkan.rs` has its own internal lock
/// (`whisper_rs::vulkan::VULKAN_LOCK`) guarding its `list_devices()` FFI
/// section, but `whisper-rs` is not a direct dependency of this crate (only
/// reachable indirectly via the `transcribe-rs` re-export — see
/// `Cargo.toml`), so that lock cannot be shared across the crate boundary
/// without adding a new direct dependency edge, which is out of scope for
/// this pass (see `tickets/T-212-gpu-selection.md`). This mirrors the same
/// guarantee at the one layer this crate owns: nothing in this app touches
/// the Vulkan backend except through the two call sites above, so
/// serializing them here closes the practical race even though the two
/// locks are technically separate objects.
static VULKAN_OP_LOCK: Mutex<()> = Mutex::new(());

/* ── Progress of the running engine call (the overlay's percentage) ── */

/// The local engine call running right now. Only one runs at a time per engine
/// slot: the engine is taken out of it for the duration of the call.
struct RunningJob {
    /// When the engine began working; None while a Whisper call still waits for
    /// the GPU lock (a model load can hold it), which counts as no progress.
    started: Option<std::time::Instant>,
    /// Expected wall time in seconds, from this model's recent timings; None
    /// until it has a few since the app started.
    expected: Option<f32>,
    /// Whisper's own percentage (0-100); -1 until it reports. Other engines never do.
    reported: Arc<AtomicI32>,
}

static RUNNING_JOB: Mutex<Option<RunningJob>> = Mutex::new(None);

/// This machine's recent engine calls per model id, oldest first: (seconds of
/// audio, seconds the call took).
static MODEL_TIMINGS: Mutex<BTreeMap<String, VecDeque<(f32, f32)>>> = Mutex::new(BTreeMap::new());

/// Clips shorter than this are dominated by fixed overhead and would skew the estimate.
const MIN_TIMING_SAMPLE_SECS: f32 = 2.0;
/// Timings kept per model.
const TIMINGS_KEPT: usize = 64;
/// Weight of each older timing relative to the next newer one.
const TIMING_RECENCY: f64 = 0.97;
/// How far apart in length (natural log) two calls can be and still count as similar.
const TIMING_LENGTH_SIGMA: f64 = 0.5;
/// Time per audio second grows with length: ∝ length^this.
const TIMING_LENGTH_GROWTH: f64 = 0.3;
/// The shown percentage follows the estimate up to here, then slows down.
const PROGRESS_LINEAR_UNTIL: f32 = 0.7;
/// The STT server's requests with speakers are timed apart from plain ones.
const SPEAKERS_KEY_SUFFIX: &str = "+speakers";
/// Until a speakers request has been timed: a plain request's estimate times
/// this (telling the speakers apart is extra work on the server).
const SPEAKERS_FALLBACK_FACTOR: f32 = 1.5;

/// Expected wall time, in seconds, of transcribing `audio_secs` of audio with
/// `model_id`. Time per audio second is not constant - on a slow PC Parakeet took
/// 0.14 s for short clips but 0.21 s at 20-35 s - so one average misjudged long
/// takes. Recent calls of similar length count most; each is scaled to the asked
/// length first, so an unusual length still gets an answer. None until the model
/// has a timing.
pub fn expected_transcription_secs(model_id: &str, audio_secs: f32) -> Option<f32> {
    let timed = MODEL_TIMINGS
        .lock()
        .ok()?
        .get(model_id)
        .is_some_and(|c| !c.is_empty());
    if !timed {
        // A speakers request not timed yet: the same server's plain requests
        // (the take's own pieces are among them) give a first estimate.
        let plain = model_id.strip_suffix(SPEAKERS_KEY_SUFFIX)?;
        return expected_transcription_secs(plain, audio_secs)
            .map(|secs| secs * SPEAKERS_FALLBACK_FACTOR);
    }
    let timings = MODEL_TIMINGS.lock().ok()?;
    let calls = timings.get(model_id).filter(|c| !c.is_empty())?;
    let newest = calls.len() - 1;
    let (mut weight, mut log_ratio) = (0.0f64, 0.0f64);
    for (i, &(secs, took)) in calls.iter().enumerate() {
        let apart = (audio_secs as f64 / secs as f64).ln();
        let similar = (-(apart * apart) / (2.0 * TIMING_LENGTH_SIGMA * TIMING_LENGTH_SIGMA))
            .exp()
            .max(1e-4);
        let w = TIMING_RECENCY.powi((newest - i) as i32) * similar;
        weight += w;
        log_ratio += w * ((took as f64 / secs as f64).ln() + TIMING_LENGTH_GROWTH * apart);
    }
    Some(((log_ratio / weight).exp() * audio_secs as f64) as f32)
}

/// Share of a call to show as done when `r` = time so far / expected time: true
/// to the estimate up to 70%, then slowing down smoothly towards 100% - so a call
/// that runs longer than expected keeps creeping on instead of stopping at 99%.
pub fn eased_progress(r: f32) -> f32 {
    let k = PROGRESS_LINEAR_UNTIL;
    if r <= k {
        r.max(0.0)
    } else {
        1.0 - (1.0 - k) * (-(r - k) / (1.0 - k)).exp()
    }
}

/// Work left in the running local engine call, in expected seconds; None when
/// nothing is running or there is no basis yet. Whisper's own percentage, once
/// it reports one, is a floor under the time-based figure.
pub fn running_transcription_remaining() -> Option<f32> {
    let job = RUNNING_JOB.lock().ok()?;
    let job = job.as_ref()?;
    let reported = job.reported.load(Ordering::Relaxed);
    let reported = (reported >= 0).then(|| reported as f32 / 100.0);
    match job.expected {
        Some(expected) => {
            let elapsed = job.started.map_or(0.0, |s| s.elapsed().as_secs_f32());
            let done = eased_progress(elapsed / expected.max(0.001));
            Some(expected * (1.0 - done.max(reported.unwrap_or(0.0)).min(1.0)))
        }
        // No timings yet: Whisper's figure alone, the whole call counting as 1.
        None => reported.map(|r| 1.0 - r.min(1.0)),
    }
}

/// Publishes an engine call for the overlay's percentage; the returned guard
/// clears it however the call ends. `started` is None while the call still
/// waits for the GPU (it counts as no progress).
fn begin_running_job(
    started: Option<std::time::Instant>,
    expected: Option<f32>,
    reported: Arc<AtomicI32>,
) -> RunningJobGuard {
    if let Ok(mut job) = RUNNING_JOB.lock() {
        *job = Some(RunningJob {
            started,
            expected,
            reported,
        });
    }
    RunningJobGuard
}

/// An online engine's request (API, OpenRouter, STT server) on the dictation
/// path: published as the running job (so the overlay's percentage has an
/// estimate from this engine's recent requests) and timed for the next ones.
/// `speed_key` names the engine and mode (the STT server with speakers is
/// slower than without).
fn timed_online_request<T>(
    speed_key: &str,
    audio_secs: f32,
    request: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let started = std::time::Instant::now();
    let job_guard = begin_running_job(
        Some(started),
        expected_transcription_secs(speed_key, audio_secs),
        Arc::new(AtomicI32::new(-1)),
    );
    let result = request();
    drop(job_guard);
    if result.is_ok() {
        record_model_timing(
            speed_key.to_string(),
            audio_secs,
            started.elapsed().as_secs_f32(),
        );
    }
    result
}

/// Keeps a successful call's timing for the model's estimates; clips too short
/// to say anything about speed are skipped.
fn record_model_timing(speed_key: String, audio_secs: f32, took: f32) {
    if audio_secs < MIN_TIMING_SAMPLE_SECS || speed_key.is_empty() {
        return;
    }
    // Held from the change to the write: an older snapshot never overwrites a newer one.
    let _write = TIMINGS_WRITE.lock().unwrap_or_else(|p| p.into_inner());
    let snapshot = match MODEL_TIMINGS.lock() {
        Ok(mut timings) => {
            let calls = timings.entry(speed_key).or_default();
            if calls.len() == TIMINGS_KEPT {
                calls.pop_front();
            }
            calls.push_back((audio_secs, took));
            timings.clone()
        }
        Err(_) => return,
    };
    save_model_timings(&snapshot);
}

/// Where the timings are kept between runs (set once at startup). Before 2.5.1
/// they lived in memory only: after every restart - an update is one - the
/// overlay had no estimate, so no percentage and no progress line, until the
/// model had been timed again.
static TIMINGS_FILE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
/// Serializes the file writes (engine calls can finish on different threads);
/// taken before MODEL_TIMINGS, never inside it.
static TIMINGS_WRITE: Mutex<()> = Mutex::new(());

type TimingsMap = BTreeMap<String, VecDeque<(f32, f32)>>;

/// Reads the timings saved by an earlier run (missing or unreadable = none).
fn load_model_timings(path: std::path::PathBuf) {
    if let Some(saved) = std::fs::read(&path)
        .ok()
        .and_then(|bytes| parse_timings(&bytes))
    {
        if let Ok(mut timings) = MODEL_TIMINGS.lock() {
            for (key, calls) in saved {
                timings.entry(key).or_insert(calls);
            }
        }
    }
    let _ = TIMINGS_FILE.set(path);
}

/// The saved file's timings: clips too short to count and anything out of
/// range dropped, at most TIMINGS_KEPT per model (the newest).
fn parse_timings(bytes: &[u8]) -> Option<TimingsMap> {
    let raw: BTreeMap<String, Vec<(f32, f32)>> = serde_json::from_slice(bytes).ok()?;
    Some(
        raw.into_iter()
            .map(|(key, calls)| {
                let mut calls: VecDeque<_> = calls
                    .into_iter()
                    .filter(|&(secs, took)| {
                        secs.is_finite()
                            && took.is_finite()
                            && secs >= MIN_TIMING_SAMPLE_SECS
                            && took > 0.0
                    })
                    .collect();
                while calls.len() > TIMINGS_KEPT {
                    calls.pop_front();
                }
                (key, calls)
            })
            .filter(|(key, calls)| !key.is_empty() && !calls.is_empty())
            .collect(),
    )
}

/// Called with TIMINGS_WRITE held.
fn save_model_timings(timings: &TimingsMap) {
    let Some(path) = TIMINGS_FILE.get() else {
        return;
    };
    let Ok(json) = serde_json::to_vec(timings) else {
        return;
    };
    // Whole file or nothing: written aside, then put in place.
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        if let Err(e) = std::fs::rename(&tmp, path) {
            debug!("Could not keep the transcription timings: {e}");
        }
    }
}

/* ── FLM used beside the engine slots (Files, watched folders) ── */

/// Direct FLM calls in flight (Files / watched folders using FLM while dictation
/// runs another model). Taken under `load_flight` together with the check that
/// the server is alive, so nothing that also holds `load_flight` (an idle stop,
/// an unload, a reload) can take the server away while a call is in flight.
#[cfg(not(target_os = "macos"))]
static FLM_IN_FLIGHT: AtomicU64 = AtomicU64::new(0);
/// An unload was refused because FLM was busy; with "Immediately" the idle
/// watcher does it once FLM is free.
#[cfg(not(target_os = "macos"))]
static FLM_UNLOAD_DEFERRED: AtomicBool = AtomicBool::new(false);
/// When FLM last finished a call or started (ms since the Unix epoch).
#[cfg(not(target_os = "macos"))]
static FLM_LAST_USED_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(not(target_os = "macos"))]
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// FLM was just used (or started): its idle time counts from now.
#[cfg(not(target_os = "macos"))]
fn flm_note_used() {
    FLM_LAST_USED_MS.store(now_ms(), Ordering::Relaxed);
}

/// One direct FLM call in flight; ends (and counts as use) when dropped.
#[cfg(not(target_os = "macos"))]
struct FlmLease;

#[cfg(not(target_os = "macos"))]
impl Drop for FlmLease {
    fn drop(&mut self) {
        flm_note_used();
        FLM_IN_FLIGHT.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Clears the running job however the engine call ends, including a panic.
struct RunningJobGuard;

impl Drop for RunningJobGuard {
    fn drop(&mut self) {
        if let Ok(mut job) = RUNNING_JOB.lock() {
            *job = None;
        }
    }
}

/// Run `f` while holding [`VULKAN_OP_LOCK`]. `commands::models::list_gpu_devices`
/// wraps its enumeration call in this so it can never overlap the GPU
/// Whisper model-load path below.
/// One GPU adapter, described without reference to the engine crate.
///
/// The engine type is deliberately NOT re-exported: `commands/models.rs` used to
/// call `transcribe_rs` directly, which meant the CI mock - which removes that
/// dependency entirely to avoid compiling whisper and Vulkan - left an unresolved
/// path in a module it does not replace. Keeping the engine behind this boundary is
/// what makes the mock swap actually compile.
#[derive(Debug, Clone)]
pub struct GpuDevice {
    pub index: i32,
    pub name: String,
    pub vram_total_mb: u64,
}

/// Enumerate the GPU adapters the Whisper Vulkan backend can see.
pub fn list_gpu_devices() -> Vec<GpuDevice> {
    with_vulkan_op_lock(|| {
        transcribe_rs::engines::whisper::list_gpu_devices()
            .into_iter()
            .map(|d| GpuDevice {
                index: d.index,
                name: d.name,
                vram_total_mb: d.vram_total_mb,
            })
            .collect()
    })
}

pub fn with_vulkan_op_lock<R>(f: impl FnOnce() -> R) -> R {
    let _guard = VULKAN_OP_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    f()
}

/// Resolve `AppSettings::transcribe_gpu_device` (T-212) into whisper-rs load
/// parameters. Sentinel encoding, kept in a single `i32` field per the
/// Settings Pattern rather than a parallel accelerator enum:
///   * `-1` (default) = Auto — whisper.cpp's own default (GPU on, device 0),
///     i.e. identical behavior to before this setting existed.
///   * `-2` = force CPU (`use_gpu: false`).
///   * `>= 0` = explicit Vulkan device index from `list_gpu_devices()`.
/// Any other negative value is treated as Auto — defensive against a
/// corrupt or future settings file rather than a hard error.
fn whisper_gpu_params_for_setting(device_setting: i32) -> WhisperModelParams {
    match device_setting {
        -2 => WhisperModelParams {
            use_gpu: false,
            gpu_device: 0,
        },
        -1 => WhisperModelParams::default(),
        n if n >= 0 => WhisperModelParams {
            use_gpu: true,
            gpu_device: n,
        },
        other => {
            warn!(
                "Unrecognized transcribe_gpu_device value {} — defaulting to Auto",
                other
            );
            WhisperModelParams::default()
        }
    }
}

/// Given the outcome of a Whisper GPU load attempt, decide whether a
/// safety-net retry with default (Auto) GPU parameters is appropriate
/// (adversarial review finding 6, T-212 follow-up).
///
/// Only an explicit Vulkan device index (`>= 0`) gets the retry: that
/// setting means "use GPU, specifically this adapter", and if the adapter
/// failed to initialize, falling back to Auto (still GPU-on, just letting
/// whisper.cpp pick) is a reasonable, non-surprising degradation.
///
/// A force-CPU setting (`-2`) failing must NOT retry with GPU-on
/// parameters — `WhisperModelParams::default()` has `use_gpu: true`, so a
/// blanket retry-on-any-error would silently violate the "CPU Only"
/// contract the user explicitly chose. Auto (`-1`) itself failing also has
/// no softer GPU fallback to retry with (it's already the default), so it
/// isn't retried either — both `-2` and `-1` failures surface directly.
fn should_retry_with_default_gpu_params(effective_device_setting: i32) -> bool {
    effective_device_setting >= 0
}

/// Resolve the requested `transcribe_gpu_device` setting against the
/// currently-visible Vulkan adapters (adversarial review finding 5, T-212
/// follow-up). An explicit device index (`>= 0`) that isn't present in
/// `available_indices` — a disappeared adapter, a stale index left over
/// from an older settings file, a driver that reordered devices, etc. — is
/// NOT attempted as-is: whisper.cpp can fail to attach a GPU backend for an
/// unknown index, silently add the CPU backend anyway, and report success,
/// so an Err-only check at load time would never catch it. This validates
/// BEFORE the load and degrades to Auto (`-1`) instead. Auto and force-CPU
/// (`-2`) pass through unchanged — neither names a specific adapter, so
/// there's nothing to validate against the adapter list.
fn resolve_effective_gpu_setting(gpu_setting: i32, available_indices: &[i32]) -> i32 {
    if gpu_setting >= 0 && !available_indices.contains(&gpu_setting) {
        -1
    } else {
        gpu_setting
    }
}

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
    SttServer,
}

thread_local! {
    /// Set by `stt_server_take_expecting` around its `transcribe_inner` call:
    /// `Some(None)` = this call is a dictation take that wants its speaker
    /// text; the STT branch fills it in. Same thread, so no other caller
    /// (watched folders, files) ever sees it.
    static STT_TAKE_SPEAKERS: std::cell::RefCell<Option<Option<String>>> =
        const { std::cell::RefCell::new(None) };
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

// NOTE: Clone is implemented MANUALLY below (not derived) so `owns_watcher` can
// be forced false on clones — see that field's doc + the `impl Clone`. T-307.
pub struct TranscriptionManager {
    engine: Arc<Mutex<EngineState>>,
    /// Dedicated SECOND engine slot for the Translator folder-batch worker,
    /// used ONLY when its model differs from the dictation model AND is a local
    /// engine — so the two models stay resident in PARALLEL (e.g. dictation on
    /// FLM/NPU in `engine`, Translator on a Whisper/iGPU model here) instead of
    /// thrashing one shared slot. External engines (FLM/API/OpenRouter) never
    /// use this slot: one NPU can't host two FLM contexts, and HTTP engines are
    /// stateless. Its own idle-unload is `translator_model_unload_timeout`.
    translator_engine: Arc<Mutex<EngineState>>,
    model_manager: Arc<ModelManager>,
    app_handle: AppHandle,
    last_activity: Arc<AtomicU64>,
    /// Last-use timestamp for the Translator slot's idle watcher.
    translator_last_activity: Arc<AtomicU64>,
    shutdown_signal: Arc<AtomicBool>,
    watcher_handle: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
    /// True ONLY for the original manager built by `new()` (the one wrapped in
    /// the Tauri-state `Arc`); forced false on every `clone()`. Guards `Drop`
    /// (T-307): a transient by-value clone — the idle watcher's own captured
    /// copy, or the throwaway clone `initiate_model_load` moves into its
    /// background load thread — must NEVER signal shutdown or take/join the
    /// SHARED watcher. Before this guard, the first background load's clone
    /// dropping killed the sole idle-unload watcher for the rest of the session
    /// (idle model-unload silently stopped) and could cross-thread-deadlock or
    /// self-join on `watcher_handle`.
    owns_watcher: bool,
    is_loading: Arc<Mutex<bool>>,
    loading_condvar: Arc<Condvar>,
    /// Single-flight guard held across the WHOLE of `load_model`, so every
    /// loader — recording start (via `initiate_model_load`), external-model
    /// selection warm-up (commands/models.rs), and the Translator's override
    /// loads (managers/translator.rs) — serializes. Without it two concurrent
    /// loads could both reach the FLM arm and spawn a second `flm serve` on the
    /// same port (spurious failure / mismatch). Distinct from `is_loading`,
    /// which only stops `initiate_model_load` from spawning a second BACKGROUND
    /// load; direct callers (Translator, selection) bypass that flag.
    load_flight: Arc<Mutex<()>>,
    /// Monotonic counter bumped on every EXTERNAL-model selection. A background
    /// warm-up thread captures its value and, after winning `load_flight`,
    /// skips the (re)load if a newer selection has since bumped it — so rapid
    /// re-selections are latest-intent-wins and a freshly-loaded FLM isn't
    /// pointlessly restarted by a stale queued request.
    external_select_gen: Arc<AtomicU64>,
    /// Flag to prevent model unload during live/progressive transcription.
    is_live_transcribing: Arc<AtomicBool>,
    /// Same protection for the Translator's folder-batch jobs — a separate
    /// flag so the batch worker never fights the live pipeline's writes.
    is_batch_transcribing: Arc<AtomicBool>,
    #[cfg(not(target_os = "macos"))]
    flm_manager: Arc<Mutex<Option<crate::managers::flm::FlmManager>>>,
}

impl Clone for TranscriptionManager {
    // T-307: clones share all Arc-backed state but are NEVER the watcher owner
    // (`owns_watcher: false`), so a clone's Drop is a no-op for the shared idle
    // watcher. Only the original from `new()` tears it down, at real shutdown.
    fn clone(&self) -> Self {
        Self {
            engine: Arc::clone(&self.engine),
            translator_engine: Arc::clone(&self.translator_engine),
            model_manager: Arc::clone(&self.model_manager),
            app_handle: self.app_handle.clone(),
            last_activity: Arc::clone(&self.last_activity),
            translator_last_activity: Arc::clone(&self.translator_last_activity),
            shutdown_signal: Arc::clone(&self.shutdown_signal),
            watcher_handle: Arc::clone(&self.watcher_handle),
            owns_watcher: false,
            is_loading: Arc::clone(&self.is_loading),
            loading_condvar: Arc::clone(&self.loading_condvar),
            load_flight: Arc::clone(&self.load_flight),
            external_select_gen: Arc::clone(&self.external_select_gen),
            is_live_transcribing: Arc::clone(&self.is_live_transcribing),
            is_batch_transcribing: Arc::clone(&self.is_batch_transcribing),
            #[cfg(not(target_os = "macos"))]
            flm_manager: Arc::clone(&self.flm_manager),
        }
    }
}

impl TranscriptionManager {
    pub fn new(app_handle: &AppHandle, model_manager: Arc<ModelManager>) -> Result<Self> {
        // The overlay's estimates start from the last run's timings.
        if let Ok(dir) = crate::portable::resolve_app_data_dir(app_handle) {
            load_model_timings(dir.join("model_timings.json"));
        }
        let manager = Self {
            engine: Arc::new(Mutex::new(EngineState {
                engine: None,
                model_id: None,
                instance: 0,
            })),
            translator_engine: Arc::new(Mutex::new(EngineState {
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
            translator_last_activity: Arc::new(AtomicU64::new(
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            )),
            shutdown_signal: Arc::new(AtomicBool::new(false)),
            watcher_handle: Arc::new(Mutex::new(None)),
            // This original owns the watcher; clones do not (see impl Clone). T-307.
            owns_watcher: true,
            is_loading: Arc::new(Mutex::new(false)),
            loading_condvar: Arc::new(Condvar::new()),
            load_flight: Arc::new(Mutex::new(())),
            external_select_gen: Arc::new(AtomicU64::new(0)),
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

                    // Evaluate the MAIN slot's idle-unload INDEPENDENTLY of the
                    // TRANSLATOR slot's (below). This runs inside a scoped
                    // closure so its early exits are local `return`s: a bare
                    // `continue` here used to abort the whole tick and starve
                    // the translator-slot check that follows (so a translator
                    // model never unloaded while the main slot was on
                    // `Immediately`, or while a live/batch take held the flags).
                    (|| {
                        let timeout_seconds = settings
                            .model_unload_timeout
                            .to_seconds(settings.model_unload_custom_seconds);

                        let Some(limit_seconds) = timeout_seconds else {
                            return;
                        };

                        // Skip polling-based unloading for immediate timeout since it's handled directly in transcribe()
                        if settings.model_unload_timeout == ModelUnloadTimeout::Immediately {
                            // ...except an FLM unload put off because a file was
                            // using the server then: done once nothing does.
                            #[cfg(not(target_os = "macos"))]
                            if FLM_UNLOAD_DEFERRED.load(Ordering::Acquire)
                                && FLM_IN_FLIGHT.load(Ordering::Acquire) == 0
                                && !manager_cloned.is_live_transcribing.load(Ordering::Relaxed)
                                && !manager_cloned.is_batch_transcribing.load(Ordering::Relaxed)
                                && matches!(
                                    manager_cloned.lock_engine().engine,
                                    Some(LoadedEngine::FlmWhisper)
                                )
                            {
                                FLM_UNLOAD_DEFERRED.store(false, Ordering::Release);
                                manager_cloned.maybe_unload_immediately("FLM no longer in use");
                            }
                            return;
                        }

                        let last = manager_cloned.last_activity.load(Ordering::Relaxed);
                        // FLM in the dictation slot is also busy when Files or a
                        // watched folder use it (they never touch last_activity).
                        #[cfg(not(target_os = "macos"))]
                        let last = if matches!(
                            manager_cloned.lock_engine().engine,
                            Some(LoadedEngine::FlmWhisper)
                        ) {
                            last.max(FLM_LAST_USED_MS.load(Ordering::Relaxed))
                        } else {
                            last
                        };
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
                            return;
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
                    })();

                    // An FLM server the dictation slot no longer holds (Files /
                    // watched folders started it, or dictation moved to another
                    // model) stops after a while unused.
                    #[cfg(not(target_os = "macos"))]
                    manager_cloned.stop_idle_flm();

                    // Translator slot: ALWAYS evaluated each tick, independent of
                    // the main-slot decision above. Independent idle-unload driven by
                    // `translator_model_unload_timeout` vs
                    // `translator_last_activity`. Unlike the main slot,
                    // `Immediately` is NOT special-cased away here (there is no
                    // per-take stop hook that would otherwise handle it) — with
                    // a 0-second limit it simply unloads on the next tick after
                    // a batch job releases `is_batch_transcribing`. The batch
                    // flag guard means we never yank the model mid-file (a
                    // paused-but-active job keeps the flag set).
                    let translator_timeout = settings
                        .translator_model_unload_timeout
                        .to_seconds(settings.translator_model_unload_custom_seconds);
                    if let Some(tr_limit) = translator_timeout {
                        if !manager_cloned.is_batch_transcribing.load(Ordering::Relaxed)
                            && !crate::commands::files::file_job_running()
                        {
                            let last = manager_cloned
                                .translator_last_activity
                                .load(Ordering::Relaxed);
                            let now_ms = SystemTime::now()
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as u64;
                            if now_ms.saturating_sub(last) > tr_limit * 1000
                                && manager_cloned.is_translator_model_loaded()
                            {
                                if let Ok(()) = manager_cloned.unload_translator_model() {
                                    debug!("Translator model unloaded due to inactivity");
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

    /// Lock the Translator slot's engine mutex, poison-recovering like
    /// [`Self::lock_engine`]. The two slots are independent mutexes — a
    /// caller must never hold both at once, and no code path here does.
    fn lock_translator_engine(&self) -> MutexGuard<'_, EngineState> {
        self.translator_engine.lock().unwrap_or_else(|poisoned| {
            warn!("Translator engine mutex was poisoned by a previous panic, recovering");
            poisoned.into_inner()
        })
    }

    pub fn is_model_loaded(&self) -> bool {
        let state = self.lock_engine();
        state.engine.is_some()
    }

    pub fn unload_model(&self) -> Result<()> {
        // Same single-flight lock the loader holds: without it an unload could
        // land BETWEEN a load installing the FLM subprocess and installing the
        // engine state, stopping the new child yet leaving `FlmWhisper` marked
        // loaded (a subprocess-less phantom). Load and unload must be mutually
        // exclusive. Lock order is always load_flight → engine (never the
        // reverse), matching load_model, so this cannot deadlock. Poison-safe.
        let _flight = self
            .load_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let unload_start = std::time::Instant::now();
        debug!("Starting to unload model");

        // FLM in use right now (a dictation take, a file, a watched folder - each
        // holds a lease taken under load_flight, which we hold): leave it all as
        // it is; the idle unload tries again later. Unloading would stop the
        // server under that request, or empty the dictation slot mid-take.
        #[cfg(not(target_os = "macos"))]
        if FLM_IN_FLIGHT.load(Ordering::Acquire) > 0
            && matches!(self.lock_engine().engine, Some(LoadedEngine::FlmWhisper))
        {
            info!("FLM is busy; not unloading it now");
            FLM_UNLOAD_DEFERRED.store(true, Ordering::Release);
            return Err(anyhow::anyhow!(
                "FLM is transcribing right now - try again in a moment"
            ));
        }

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
                        // Stop the FLM subprocess (an FLM call in flight was
                        // ruled out above, under load_flight).
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
                    LoadedEngine::OpenRouterWhisper | LoadedEngine::SttServer => {
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

    /// Whether `model_id` runs on FLM (the NPU server).
    pub fn is_flm_model(&self, model_id: &str) -> bool {
        #[cfg(not(target_os = "macos"))]
        {
            self.model_manager
                .get_model_info(model_id)
                .is_some_and(|m| matches!(m.engine_type, EngineType::FlmWhisper))
        }
        #[cfg(target_os = "macos")]
        {
            let _ = model_id;
            false
        }
    }

    /// Transcribes with FLM beside the engine slots, whatever dictation uses:
    /// Files and watched folders put FLM (NPU) to work while dictation keeps
    /// its own model (often Whisper on the GPU). One server serves everyone
    /// (port 52625): a running one is used, else it is started here - without
    /// touching the dictation slot.
    ///
    /// Locks: load_flight → flm_manager only to check/start the server and take
    /// the lease; then flm_manager alone for the request. Nothing upstream is
    /// ever taken while flm_manager is held.
    #[cfg(not(target_os = "macos"))]
    pub fn transcribe_flm_direct(&self, model_id: &str, audio: Vec<f32>) -> Result<String> {
        use crate::managers::flm::FlmManager;
        let model_info = self
            .model_manager
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;
        if !matches!(model_info.engine_type, EngineType::FlmWhisper) {
            return Err(anyhow::anyhow!("'{}' is not an FLM model", model_id));
        }
        if audio.is_empty() {
            return Ok(String::new());
        }
        let lease = {
            let _flight = self
                .load_flight
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let mut flm = self
                .flm_manager
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if !flm.as_mut().is_some_and(|f| f.is_running()) {
                if let Some(mut old) = flm.take() {
                    old.stop();
                }
                let name = if model_info.filename.is_empty() {
                    "whisper-v3:turbo"
                } else {
                    model_info.filename.as_str()
                };
                info!("FLM: starting the server for a file");
                *flm = Some(FlmManager::start_serve(name)?);
            }
            FLM_IN_FLIGHT.fetch_add(1, Ordering::AcqRel);
            FlmLease
        };

        let settings = get_settings(&self.app_handle);
        let translate = settings.translate_to_english && model_info.supports_translation;
        let language = normalize_language_for_engine(&settings.selected_language)
            .or_else(|| Some("en".to_string()));
        let audio_secs = audio.len() as f32 / 16_000.0;
        let result = {
            let flm = self
                .flm_manager
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let started = std::time::Instant::now();
            let result = match flm.as_ref() {
                Some(flm) => flm.transcribe(audio, language.as_deref(), translate),
                None => Err(anyhow::anyhow!("The FLM server is not running")),
            };
            if result.is_ok() {
                record_model_timing(
                    model_id.to_string(),
                    audio_secs,
                    started.elapsed().as_secs_f32(),
                );
            }
            result
        };
        drop(lease);
        let raw_text = result?;
        let corrected = if !settings.custom_words.is_empty() {
            apply_custom_words(
                &raw_text,
                &settings.custom_words,
                settings.word_correction_threshold,
            )
        } else {
            raw_text
        };
        Ok(filter_transcription_output(&corrected))
    }

    /// One Custom STT server request with the current settings. `speakers`:
    /// use the speakers model when speakers are on (a take); `false` always
    /// asks for plain text (pieces of a file, fallbacks). Custom words are sent
    /// as the prompt and corrected afterwards, in the speaker text too.
    fn stt_server_request(
        &self,
        settings: &crate::settings::AppSettings,
        audio: &[f32],
        speakers: bool,
    ) -> Result<crate::managers::stt_server::SttServerResult> {
        use crate::managers::stt_server;
        if settings.stt_server_url.trim().is_empty() {
            return Err(anyhow::anyhow!(
                "The STT server address is not set. Set it in Advanced settings > STT server."
            ));
        }
        if audio.is_empty() {
            return Ok(Default::default());
        }
        let use_speakers = speakers && settings.stt_server_speakers;
        let model = if use_speakers {
            settings.stt_server_speakers_model.trim()
        } else {
            settings.stt_server_model.trim()
        };
        // "auto" stays unset: the server detects the language itself.
        let language = normalize_language_for_engine(&settings.selected_language);
        let prompt = settings.custom_words.join(", ");
        let mut result = stt_server::transcribe(
            &settings.stt_server_url,
            &settings.stt_server_key,
            model,
            audio,
            language.as_deref(),
            Some(prompt.as_str()),
            use_speakers,
        )?;
        let correct = |text: &str| {
            let corrected = if settings.custom_words.is_empty() {
                text.to_string()
            } else {
                apply_custom_words(
                    text,
                    &settings.custom_words,
                    settings.word_correction_threshold,
                )
            };
            filter_transcription_output(&corrected)
        };
        result.text = correct(&result.text);
        if !use_speakers {
            result.diarized = None;
        }
        result.diarized = result
            .diarized
            .map(|d| stt_server::correct_diarized(&d, |s| correct(s)));
        Ok(result)
    }

    /// A dictation take on the Custom STT server: text and, with speakers on,
    /// the speaker text. Goes through `transcribe_inner` like any take (it
    /// waits for a model still loading and keeps the model's idle timer
    /// fresh).
    pub fn stt_server_take_expecting(
        &self,
        expected_model: &str,
        audio: Vec<f32>,
    ) -> Result<crate::managers::stt_server::SttServerResult> {
        STT_TAKE_SPEAKERS.with(|c| *c.borrow_mut() = Some(None));
        let result = self.transcribe_inner(expected_model, audio, false);
        let diarized = STT_TAKE_SPEAKERS.with(|c| c.borrow_mut().take()).flatten();
        result.map(|(text, _)| crate::managers::stt_server::SttServerResult { text, diarized })
    }

    /// History's "Transcribe again with speakers": the whole recording to the
    /// Custom STT server's speakers model, whatever the dictation model is and
    /// whether speakers are on for takes.
    /// `settings`: the snapshot the caller checked (a change made meanwhile
    /// does not switch the server or model under it).
    pub fn stt_server_speakers_direct(
        &self,
        settings: &crate::settings::AppSettings,
        audio: &[f32],
    ) -> Result<crate::managers::stt_server::SttServerResult> {
        let mut settings = settings.clone();
        settings.stt_server_speakers = true;
        self.stt_server_request(&settings, audio, true)
    }

    /// History's "Transcribe again" for a Custom STT server take: the whole
    /// recording in one request, with speakers when they are on.
    pub fn stt_server_take(
        &self,
        audio: &[f32],
    ) -> Result<crate::managers::stt_server::SttServerResult> {
        let settings = get_settings(&self.app_handle);
        self.stt_server_request(&settings, audio, true)
    }

    /// An online engine (API, OpenRouter) for a file: straight to its endpoint,
    /// never through the dictation slot, so a file can use one whatever the
    /// dictation model is. Same request, language and custom words as a take;
    /// an OpenRouter cost is not added to the take being dictated.
    pub fn transcribe_online_direct(&self, model_id: &str, audio: Vec<f32>) -> Result<String> {
        let model_info = self
            .model_manager
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;
        if audio.is_empty() {
            return Ok(String::new());
        }
        let settings = get_settings(&self.app_handle);
        let translate = settings.translate_to_english && model_info.supports_translation;
        let raw_text = match model_info.engine_type {
            EngineType::ApiWhisper => {
                let language = normalize_language_for_engine(&settings.selected_language)
                    .or_else(|| Some("en".to_string()));
                crate::managers::api_transcription::transcribe(
                    &settings.api_transcription_url,
                    &settings.api_transcription_key,
                    &settings.api_transcription_model,
                    audio,
                    language.as_deref(),
                    translate,
                )?
            }
            EngineType::OpenRouterWhisper => {
                let base_url = settings.openrouter_transcription_url.trim();
                let api_key = settings.openrouter_transcription_key.trim();
                if base_url.is_empty() || api_key.is_empty() {
                    return Err(anyhow::anyhow!(
                        "OpenRouter transcription needs a base URL and API key. \
                         Set them in Advanced > Providers."
                    ));
                }
                let model = match settings.openrouter_transcription_model.trim() {
                    "" => "openai/whisper-large-v3",
                    m => m,
                };
                let language = normalize_language_for_engine(&settings.selected_language);
                let translate = matches!(
                    settings.openrouter_transcription_route,
                    crate::settings::OpenRouterTranscriptionRoute::Chat
                ) && translate;
                crate::managers::openrouter_transcription::transcribe_unbilled(
                    base_url,
                    api_key,
                    model,
                    &audio,
                    language.as_deref(),
                    settings.openrouter_transcription_route,
                    settings.openrouter_transcription_audio_format,
                    translate,
                )?
            }
            EngineType::SttServer => {
                // Already corrected (custom words) inside the request helper.
                return self
                    .stt_server_request(&settings, &audio, false)
                    .map(|r| r.text);
            }
            _ => {
                return Err(anyhow::anyhow!("'{}' is not an online model", model_id));
            }
        };
        let corrected = if !settings.custom_words.is_empty() {
            apply_custom_words(
                &raw_text,
                &settings.custom_words,
                settings.word_correction_threshold,
            )
        } else {
            raw_text
        };
        Ok(filter_transcription_output(&corrected))
    }

    /// No FLM on macOS (`is_flm_model` is always false there).
    #[cfg(target_os = "macos")]
    pub fn transcribe_flm_direct(&self, model_id: &str, _audio: Vec<f32>) -> Result<String> {
        Err(anyhow::anyhow!(
            "FLM is not available on macOS ('{}')",
            model_id
        ))
    }

    /// Stops an FLM server that the dictation slot does not hold once nothing
    /// has used it for the model unload timeout. Rechecked under load_flight, which
    /// every lease is taken under, so it never stops a server in use.
    #[cfg(not(target_os = "macos"))]
    fn stop_idle_flm(&self) {
        // The same "unload model after" as dictation's (Never = keep it).
        let settings = get_settings(&self.app_handle);
        let Some(limit_secs) = settings
            .model_unload_timeout
            .to_seconds(settings.model_unload_custom_seconds)
        else {
            return;
        };
        let idle = |since: u64| now_ms().saturating_sub(since) > limit_secs * 1000;
        if FLM_IN_FLIGHT.load(Ordering::Acquire) > 0
            || !idle(FLM_LAST_USED_MS.load(Ordering::Relaxed))
        {
            return;
        }
        let _flight = self
            .load_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if FLM_IN_FLIGHT.load(Ordering::Acquire) > 0
            || !idle(FLM_LAST_USED_MS.load(Ordering::Relaxed))
        {
            return;
        }
        if matches!(self.lock_engine().engine, Some(LoadedEngine::FlmWhisper)) {
            return; // the dictation slot's own idle unload governs it
        }
        let mut flm = self
            .flm_manager
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(mut server) = flm.take() {
            server.stop();
            info!("FLM: stopped the server after {limit_secs} s unused");
        }
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

    /// Bump the external-selection generation and return the new value. The
    /// caller passes it to `reload_external_model_if_latest` from a background
    /// thread; a later selection bumps it again, letting the stale load bail.
    pub fn next_external_select_gen(&self) -> u64 {
        self.external_select_gen.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Background warm-up for an EXTERNAL model selection. Force-reloads (so a
    /// dead FLM is restarted) UNLESS a newer selection has bumped the
    /// generation while this request waited for `load_flight` — in which case
    /// the newer request owns the load and this one is a no-op. This makes
    /// concurrent/rapid re-selections latest-intent-wins and avoids restarting
    /// a freshly-loaded FLM out from under it.
    pub fn reload_external_model_if_latest(&self, model_id: &str, select_gen: u64) -> Result<()> {
        let _flight = self
            .load_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.external_select_gen.load(Ordering::SeqCst) != select_gen {
            debug!(
                "External selection for '{}' (gen {}) superseded before load — skipping",
                model_id, select_gen
            );
            return Ok(());
        }
        // Coalesce concurrent same-model selections into ONE reload: if this
        // exact model is already loaded AND healthy, an earlier selection that
        // won the flight first already (re)started it — skip the redundant
        // restart. A genuinely DEAD FLM fails the liveness check and is still
        // restarted (the reselect-to-recover path). Engine lock is taken and
        // released before flm_manager (never nested) to preserve lock order.
        let already_loaded = {
            let state = self.lock_engine();
            state.engine.is_some() && state.model_id.as_deref() == Some(model_id)
        };
        if already_loaded {
            #[cfg(not(target_os = "macos"))]
            let healthy = {
                let is_flm = self
                    .model_manager
                    .get_model_info(model_id)
                    .map(|m| matches!(m.engine_type, EngineType::FlmWhisper))
                    .unwrap_or(false);
                if is_flm {
                    self.flm_manager
                        .lock()
                        .ok()
                        .and_then(|mut g| g.as_mut().map(|f| f.is_running()))
                        .unwrap_or(false)
                } else {
                    // Stateless HTTP engine (ApiWhisper/OpenRouter): loaded == healthy.
                    true
                }
            };
            #[cfg(target_os = "macos")]
            let healthy = true;
            if healthy {
                debug!(
                    "External model '{}' already loaded and healthy — skipping redundant reload",
                    model_id
                );
                return Ok(());
            }
        }
        self.load_model_locked(model_id)
    }

    pub fn load_model(&self, model_id: &str) -> Result<()> {
        // Single-flight: serialize the entire load across ALL callers so two
        // concurrent loads can't both spawn `flm serve` on the same port (and
        // so an FLM restart always fully stops the old child before starting a
        // new one). Poison-safe.
        let _flight = self
            .load_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.load_model_locked(model_id)
    }

    /// The actual load, assuming `load_flight` is ALREADY held by the caller.
    /// Never acquire `load_flight` in here (it is not reentrant).
    fn load_model_locked(&self, model_id: &str) -> Result<()> {
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
                // T-212: resolve the persisted GPU-device selection (Auto / CPU /
                // explicit Vulkan device index) into whisper-rs load params. If
                // the chosen device fails to initialize (disappeared adapter,
                // driver reordered devices, invalid stale index, etc.), never
                // brick transcription — log a warning and retry once with the
                // default (Auto) params before giving up for real.
                let gpu_setting = get_settings(&self.app_handle).transcribe_gpu_device;

                // Adversarial review finding 4: hold the app-wide Vulkan
                // serialization lock across BOTH the pre-load adapter-list
                // validation (finding 5) and the load attempt(s) below, so
                // this whole GPU-touching sequence can't overlap a
                // concurrent `list_gpu_devices` enumeration call from the
                // settings UI.
                with_vulkan_op_lock(|| -> Result<()> {
                    // Finding 5: an explicit device index (>= 0) that no
                    // longer exists (disappeared adapter, driver reordered
                    // devices, stale index from an old settings file) must
                    // not be allowed to silently run on CPU. whisper.cpp can
                    // fail to attach a GPU backend for an invalid index,
                    // then add the CPU backend anyway and report success —
                    // so an Err-only check at load time never catches it.
                    // Validate the index against the LIVE adapter list
                    // before ever attempting the load, and fall back to
                    // Auto (with a warning) if it's out of range.
                    let effective_gpu_setting = if gpu_setting >= 0 {
                        let available = transcribe_rs::engines::whisper::list_gpu_devices();
                        let available_count = available.len();
                        let available_indices: Vec<i32> =
                            available.into_iter().map(|d| d.index).collect();
                        let resolved =
                            resolve_effective_gpu_setting(gpu_setting, &available_indices);
                        if resolved != gpu_setting {
                            warn!(
                                "transcribe_gpu_device={} does not match any of the {} \
                                 currently-visible Vulkan adapter(s) — falling back to Auto",
                                gpu_setting, available_count
                            );
                        }
                        resolved
                    } else {
                        gpu_setting
                    };

                    let gpu_params = whisper_gpu_params_for_setting(effective_gpu_setting);
                    if let Err(e) = engine.load_model_with_params(&model_path, gpu_params.clone()) {
                        // Finding 6: only an explicit GPU index gets a
                        // safety-net retry with default (Auto, still
                        // GPU-on) parameters. A force-CPU (`-2`) load
                        // failing must surface as a real error — retrying
                        // it with `WhisperModelParams::default()` (GPU-on)
                        // would silently break the "CPU Only" contract.
                        let retryable = should_retry_with_default_gpu_params(effective_gpu_setting);
                        warn!(
                            "Failed to load whisper model {} with transcribe_gpu_device={} \
                             (use_gpu={}, gpu_device={}): {}{}",
                            model_id,
                            effective_gpu_setting,
                            gpu_params.use_gpu,
                            gpu_params.gpu_device,
                            e,
                            if retryable {
                                " — retrying with default GPU parameters"
                            } else {
                                " — force-CPU load failed, not retrying with GPU"
                            }
                        );

                        if retryable {
                            engine
                                .load_model_with_params(&model_path, WhisperModelParams::default())
                                .map_err(|e| {
                                    let error_msg =
                                        format!("Failed to load whisper model {}: {}", model_id, e);
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
                        } else {
                            let error_msg =
                                format!("Failed to load whisper model {}: {}", model_id, e);
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
                    }
                    Ok(())
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
                // A server still running (Files / watched folders started it, or
                // an earlier dictation load) is reused: restarting it would fail
                // a file's request in flight and cost the NPU's warm-up. A dead
                // one is stopped and dropped BEFORE spawning a new one, so
                // re-selecting FLM restarts a dead server (recovery) and the new
                // `flm serve` doesn't collide with the old one on port 52625.
                // Serialized by `load_flight`, so this check→start is atomic with
                // respect to other loaders and to the Files lease.
                let reuse = {
                    let mut existing = self
                        .flm_manager
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if existing.as_mut().is_some_and(|f| f.is_running()) {
                        true
                    } else {
                        if let Some(mut old) = existing.take() {
                            old.stop();
                        }
                        false
                    }
                };
                let started = if reuse {
                    info!("FLM: the dictation slot uses the server already running");
                    Ok(None)
                } else {
                    FlmManager::start_serve(flm_model_name).map(Some)
                };
                let flm = match started {
                    Ok(flm) => flm,
                    Err(e) => {
                        let error_msg = format!("Failed to start FLM server: {}", e);
                        // We already stopped+dropped the previous FLM child
                        // above, and the user's selection has already been
                        // persisted to FLM — so leaving ANY engine installed
                        // (a stale FlmWhisper with no subprocess, OR a
                        // previously-loaded different engine) makes runtime and
                        // persisted state disagree. Clear unconditionally so
                        // recording init / preflight see "not loaded" and retry
                        // the (selected) FLM instead of silently running the old
                        // engine or skipping recovery.
                        {
                            let mut state = self.lock_engine();
                            state.engine = None;
                            state.model_id = None;
                            state.instance += 1;
                        }
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
                };
                if let Some(flm) = flm {
                    *self
                        .flm_manager
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(flm);
                }
                flm_note_used();
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
                let has_config = !settings.openrouter_transcription_url.trim().is_empty()
                    && !settings.openrouter_transcription_key.trim().is_empty();
                if !has_config {
                    let error_msg = "OpenRouter transcription needs a base URL and API key. Set them in Advanced > Providers.".to_string();
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
            EngineType::SttServer => {
                let settings = get_settings(&self.app_handle);
                if settings.stt_server_url.trim().is_empty() {
                    let error_msg =
                        "The STT server address is not set. Set it in Advanced settings > STT server."
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
                    "STT server engine selected (url: {}, speakers: {})",
                    settings.stt_server_url, settings.stt_server_speakers
                );
                LoadedEngine::SttServer
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
        self.transcribe_inner(expected_model, audio, false)
            .map(|(text, _)| text)
    }

    /// Transcribe for the live preview: the text plus, when the engine can tell
    /// (Parakeet), each word with its start in seconds from the start of `audio`.
    pub fn transcribe_with_words(
        &self,
        audio: Vec<f32>,
    ) -> Result<(String, Option<Vec<(f32, String)>>)> {
        let expected = get_settings(&self.app_handle).selected_model;
        self.transcribe_inner(&expected, audio, true)
    }

    fn transcribe_inner(
        &self,
        expected_model: &str,
        audio: Vec<f32>,
        want_words: bool,
    ) -> Result<(String, Option<Vec<(f32, String)>>)> {
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
            return Ok((String::new(), None));
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
                // A lease, taken under load_flight with the slot checked again: an
                // unload (tray, idle) between the check above and the request can
                // then no longer stop the server under this take (unload keeps a
                // leased server running). Order load_flight → engine, as always.
                let (speed_key, lease) = {
                    let _flight = self
                        .load_flight
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let engine_guard = self.lock_engine();
                    if !matches!(&engine_guard.engine, Some(LoadedEngine::FlmWhisper)) {
                        return Err(anyhow::anyhow!(
                            "The FLM model was unloaded mid-take — please retry."
                        ));
                    }
                    Self::ensure_expected_model(&engine_guard, expected_model)?;
                    FLM_IN_FLIGHT.fetch_add(1, Ordering::AcqRel);
                    (engine_guard.model_id.clone().unwrap_or_default(), FlmLease)
                };
                let language = normalize_language_for_engine(&settings.selected_language)
                    .or_else(|| Some("en".to_string()));
                let audio_secs = audio.len() as f32 / 16_000.0;
                // Transcribe under the flm lock, then RELEASE it before
                // maybe_unload_immediately: unload_model re-locks flm_manager, so
                // holding it across the unload self-deadlocks (std Mutex is not
                // reentrant). The `.map` yields an owned Result that does not
                // borrow the guard, so the guard drops at the block's end.
                let flm_result = {
                    let flm_guard = self.flm_manager.lock().unwrap();
                    // The overlay's percentage and this model's timings (FLM gives
                    // no figure of its own): timed from here, once the server is
                    // ours, so waiting for a file's request is not counted.
                    let started = std::time::Instant::now();
                    let job_guard = begin_running_job(
                        Some(started),
                        expected_transcription_secs(&speed_key, audio_secs),
                        Arc::new(AtomicI32::new(-1)),
                    );
                    let result = flm_guard
                        .as_ref()
                        .map(|flm| flm.transcribe(audio, language.as_deref(), effective_translate));
                    drop(job_guard);
                    if matches!(result, Some(Ok(_))) {
                        record_model_timing(speed_key, audio_secs, started.elapsed().as_secs_f32());
                    }
                    result
                };
                // Released before maybe_unload_immediately below (a held lease
                // would keep the server running); counts as use.
                drop(lease);
                match flm_result {
                    Some(raw) => {
                        let raw_text = raw?;
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
                        return Ok((filtered, None));
                    }
                    // Engine state says FLM but the subprocess is gone (e.g. a
                    // failed restart) — fail loudly instead of falling through to
                    // the local-engine path, which would misread the empty slot.
                    None => {
                        return Err(anyhow::anyhow!(
                            "FLM engine is selected but its server is not running. \
                             Reselect the model to restart it."
                        ));
                    }
                }
            }
        }

        // Custom STT server: one request for the whole take; its speaker text
        // (if any) is kept for the take's History row.
        {
            let engine_guard = self.lock_engine();
            if matches!(&engine_guard.engine, Some(LoadedEngine::SttServer)) {
                Self::ensure_expected_model(&engine_guard, expected_model)?;
                drop(engine_guard);
                // Plain text for watched folders and other shared-path
                // callers; a dictation take (`stt_server_take_expecting`)
                // also gets its speaker text.
                let take = STT_TAKE_SPEAKERS.with(|c| c.borrow().is_some());
                let speed_key = if take && settings.stt_server_speakers {
                    format!("{expected_model}{SPEAKERS_KEY_SUFFIX}")
                } else {
                    expected_model.to_string()
                };
                let audio_secs = audio.len() as f32 / 16_000.0;
                let result = timed_online_request(&speed_key, audio_secs, || {
                    self.stt_server_request(&settings, &audio, take)
                })?;
                if take {
                    STT_TAKE_SPEAKERS.with(|c| *c.borrow_mut() = Some(result.diarized));
                }
                self.maybe_unload_immediately("STT server");
                return Ok((result.text, None));
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
                let audio_secs = audio.len() as f32 / 16_000.0;
                let raw_text = timed_online_request(expected_model, audio_secs, || {
                    crate::managers::api_transcription::transcribe(
                        &settings.api_transcription_url,
                        &settings.api_transcription_key,
                        &settings.api_transcription_model,
                        audio,
                        language.as_deref(),
                        effective_translate,
                    )
                })?;
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
                return Ok((filtered, None));
            }
        }

        // If OpenRouter transcription is active, POST base64 audio (JSON). This
        // engine runs per segment/chunk, so it works in live and chunked modes.
        {
            let engine_guard = self.lock_engine();
            if matches!(&engine_guard.engine, Some(LoadedEngine::OpenRouterWhisper)) {
                Self::ensure_expected_model(&engine_guard, expected_model)?;
                drop(engine_guard);
                // T-308: dedicated OpenRouter URL + key (independent of the
                // llm_providers registry). Never send an unauthenticated request.
                let base_url = settings.openrouter_transcription_url.trim();
                let api_key = settings.openrouter_transcription_key.trim();
                if base_url.is_empty() || api_key.is_empty() {
                    return Err(anyhow::anyhow!(
                        "OpenRouter transcription needs a base URL and API key. \
                         Set them in Advanced > Providers."
                    ));
                }
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
                // Translation is only meaningful on the Chat route (the STT
                // endpoint has no translation control); gate on the global
                // translate + the model's declared support.
                let translate = matches!(
                    settings.openrouter_transcription_route,
                    crate::settings::OpenRouterTranscriptionRoute::Chat
                ) && effective_translate;
                let audio_secs = audio.len() as f32 / 16_000.0;
                let raw_text = timed_online_request(expected_model, audio_secs, || {
                    crate::managers::openrouter_transcription::transcribe(
                        base_url,
                        api_key,
                        model,
                        &audio,
                        language.as_deref(),
                        settings.openrouter_transcription_route,
                        settings.openrouter_transcription_audio_format,
                        translate,
                    )
                })?;
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
                return Ok((filtered, None));
            }
        }

        // Perform transcription with the appropriate engine.
        // We use catch_unwind to prevent engine panics from poisoning the mutex,
        // which would make the app hang indefinitely on subsequent operations.
        // Only Parakeet returns real word segments (TimestampGranularity::Word);
        // other engines' segments are sentences or tokens, not words.
        let is_parakeet;
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
            is_parakeet = matches!(engine, LoadedEngine::Parakeet(_));

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

            // Timed by the audio's own length (before any padding), like the
            // estimates for chunks still waiting their turn.
            let audio_secs = audio.len() as f32 / 16_000.0;

            // Pad trailing silence: engines drop final tokens when the audio
            // ends abruptly (tail segment at stop, ~45 s hard cuts, live 3 s
            // timer cuts). Whisper too, since 2.0.5: a segment cut at a short
            // pause lost its last word ("So dot" came out "So") although the
            // word was in the recording.
            debug!(
                "Padding {} samples of trailing silence",
                TRAILING_SILENCE_PAD_SAMPLES
            );
            let audio = pad_trailing_silence(audio, TRAILING_SILENCE_PAD_SAMPLES);

            // Publish this call for the overlay's progress percentage, and time it
            // to keep the model's timings current.
            let speed_key = taken_model_id.clone().unwrap_or_default();
            let reported = Arc::new(AtomicI32::new(-1));
            let expected = expected_transcription_secs(&speed_key, audio_secs);
            let job_started = std::time::Instant::now();
            // Whisper first waits for the GPU lock: its clock starts once it
            // holds it (below). Only engine time counts, for the percentage
            // and for the timings.
            let waits_for_gpu = matches!(engine, LoadedEngine::Whisper(_));
            let job_guard = begin_running_job(
                (!waits_for_gpu).then_some(job_started),
                expected,
                Arc::clone(&reported),
            );
            let engine_started = std::cell::Cell::new(job_started);

            let transcribe_result = catch_unwind(AssertUnwindSafe(
                || -> Result<transcribe_rs::TranscriptionResult> {
                    match &mut engine {
                        LoadedEngine::Whisper(whisper_engine) => {
                            let params = WhisperInferenceParams {
                                language: normalize_language_for_engine(
                                    &settings.selected_language,
                                ),
                                translate: effective_translate,
                                progress: Some(Arc::clone(&reported)),
                                ..Default::default()
                            };

                            // ggml-vulkan is not safe for concurrent device use.
                            // Hold the SAME leaf Vulkan lock the GPU model-LOADS
                            // take, but only around this inference call — so a
                            // translator/dictation load's GPU init can never run
                            // concurrently with Whisper inference on the shared
                            // iGPU (graceful turn-taking). The slot mutex was
                            // already released above (`drop(engine_guard)`), and
                            // this lock is a LEAF (never acquires the slot mutex,
                            // load_flight, or CHUNK_TRANSCRIBE_LOCK), so the lock
                            // order stays acyclic.
                            with_vulkan_op_lock(|| {
                                let now = std::time::Instant::now();
                                engine_started.set(now);
                                if let Ok(mut job) = RUNNING_JOB.lock() {
                                    if let Some(job) = job.as_mut() {
                                        job.started = Some(now);
                                    }
                                }
                                whisper_engine.transcribe_samples(audio, Some(params))
                            })
                            .map_err(|e| anyhow::anyhow!("Whisper transcription failed: {}", e))
                        }
                        LoadedEngine::Parakeet(parakeet_engine) => {
                            let params = ParakeetInferenceParams {
                                timestamp_granularity: if want_words {
                                    TimestampGranularity::Word
                                } else {
                                    TimestampGranularity::Segment
                                },
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
                        LoadedEngine::SttServer => {
                            // Handled earlier via the STT server path; never reached.
                            Err(anyhow::anyhow!(
                                "SttServer engine should use the STT server path"
                            ))
                        }
                    }
                },
            ));

            drop(job_guard);
            if transcribe_result.as_ref().is_ok_and(|r| r.is_ok()) {
                let took = engine_started.get().elapsed().as_secs_f32();
                record_model_timing(speed_key, audio_secs, took);
            }

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

        let words = if want_words && is_parakeet {
            result.segments.as_ref().map(|segments| {
                segments
                    .iter()
                    .map(|seg| (seg.start, seg.text.trim().to_string()))
                    .filter(|(_, word)| !word.is_empty())
                    .collect()
            })
        } else {
            None
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

        Ok((final_result, words))
    }

    // ===================================================================
    // Translator dedicated parallel engine slot (T-300)
    //
    // A SECOND resident local engine so the Translator folder-batch worker
    // can keep its model loaded in parallel with the dictation model instead
    // of thrashing the shared `engine` slot. Only LOCAL engines live here —
    // external engines (FLM/API/OpenRouter) keep the shared/stateless path.
    //
    // Lock order (must stay acyclic, identical to the main slot):
    //   load_flight -> (with_vulkan_op_lock) -> engine | translator_engine.
    // `CHUNK_TRANSCRIBE_LOCK` (actions.rs) is OUTERMOST for inference and is
    // held by the caller in managers/translator.rs — never acquired here.
    // A slot lock is never held while awaiting `load_flight` or the chunk
    // lock. `build_local_engine`/`run_engine_inference` are private, isolated
    // duplicates of the live-path arms so the live dictation path stays
    // byte-identical (deliberately NOT sharing code with `load_model_locked`
    // / `transcribe_expecting`).
    // ===================================================================

    /// Build a LOCAL transcription engine for `model_id`, emit-free (no
    /// `model-state-changed` events — those belong to the dictation slot).
    /// External engines are rejected: they use the shared path. Mirrors the
    /// local arms of `load_model_locked` (Whisper GPU logic incl.
    /// `with_vulkan_op_lock`, Parakeet, Moonshine, MoonshineStreaming,
    /// SenseVoice) but is a separate copy so the live path is untouched.
    fn build_local_engine(
        &self,
        model_id: &str,
        model_path: &std::path::Path,
        model_info: &crate::managers::model::ModelInfo,
    ) -> Result<LoadedEngine> {
        match model_info.engine_type {
            EngineType::Whisper => {
                let mut engine = WhisperEngine::new();
                let gpu_setting = get_settings(&self.app_handle).transcribe_gpu_device;
                // Hold the app-wide Vulkan lock across adapter validation and
                // the load attempt(s), exactly like the live path.
                with_vulkan_op_lock(|| -> Result<()> {
                    let effective_gpu_setting = if gpu_setting >= 0 {
                        let available = transcribe_rs::engines::whisper::list_gpu_devices();
                        let available_count = available.len();
                        let available_indices: Vec<i32> =
                            available.into_iter().map(|d| d.index).collect();
                        let resolved =
                            resolve_effective_gpu_setting(gpu_setting, &available_indices);
                        if resolved != gpu_setting {
                            warn!(
                                "translator: transcribe_gpu_device={} does not match any of the \
                                 {} currently-visible Vulkan adapter(s) — falling back to Auto",
                                gpu_setting, available_count
                            );
                        }
                        resolved
                    } else {
                        gpu_setting
                    };

                    let gpu_params = whisper_gpu_params_for_setting(effective_gpu_setting);
                    if let Err(e) = engine.load_model_with_params(model_path, gpu_params.clone()) {
                        let retryable = should_retry_with_default_gpu_params(effective_gpu_setting);
                        warn!(
                            "translator: failed to load whisper model {} with \
                             transcribe_gpu_device={} (use_gpu={}, gpu_device={}): {}{}",
                            model_id,
                            effective_gpu_setting,
                            gpu_params.use_gpu,
                            gpu_params.gpu_device,
                            e,
                            if retryable {
                                " — retrying with default GPU parameters"
                            } else {
                                " — force-CPU load failed, not retrying with GPU"
                            }
                        );
                        if retryable {
                            engine
                                .load_model_with_params(model_path, WhisperModelParams::default())
                                .map_err(|e| {
                                    anyhow::anyhow!(
                                        "Failed to load whisper model {}: {}",
                                        model_id,
                                        e
                                    )
                                })?;
                        } else {
                            return Err(anyhow::anyhow!(
                                "Failed to load whisper model {}: {}",
                                model_id,
                                e
                            ));
                        }
                    }
                    Ok(())
                })?;
                Ok(LoadedEngine::Whisper(engine))
            }
            EngineType::Parakeet => {
                let mut engine = ParakeetEngine::new();
                engine
                    .load_model_with_params(model_path, ParakeetModelParams::int8())
                    .map_err(|e| {
                        anyhow::anyhow!("Failed to load parakeet model {}: {}", model_id, e)
                    })?;
                Ok(LoadedEngine::Parakeet(engine))
            }
            EngineType::Moonshine => {
                let mut engine = MoonshineEngine::new();
                engine
                    .load_model_with_params(
                        model_path,
                        MoonshineModelParams::variant(ModelVariant::Base),
                    )
                    .map_err(|e| {
                        anyhow::anyhow!("Failed to load moonshine model {}: {}", model_id, e)
                    })?;
                Ok(LoadedEngine::Moonshine(engine))
            }
            EngineType::MoonshineStreaming => {
                let mut engine = MoonshineStreamingEngine::new();
                engine
                    .load_model_with_params(model_path, StreamingModelParams::default())
                    .map_err(|e| {
                        anyhow::anyhow!(
                            "Failed to load moonshine streaming model {}: {}",
                            model_id,
                            e
                        )
                    })?;
                Ok(LoadedEngine::MoonshineStreaming(engine))
            }
            EngineType::SenseVoice => {
                let mut engine = SenseVoiceEngine::new();
                engine
                    .load_model_with_params(model_path, SenseVoiceModelParams::int8())
                    .map_err(|e| {
                        anyhow::anyhow!("Failed to load SenseVoice model {}: {}", model_id, e)
                    })?;
                Ok(LoadedEngine::SenseVoice(engine))
            }
            // FlmWhisper (cfg-gated) / ApiWhisper / OpenRouterWhisper — external,
            // never resident in this slot.
            _ => Err(anyhow::anyhow!(
                "Model '{}' is not a local engine — the Translator parallel slot \
                 only hosts local engines (external engines use the shared path).",
                model_id
            )),
        }
    }

    /// Run the local inference match for `engine`. A private copy of the
    /// live-path match so `transcribe_expecting` stays byte-identical.
    /// Callers wrap this in `catch_unwind`. External arms are unreachable
    /// (this slot never holds them) but kept for match exhaustiveness.
    fn run_engine_inference(
        engine: &mut LoadedEngine,
        audio: Vec<f32>,
        settings: &AppSettings,
        effective_translate: bool,
    ) -> Result<transcribe_rs::TranscriptionResult> {
        match engine {
            LoadedEngine::Whisper(whisper_engine) => {
                let params = WhisperInferenceParams {
                    language: normalize_language_for_engine(&settings.selected_language),
                    translate: effective_translate,
                    ..Default::default()
                };
                // Same leaf Vulkan lock the GPU model-loads take — this Whisper
                // inference must not overlap concurrent GPU device use (a load's
                // Vulkan init on the shared iGPU). The translator slot mutex is
                // released by the caller before inference and this lock never
                // acquires it (or load_flight / CHUNK_TRANSCRIBE_LOCK), so the
                // lock order stays acyclic.
                with_vulkan_op_lock(|| whisper_engine.transcribe_samples(audio, Some(params)))
                    .map_err(|e| anyhow::anyhow!("Whisper transcription failed: {}", e))
            }
            LoadedEngine::Parakeet(parakeet_engine) => {
                let params = ParakeetInferenceParams {
                    timestamp_granularity: TimestampGranularity::Segment,
                    ..Default::default()
                };
                parakeet_engine
                    .transcribe_samples(audio, Some(params))
                    .map_err(|e| anyhow::anyhow!("Parakeet transcription failed: {}", e))
            }
            LoadedEngine::Moonshine(moonshine_engine) => moonshine_engine
                .transcribe_samples(audio, None)
                .map_err(|e| anyhow::anyhow!("Moonshine transcription failed: {}", e)),
            LoadedEngine::MoonshineStreaming(streaming_engine) => streaming_engine
                .transcribe_samples(audio, None)
                .map_err(|e| anyhow::anyhow!("Moonshine streaming transcription failed: {}", e)),
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
                    .map_err(|e| anyhow::anyhow!("SenseVoice transcription failed: {}", e))
            }
            #[cfg(not(target_os = "macos"))]
            LoadedEngine::FlmWhisper => Err(anyhow::anyhow!(
                "FlmWhisper engine should use FLM manager path"
            )),
            LoadedEngine::ApiWhisper => {
                Err(anyhow::anyhow!("ApiWhisper engine should use API path"))
            }
            LoadedEngine::OpenRouterWhisper => Err(anyhow::anyhow!(
                "OpenRouterWhisper engine should use the OpenRouter path"
            )),
            LoadedEngine::SttServer => Err(anyhow::anyhow!(
                "SttServer engine should use the STT server path"
            )),
        }
    }

    /// Whether a model is resident in the Translator parallel slot.
    pub fn is_translator_model_loaded(&self) -> bool {
        self.lock_translator_engine().engine.is_some()
    }

    /// The model id currently resident in the Translator parallel slot.
    pub fn get_current_translator_model(&self) -> Option<String> {
        self.lock_translator_engine().model_id.clone()
    }

    /// Load `model_id` into the Translator parallel slot. Acquires
    /// `load_flight` (serializing with every other loader) and REJECTS
    /// external engine types — FLM/API/OpenRouter must keep the shared path
    /// (one NPU can't host two contexts; HTTP is stateless). No-op if the
    /// slot already holds this exact model. Emits no dictation model-state
    /// events. Lock order: load_flight -> (with_vulkan_op_lock inside
    /// build_local_engine) -> translator_engine (taken only AFTER the build
    /// completes, never held across the build or the flight wait).
    pub fn load_translator_model(&self, model_id: &str) -> Result<()> {
        let _flight = self
            .load_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        // Already the right model? Nothing to do.
        {
            let state = self.lock_translator_engine();
            if state.engine.is_some() && state.model_id.as_deref() == Some(model_id) {
                return Ok(());
            }
        }

        let model_info = self
            .model_manager
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

        if model_info.engine_type.is_external() {
            return Err(anyhow::anyhow!(
                "Translator parallel slot is for local engines only; external \
                 engine model '{}' must use the shared transcription path.",
                model_id
            ));
        }
        if !model_info.is_downloaded {
            return Err(anyhow::anyhow!("Model not downloaded"));
        }

        let model_path = self.model_manager.get_model_path(model_id)?;
        let load_start = std::time::Instant::now();
        debug!("Translator: loading parallel model {}", model_id);
        let loaded_engine = self.build_local_engine(model_id, &model_path, &model_info)?;

        {
            let mut state = self.lock_translator_engine();
            state.engine = Some(loaded_engine);
            state.model_id = Some(model_id.to_string());
            state.instance += 1;
        }
        self.translator_last_activity.store(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            Ordering::Relaxed,
        );
        debug!(
            "Translator: parallel model {} loaded (took {}ms)",
            model_id,
            load_start.elapsed().as_millis()
        );
        Ok(())
    }

    /// Unload the Translator parallel slot. Acquires `load_flight` then the
    /// slot lock (never the reverse), matching `unload_model`. Only local
    /// engine variants are ever resident here; the external arms are inert.
    pub fn unload_translator_model(&self) -> Result<()> {
        let _flight = self
            .load_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut state = self.lock_translator_engine();
        if let Some(ref mut loaded_engine) = state.engine {
            match loaded_engine {
                LoadedEngine::Whisper(e) => e.unload_model(),
                LoadedEngine::Parakeet(e) => e.unload_model(),
                LoadedEngine::Moonshine(e) => e.unload_model(),
                LoadedEngine::MoonshineStreaming(e) => e.unload_model(),
                LoadedEngine::SenseVoice(e) => e.unload_model(),
                // External engines are never installed in this slot.
                _ => {}
            }
        }
        state.engine = None;
        state.model_id = None;
        state.instance += 1;
        debug!("Translator: parallel model unloaded");
        Ok(())
    }

    /// Immediate-unload hook for the Translator slot, mirroring
    /// `maybe_unload_immediately`. Guarded by `is_batch_transcribing` so a
    /// multi-segment file never drops the model between its own segments —
    /// which in practice means `Immediately` is realized by the idle watcher
    /// on the next tick after the batch flag clears, not literally per
    /// segment. Kept for spec-compliance / defensiveness.
    fn maybe_unload_translator_immediately(&self) {
        // A Files job shares this slot (same model for files and folders since
        // 2.2.1): unloading between its pieces would reload it every piece.
        if self.is_batch_transcribing.load(Ordering::Relaxed)
            || crate::commands::files::file_job_running()
        {
            return;
        }
        let settings = get_settings(&self.app_handle);
        if settings.translator_model_unload_timeout == ModelUnloadTimeout::Immediately
            && self.is_translator_model_loaded()
        {
            if let Err(e) = self.unload_translator_model() {
                warn!("Failed to immediately unload translator model: {}", e);
            }
        }
    }

    /// Transcribe `audio` through the Translator parallel slot, revalidating
    /// that the slot still holds `expected_model`. The caller (the Translator
    /// batch worker) MUST already hold `CHUNK_TRANSCRIBE_LOCK` so local
    /// inference serializes with the live pipeline (graceful iGPU
    /// turn-taking). This method never touches `load_flight` or the chunk
    /// lock — it takes the slot lock only briefly to take/put the engine,
    /// exactly like `transcribe_expecting` (finding-8 instance guard).
    pub fn transcribe_translator_expecting(
        &self,
        expected_model: &str,
        audio: Vec<f32>,
    ) -> Result<String> {
        self.translator_last_activity.store(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            Ordering::Relaxed,
        );

        if audio.is_empty() {
            self.maybe_unload_translator_immediately();
            return Ok(String::new());
        }

        let settings = get_settings(&self.app_handle);
        // Honor "translate to English" only when the Translator's model
        // actually supports it (mirrors the live path's guard).
        let effective_translate = settings.translate_to_english
            && self
                .model_manager
                .get_model_info(expected_model)
                .map(|m| m.supports_translation)
                .unwrap_or(false);

        // Take the engine out (revalidating the expected model), then release
        // the slot lock before inference — no mutex held during the call.
        let mut guard = self.lock_translator_engine();
        let mut engine = match guard.engine.take() {
            Some(e) => e,
            None => {
                return Err(anyhow::anyhow!(
                    "Translator model is not loaded for transcription."
                ));
            }
        };
        // Clone the loaded id BEFORE the mismatch check so the error branch can
        // put the engine back (mutable borrow) without a live immutable borrow
        // of `guard` — mirrors the live path's `taken_model_id` clone (E0502).
        let taken_model_id = guard.model_id.clone();
        if !expected_model.is_empty() {
            if let Some(loaded) = taken_model_id.as_deref() {
                if loaded != expected_model {
                    guard.engine = Some(engine);
                    return Err(anyhow::anyhow!(
                        "Loaded translator model '{}' does not match the expected \
                         model '{}' (it changed mid-job) — please retry.",
                        loaded,
                        expected_model
                    ));
                }
            }
        }
        let taken_instance = guard.instance;
        drop(guard);

        // Pad trailing silence for engines that drop final tokens on abrupt
        // audio ends (Whisper untouched) — identical to the live path.
        let audio = match &engine {
            LoadedEngine::Whisper(_) => audio,
            _ => pad_trailing_silence(audio, TRAILING_SILENCE_PAD_SAMPLES),
        };

        let transcribe_result = catch_unwind(AssertUnwindSafe(|| {
            Self::run_engine_inference(&mut engine, audio, &settings, effective_translate)
        }));

        let result = match transcribe_result {
            Ok(inner_result) => {
                // Put the engine back UNLESS a concurrent load/unload mutated
                // the slot (instance changed) — same instance-guard as the
                // live path (finding 8).
                let mut guard = self.lock_translator_engine();
                if guard.engine.is_none() && guard.instance == taken_instance {
                    guard.engine = Some(engine);
                } else {
                    info!(
                        "Translator slot changed during transcription; dropping the \
                         previous engine instead of restoring it"
                    );
                    drop(guard);
                    drop(engine);
                }
                inner_result?
            }
            Err(panic_payload) => {
                let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                error!("Translator transcription engine panicked: {}", panic_msg);
                // Clear the slot ONLY if our instance is still current.
                {
                    let mut state = self.lock_translator_engine();
                    if state.instance == taken_instance {
                        state.engine = None;
                        state.model_id = None;
                        state.instance += 1;
                    }
                }
                return Err(anyhow::anyhow!(
                    "Translator transcription engine panicked: {}. The model will \
                     reload on next attempt.",
                    panic_msg
                ));
            }
        };

        let corrected_result = if !settings.custom_words.is_empty() {
            apply_custom_words(
                &result.text,
                &settings.custom_words,
                settings.word_correction_threshold,
            )
        } else {
            result.text
        };
        let filtered_result = filter_transcription_output(&corrected_result);

        self.maybe_unload_translator_immediately();
        Ok(filtered_result)
    }
}

impl Drop for TranscriptionManager {
    fn drop(&mut self) {
        // T-307: only the ORIGINAL owner tears the shared idle watcher down. A
        // transient by-value clone (the watcher's own captured copy, or the
        // throwaway clone `initiate_model_load` moves into its background load
        // thread) must NOT signal shutdown or take/join the shared handle —
        // doing so killed the sole idle-unload watcher for the rest of the
        // session and could cross-thread-deadlock / self-join on the mutex.
        if !self.owns_watcher {
            return;
        }

        debug!("Shutting down TranscriptionManager");

        // Signal the watcher thread to shutdown
        self.shutdown_signal.store(true, Ordering::Relaxed);

        // Take the handle in its OWN scope so the mutex guard is dropped BEFORE
        // join() (Rust 2024 if-let temporary scoping would otherwise keep the
        // guard locked across the join). Tolerate a poisoned lock.
        let handle = self
            .watcher_handle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();

        // Wait for the thread to finish gracefully
        if let Some(handle) = handle {
            if let Err(e) = handle.join() {
                warn!("Failed to join idle watcher thread: {:?}", e);
            } else {
                debug!("Idle watcher thread joined successfully");
            }
        }
    }
}

#[cfg(test)]
mod progress_estimate_tests {
    use super::{MODEL_TIMINGS, eased_progress, expected_transcription_secs};
    use std::collections::VecDeque;

    #[test]
    fn eased_progress_follows_the_estimate_then_slows_down() {
        assert_eq!(eased_progress(0.0), 0.0);
        assert!((eased_progress(0.5) - 0.5).abs() < 1e-6);
        assert!((eased_progress(0.7) - 0.7).abs() < 1e-6);
        // Past the estimate it still moves, ever slower, and never reaches 100%.
        let (at_1, at_1_5, at_3) = (
            eased_progress(1.0),
            eased_progress(1.5),
            eased_progress(3.0),
        );
        assert!(0.85 < at_1 && at_1 < at_1_5 && at_1_5 < at_3 && at_3 < 1.0);
    }

    #[test]
    fn estimate_needs_timings_and_a_longer_clip_costs_more_per_second() {
        let id = "progress-estimate-test-model";
        assert!(expected_transcription_secs(id, 10.0).is_none());
        MODEL_TIMINGS
            .lock()
            .unwrap()
            .insert(id.into(), VecDeque::from(vec![(5.0, 0.7); 3]));
        let short = expected_transcription_secs(id, 5.0).unwrap();
        assert!((short - 0.7).abs() < 0.01);
        let long = expected_transcription_secs(id, 20.0).unwrap();
        assert!(long > 4.0 * short);
        MODEL_TIMINGS.lock().unwrap().remove(id);
    }

    #[test]
    fn a_speakers_request_not_timed_yet_borrows_the_plain_estimate() {
        let id = "progress-estimate-test-server";
        let speakers = format!("{id}+speakers");
        assert!(expected_transcription_secs(&speakers, 10.0).is_none());
        MODEL_TIMINGS
            .lock()
            .unwrap()
            .insert(id.into(), VecDeque::from(vec![(10.0, 1.0); 3]));
        let plain = expected_transcription_secs(id, 10.0).unwrap();
        let borrowed = expected_transcription_secs(&speakers, 10.0).unwrap();
        assert!((borrowed - plain * 1.5).abs() < 0.01);
        // Its own timings win once it has them.
        MODEL_TIMINGS
            .lock()
            .unwrap()
            .insert(speakers.clone(), VecDeque::from(vec![(10.0, 4.0); 3]));
        assert!((expected_transcription_secs(&speakers, 10.0).unwrap() - 4.0).abs() < 0.01);
        MODEL_TIMINGS.lock().unwrap().remove(id);
        MODEL_TIMINGS.lock().unwrap().remove(&speakers);
    }

    #[test]
    fn saved_timings_are_read_back_without_bad_or_short_entries() {
        let json = br#"{"m":[[5.0,0.7],[0.5,0.1],[6.0,-1.0]],"":[[5.0,1.0]],"e":[]}"#;
        let parsed = super::parse_timings(json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed["m"], VecDeque::from(vec![(5.0, 0.7)]));
        assert!(super::parse_timings(b"not json").is_none());
        let many = format!(
            "{{\"m\":[{}]}}",
            vec!["[5.0,1.0]"; super::TIMINGS_KEPT + 3].join(",")
        );
        assert_eq!(
            super::parse_timings(many.as_bytes()).unwrap()["m"].len(),
            super::TIMINGS_KEPT
        );
    }
}

#[cfg(test)]
mod gpu_device_setting_tests {
    use super::{
        resolve_effective_gpu_setting, should_retry_with_default_gpu_params,
        whisper_gpu_params_for_setting,
    };

    // T-212 acceptance: Auto, CPU forcing, and explicit nonzero device
    // selection all resolve to the correct whisper-rs load parameters.

    #[test]
    fn auto_sentinel_matches_default_gpu_behavior() {
        let params = whisper_gpu_params_for_setting(-1);
        assert!(params.use_gpu);
        assert_eq!(params.gpu_device, 0);
    }

    #[test]
    fn cpu_sentinel_forces_cpu() {
        let params = whisper_gpu_params_for_setting(-2);
        assert!(!params.use_gpu);
    }

    #[test]
    fn explicit_nonzero_device_is_threaded_through() {
        let params = whisper_gpu_params_for_setting(2);
        assert!(params.use_gpu);
        assert_eq!(params.gpu_device, 2);
    }

    #[test]
    fn explicit_device_zero_is_threaded_through() {
        let params = whisper_gpu_params_for_setting(0);
        assert!(params.use_gpu);
        assert_eq!(params.gpu_device, 0);
    }

    #[test]
    fn unrecognized_negative_value_falls_back_to_auto() {
        // A corrupt/future settings value outside the known sentinels (not
        // -1 or -2) must never be misread as "device -37" — it degrades to
        // Auto instead of panicking or erroring at load time.
        let params = whisper_gpu_params_for_setting(-37);
        assert!(params.use_gpu);
        assert_eq!(params.gpu_device, 0);
    }

    // Adversarial review finding 6: only an explicit GPU index (>= 0) is
    // eligible for the safety-net retry-with-Auto. -2 (force CPU) and -1
    // (Auto itself) must never retry with GPU-on default params.

    #[test]
    fn explicit_device_zero_is_retryable() {
        assert!(should_retry_with_default_gpu_params(0));
    }

    #[test]
    fn explicit_nonzero_device_is_retryable() {
        assert!(should_retry_with_default_gpu_params(5));
    }

    #[test]
    fn force_cpu_sentinel_is_not_retryable() {
        // The bug this guards against: WhisperModelParams::default() has
        // use_gpu: true, so retrying a failed force-CPU load with it would
        // silently turn "CPU Only" into "GPU".
        assert!(!should_retry_with_default_gpu_params(-2));
    }

    #[test]
    fn auto_sentinel_is_not_retryable() {
        // Auto failing has no softer GPU fallback to retry with — it IS
        // the default already.
        assert!(!should_retry_with_default_gpu_params(-1));
    }

    #[test]
    fn unrecognized_negative_value_is_not_retryable() {
        assert!(!should_retry_with_default_gpu_params(-99));
    }

    // Adversarial review finding 5: an explicit device index must be
    // validated against the live adapter list before load, and degrade to
    // Auto (not silently run on CPU) when it's out of range.

    #[test]
    fn explicit_device_present_in_list_passes_through_unchanged() {
        assert_eq!(resolve_effective_gpu_setting(1, &[0, 1, 2]), 1);
    }

    #[test]
    fn explicit_device_zero_present_in_list_passes_through_unchanged() {
        assert_eq!(resolve_effective_gpu_setting(0, &[0, 1]), 0);
    }

    #[test]
    fn explicit_device_missing_from_list_falls_back_to_auto() {
        assert_eq!(resolve_effective_gpu_setting(4, &[0, 1]), -1);
    }

    #[test]
    fn explicit_device_falls_back_to_auto_when_no_adapters_present() {
        // e.g. macOS (no Vulkan device registry) or a Vulkan backend that
        // reports zero devices.
        assert_eq!(resolve_effective_gpu_setting(0, &[]), -1);
    }

    #[test]
    fn auto_sentinel_is_never_validated_against_the_adapter_list() {
        // Auto doesn't name a specific adapter, so an empty/mismatched
        // device list must not perturb it.
        assert_eq!(resolve_effective_gpu_setting(-1, &[]), -1);
    }

    #[test]
    fn cpu_sentinel_is_never_validated_against_the_adapter_list() {
        assert_eq!(resolve_effective_gpu_setting(-2, &[]), -2);
    }
}
