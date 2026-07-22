use log::{debug, warn};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

pub const APPLE_INTELLIGENCE_PROVIDER_ID: &str = "apple_intelligence";
// Referenced only in the macOS-gated default provider entry below.
#[allow(dead_code)]
pub const APPLE_INTELLIGENCE_DEFAULT_MODEL_ID: &str = "Apple Intelligence";

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

// Custom deserializer to handle both old numeric format (1-5) and new string format ("trace", "debug", etc.)
impl<'de> Deserialize<'de> for LogLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LogLevelVisitor;

        impl<'de> Visitor<'de> for LogLevelVisitor {
            type Value = LogLevel;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or integer representing log level")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<LogLevel, E> {
                match value.to_lowercase().as_str() {
                    "trace" => Ok(LogLevel::Trace),
                    "debug" => Ok(LogLevel::Debug),
                    "info" => Ok(LogLevel::Info),
                    "warn" => Ok(LogLevel::Warn),
                    "error" => Ok(LogLevel::Error),
                    _ => Err(E::unknown_variant(
                        value,
                        &["trace", "debug", "info", "warn", "error"],
                    )),
                }
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<LogLevel, E> {
                match value {
                    1 => Ok(LogLevel::Trace),
                    2 => Ok(LogLevel::Debug),
                    3 => Ok(LogLevel::Info),
                    4 => Ok(LogLevel::Warn),
                    5 => Ok(LogLevel::Error),
                    _ => Err(E::invalid_value(de::Unexpected::Unsigned(value), &"1-5")),
                }
            }
        }

        deserializer.deserialize_any(LogLevelVisitor)
    }
}

impl From<LogLevel> for tauri_plugin_log::LogLevel {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Trace => tauri_plugin_log::LogLevel::Trace,
            LogLevel::Debug => tauri_plugin_log::LogLevel::Debug,
            LogLevel::Info => tauri_plugin_log::LogLevel::Info,
            LogLevel::Warn => tauri_plugin_log::LogLevel::Warn,
            LogLevel::Error => tauri_plugin_log::LogLevel::Error,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct ShortcutBinding {
    pub id: String,
    pub name: String,
    pub description: String,
    pub default_binding: String,
    pub current_binding: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct LLMPrompt {
    pub id: String,
    pub name: String,
    pub prompt: String,
}

/// An image attached to a saved model prompt, persisted as a base64 data URL so
/// it survives restarts as part of the prompt library.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct NamedImage {
    pub name: String,
    pub data_url: String,
}

/// A named, reusable text entry — a saved model or judge prompt for the
/// model-testing tool.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct NamedText {
    pub id: String,
    pub name: String,
    pub text: String,
    /// Optional attached image (model prompts only; judge prompts leave this
    /// `None`). Persisted so a saved prompt restores its picture.
    #[serde(default)]
    pub image: Option<NamedImage>,
}

/// A saved model-testing preset: a model prompt paired with a judge prompt.
/// Presets reference the saved prompts that make them up (so selecting a preset
/// also selects its parts in the prompt pickers); the raw-text fields are kept
/// as a fallback for pre-0.18 presets and when a referenced prompt is deleted.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct ModelTestPreset {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub model_prompt_id: Option<String>,
    #[serde(default)]
    pub judge_prompt_id: Option<String>,
    #[serde(default)]
    pub model_prompt: String,
    #[serde(default)]
    pub judge_prompt: String,
}

/// The model-testing prompt library: separately-saved model and judge prompts,
/// plus combined presets (a model prompt + judge prompt under one name).
#[derive(Serialize, Deserialize, Debug, Clone, Type, Default)]
pub struct ModelTestLibrary {
    #[serde(default)]
    pub model_prompts: Vec<NamedText>,
    #[serde(default)]
    pub judge_prompts: Vec<NamedText>,
    #[serde(default)]
    pub presets: Vec<ModelTestPreset>,
}

fn default_true() -> bool {
    true
}

/// A registered LLM provider. This single registry powers token counting, LLM
/// post-processing, and the model-testing tool. Post-processing and the tester
/// reference a provider by its stable `id` (surfaced in the UI as "#1", "#2",
/// …); editing a provider here updates everywhere that references it.
///
/// `kind` selects the API dialect and the token-counting strategy:
/// - `anthropic`          — Anthropic Messages API (+ dedicated count_tokens)
/// - `gemini`             — Google Gemini (+ dedicated countTokens)
/// - `openai_local`       — bundled tiktoken; token counting only (no chat)
/// - `openai_compatible`  — OpenAI `/chat/completions` (FLM, LM Studio, Ollama…)
/// - `openrouter`         — OpenAI-compatible **with real per-request cost**
/// - `apple_intelligence` — on-device Apple model (macOS ARM only)
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct LlmProvider {
    /// Stable unique id; never changes once assigned, even if `name` changes.
    pub id: String,
    pub kind: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// User-facing display label.
    pub name: String,
    #[serde(default)]
    pub base_url: String,
    /// Whether the UI lets the user edit `base_url` (false for `openai_local`).
    #[serde(default)]
    pub allow_base_url_edit: bool,
    #[serde(default)]
    pub api_key: String,
    pub model: String,
    /// Provider/model supports OpenAI-style JSON-schema structured output.
    #[serde(default)]
    pub supports_structured_output: bool,
    /// Cost in USD per 1,000,000 input tokens. Ignored when `reports_cost()`
    /// (OpenRouter returns the real charge) or for free/local providers.
    #[serde(default)]
    pub cost_input_per_million: f64,
    /// Cost in USD per 1,000,000 output tokens.
    #[serde(default)]
    pub cost_output_per_million: f64,
    /// Providers sharing a non-empty group run serially against each other when
    /// `sequential` is set (e.g. all FLM services share one model loader).
    #[serde(default)]
    pub concurrency_group: String,
    /// When true, do not run concurrently with others in `concurrency_group`.
    #[serde(default)]
    pub sequential: bool,
    /// When true, the user-entered cost is frozen: the automatic OpenRouter
    /// price lookup on model change will not overwrite it.
    #[serde(default)]
    pub persist_price: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum OverlayPosition {
    None,
    Top,
    Bottom,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum ModelUnloadTimeout {
    Never,
    Immediately,
    Min2,
    Min5,
    Min10,
    Min15,
    Hour1,
    /// User-defined duration; the value lives in `model_unload_custom_seconds`.
    Custom,
    Sec5, // Debug mode only
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum PasteMethod {
    CtrlV,
    Direct,
    None,
    ShiftInsert,
    CtrlShiftV,
    ExternalScript,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardHandling {
    DontModify,
    CopyToClipboard,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum AutoSubmitKey {
    Enter,
    CtrlEnter,
    CmdEnter,
}

/// Extra wait before the original clipboard is restored after a paste.
/// Remote sessions (Citrix/RDP) fetch clipboard data on demand AFTER the paste
/// keystroke arrives; restoring too early hands them the pre-recording content.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardRestoreDelay {
    None,
    Ms250,
    Ms500,
    Ms1000,
    Ms2500,
    Ms5000,
}

impl Default for ClipboardRestoreDelay {
    fn default() -> Self {
        ClipboardRestoreDelay::None
    }
}

impl ClipboardRestoreDelay {
    /// Extra milliseconds added on top of the base ~50 ms settle time.
    pub fn to_ms(self) -> u64 {
        match self {
            ClipboardRestoreDelay::None => 0,
            ClipboardRestoreDelay::Ms250 => 250,
            ClipboardRestoreDelay::Ms500 => 500,
            ClipboardRestoreDelay::Ms1000 => 1000,
            ClipboardRestoreDelay::Ms2500 => 2500,
            ClipboardRestoreDelay::Ms5000 => 5000,
        }
    }
}

/// Extra delay inserted BEFORE the auto-submit key (Enter) when a "Transcribe &
/// Submit" — or any anchored auto-submit — delivery had to JUMP the foreground
/// to reach its target, added on top of the fixed ~50 ms base. A freshly
/// activated window (especially an RDP/Citrix session) may still be committing
/// the pasted text when the base 50 ms Enter fires, so the submit is missed.
/// It applies ONLY on a real jump (target was not already foreground), so the
/// already-focused case keeps its current snappiness. `None` reproduces the
/// pre-0.53 behavior. Windows-only in effect (the Jumper is Windows-only).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum JumperSubmitDelay {
    None,
    Ms100,
    Ms250,
    Ms500,
    Ms1000,
    Ms2000,
}

impl Default for JumperSubmitDelay {
    fn default() -> Self {
        JumperSubmitDelay::Ms250
    }
}

impl JumperSubmitDelay {
    /// Extra milliseconds added on top of the base ~50 ms pre-submit settle.
    pub fn to_ms(self) -> u64 {
        match self {
            JumperSubmitDelay::None => 0,
            JumperSubmitDelay::Ms100 => 100,
            JumperSubmitDelay::Ms250 => 250,
            JumperSubmitDelay::Ms500 => 500,
            JumperSubmitDelay::Ms1000 => 1000,
            JumperSubmitDelay::Ms2000 => 2000,
        }
    }
}

/// Extra wait AFTER a jump activates the target window and BEFORE the paste
/// keystroke fires. `begin_delivery` already settles a fixed ~60 ms after
/// activation, but a freshly-activated window — especially an RDP/Citrix
/// session — may still be transitioning (completing activation / moving
/// focus) when the Ctrl+V lands, so the paste is swallowed or goes nowhere.
///
/// Like [`JumperSubmitDelay`], it applies ONLY on a real jump (target was not
/// already foreground), so the already-focused case keeps its snappiness.
/// `None` reproduces the pre-0.55 behavior. Windows-only in effect.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum JumperPasteDelay {
    None,
    Ms100,
    Ms250,
    Ms500,
    Ms1000,
    Ms2000,
}

impl Default for JumperPasteDelay {
    fn default() -> Self {
        JumperPasteDelay::Ms250
    }
}

impl JumperPasteDelay {
    /// Extra milliseconds waited on top of the fixed ~60 ms post-activation
    /// settle, before the paste keystroke, on a real jump.
    pub fn to_ms(self) -> u64 {
        match self {
            JumperPasteDelay::None => 0,
            JumperPasteDelay::Ms100 => 100,
            JumperPasteDelay::Ms250 => 250,
            JumperPasteDelay::Ms500 => 500,
            JumperPasteDelay::Ms1000 => 1000,
            JumperPasteDelay::Ms2000 => 2000,
        }
    }
}

/// Remote desktop sessions settle much slower than local windows, so the
/// remote-target delays default higher than the local ones.
fn default_jumper_submit_delay_remote() -> JumperSubmitDelay {
    JumperSubmitDelay::Ms500
}
fn default_jumper_paste_delay_remote() -> JumperPasteDelay {
    JumperPasteDelay::Ms1000
}

/// Default classifier substrings for remote-desktop jump targets. Chosen from
/// the real identities RDP/Citrix clients present (msrdc, mstsc, Citrix.*),
/// deliberately NOT matching local terminals/browsers. User-editable.
fn default_jumper_remote_match_strings() -> Vec<String> {
    vec![
        "msrdc".to_string(),
        "mstsc".to_string(),
        "Citrix".to_string(),
    ]
}

/// Classify a jump target as a REMOTE desktop session: true when ANY of the
/// user's `match_strings` appears (case-insensitively) as a substring of the
/// target's app name, top-level window class, or focused-control class. An
/// empty match list is never remote. Pure + Windows-agnostic so it's directly
/// unit-testable; the delivery path and the anchor-status builder both call it.
pub fn is_remote_target(
    app: &str,
    window_class: &str,
    control_class: &str,
    match_strings: &[String],
) -> bool {
    let app = app.to_ascii_lowercase();
    let window_class = window_class.to_ascii_lowercase();
    let control_class = control_class.to_ascii_lowercase();
    match_strings.iter().any(|raw| {
        let needle = raw.trim().to_ascii_lowercase();
        !needle.is_empty()
            && (app.contains(&needle)
                || window_class.contains(&needle)
                || control_class.contains(&needle))
    })
}

/// What a recording shortcut ADDITIONALLY does to the anchor when pressed —
/// configured separately for the idle press (nothing running) and the finish
/// press (a transcription is in progress), per flow. `None` (default) keeps
/// the anchor completely out of the flow.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum AnchorAction {
    /// Do nothing with the anchor.
    None,
    /// Idle press: jump to the anchor now. Finish press: deliver this
    /// transcription INTO the anchor (verified anchored delivery).
    Jump,
    /// Anchor the currently focused field.
    Set,
    /// Clear the anchor.
    Clear,
}

impl Default for AnchorAction {
    fn default() -> Self {
        AnchorAction::None
    }
}

/// What the "Transcribe & Submit" shortcut does when pressed with no active recording.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum SubmitIdleBehavior {
    /// Start a normal recording (mirrors the Transcribe shortcut on stop).
    StartNormal,
    /// Ignore the press — act only as a finisher for a recording already running.
    DoNothing,
    /// Start a recording that pastes + submits (Enter) when stopped.
    StartAndSubmit,
}

impl Default for SubmitIdleBehavior {
    fn default() -> Self {
        SubmitIdleBehavior::StartNormal
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum RecordingRetentionPeriod {
    Never,
    PreserveLimit,
    Days3,
    Weeks2,
    Months3,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionMode {
    Live,
    PostRecording,
}

/// UI appearance mode. `System` follows the OS `prefers-color-scheme` (and
/// keeps tracking it live); `Light` and `Dark` force one of Handy's two
/// palettes regardless of subsequent OS theme changes. Defaults to `System`
/// so existing installs are unaffected until the user opts into a forced mode.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    System,
    Light,
    Dark,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::System
    }
}

/// How the Translator's folder-batch work shares the (single-tenant) engine
/// with live dictation.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum TranslatorPriority {
    /// Live dictation preempts: batch pauses at the next segment boundary the
    /// moment a recording starts and resumes when the pipeline is idle again.
    LiveFirst,
    /// Batch keeps the engine while recording; live segment texts queue behind
    /// batch segments (delivery still completes, just later). Batch always
    /// yields during stop-processing so the final pass is never starved.
    FolderFirst,
    /// First come, first served: the file being transcribed finishes its
    /// segments, but the next queued file waits until live work is done.
    Fifo,
}

/// One watched folder for the Translator batch tool.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct TranslatorFolder {
    pub path: String,
    pub enabled: bool,
}

/// Persisted identity of a jump-slot target. Window handles are random per
/// boot, so what survives a restart is the DESCRIPTION of the target —
/// re-resolved against live windows when the app starts (and lazily on use).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct SavedJumpSlot {
    /// Executable stem ("chrome", "Citrix.DesktopViewer.App").
    pub app: String,
    /// Top-level window class.
    pub window_class: String,
    /// Focused-control class captured with the slot.
    pub control_class: String,
    /// Cursor position captured with the slot (T-301). `#[serde(default)]` so
    /// pre-0.48 stores (no cursor) deserialize to `None`.
    #[serde(default)]
    pub cursor: Option<SavedCursor>,
}

/// Coordinate mode for Jumper cursor restore (T-301).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
pub enum CursorMode {
    /// Restore to the same fraction of the anchor window's CLIENT area — same
    /// UI spot even after the window moves/resizes or changes monitor DPI.
    #[default]
    AppRelative,
    /// Restore to the same absolute (physical, virtual-screen) monitor pixel.
    ScreenAbsolute,
}

/// A cursor position captured alongside a jump-slot anchor (T-301). Stored in
/// BOTH representations so restore can pick the best available:
///  - `abs_x/abs_y`: absolute physical pixel in virtual-screen space
///    (ScreenAbsolute mode, and the fallback when the window can't be resolved).
///  - `norm_x/norm_y`: fraction (0..1) of the anchor window's CLIENT area at
///    capture (AppRelative) — resolution/DPI-independent, so it lands on the
///    same UI spot after the window moves or rescales. `None` if the client
///    rect was unavailable at capture.
///
/// NOTE (per Codex T-301 review): NO monitor-identity fields — a persisted
/// monitor RECT is not stable identity; restore instead clamps to the nearest
/// CURRENTLY-present monitor. All coords are PHYSICAL virtual-screen pixels
/// (requires verified Per-Monitor-V2 DPI awareness at restore time).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Type)]
pub struct SavedCursor {
    pub abs_x: i32,
    pub abs_y: i32,
    pub norm_x: Option<f64>,
    pub norm_y: Option<f64>,
    pub mode: CursorMode,
}

/// A frozen snapshot of the settings fields that affect HOW a transcription
/// job's audio is turned into text — model, language, translation, custom
/// words, and the custom-word fuzzy-match threshold — taken once and held
/// constant for the rest of a multi-segment job (T-108). Building this via
/// `AppSettings::transcription_policy_snapshot` and threading it through a
/// job's segments (instead of re-reading live settings per segment) prevents
/// a mid-job settings change from producing a sidecar assembled from
/// incompatible modes (e.g. half translated, half not).
///
/// `word_correction_threshold` (T-108 follow-up finding) governs the
/// Levenshtein/Soundex fuzzy match in `audio_toolkit::text::apply_custom_words`
/// and therefore affects EVERY segment's custom-word output exactly like
/// `custom_words` itself does — omitting it left a mid-job change to the
/// threshold (Advanced > Custom Words) able to make some segments of the same
/// job correct words the others didn't, the identical inconsistency this
/// snapshot exists to prevent for every other policy field.
///
/// This is the DATA half of T-108 only. Wiring `managers/translator.rs`'s
/// per-tick `get_settings` re-reads (and `TranscriptionManager::transcribe`'s
/// own internal settings re-read) to snapshot once at job start and hold this
/// value for every segment is cross-file work tracked as a follow-up in the
/// T-108 ticket — those files are owned by a different concurrent workstream.
// Not yet constructed outside this file's own tests (see follow-up above) —
// `#[allow(dead_code)]` prevents a dead-code warning until translator.rs is
// wired up to call `transcription_policy_snapshot`.
#[allow(dead_code)]
// `f64` doesn't implement `Eq` (NaN breaks reflexivity), so this can only
// derive `PartialEq`, not `Eq`, once `word_correction_threshold` joins the
// struct — `==` comparisons (used by the tests below) still work fine via
// `PartialEq`.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptionPolicySnapshot {
    pub model: String,
    pub language: String,
    pub translate_to_english: bool,
    pub custom_words: Vec<String>,
    pub word_correction_threshold: f64,
}

/// Which OpenRouter endpoint the OpenRouter transcription engine uses.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum OpenRouterTranscriptionRoute {
    /// Dedicated `/audio/transcriptions` JSON endpoint (Whisper-style models).
    Stt,
    /// Chat `/chat/completions` with an `input_audio` part (audio-capable LLMs).
    Chat,
}

impl Default for OpenRouterTranscriptionRoute {
    fn default() -> Self {
        OpenRouterTranscriptionRoute::Stt
    }
}

/// Audio container sent to remote (OpenRouter) transcription. Opus is ~10×
/// smaller than WAV — light on bandwidth — but support varies by model.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionAudioFormat {
    Wav,
    Opus,
}

impl Default for TranscriptionAudioFormat {
    fn default() -> Self {
        TranscriptionAudioFormat::Opus
    }
}

fn default_transcription_mode() -> TranscriptionMode {
    TranscriptionMode::PostRecording
}

fn default_transcription_mode_ptt() -> TranscriptionMode {
    TranscriptionMode::Live
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardImplementation {
    Tauri,
    HandyKeys,
}

impl Default for KeyboardImplementation {
    fn default() -> Self {
        // Default to HandyKeys only on macOS where it's well-tested.
        // Windows and Linux use Tauri by default (handy-keys not sufficiently tested yet).
        #[cfg(target_os = "macos")]
        return KeyboardImplementation::HandyKeys;
        #[cfg(not(target_os = "macos"))]
        return KeyboardImplementation::Tauri;
    }
}

impl Default for ModelUnloadTimeout {
    fn default() -> Self {
        ModelUnloadTimeout::Never
    }
}

impl Default for PasteMethod {
    fn default() -> Self {
        // Default to CtrlV for macOS and Windows, Direct for Linux
        #[cfg(target_os = "linux")]
        return PasteMethod::Direct;
        #[cfg(not(target_os = "linux"))]
        return PasteMethod::CtrlV;
    }
}

impl Default for ClipboardHandling {
    fn default() -> Self {
        ClipboardHandling::DontModify
    }
}

impl Default for AutoSubmitKey {
    fn default() -> Self {
        AutoSubmitKey::Enter
    }
}

impl ModelUnloadTimeout {
    pub fn to_minutes(self) -> Option<u64> {
        match self {
            ModelUnloadTimeout::Never => None,
            ModelUnloadTimeout::Immediately => Some(0), // Special case for immediate unloading
            ModelUnloadTimeout::Min2 => Some(2),
            ModelUnloadTimeout::Min5 => Some(5),
            ModelUnloadTimeout::Min10 => Some(10),
            ModelUnloadTimeout::Min15 => Some(15),
            ModelUnloadTimeout::Hour1 => Some(60),
            ModelUnloadTimeout::Custom => None, // duration lives in model_unload_custom_seconds
            ModelUnloadTimeout::Sec5 => Some(0), // Special case for debug - handled separately
        }
    }

    /// Idle seconds before unloading; `None` = never unload. `custom_seconds`
    /// supplies the user-defined duration for the `Custom` variant.
    pub fn to_seconds(self, custom_seconds: u64) -> Option<u64> {
        match self {
            ModelUnloadTimeout::Never => None,
            ModelUnloadTimeout::Immediately => Some(0), // Special case for immediate unloading
            ModelUnloadTimeout::Sec5 => Some(5),
            ModelUnloadTimeout::Custom => Some(custom_seconds.max(1)),
            _ => self.to_minutes().map(|m| m * 60),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum SoundTheme {
    Marimba,
    Pop,
    Custom,
}

impl SoundTheme {
    fn as_str(&self) -> &'static str {
        match self {
            SoundTheme::Marimba => "marimba",
            SoundTheme::Pop => "pop",
            SoundTheme::Custom => "custom",
        }
    }

    pub fn to_start_path(&self) -> String {
        format!("resources/{}_start.wav", self.as_str())
    }

    pub fn to_stop_path(&self) -> String {
        format!("resources/{}_stop.wav", self.as_str())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum TypingTool {
    Auto,
    Wtype,
    Kwtype,
    Dotool,
    Ydotool,
    Xdotool,
}

impl Default for TypingTool {
    fn default() -> Self {
        TypingTool::Auto
    }
}

/* still handy for composing the initial JSON in the store ------------- */
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct AppSettings {
    pub bindings: HashMap<String, ShortcutBinding>,
    /// VESTIGIAL — no backend logic reads this (PTT behavior is keyed purely off
    /// the `transcribe_ptt` binding id). Kept for settings-file compatibility.
    pub push_to_talk: bool,
    pub audio_feedback: bool,
    #[serde(default = "default_audio_feedback_volume")]
    pub audio_feedback_volume: f32,
    #[serde(default = "default_sound_theme")]
    pub sound_theme: SoundTheme,
    #[serde(default = "default_start_hidden")]
    pub start_hidden: bool,
    #[serde(default = "default_autostart_enabled")]
    pub autostart_enabled: bool,
    #[serde(default = "default_typing_start_delay_secs")]
    pub typing_start_delay_secs: u32,
    #[serde(default = "default_typing_key_delay_ms")]
    pub typing_key_delay_ms: u32,
    #[serde(default = "default_model")]
    pub selected_model: String,
    /// Vulkan GPU device selection for local Whisper transcription (T-212).
    /// Sentinel-encoded rather than a parallel accelerator enum: `-1`
    /// (default) = Auto (whisper.cpp's own default: GPU on, device 0 — the
    /// behavior before this setting existed), `-2` = force CPU, `>= 0` = an
    /// explicit Vulkan device index from `list_gpu_devices`. Applied at
    /// Whisper model load (`managers/transcription.rs`); an adapter that
    /// fails to init (disappeared, stale index, driver reordering) falls
    /// back to Auto rather than bricking transcription.
    #[serde(default = "default_transcribe_gpu_device")]
    pub transcribe_gpu_device: i32,
    #[serde(default = "default_always_on_microphone")]
    pub always_on_microphone: bool,
    #[serde(default)]
    pub selected_microphone: Option<String>,
    #[serde(default)]
    pub clamshell_microphone: Option<String>,
    #[serde(default)]
    pub selected_output_device: Option<String>,
    #[serde(default = "default_translate_to_english")]
    pub translate_to_english: bool,
    #[serde(default = "default_selected_language")]
    pub selected_language: String,
    #[serde(default = "default_overlay_position")]
    pub overlay_position: OverlayPosition,
    /// UI appearance: System (follows OS), Light, or Dark. Resolved and
    /// applied by the frontend (main window via React; the overlay/floating
    /// windows via a Rust-side push since they have no settings store — see
    /// `apply_theme_to_aux_windows` in lib.rs).
    #[serde(default)]
    pub app_theme: Theme,
    #[serde(default = "default_debug_mode")]
    pub debug_mode: bool,
    #[serde(default = "default_log_level")]
    pub log_level: LogLevel,
    #[serde(default)]
    pub custom_words: Vec<String>,
    #[serde(default)]
    pub model_unload_timeout: ModelUnloadTimeout,
    /// Idle duration in seconds for `ModelUnloadTimeout::Custom`.
    #[serde(default = "default_model_unload_custom_seconds")]
    pub model_unload_custom_seconds: u64,
    #[serde(default = "default_word_correction_threshold")]
    pub word_correction_threshold: f64,
    #[serde(default = "default_history_limit")]
    pub history_limit: usize,
    #[serde(default = "default_recording_retention_period")]
    pub recording_retention_period: RecordingRetentionPeriod,
    #[serde(default)]
    pub paste_method: PasteMethod,
    #[serde(default)]
    pub paste_method_ptt: PasteMethod,
    #[serde(default)]
    pub clipboard_handling: ClipboardHandling,
    #[serde(default = "default_auto_submit")]
    pub auto_submit: bool,
    #[serde(default)]
    pub auto_submit_key: AutoSubmitKey,
    #[serde(default = "default_post_process_enabled")]
    pub post_process_enabled: bool,
    /// Unified registry of LLM providers powering token counting,
    /// post-processing, and the model-testing tool. Aliased from the old
    /// `token_count_providers` key so existing configs (and their API keys)
    /// migrate automatically on first load.
    #[serde(default = "default_llm_providers", alias = "token_count_providers")]
    pub llm_providers: Vec<LlmProvider>,
    /// Stable id (into `llm_providers`) of the provider used for
    /// post-processing. Empty when not yet configured.
    #[serde(default)]
    pub post_process_provider_ref: String,
    /// Sampling temperature for post-processing (0.0 = most deterministic).
    #[serde(default = "default_post_process_temperature")]
    pub post_process_temperature: f32,
    #[serde(default = "default_post_process_prompts")]
    pub post_process_prompts: Vec<LLMPrompt>,
    #[serde(default = "default_selected_prompt_id")]
    pub post_process_selected_prompt_id: Option<String>,
    #[serde(default)]
    pub mute_while_recording: bool,
    #[serde(default)]
    pub append_trailing_space: bool,
    #[serde(default = "default_crash_resilient_recording")]
    pub crash_resilient_recording: bool,
    #[serde(default = "default_app_language")]
    pub app_language: String,
    #[serde(default)]
    pub experimental_enabled: bool,
    #[serde(default)]
    pub keyboard_implementation: KeyboardImplementation,
    #[serde(default = "default_show_tray_icon")]
    pub show_tray_icon: bool,
    #[serde(default = "default_paste_delay_ms")]
    pub paste_delay_ms: u64,
    #[serde(default = "default_typing_tool")]
    pub typing_tool: TypingTool,
    pub external_script_path: Option<String>,
    #[serde(default = "default_transcription_mode")]
    pub transcription_mode: TranscriptionMode,
    #[serde(default = "default_transcription_mode_ptt")]
    pub transcription_mode_ptt: TranscriptionMode,
    #[serde(default)]
    pub api_transcription_url: String,
    #[serde(default)]
    pub api_transcription_key: String,
    #[serde(default)]
    pub api_transcription_model: String,
    /// T-308: per-provider (LOCAL) language + translate for the OpenAI-compatible
    /// API transcription engine. Global `selected_language`/`translate_to_english`
    /// apply ONLY to engines that can't be custom-configured; these override for
    /// `ApiWhisper`. "auto" behaves as today (normalized, then "en" for the API).
    #[serde(default = "default_selected_language")]
    pub api_transcription_language: String,
    #[serde(default)]
    pub api_transcription_translate_to_english: bool,
    #[serde(default)]
    pub post_process_disable_thinking: bool,
    /// DEPRECATED (T-308): legacy `llm_providers` id that used to supply the
    /// OpenRouter transcription base URL + API key. Kept ONLY as a one-time
    /// migration source into the dedicated fields below; `skip_serializing` so it
    /// is dropped from the store after migration. No production read remains.
    #[serde(default, skip_serializing)]
    pub openrouter_transcription_provider_ref: String,
    /// T-308: dedicated OpenRouter transcription endpoint, independent of the
    /// `llm_providers` registry.
    #[serde(default = "default_openrouter_transcription_url")]
    pub openrouter_transcription_url: String,
    #[serde(default)]
    pub openrouter_transcription_key: String,
    /// Transcription model id (e.g. `openai/whisper-large-v3` or
    /// `google/gemini-2.5-flash`).
    #[serde(default = "default_openrouter_transcription_model")]
    pub openrouter_transcription_model: String,
    #[serde(default)]
    pub openrouter_transcription_route: OpenRouterTranscriptionRoute,
    #[serde(default)]
    pub openrouter_transcription_audio_format: TranscriptionAudioFormat,
    /// T-308: per-provider (LOCAL) language + translate for OpenRouter.
    #[serde(default = "default_selected_language")]
    pub openrouter_transcription_language: String,
    #[serde(default)]
    pub openrouter_transcription_translate_to_english: bool,
    /// T-308 one-time migration guard (see `ensure_custom_asr_config`). Absent in
    /// pre-0.54 stores → `false` → migration runs once; fresh installs get `true`.
    #[serde(default)]
    pub custom_asr_config_migrated: bool,
    /// Saved prompts + presets for the model-testing tool.
    #[serde(default)]
    pub model_test_library: ModelTestLibrary,
    /// Expose a localhost MCP + CLI server so the app can be driven by Claude
    /// (app/Code) and the `handy` CLI.
    #[serde(default)]
    pub mcp_server_enabled: bool,
    /// Port the localhost MCP/CLI server binds to (127.0.0.1 only).
    #[serde(default = "default_mcp_port")]
    pub mcp_server_port: u16,
    /// Bearer token guarding the MCP/CLI server. Generated on first enable.
    #[serde(default)]
    pub mcp_server_token: String,
    /// Paste method used by the "Transcribe & Submit" shortcut. Restricted in the
    /// UI to `CtrlV` or `ShiftInsert`.
    #[serde(default = "default_submit_paste_method")]
    pub submit_paste_method: PasteMethod,
    /// Key the "Transcribe & Submit" shortcut always sends after pasting
    /// (`Enter` / `CtrlEnter` / `CmdEnter`), independent of the global `auto_submit`.
    #[serde(default)]
    pub submit_key: AutoSubmitKey,
    /// What the "Transcribe & Submit" shortcut does when pressed with no active
    /// recording.
    #[serde(default)]
    pub submit_idle_behavior: SubmitIdleBehavior,
    /// Clipboard handling for pastes made by the "Transcribe & Submit" shortcut
    /// (independent of the global `clipboard_handling`).
    #[serde(default)]
    pub submit_clipboard_handling: ClipboardHandling,
    /// Extra wait before restoring the original clipboard after a normal paste.
    #[serde(default)]
    pub clipboard_restore_delay: ClipboardRestoreDelay,
    /// Extra wait before restoring the original clipboard after a
    /// "Transcribe & Submit" paste.
    #[serde(default)]
    pub submit_clipboard_restore_delay: ClipboardRestoreDelay,
    /// Extra pre-submit delay when an anchored auto-submit delivery jumped the
    /// foreground (Windows Jumper). See [`JumperSubmitDelay`]. Default `Ms250`.
    #[serde(default)]
    pub jumper_submit_delay: JumperSubmitDelay,
    /// Extra post-jump, pre-PASTE settle when an anchored delivery jumped the
    /// foreground (Windows Jumper) to a NON-remote (local) target. See
    /// [`JumperPasteDelay`]. Default `Ms250`.
    #[serde(default)]
    pub jumper_paste_delay: JumperPasteDelay,
    /// Same as [`jumper_submit_delay`], but used when the jump target is
    /// classified as a REMOTE desktop session (see `jumper_remote_match_strings`).
    /// Remote sessions (RDP/Citrix) settle much slower, so this defaults higher.
    #[serde(default = "default_jumper_submit_delay_remote")]
    pub jumper_submit_delay_remote: JumperSubmitDelay,
    /// Same as [`jumper_paste_delay`], but used when the jump target is
    /// classified as a REMOTE desktop session. Defaults higher.
    #[serde(default = "default_jumper_paste_delay_remote")]
    pub jumper_paste_delay_remote: JumperPasteDelay,
    /// Case-insensitive substrings that classify a jump target as a REMOTE
    /// desktop session (matched against the target's app/window-class/control-
    /// class). When a jump target matches, the `*_remote` delays are used
    /// instead of the local ones. Seeded with common RDP/Citrix identifiers;
    /// fully user-editable (may be cleared). Windows-only in effect.
    #[serde(default = "default_jumper_remote_match_strings")]
    pub jumper_remote_match_strings: Vec<String>,
    /// Return focus after an anchored delivery, per finishing flow. The
    /// starting location is captured automatically every time a delivery
    /// begins (an internal, invisible slot) — no user slot is involved.
    #[serde(default = "default_return_focus")]
    pub return_focus_output: bool,
    #[serde(default = "default_return_focus")]
    pub return_focus_submit: bool,
    /// Deprecated (pre-0.40 per-slot return-focus): read once to seed the
    /// per-flow fields above, never written back. Anchors are now ALWAYS
    /// kept after delivery — the old keep/one-shot options are gone.
    #[serde(default = "default_return_focus", skip_serializing)]
    pub anchor_return_focus: bool,
    /// Persist jump-slot targets across restarts (opt-in, applies to ALL
    /// slots). Handles can't survive a reboot, so the saved identity is
    /// re-resolved against live windows; unresolved slots show red until
    /// their app reappears.
    #[serde(default)]
    pub jumper_persist: bool,
    /// Saved identities, index = slot (only used when `jumper_persist`).
    #[serde(default = "default_jumper_saved_slots")]
    pub jumper_saved_slots: Vec<Option<SavedJumpSlot>>,
    /// Anchor action for the typical-output shortcuts on an idle press.
    #[serde(default)]
    pub anchor_action_output_idle: AnchorAction,
    /// Anchor action for the typical-output shortcuts on the finish press.
    #[serde(default)]
    pub anchor_action_output_stop: AnchorAction,
    /// Anchor action for Transcribe & Submit on an idle press.
    #[serde(default)]
    pub anchor_action_submit_idle: AnchorAction,
    /// Anchor action for Transcribe & Submit on the finish press.
    #[serde(default)]
    pub anchor_action_submit_stop: AnchorAction,
    /// Which jump slot each event action targets (0 = hot, 1–4 = static).
    #[serde(default)]
    pub anchor_action_output_idle_slot: u8,
    #[serde(default)]
    pub anchor_action_output_stop_slot: u8,
    #[serde(default)]
    pub anchor_action_submit_idle_slot: u8,
    #[serde(default)]
    pub anchor_action_submit_stop_slot: u8,
    /// Deprecated (0.40–0.45): a single global track-last-output switch. Kept
    /// only as the migration source for the per-flow fields below; no longer
    /// read by the delivery logic. See `jumper_track_output_enabled` /
    /// `jumper_track_submit_enabled`.
    #[serde(default)]
    pub jumper_track_enabled: bool,
    #[serde(default)]
    pub jumper_track_slot: u8,
    /// Deprecated (pre-0.40 per-flow track toggles): read once to seed the
    /// 0.40 global switch, never written back.
    #[serde(default, skip_serializing)]
    pub jumper_track_output: bool,
    #[serde(default, skip_serializing)]
    pub jumper_track_submit: bool,
    /// Track-last-output, PER FLOW and INDEPENDENT (0.46+). When on, the chosen
    /// slot auto-captures where the text landed after every paste of that flow
    /// (before any focus return). The dictate/"Transcribe" flow and the
    /// "Transcribe & Submit" flow each have their own switch + slot — mirroring
    /// `return_focus_output` / `return_focus_submit`. Default off.
    #[serde(default)]
    pub jumper_track_output_enabled: bool,
    /// Slot (0 = hot, 1–4 = static) the dictate flow tracks into.
    #[serde(default)]
    pub jumper_track_output_slot: u8,
    #[serde(default)]
    pub jumper_track_submit_enabled: bool,
    /// Slot the Transcribe & Submit flow tracks into.
    #[serde(default)]
    pub jumper_track_submit_slot: u8,
    /// One-time migration marker for the 0.40 Jumper settings rework.
    #[serde(default)]
    pub jumper_v2_migrated: bool,
    /// One-time migration marker for the 0.46 per-flow track-output split.
    #[serde(default)]
    pub jumper_v3_migrated: bool,
    /// One-time migration marker for T-305 (statics 4→9, Hot 2 moved 5→10).
    #[serde(default)]
    pub jumper_v4_migrated: bool,
    /// Save & restore the mouse cursor on a jump, PER SLOT (T-302, 0.49+).
    /// Index = slot (0 = Hot 1, 1-4 = static, 5 = Hot 2). Length SLOT_COUNT,
    /// normalized in `ensure_jumper_v2`. When a slot's flag is on, the cursor is captured
    /// with that slot's anchor and restored after a jump activates the window
    /// (and, for delivery, AFTER the paste). Replaces the 0.48 per-flow toggles.
    /// Default all-off.
    #[serde(default = "default_jumper_save_cursor_slots")]
    pub jumper_save_cursor_slots: Vec<bool>,
    /// LEGACY (pre-0.51): the single shared cursor mode. Retained ONLY as the
    /// one-time migration seed for `jumper_cursor_mode_slots` (see
    /// `ensure_jumper_v2`). Not written after upgrade and not read outside the
    /// migration; the per-slot vector below is the live source of truth.
    #[serde(default)]
    pub jumper_cursor_mode: CursorMode,
    /// Coordinate mode for cursor restore, PER SLOT (T-304, 0.51+): AppRelative
    /// (same spot inside the app; default) or ScreenAbsolute (fixed monitor
    /// pixel). Index = slot (0 = Hot 1, 1-4 = static, 5 = Hot 2). Length
    /// SLOT_COUNT, seeded from the legacy `jumper_cursor_mode` and normalized in
    /// `ensure_jumper_v2`. Each slot's mode is stamped onto its `SavedCursor` at
    /// capture time (anchor.rs `capture_cursor`), resolving this by target slot.
    #[serde(default = "default_jumper_cursor_mode_slots")]
    pub jumper_cursor_mode_slots: Vec<CursorMode>,
    /// When true, an on-finish Jumper jump/anchor action fires ONLY if the
    /// take's finishing flow matches its starting flow — e.g. a recording
    /// started as plain Transcribe but finished via Transcribe & Submit will
    /// NOT run the submit flow's on-finish jump (the submit paste still
    /// happens; only the jump is gated). Default false (fire regardless). T-302.
    #[serde(default)]
    pub anchor_on_finish_require_same_flow: bool,
    /// Translator: watch folders and batch-transcribe new audio files into
    /// `.txt` sidecars using the currently selected engine.
    #[serde(default)]
    pub translator_enabled: bool,
    #[serde(default)]
    pub translator_folders: Vec<TranslatorFolder>,
    /// One-time flag: the default watch folder ({app_data}/recordings) is
    /// seeded on first Translator startup; never re-seeded after the user
    /// edits the list (removing every folder must stick).
    #[serde(default)]
    pub translator_seeded: bool,
    #[serde(default = "default_translator_priority")]
    pub translator_priority: TranslatorPriority,
    /// Model the Translator batch uses. Empty = same as dictation. Only ids
    /// from the transcription-model registry are accepted (every entry there
    /// is ASR-capable by construction — LLM chat providers are not offered).
    #[serde(default)]
    pub translator_model: String,
    /// Idle-unload policy for the Translator's DEDICATED engine slot (used when
    /// `translator_model` differs from the dictation model and is a local
    /// engine, so both stay resident in parallel). Independent of the main
    /// model's `model_unload_timeout`. Default Never.
    #[serde(default)]
    pub translator_model_unload_timeout: ModelUnloadTimeout,
    /// Idle seconds for `translator_model_unload_timeout == Custom`.
    #[serde(default = "default_model_unload_custom_seconds")]
    pub translator_model_unload_custom_seconds: u64,
    /// Folder scan interval in seconds (no UI; edit settings_store.json).
    #[serde(default = "default_translator_poll_secs")]
    pub translator_poll_secs: u64,
}

fn default_translator_priority() -> TranslatorPriority {
    TranslatorPriority::LiveFirst
}

fn default_translator_poll_secs() -> u64 {
    15
}

fn default_return_focus() -> bool {
    true
}

fn default_jumper_saved_slots() -> Vec<Option<SavedJumpSlot>> {
    vec![None; crate::anchor::SLOT_COUNT]
}

fn default_model_unload_custom_seconds() -> u64 {
    300
}

/// Per-slot save-cursor flags, one per jump slot (0=Hot 1, 1-4=static, 5=Hot 2). T-302/T-303.
fn default_jumper_save_cursor_slots() -> Vec<bool> {
    vec![false; crate::anchor::SLOT_COUNT]
}

/// Per-slot cursor-mode default (T-304). Deliberately EMPTY — unlike
/// `default_jumper_save_cursor_slots`'s length-SLOT_COUNT default — so
/// `ensure_jumper_v2` can distinguish an un-migrated store (empty/short) from a
/// present one and SEED every slot from the legacy global `jumper_cursor_mode`.
/// A length-SLOT_COUNT default here would mask the upgrade and silently reset a
/// ScreenAbsolute user back to AppRelative.
fn default_jumper_cursor_mode_slots() -> Vec<CursorMode> {
    Vec::new()
}

fn default_mcp_port() -> u16 {
    8765
}

fn default_submit_paste_method() -> PasteMethod {
    PasteMethod::CtrlV
}

fn default_model() -> String {
    "".to_string()
}

/// `-1` = Auto — see `transcribe_gpu_device` doc comment for the full
/// sentinel encoding.
fn default_transcribe_gpu_device() -> i32 {
    -1
}

fn default_always_on_microphone() -> bool {
    false
}

fn default_translate_to_english() -> bool {
    false
}

fn default_crash_resilient_recording() -> bool {
    true
}

fn default_start_hidden() -> bool {
    false
}

fn default_autostart_enabled() -> bool {
    false
}

fn default_typing_start_delay_secs() -> u32 {
    10
}

/// 15 ms between simulated keystrokes: reliable locally and over RDP while
/// still fast. Instant injection is known to drop characters in remote
/// sessions and some VM consoles.
fn default_typing_key_delay_ms() -> u32 {
    15
}

fn default_openrouter_transcription_url() -> String {
    "https://openrouter.ai/api/v1".to_string()
}

fn default_openrouter_transcription_model() -> String {
    "openai/whisper-large-v3".to_string()
}

fn default_selected_language() -> String {
    "auto".to_string()
}

fn default_overlay_position() -> OverlayPosition {
    #[cfg(target_os = "linux")]
    return OverlayPosition::None;
    #[cfg(not(target_os = "linux"))]
    return OverlayPosition::Bottom;
}

fn default_debug_mode() -> bool {
    false
}

fn default_log_level() -> LogLevel {
    LogLevel::Debug
}

fn default_word_correction_threshold() -> f64 {
    0.18
}

fn default_paste_delay_ms() -> u64 {
    60
}

fn default_auto_submit() -> bool {
    false
}

fn default_history_limit() -> usize {
    5
}

fn default_recording_retention_period() -> RecordingRetentionPeriod {
    RecordingRetentionPeriod::PreserveLimit
}

fn default_audio_feedback_volume() -> f32 {
    1.0
}

fn default_sound_theme() -> SoundTheme {
    SoundTheme::Marimba
}

fn default_post_process_enabled() -> bool {
    false
}

fn default_app_language() -> String {
    tauri_plugin_os::locale()
        .map(|l| l.replace('_', "-"))
        .unwrap_or_else(|| "en".to_string())
}

fn default_show_tray_icon() -> bool {
    true
}

/// Built-in post-processing prompt id (the 2-mode "structure / directive"
/// prompt). Kept stable so migrations can detect & re-select it.
pub const DEFAULT_PROMPT_ID: &str = "default_structure";

fn default_post_process_temperature() -> f32 {
    0.3
}

fn default_selected_prompt_id() -> Option<String> {
    Some(DEFAULT_PROMPT_ID.to_string())
}

/// The default LLM provider registry. The previously-separate token-count
/// slots are merged with three OpenRouter comparison seats (pre-filled — the
/// user just adds a key and picks models) and a free-form Custom slot. Local
/// families (FLM, LM Studio) are marked sequential so the model-testing tool
/// serializes them against their own kind (a single loader can't serve two
/// models at once) while still running in parallel with cloud providers.
pub fn default_llm_providers() -> Vec<LlmProvider> {
    let mut providers = vec![
        LlmProvider {
            id: "openai".to_string(),
            kind: "openai_local".to_string(),
            enabled: true,
            name: "OpenAI (local tiktoken)".to_string(),
            base_url: String::new(),
            allow_base_url_edit: false,
            api_key: String::new(),
            model: "o200k_base".to_string(),
            supports_structured_output: false,
            cost_input_per_million: 0.0,
            cost_output_per_million: 0.0,
            concurrency_group: String::new(),
            sequential: false,
            persist_price: false,
        },
        LlmProvider {
            id: "gemini".to_string(),
            kind: "gemini".to_string(),
            enabled: false,
            name: "Gemini".to_string(),
            base_url: "https://generativelanguage.googleapis.com".to_string(),
            allow_base_url_edit: true,
            api_key: String::new(),
            model: "gemini-2.0-flash".to_string(),
            supports_structured_output: true,
            cost_input_per_million: 0.10,
            cost_output_per_million: 0.40,
            concurrency_group: String::new(),
            sequential: false,
            persist_price: false,
        },
        LlmProvider {
            id: "anthropic".to_string(),
            kind: "anthropic".to_string(),
            enabled: false,
            name: "Anthropic".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            allow_base_url_edit: true,
            api_key: String::new(),
            model: "claude-haiku-4-5".to_string(),
            supports_structured_output: false,
            cost_input_per_million: 1.0,
            cost_output_per_million: 5.0,
            concurrency_group: String::new(),
            sequential: false,
            persist_price: false,
        },
        LlmProvider {
            id: "flm1".to_string(),
            kind: "openai_compatible".to_string(),
            enabled: true,
            name: "FLM service 1".to_string(),
            base_url: "http://127.0.0.1:52626/v1".to_string(),
            allow_base_url_edit: true,
            api_key: String::new(),
            model: "qwen3:0.6b".to_string(),
            supports_structured_output: false,
            cost_input_per_million: 0.0,
            cost_output_per_million: 0.0,
            concurrency_group: "flm".to_string(),
            sequential: true,
            persist_price: false,
        },
        LlmProvider {
            id: "flm2".to_string(),
            kind: "openai_compatible".to_string(),
            enabled: false,
            name: "FLM service 2".to_string(),
            base_url: "http://127.0.0.1:52625/v1".to_string(),
            allow_base_url_edit: true,
            api_key: String::new(),
            model: "llama3.2:1b".to_string(),
            supports_structured_output: false,
            cost_input_per_million: 0.0,
            cost_output_per_million: 0.0,
            concurrency_group: "flm".to_string(),
            sequential: true,
            persist_price: false,
        },
        LlmProvider {
            id: "lmstudio1".to_string(),
            kind: "openai_compatible".to_string(),
            enabled: true,
            name: "LM Studio 1".to_string(),
            base_url: "http://127.0.0.1:1234/v1".to_string(),
            allow_base_url_edit: true,
            api_key: String::new(),
            model: "timecapsulellm-v2-llama-1.2b".to_string(),
            supports_structured_output: false,
            cost_input_per_million: 0.0,
            cost_output_per_million: 0.0,
            concurrency_group: "lmstudio".to_string(),
            sequential: true,
            persist_price: false,
        },
        LlmProvider {
            id: "lmstudio2".to_string(),
            kind: "openai_compatible".to_string(),
            enabled: false,
            name: "LM Studio 2".to_string(),
            base_url: "http://127.0.0.1:1234/v1".to_string(),
            allow_base_url_edit: true,
            api_key: String::new(),
            model: "qwen3-14b".to_string(),
            supports_structured_output: false,
            cost_input_per_million: 0.0,
            cost_output_per_million: 0.0,
            concurrency_group: "lmstudio".to_string(),
            sequential: true,
            persist_price: false,
        },
        LlmProvider {
            id: "openrouter1".to_string(),
            kind: "openrouter".to_string(),
            enabled: false,
            name: "OpenRouter 1".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            allow_base_url_edit: true,
            api_key: String::new(),
            model: String::new(),
            supports_structured_output: true,
            cost_input_per_million: 0.0,
            cost_output_per_million: 0.0,
            concurrency_group: String::new(),
            sequential: false,
            persist_price: false,
        },
        LlmProvider {
            id: "openrouter2".to_string(),
            kind: "openrouter".to_string(),
            enabled: false,
            name: "OpenRouter 2".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            allow_base_url_edit: true,
            api_key: String::new(),
            model: String::new(),
            supports_structured_output: true,
            cost_input_per_million: 0.0,
            cost_output_per_million: 0.0,
            concurrency_group: String::new(),
            sequential: false,
            persist_price: false,
        },
        LlmProvider {
            id: "openrouter3".to_string(),
            kind: "openrouter".to_string(),
            enabled: false,
            name: "OpenRouter 3".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            allow_base_url_edit: true,
            api_key: String::new(),
            model: String::new(),
            supports_structured_output: true,
            cost_input_per_million: 0.0,
            cost_output_per_million: 0.0,
            concurrency_group: String::new(),
            sequential: false,
            persist_price: false,
        },
    ];

    // Apple Intelligence (macOS ARM64 only). Availability is checked lazily
    // when the user actually runs it (see actions.rs) to avoid an early-init
    // SIGABRT on some macOS betas.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        providers.push(LlmProvider {
            id: APPLE_INTELLIGENCE_PROVIDER_ID.to_string(),
            kind: "apple_intelligence".to_string(),
            enabled: false,
            name: "Apple Intelligence".to_string(),
            base_url: "apple-intelligence://local".to_string(),
            allow_base_url_edit: false,
            api_key: String::new(),
            model: APPLE_INTELLIGENCE_DEFAULT_MODEL_ID.to_string(),
            supports_structured_output: true,
            cost_input_per_million: 0.0,
            cost_output_per_million: 0.0,
            concurrency_group: "apple".to_string(),
            sequential: true,
            persist_price: false,
        });
    }

    // Custom OpenAI-compatible slot always comes last.
    providers.push(LlmProvider {
        id: "custom".to_string(),
        kind: "openai_compatible".to_string(),
        enabled: false,
        name: "Custom".to_string(),
        base_url: "http://localhost:11434/v1".to_string(),
        allow_base_url_edit: true,
        api_key: String::new(),
        model: String::new(),
        supports_structured_output: false,
        cost_input_per_million: 0.0,
        cost_output_per_million: 0.0,
        concurrency_group: String::new(),
        sequential: false,
        persist_price: false,
    });

    providers
}

fn default_post_process_prompts() -> Vec<LLMPrompt> {
    vec![LLMPrompt {
        id: DEFAULT_PROMPT_ID.to_string(),
        name: "Structure & Clean".to_string(),
        prompt: "You are a transcription post-processor. The user dictated text by voice. Return a clean, well-structured version of it WITHOUT changing their meaning or word choices.\n\nFIRST, check for a processing directive. If the transcript opens with wording like \"instructions\", \"processing\", \"processing info\", or \"processing instruction(s)\" followed by directions, treat that opening as INSTRUCTIONS to follow and treat the remainder as the text to process. Apply the instructions to the remainder and return the result. Do not echo the directive itself.\n\nOTHERWISE, apply DEFAULT STRUCTURING:\n1. Begin with a one- to two-sentence summary of the text in italics, under a short \"Summary\" heading.\n2. Then output the cleaned text, formatted to fit the content: free-flowing speech becomes clean paragraphs; a list, steps, or enumerated points become a numbered (1, 2, 3) or bulleted list.\n3. Preserve the user's wording. Fix only spelling, capitalization, punctuation, obvious transcription errors, and filler words (um, uh). Do NOT paraphrase, reorder, or rephrase.\n4. Keep the original language (if it was French, keep it in French).\n5. Where the transcription is likely wrong and you are NOT confident of the intended word (e.g. a garbled name or technical term), do not silently guess — flag it inline as !!! your-best-guess — confirm? so it stands out.\n\nReturn only the processed text (summary + body). No preamble, no explanation.\n\nThe transcript to process is delimited in the user message.".to_string(),
    }]
}

fn default_typing_tool() -> TypingTool {
    TypingTool::Auto
}

/// Bring saved settings up to date with the current defaults:
/// 1. ensure every default LLM provider exists and its *capability* fields
///    (kind, structured-output support, base-url editability, concurrency
///    family) match the defaults — without clobbering user data (name,
///    base_url, api_key, model, enabled, cost);
/// 2. ensure the built-in post-processing prompt exists and is selected,
///    migrating off the legacy "Improve Transcriptions" default.
/// One-time migration to the 0.40 Jumper settings model: the per-flow track
/// toggles collapse into one global switch (target slot = hot, where the old
/// toggles always captured), per-slot return-focus becomes per-flow (seeded
/// from the old hot-slot value, which is what both flow groups displayed),
/// and the keep/one-shot options disappear — anchors are always kept now.
fn ensure_jumper_v2(settings: &mut AppSettings) -> bool {
    let mut changed = false;
    if !settings.jumper_v2_migrated {
        settings.jumper_track_enabled =
            settings.jumper_track_output || settings.jumper_track_submit;
        settings.jumper_track_slot = 0;
        settings.return_focus_output = settings.anchor_return_focus;
        settings.return_focus_submit = settings.anchor_return_focus;
        settings.jumper_v2_migrated = true;
        changed = true;
    }
    // 0.46: split the single global track-output switch into independent
    // per-flow switches. Seed BOTH flows from the old global so existing
    // behavior is preserved on upgrade; the user can then toggle them
    // independently. Runs after the v2 block above so it reads the freshly
    // migrated global value.
    if !settings.jumper_v3_migrated {
        settings.jumper_track_output_enabled = settings.jumper_track_enabled;
        settings.jumper_track_submit_enabled = settings.jumper_track_enabled;
        settings.jumper_track_output_slot = settings.jumper_track_slot;
        settings.jumper_track_submit_slot = settings.jumper_track_slot;
        settings.jumper_v3_migrated = true;
        changed = true;
    }
    // Normalize corrupt tracked-slot indices (hand-edited store) to hot so the
    // UI dropdowns and the capture targets always agree.
    if settings.jumper_track_slot as usize >= crate::anchor::SLOT_COUNT {
        settings.jumper_track_slot = 0;
        changed = true;
    }
    if settings.jumper_track_output_slot as usize >= crate::anchor::SLOT_COUNT {
        settings.jumper_track_output_slot = 0;
        changed = true;
    }
    if settings.jumper_track_submit_slot as usize >= crate::anchor::SLOT_COUNT {
        settings.jumper_track_submit_slot = 0;
        changed = true;
    }
    // T-302: keep the per-slot save-cursor vector exactly SLOT_COUNT long so
    // indexing by slot is always in bounds (pad missing with false, truncate
    // any hand-edited overflow).
    if settings.jumper_save_cursor_slots.len() != crate::anchor::SLOT_COUNT {
        settings
            .jumper_save_cursor_slots
            .resize(crate::anchor::SLOT_COUNT, false);
        changed = true;
    }
    // T-304: per-slot cursor mode. Normalize to SLOT_COUNT AT LOAD, SEEDING any
    // newly added entries from the legacy global `jumper_cursor_mode`. A store
    // upgrading from the shared-mode era has no `jumper_cursor_mode_slots` (so
    // serde gives the empty default) → resize fills all SLOT_COUNT slots with
    // the user's prior global mode, preserving behavior. A hand-edited short vec
    // keeps its entries and pads the rest from the same seed. `CursorMode: Copy`,
    // so `resize` can take the seed value directly.
    if settings.jumper_cursor_mode_slots.len() != crate::anchor::SLOT_COUNT {
        let seed = settings.jumper_cursor_mode;
        settings
            .jumper_cursor_mode_slots
            .resize(crate::anchor::SLOT_COUNT, seed);
        changed = true;
    }
    // T-303: same for the persisted saved-slot identities — normalize to
    // SLOT_COUNT AT LOAD so a pre-0.50 store (len 5) gains an empty index-5
    // (Hot 2) before any restore reads it, not only lazily on the next write.
    if settings.jumper_saved_slots.len() != crate::anchor::SLOT_COUNT {
        settings
            .jumper_saved_slots
            .resize(crate::anchor::SLOT_COUNT, None);
        changed = true;
    }
    // T-305: statics grew 4→9 (SLOT_COUNT 6→11) and Hot 2 moved off index 5
    // (now Static 5) to the new top index HOT2=10. One-time, and AFTER the
    // resize blocks above so index HOT2 already exists: relocate Hot 2's
    // persisted data 5→10 across all three per-slot vecs, reset index 5 to a
    // fresh empty static (matching how 6–9 were padded above), and remap any
    // flow slot-index setting that still points at the old Hot 2 (==5) to the
    // new HOT2. Values 0–4 are untouched; no value >5 could exist since the
    // old SLOT_COUNT was 6.
    if !settings.jumper_v4_migrated {
        const OLD_HOT2: usize = 5;
        let new_hot2 = crate::anchor::HOT2;
        // `.take()` moves Hot 2's slot identity to index 10 and leaves index 5
        // as None; the two explicit resets below give the new Static 5 the same
        // cursor prefs the freshly-padded statics 6–9 received.
        settings.jumper_saved_slots[new_hot2] = settings.jumper_saved_slots[OLD_HOT2].take();
        settings.jumper_save_cursor_slots[new_hot2] = settings.jumper_save_cursor_slots[OLD_HOT2];
        settings.jumper_cursor_mode_slots[new_hot2] = settings.jumper_cursor_mode_slots[OLD_HOT2];
        settings.jumper_save_cursor_slots[OLD_HOT2] = false;
        settings.jumper_cursor_mode_slots[OLD_HOT2] = settings.jumper_cursor_mode;
        // Remap flow slot-index settings that targeted the old Hot 2.
        let (old, new) = (OLD_HOT2 as u8, new_hot2 as u8);
        for s in [
            &mut settings.anchor_action_output_idle_slot,
            &mut settings.anchor_action_output_stop_slot,
            &mut settings.anchor_action_submit_idle_slot,
            &mut settings.anchor_action_submit_stop_slot,
            &mut settings.jumper_track_output_slot,
            &mut settings.jumper_track_submit_slot,
            &mut settings.jumper_track_slot,
        ] {
            if *s == old {
                *s = new;
            }
        }
        settings.jumper_v4_migrated = true;
        changed = true;
    }
    changed
}

/// T-308 one-time migration: move OpenRouter transcription config OFF the
/// `llm_providers` registry into dedicated fields, and seed each configurable
/// engine's LOCAL language/translate from the (previously global) values. Reads
/// `openrouter_transcription_provider_ref` (a `skip_serializing` field), so it
/// MUST run before any store write can drop it. Idempotent via
/// `custom_asr_config_migrated`. Never mutates/deletes the referenced provider
/// (it may still back post-processing / model-testing). Returns true if changed.
fn ensure_custom_asr_config(settings: &mut AppSettings) -> bool {
    if settings.custom_asr_config_migrated {
        return false;
    }
    // Seed local language/translate from the global values (one-time).
    settings.api_transcription_language = settings.selected_language.clone();
    settings.api_transcription_translate_to_english = settings.translate_to_english;
    settings.openrouter_transcription_language = settings.selected_language.clone();

    // Resolve the legacy provider ref into the dedicated URL/key (copy regardless
    // of enabled state). Collect into owned values so the immutable borrow ends
    // before we mutate `settings`.
    let provider_ref = settings.openrouter_transcription_provider_ref.clone();
    let (legacy_url, legacy_key) = settings
        .llm_provider(&provider_ref)
        .map(|p| (p.base_url.clone(), p.api_key.clone()))
        .unwrap_or_default();
    if !legacy_url.is_empty() {
        settings.openrouter_transcription_url = legacy_url;
    } else if settings.openrouter_transcription_url.is_empty() {
        settings.openrouter_transcription_url = default_openrouter_transcription_url();
    }
    settings.openrouter_transcription_key = legacy_key;

    // Route governs OpenRouter translate: Chat can translate (inherit old global
    // preference); Stt has no translation control (force off).
    settings.openrouter_transcription_translate_to_english = matches!(
        settings.openrouter_transcription_route,
        OpenRouterTranscriptionRoute::Chat
    ) && settings.translate_to_english;

    if settings.openrouter_transcription_model.trim().is_empty() {
        settings.openrouter_transcription_model = default_openrouter_transcription_model();
    }

    settings.custom_asr_config_migrated = true;
    true
}

fn ensure_llm_defaults(settings: &mut AppSettings) -> bool {
    let mut changed = false;

    for default in default_llm_providers() {
        match settings
            .llm_providers
            .iter_mut()
            .find(|p| p.id == default.id)
        {
            Some(existing) => {
                if existing.kind != default.kind {
                    existing.kind = default.kind.clone();
                    changed = true;
                }
                if existing.supports_structured_output != default.supports_structured_output {
                    existing.supports_structured_output = default.supports_structured_output;
                    changed = true;
                }
                if existing.allow_base_url_edit != default.allow_base_url_edit {
                    existing.allow_base_url_edit = default.allow_base_url_edit;
                    changed = true;
                }
                // The concurrency family is app-defined; set it once when
                // migrating from the old token-count config (which had none).
                if existing.concurrency_group.is_empty() && !default.concurrency_group.is_empty() {
                    existing.concurrency_group = default.concurrency_group.clone();
                    existing.sequential = default.sequential;
                    changed = true;
                }
            }
            None => {
                settings.llm_providers.push(default);
                changed = true;
            }
        }
    }

    if !settings
        .post_process_prompts
        .iter()
        .any(|p| p.id == DEFAULT_PROMPT_ID)
    {
        settings
            .post_process_prompts
            .insert(0, default_post_process_prompts().remove(0));
        changed = true;
    }

    let needs_select = match &settings.post_process_selected_prompt_id {
        None => true,
        // Legacy default — replace with the new structuring prompt.
        Some(id) => id == "default_improve_transcriptions",
    };
    if needs_select {
        settings.post_process_selected_prompt_id = Some(DEFAULT_PROMPT_ID.to_string());
        changed = true;
    }

    changed
}

/// Normalize `selected_language` for use with transcription engines.
/// Returns `None` for "auto" (let the engine detect), otherwise returns the
/// ISO 639-1 code (mapping "zh-Hans"/"zh-Hant" to "zh").
pub fn normalize_language_for_engine(selected_language: &str) -> Option<String> {
    if selected_language == "auto" {
        None
    } else if selected_language == "zh-Hans" || selected_language == "zh-Hant" {
        Some("zh".to_string())
    } else {
        Some(selected_language.to_string())
    }
}

pub const SETTINGS_STORE_PATH: &str = "settings_store.json";

pub fn get_default_settings() -> AppSettings {
    #[cfg(target_os = "windows")]
    let default_shortcut = "ctrl+space";
    #[cfg(target_os = "macos")]
    let default_shortcut = "option+space";
    #[cfg(target_os = "linux")]
    let default_shortcut = "ctrl+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_shortcut = "alt+space";

    let mut bindings = HashMap::new();
    bindings.insert(
        "transcribe".to_string(),
        ShortcutBinding {
            id: "transcribe".to_string(),
            name: "Transcribe".to_string(),
            description: "Converts your speech into text.".to_string(),
            default_binding: default_shortcut.to_string(),
            current_binding: default_shortcut.to_string(),
        },
    );
    #[cfg(target_os = "windows")]
    let default_ptt_shortcut = "ctrl+alt+space";
    // NOT "option+alt+space": option IS alt on macOS, so that chord collapses to
    // "option+space" — identical to the transcribe default — and one of the two
    // bindings would nondeterministically fail to register every launch.
    #[cfg(target_os = "macos")]
    let default_ptt_shortcut = "ctrl+option+space";
    #[cfg(target_os = "linux")]
    let default_ptt_shortcut = "ctrl+alt+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_ptt_shortcut = "alt+ctrl+space";

    bindings.insert(
        "transcribe_ptt".to_string(),
        ShortcutBinding {
            id: "transcribe_ptt".to_string(),
            name: "Push-to-Talk".to_string(),
            description: "Hold to record, release to stop.".to_string(),
            default_binding: default_ptt_shortcut.to_string(),
            current_binding: default_ptt_shortcut.to_string(),
        },
    );

    #[cfg(target_os = "windows")]
    let default_post_process_shortcut = "ctrl+shift+space";
    #[cfg(target_os = "macos")]
    let default_post_process_shortcut = "option+shift+space";
    #[cfg(target_os = "linux")]
    let default_post_process_shortcut = "ctrl+shift+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_post_process_shortcut = "alt+shift+space";

    bindings.insert(
        "transcribe_with_post_process".to_string(),
        ShortcutBinding {
            id: "transcribe_with_post_process".to_string(),
            name: "Transcribe with Post-Processing".to_string(),
            description: "Converts your speech into text and applies AI post-processing."
                .to_string(),
            default_binding: default_post_process_shortcut.to_string(),
            current_binding: default_post_process_shortcut.to_string(),
        },
    );
    bindings.insert(
        "cancel".to_string(),
        ShortcutBinding {
            id: "cancel".to_string(),
            name: "Cancel".to_string(),
            description: "Cancels the current recording.".to_string(),
            default_binding: "escape".to_string(),
            current_binding: "escape".to_string(),
        },
    );

    #[cfg(target_os = "macos")]
    let default_type_text_shortcut = "cmd+option+t";
    #[cfg(not(target_os = "macos"))]
    let default_type_text_shortcut = "ctrl+alt+t";

    bindings.insert(
        "type_text".to_string(),
        ShortcutBinding {
            id: "type_text".to_string(),
            name: "Type Text".to_string(),
            description: "Types the Keyboard Typer text into the focused window.".to_string(),
            default_binding: default_type_text_shortcut.to_string(),
            current_binding: default_type_text_shortcut.to_string(),
        },
    );

    #[cfg(target_os = "macos")]
    let default_submit_shortcut = "cmd+option+s";
    #[cfg(not(target_os = "macos"))]
    let default_submit_shortcut = "ctrl+alt+s";

    // Anchor & Deliver (Windows-only feature; bindings are skipped at
    // registration time on other platforms). k/j are AltGr-safe on European
    // layouts (unlike 'a': AltGr+A types 'ą' on Polish).
    bindings.insert(
        "anchor_set".to_string(),
        ShortcutBinding {
            id: "anchor_set".to_string(),
            name: "Set Anchor".to_string(),
            description: "Anchors the focused text field as the delivery target.".to_string(),
            default_binding: "ctrl+alt+k".to_string(),
            current_binding: "ctrl+alt+k".to_string(),
        },
    );
    bindings.insert(
        "anchor_jump".to_string(),
        ShortcutBinding {
            id: "anchor_jump".to_string(),
            name: "Jump to Anchor".to_string(),
            description: "Brings the anchored window and field into focus.".to_string(),
            default_binding: "ctrl+alt+j".to_string(),
            current_binding: "ctrl+alt+j".to_string(),
        },
    );
    // Second hot anchor (Hot 2, T-303). h/g are AltGr-safe on European layouts.
    bindings.insert(
        "anchor_set_2".to_string(),
        ShortcutBinding {
            id: "anchor_set_2".to_string(),
            name: "Set Anchor 2".to_string(),
            description: "Anchors the focused text field as the second delivery target."
                .to_string(),
            default_binding: "ctrl+alt+h".to_string(),
            current_binding: "ctrl+alt+h".to_string(),
        },
    );
    bindings.insert(
        "anchor_jump_2".to_string(),
        ShortcutBinding {
            id: "anchor_jump_2".to_string(),
            name: "Jump to Anchor 2".to_string(),
            description: "Brings the second anchored window and field into focus.".to_string(),
            default_binding: "ctrl+alt+g".to_string(),
            current_binding: "ctrl+alt+g".to_string(),
        },
    );

    // Jumper static slots 1–9 (Windows-only feature; registration skipped
    // elsewhere). Digits avoid AltGr letter collisions on European layouts.
    // T-305 grew this from 4 to 9; ensure_default_bindings back-fills 5–9 on
    // existing stores. static N == slot index N (Hot 2 lives at index 10).
    for i in 1..=9u8 {
        let set_id = format!("jump_set_slot_{}", i);
        bindings.insert(
            set_id.clone(),
            ShortcutBinding {
                id: set_id,
                name: format!("Set Jump Slot {}", i),
                description: format!("Memorizes the focused field as jump slot {}.", i),
                default_binding: format!("ctrl+alt+shift+{}", i),
                current_binding: format!("ctrl+alt+shift+{}", i),
            },
        );
        let jump_id = format!("jump_slot_{}", i);
        bindings.insert(
            jump_id.clone(),
            ShortcutBinding {
                id: jump_id,
                name: format!("Jump to Slot {}", i),
                description: format!("Brings jump slot {}'s window and field into focus.", i),
                default_binding: format!("ctrl+alt+{}", i),
                current_binding: format!("ctrl+alt+{}", i),
            },
        );
    }

    bindings.insert(
        "transcribe_and_submit".to_string(),
        ShortcutBinding {
            id: "transcribe_and_submit".to_string(),
            name: "Transcribe & Submit".to_string(),
            description: "Finish recording, paste the transcription, then press the submit key."
                .to_string(),
            default_binding: default_submit_shortcut.to_string(),
            current_binding: default_submit_shortcut.to_string(),
        },
    );

    AppSettings {
        bindings,
        push_to_talk: true,
        audio_feedback: false,
        audio_feedback_volume: default_audio_feedback_volume(),
        sound_theme: default_sound_theme(),
        start_hidden: default_start_hidden(),
        autostart_enabled: default_autostart_enabled(),
        typing_start_delay_secs: default_typing_start_delay_secs(),
        typing_key_delay_ms: default_typing_key_delay_ms(),
        selected_model: "".to_string(),
        transcribe_gpu_device: default_transcribe_gpu_device(),
        always_on_microphone: false,
        selected_microphone: None,
        clamshell_microphone: None,
        selected_output_device: None,
        translate_to_english: false,
        selected_language: "auto".to_string(),
        overlay_position: default_overlay_position(),
        app_theme: Theme::default(),
        debug_mode: false,
        log_level: default_log_level(),
        custom_words: Vec::new(),
        model_unload_timeout: ModelUnloadTimeout::Never,
        model_unload_custom_seconds: default_model_unload_custom_seconds(),
        word_correction_threshold: default_word_correction_threshold(),
        history_limit: default_history_limit(),
        recording_retention_period: default_recording_retention_period(),
        paste_method: PasteMethod::default(),
        paste_method_ptt: PasteMethod::default(),
        clipboard_handling: ClipboardHandling::default(),
        auto_submit: default_auto_submit(),
        auto_submit_key: AutoSubmitKey::default(),
        post_process_enabled: default_post_process_enabled(),
        llm_providers: default_llm_providers(),
        post_process_provider_ref: String::new(),
        post_process_temperature: default_post_process_temperature(),
        post_process_prompts: default_post_process_prompts(),
        post_process_selected_prompt_id: default_selected_prompt_id(),
        mute_while_recording: false,
        append_trailing_space: false,
        crash_resilient_recording: default_crash_resilient_recording(),
        app_language: default_app_language(),
        experimental_enabled: false,
        keyboard_implementation: KeyboardImplementation::default(),
        show_tray_icon: default_show_tray_icon(),
        paste_delay_ms: default_paste_delay_ms(),
        typing_tool: default_typing_tool(),
        external_script_path: None,
        transcription_mode: default_transcription_mode(),
        transcription_mode_ptt: default_transcription_mode_ptt(),
        api_transcription_url: String::new(),
        api_transcription_key: String::new(),
        api_transcription_model: String::new(),
        api_transcription_language: default_selected_language(),
        api_transcription_translate_to_english: false,
        post_process_disable_thinking: false,
        openrouter_transcription_provider_ref: String::new(),
        openrouter_transcription_url: default_openrouter_transcription_url(),
        openrouter_transcription_key: String::new(),
        openrouter_transcription_model: default_openrouter_transcription_model(),
        openrouter_transcription_route: OpenRouterTranscriptionRoute::default(),
        openrouter_transcription_audio_format: TranscriptionAudioFormat::default(),
        openrouter_transcription_language: default_selected_language(),
        openrouter_transcription_translate_to_english: false,
        custom_asr_config_migrated: true,
        model_test_library: ModelTestLibrary::default(),
        mcp_server_enabled: false,
        mcp_server_port: default_mcp_port(),
        mcp_server_token: String::new(),
        submit_paste_method: default_submit_paste_method(),
        submit_key: AutoSubmitKey::default(),
        submit_idle_behavior: SubmitIdleBehavior::default(),
        submit_clipboard_handling: ClipboardHandling::default(),
        clipboard_restore_delay: ClipboardRestoreDelay::default(),
        submit_clipboard_restore_delay: ClipboardRestoreDelay::default(),
        jumper_submit_delay: JumperSubmitDelay::default(),
        jumper_paste_delay: JumperPasteDelay::default(),
        jumper_submit_delay_remote: default_jumper_submit_delay_remote(),
        jumper_paste_delay_remote: default_jumper_paste_delay_remote(),
        jumper_remote_match_strings: default_jumper_remote_match_strings(),
        return_focus_output: default_return_focus(),
        return_focus_submit: default_return_focus(),
        anchor_return_focus: default_return_focus(),
        jumper_persist: false,
        jumper_saved_slots: default_jumper_saved_slots(),
        anchor_action_output_idle: AnchorAction::default(),
        anchor_action_output_stop: AnchorAction::default(),
        anchor_action_submit_idle: AnchorAction::default(),
        anchor_action_submit_stop: AnchorAction::default(),
        anchor_action_output_idle_slot: 0,
        anchor_action_output_stop_slot: 0,
        anchor_action_submit_idle_slot: 0,
        anchor_action_submit_stop_slot: 0,
        jumper_track_enabled: false,
        jumper_track_slot: 0,
        jumper_track_output: false,
        jumper_track_submit: false,
        jumper_track_output_enabled: false,
        jumper_track_output_slot: 0,
        jumper_track_submit_enabled: false,
        jumper_track_submit_slot: 0,
        // Fresh installs are already on the current model — nothing to migrate.
        jumper_v2_migrated: true,
        jumper_v3_migrated: true,
        jumper_v4_migrated: true,
        jumper_save_cursor_slots: default_jumper_save_cursor_slots(),
        jumper_cursor_mode: CursorMode::AppRelative,
        // Fresh install: length-correct all-AppRelative (ensure_jumper_v2 also
        // normalizes, but keep this path self-consistent without relying on it).
        jumper_cursor_mode_slots: vec![CursorMode::AppRelative; crate::anchor::SLOT_COUNT],
        anchor_on_finish_require_same_flow: false,
        translator_enabled: false,
        translator_folders: Vec::new(),
        translator_seeded: false,
        translator_priority: default_translator_priority(),
        translator_model: String::new(),
        translator_model_unload_timeout: ModelUnloadTimeout::Never,
        translator_model_unload_custom_seconds: default_model_unload_custom_seconds(),
        translator_poll_secs: default_translator_poll_secs(),
    }
}

impl AppSettings {
    /// Look up a registered provider by its stable id.
    pub fn llm_provider(&self, id: &str) -> Option<&LlmProvider> {
        self.llm_providers.iter().find(|provider| provider.id == id)
    }

    /// The provider selected for post-processing, if configured and present.
    pub fn active_post_process_provider(&self) -> Option<&LlmProvider> {
        if self.post_process_provider_ref.is_empty() {
            return None;
        }
        self.llm_provider(&self.post_process_provider_ref)
    }

    /// Build a `TranscriptionPolicySnapshot` for a new job (T-108). Pass the
    /// already-resolved effective model id (e.g. Translator's per-job
    /// `translator_model`, validated against the model registry by the
    /// caller and falling back to live dictation's `selected_model`) as
    /// `model_override`; an empty/`None` override falls back to
    /// `selected_model` directly. Deliberately does NOT reach into the model
    /// registry itself — that dependency belongs to the caller, not
    /// settings.rs.
    // Not yet called outside this file's own tests — see the T-108 ticket
    // follow-up. Remove once translator.rs is wired up.
    #[allow(dead_code)]
    pub fn transcription_policy_snapshot(
        &self,
        model_override: Option<&str>,
    ) -> TranscriptionPolicySnapshot {
        let model = model_override
            .filter(|m| !m.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.selected_model.clone());
        TranscriptionPolicySnapshot {
            model,
            language: self.selected_language.clone(),
            translate_to_english: self.translate_to_english,
            custom_words: self.custom_words.clone(),
            word_correction_threshold: self.word_correction_threshold,
        }
    }
}

/// Insert any default bindings missing from saved settings (bindings added
/// in newer app versions, e.g. `type_text`), and migrate known-broken stored
/// defaults. Returns true if something changed and should be persisted.
fn ensure_default_bindings(settings: &mut AppSettings) -> bool {
    let mut updated = false;
    for (key, value) in get_default_settings().bindings {
        if !settings.bindings.contains_key(&key) {
            debug!("Adding missing binding: {}", key);
            settings.bindings.insert(key, value);
            updated = true;
        }
    }
    // One-time fixup: the old macOS PTT default "option+alt+space" parses
    // identically to the transcribe default "option+space" (option == alt on
    // macOS), so the two bindings collided and one silently failed to register
    // each launch. Replace it with the current per-platform default wherever it
    // is still stored. Custom user bindings are never touched.
    const COLLIDING_PTT: &str = "option+alt+space";
    if let Some(b) = settings.bindings.get_mut("transcribe_ptt") {
        if b.current_binding == COLLIDING_PTT || b.default_binding == COLLIDING_PTT {
            if let Some(fresh) = get_default_settings().bindings.get("transcribe_ptt") {
                if b.current_binding == COLLIDING_PTT {
                    b.current_binding = fresh.default_binding.clone();
                }
                b.default_binding = fresh.default_binding.clone();
                debug!("Migrated colliding PTT binding to '{}'", b.default_binding);
                updated = true;
            }
        }
    }
    updated
}

pub fn load_or_create_app_settings(app: &AppHandle) -> AppSettings {
    // Initialize store
    let store = app
        .store(crate::portable::settings_store_path(
            app,
            SETTINGS_STORE_PATH,
        ))
        .expect("Failed to initialize store");

    let mut settings = if let Some(settings_value) = store.get("settings") {
        // Parse the entire settings object
        match serde_json::from_value::<AppSettings>(settings_value) {
            Ok(settings) => {
                // Never Debug-dump the whole settings struct: it embeds LLM
                // provider API keys, the API-transcription key, and the MCP
                // bearer token, and dev builds write DEBUG logs to disk.
                debug!(
                    "Found existing settings ({} bindings, {} providers)",
                    settings.bindings.len(),
                    settings.llm_providers.len()
                );
                // Migrations/defaults run AFTER the match (below), so nothing
                // writes to the store before `ensure_custom_asr_config` reads
                // the `skip_serializing` provider_ref (T-308 finding 1).
                settings
            }
            Err(e) => {
                warn!("Failed to parse settings: {}", e);
                // Fall back to default settings if parsing fails
                let default_settings = get_default_settings();
                store.set("settings", serde_json::to_value(&default_settings).unwrap());
                default_settings
            }
        }
    } else {
        let default_settings = get_default_settings();
        store.set("settings", serde_json::to_value(&default_settings).unwrap());
        default_settings
    };

    // Migrations that read `skip_serializing` fields (provider_ref) MUST run
    // before ANY store write drops them — so run them all here (the Ok arm no
    // longer writes) and aggregate into a single write. asr-config first.
    let asr_updated = ensure_custom_asr_config(&mut settings);
    let bindings_updated = ensure_default_bindings(&mut settings);
    let jumper_updated = ensure_jumper_v2(&mut settings);
    let llm_updated = ensure_llm_defaults(&mut settings);
    if asr_updated || bindings_updated || jumper_updated || llm_updated {
        store.set("settings", serde_json::to_value(&settings).unwrap());
    }

    settings
}

pub fn get_settings(app: &AppHandle) -> AppSettings {
    let store = app
        .store(crate::portable::settings_store_path(
            app,
            SETTINGS_STORE_PATH,
        ))
        .expect("Failed to initialize store");

    let mut settings = if let Some(settings_value) = store.get("settings") {
        serde_json::from_value::<AppSettings>(settings_value).unwrap_or_else(|_| {
            let default_settings = get_default_settings();
            store.set("settings", serde_json::to_value(&default_settings).unwrap());
            default_settings
        })
    } else {
        let default_settings = get_default_settings();
        store.set("settings", serde_json::to_value(&default_settings).unwrap());
        default_settings
    };

    // MUST run before anything can write this struct back: the migration
    // sources are `skip_serializing` fields, so a write that happens before
    // the one-time migration would silently drop the old values.
    // T-308: FIRST — reads the skip_serializing provider_ref before any write.
    let asr_updated = ensure_custom_asr_config(&mut settings);
    let jumper_updated = ensure_jumper_v2(&mut settings);
    let bindings_updated = ensure_default_bindings(&mut settings);
    if ensure_llm_defaults(&mut settings) || bindings_updated || jumper_updated || asr_updated {
        store.set("settings", serde_json::to_value(&settings).unwrap());
    }

    settings
}

pub fn write_settings(app: &AppHandle, settings: AppSettings) {
    let store = app
        .store(crate::portable::settings_store_path(
            app,
            SETTINGS_STORE_PATH,
        ))
        .expect("Failed to initialize store");

    store.set("settings", serde_json::to_value(&settings).unwrap());
}

/// Process-wide lock serializing settings read-modify-write cycles started
/// through `update_settings` below (T-111). Every settings mutation in the
/// app today is a bare `let mut s = get_settings(app); s.x = y;
/// write_settings(app, s);` with no cross-thread coordination: two concurrent
/// mutators (e.g. a UI command and the Translator worker seeding its default
/// folder, or the MCP server and a shortcut handler) can each read the
/// pre-mutation struct and then write it back, and whichever writes second
/// silently discards the other's change — last-writer-wins on the whole
/// struct.
///
/// IMPORTANT — what this DOES and does NOT fix:
/// - It DOES serialize any callers that go through `update_settings`, so two
///   such callers mutating different fields concurrently both persist (see
///   the `synchronized_rmw_serializes_concurrent_field_mutations` test below,
///   which exercises the same read/mutate/write shape without needing a real
///   `AppHandle`).
/// - It does NOT retrofit the ~50 existing call sites elsewhere in the app
///   (`shortcut/mod.rs`, `commands/*.rs`, `anchor.rs`, `managers/translator.rs`,
///   etc.) that still do the bare pattern directly against `get_settings` /
///   `write_settings` — those files are owned by other concurrent
///   workstreams and are out of scope for this change. They are NOT
///   serialized against `update_settings` or against each other. Migrating
///   them to call `update_settings(app, |s| ...)` instead is mechanical and
///   tracked as the T-111 follow-up.
/// - `mcp/tools.rs` has its own separate `SETTINGS_LOCK` covering only MCP
///   mutators; it is a different lock from this one and the two do not
///   serialize against each other. Converging both onto this helper is part
///   of the same follow-up.
static SETTINGS_MUTATION_LOCK: Mutex<()> = Mutex::new(());

/// Generic read-modify-write-under-lock primitive. Pulled out of
/// `update_settings` so the serialization guarantee can be unit-tested with a
/// plain in-memory value instead of a real Tauri `AppHandle`/store (which
/// can't be constructed in a `cargo test` without launching an app). Returns
/// both the mutated value and whatever `mutate` returns, so callers that need
/// the resulting state back (like `update_settings`) don't have to re-read.
fn synchronized_rmw<T: Clone, R>(
    lock: &Mutex<()>,
    read: impl FnOnce() -> T,
    mutate: impl FnOnce(&mut T) -> R,
    write: impl FnOnce(T),
) -> (T, R) {
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut value = read();
    let result = mutate(&mut value);
    write(value.clone());
    (value, result)
}

/// Read-modify-write `AppSettings` under the process-wide lock above. Prefer
/// this over the bare `get_settings(app); s.field = x; write_settings(app,
/// s);` pattern for any new writer, and for existing writers being migrated
/// as part of the T-111 follow-up. Returns the settings as they were left
/// after `mutate` ran (and were persisted).
///
/// First production call sites (T-109, see `tickets/T-109-*.md` and
/// `tickets/T-111-*.md`): `commands/translator.rs`'s
/// `translator_set_folder_enabled`/`translator_remove_folder`, which used to
/// race each other's bare read-modify-write. The remaining ~50 call sites
/// elsewhere in the app are still on the bare pattern — migrating them stays
/// the T-111 follow-up.
pub fn update_settings(app: &AppHandle, mutate: impl FnOnce(&mut AppSettings)) -> AppSettings {
    let (settings, ()) = synchronized_rmw(
        &SETTINGS_MUTATION_LOCK,
        || get_settings(app),
        mutate,
        |settings| write_settings(app, settings),
    );
    settings
}

pub fn get_bindings(app: &AppHandle) -> HashMap<String, ShortcutBinding> {
    let settings = get_settings(app);

    settings.bindings
}

pub fn get_stored_binding(app: &AppHandle, id: &str) -> ShortcutBinding {
    let bindings = get_bindings(app);

    let binding = bindings.get(id).unwrap().clone();

    binding
}

pub fn get_history_limit(app: &AppHandle) -> usize {
    let settings = get_settings(app);
    settings.history_limit
}

pub fn get_recording_retention_period(app: &AppHandle) -> RecordingRetentionPeriod {
    let settings = get_settings(app);
    settings.recording_retention_period
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_colliding_macos_ptt_default() {
        // A stored colliding chord is replaced with the fresh per-platform default.
        let mut settings = get_default_settings();
        {
            let b = settings.bindings.get_mut("transcribe_ptt").unwrap();
            b.current_binding = "option+alt+space".to_string();
            b.default_binding = "option+alt+space".to_string();
        }
        assert!(ensure_default_bindings(&mut settings));
        let b = &settings.bindings["transcribe_ptt"];
        assert_ne!(b.current_binding, "option+alt+space");
        assert_eq!(b.current_binding, b.default_binding);

        // A custom user binding is never touched.
        let mut custom = get_default_settings();
        custom
            .bindings
            .get_mut("transcribe_ptt")
            .unwrap()
            .current_binding = "ctrl+alt+p".to_string();
        ensure_default_bindings(&mut custom);
        assert_eq!(
            custom.bindings["transcribe_ptt"].current_binding,
            "ctrl+alt+p"
        );
    }

    #[test]
    fn jumper_v4_relocates_hot2_from_index_5_to_10() {
        use crate::anchor::{HOT2, SLOT_COUNT};
        // Simulate a pre-T-305 store: old SLOT_COUNT was 6, Hot 2 lived at
        // index 5, and the v4 migration has not yet run.
        let mut s = get_default_settings();
        s.jumper_v4_migrated = false;
        let hot2_slot = SavedJumpSlot {
            app: "hot2-app".to_string(),
            window_class: "Hot2Win".to_string(),
            control_class: "Edit".to_string(),
            cursor: None,
        };
        s.jumper_saved_slots = vec![None, None, None, None, None, Some(hot2_slot.clone())];
        s.jumper_save_cursor_slots = vec![false, false, false, false, false, true];
        s.jumper_cursor_mode_slots = vec![
            CursorMode::AppRelative,
            CursorMode::AppRelative,
            CursorMode::AppRelative,
            CursorMode::AppRelative,
            CursorMode::AppRelative,
            CursorMode::ScreenAbsolute,
        ];
        // Flows pointing at old Hot 2 (5) must move; one pointing at a static (2)
        // must NOT.
        s.anchor_action_output_stop_slot = 5;
        s.anchor_action_submit_stop_slot = 5;
        s.jumper_track_output_slot = 5;
        s.anchor_action_output_idle_slot = 2;

        assert!(ensure_jumper_v2(&mut s));
        assert!(s.jumper_v4_migrated);

        // All per-slot vectors grew to the new SLOT_COUNT.
        assert_eq!(s.jumper_saved_slots.len(), SLOT_COUNT);
        assert_eq!(s.jumper_save_cursor_slots.len(), SLOT_COUNT);
        assert_eq!(s.jumper_cursor_mode_slots.len(), SLOT_COUNT);

        // Hot 2's identity + prefs moved 5 -> HOT2 (10).
        assert_eq!(s.jumper_saved_slots[HOT2], Some(hot2_slot));
        assert!(s.jumper_save_cursor_slots[HOT2]);
        assert_eq!(s.jumper_cursor_mode_slots[HOT2], CursorMode::ScreenAbsolute);

        // Old index 5 is now a fresh, empty Static 5.
        assert_eq!(s.jumper_saved_slots[5], None);
        assert!(!s.jumper_save_cursor_slots[5]);
        assert_eq!(s.jumper_cursor_mode_slots[5], CursorMode::AppRelative);

        // Flow slot indices that targeted old Hot 2 were remapped; the static
        // target (2) was left alone.
        assert_eq!(s.anchor_action_output_stop_slot as usize, HOT2);
        assert_eq!(s.anchor_action_submit_stop_slot as usize, HOT2);
        assert_eq!(s.jumper_track_output_slot as usize, HOT2);
        assert_eq!(s.anchor_action_output_idle_slot, 2);

        // Idempotent: a second pass (flag now true) changes nothing.
        let hot2_saved = s.jumper_saved_slots[HOT2].clone();
        let stop_slot = s.anchor_action_output_stop_slot;
        ensure_jumper_v2(&mut s);
        assert_eq!(s.jumper_saved_slots[HOT2], hot2_saved);
        assert_eq!(s.anchor_action_output_stop_slot, stop_slot);
    }

    #[test]
    fn model_unload_custom_maps_to_seconds() {
        assert_eq!(ModelUnloadTimeout::Custom.to_seconds(90), Some(90));
        assert_eq!(ModelUnloadTimeout::Custom.to_seconds(0), Some(1)); // clamped
        assert_eq!(ModelUnloadTimeout::Never.to_seconds(90), None);
        assert_eq!(ModelUnloadTimeout::Immediately.to_seconds(90), Some(0));
        assert_eq!(ModelUnloadTimeout::Min5.to_seconds(90), Some(300));
        assert_eq!(get_default_settings().model_unload_custom_seconds, 300);
    }

    #[test]
    fn clipboard_restore_delay_maps_to_ms() {
        assert_eq!(ClipboardRestoreDelay::None.to_ms(), 0);
        assert_eq!(ClipboardRestoreDelay::Ms250.to_ms(), 250);
        assert_eq!(ClipboardRestoreDelay::Ms500.to_ms(), 500);
        assert_eq!(ClipboardRestoreDelay::Ms1000.to_ms(), 1000);
        assert_eq!(ClipboardRestoreDelay::Ms2500.to_ms(), 2500);
        assert_eq!(ClipboardRestoreDelay::Ms5000.to_ms(), 5000);
        assert_eq!(
            get_default_settings().clipboard_restore_delay,
            ClipboardRestoreDelay::None
        );
    }

    #[test]
    fn jumper_submit_delay_maps_to_ms_and_defaults_to_250() {
        assert_eq!(JumperSubmitDelay::None.to_ms(), 0);
        assert_eq!(JumperSubmitDelay::Ms100.to_ms(), 100);
        assert_eq!(JumperSubmitDelay::Ms250.to_ms(), 250);
        assert_eq!(JumperSubmitDelay::Ms500.to_ms(), 500);
        assert_eq!(JumperSubmitDelay::Ms1000.to_ms(), 1000);
        assert_eq!(JumperSubmitDelay::Ms2000.to_ms(), 2000);
        // Ships enabled: a real jump-settle by default, not None.
        assert_eq!(JumperSubmitDelay::default(), JumperSubmitDelay::Ms250);
        assert_eq!(
            get_default_settings().jumper_submit_delay,
            JumperSubmitDelay::Ms250
        );
    }

    #[test]
    fn jumper_paste_delay_maps_to_ms_and_defaults_to_250() {
        assert_eq!(JumperPasteDelay::None.to_ms(), 0);
        assert_eq!(JumperPasteDelay::Ms100.to_ms(), 100);
        assert_eq!(JumperPasteDelay::Ms250.to_ms(), 250);
        assert_eq!(JumperPasteDelay::Ms500.to_ms(), 500);
        assert_eq!(JumperPasteDelay::Ms1000.to_ms(), 1000);
        assert_eq!(JumperPasteDelay::Ms2000.to_ms(), 2000);
        // Ships enabled: a real post-jump paste settle by default, not None.
        assert_eq!(JumperPasteDelay::default(), JumperPasteDelay::Ms250);
        assert_eq!(
            get_default_settings().jumper_paste_delay,
            JumperPasteDelay::Ms250
        );
    }

    #[test]
    fn remote_target_classifier_matches_rdp_citrix_not_local() {
        let seeds = default_jumper_remote_match_strings();
        // The user's real REMOTE anchors must classify as remote…
        assert!(is_remote_target(
            "msrdc",
            "TscShellContainerClass",
            "IHWindowClass",
            &seeds
        ));
        assert!(is_remote_target(
            "mstsc",
            "TscShellContainerClass",
            "IHWindowClass",
            &seeds
        ));
        assert!(is_remote_target(
            "Citrix.DesktopViewer.App",
            "WindowsForms10.Window.8.app.0.3553390_r3_ad1",
            "CtxICADisp",
            &seeds
        ));
        // …and the LOCAL ones must NOT.
        assert!(!is_remote_target(
            "WindowsTerminal",
            "CASCADIA_HOSTING_WINDOW_CLASS",
            "Windows.UI.Input.InputSite.WindowClass",
            &seeds
        ));
        assert!(!is_remote_target(
            "claude",
            "Chrome_WidgetWin_1",
            "Chrome_WidgetWin_1",
            &seeds
        ));
    }

    #[test]
    fn remote_target_classifier_is_case_insensitive_and_matches_any_field() {
        let seeds = vec!["ctxicadisp".to_string()]; // matches the control class only
        assert!(is_remote_target(
            "whatever",
            "whatever",
            "CtxICADisp",
            &seeds
        ));
        // Case-insensitive on the needle side too.
        assert!(is_remote_target(
            "MSRDC.exe",
            "x",
            "y",
            &["msrdc".to_string()]
        ));
    }

    #[test]
    fn remote_target_classifier_empty_or_blank_never_matches() {
        assert!(!is_remote_target("msrdc", "x", "y", &[]));
        // Blank/whitespace entries are ignored (never match everything).
        assert!(!is_remote_target("msrdc", "x", "y", &["   ".to_string()]));
    }

    #[test]
    fn default_remote_delays_are_longer_than_local() {
        let d = get_default_settings();
        assert_eq!(d.jumper_paste_delay_remote, JumperPasteDelay::Ms1000);
        assert_eq!(d.jumper_submit_delay_remote, JumperSubmitDelay::Ms500);
        assert!(d.jumper_paste_delay_remote.to_ms() > d.jumper_paste_delay.to_ms());
        assert!(d.jumper_submit_delay_remote.to_ms() > d.jumper_submit_delay.to_ms());
        assert_eq!(
            d.jumper_remote_match_strings,
            vec![
                "msrdc".to_string(),
                "mstsc".to_string(),
                "Citrix".to_string()
            ]
        );
    }

    #[test]
    fn default_settings_disable_auto_submit() {
        let settings = get_default_settings();
        assert!(!settings.auto_submit);
        assert_eq!(settings.auto_submit_key, AutoSubmitKey::Enter);
    }

    // --- T-111: settings RMW concurrency ---------------------------------

    /// Directly exercises the acceptance criterion "two concurrent mutations
    /// of different fields both persist" using `synchronized_rmw` against a
    /// plain in-memory struct (no `AppHandle` needed). Without the lock this
    /// is exactly the lost-update race T-111 describes: both threads read
    /// the zeroed struct, each bumps its own field, and whichever writes
    /// second clobbers the other's write of the *unrelated* field back to 0.
    #[test]
    fn synchronized_rmw_serializes_concurrent_field_mutations() {
        use std::sync::Arc;
        use std::thread;

        #[derive(Clone, Default)]
        struct FakeSettings {
            a: i32,
            b: i32,
        }

        static LOCK: Mutex<()> = Mutex::new(());
        let store = Arc::new(Mutex::new(FakeSettings::default()));
        const ITERS: i32 = 200;

        let store_a = Arc::clone(&store);
        let t_a = thread::spawn(move || {
            for _ in 0..ITERS {
                synchronized_rmw(
                    &LOCK,
                    || store_a.lock().unwrap().clone(),
                    |s: &mut FakeSettings| s.a += 1,
                    |s| *store_a.lock().unwrap() = s,
                );
            }
        });
        let store_b = Arc::clone(&store);
        let t_b = thread::spawn(move || {
            for _ in 0..ITERS {
                synchronized_rmw(
                    &LOCK,
                    || store_b.lock().unwrap().clone(),
                    |s: &mut FakeSettings| s.b += 1,
                    |s| *store_b.lock().unwrap() = s,
                );
            }
        });
        t_a.join().unwrap();
        t_b.join().unwrap();

        let final_state = store.lock().unwrap().clone();
        assert_eq!(final_state.a, ITERS, "field mutated on thread A was lost");
        assert_eq!(final_state.b, ITERS, "field mutated on thread B was lost");
    }

    #[test]
    fn synchronized_rmw_returns_mutated_value_and_closure_result() {
        static LOCK: Mutex<()> = Mutex::new(());
        let source = 41;
        let mut written = None;
        let (value, doubled) = synchronized_rmw(
            &LOCK,
            || source,
            |v: &mut i32| {
                *v += 1;
                *v * 2
            },
            |v| written = Some(v),
        );
        assert_eq!(value, 42);
        assert_eq!(doubled, 84);
        assert_eq!(written, Some(42));
    }

    // --- T-108: transcription policy snapshot -----------------------------

    #[test]
    fn transcription_policy_snapshot_uses_override_when_present() {
        let mut settings = get_default_settings();
        settings.selected_model = "whisper-base".to_string();
        settings.selected_language = "en".to_string();
        settings.translate_to_english = true;
        settings.custom_words = vec!["Handy".to_string()];
        settings.word_correction_threshold = 0.42;

        let snap = settings.transcription_policy_snapshot(Some("parakeet-tdt-0.6b-v3-int8"));
        assert_eq!(snap.model, "parakeet-tdt-0.6b-v3-int8");
        assert_eq!(snap.language, "en");
        assert!(snap.translate_to_english);
        assert_eq!(snap.custom_words, vec!["Handy".to_string()]);
        assert_eq!(snap.word_correction_threshold, 0.42);
    }

    /// T-108 follow-up: `word_correction_threshold` affects every segment's
    /// custom-word output exactly like `custom_words` does, so it must be
    /// part of the frozen snapshot too — this asserts it actually gets
    /// captured (a regression against the field being silently dropped from
    /// `transcription_policy_snapshot`'s constructor again).
    #[test]
    fn transcription_policy_snapshot_captures_word_correction_threshold() {
        let mut settings = get_default_settings();
        settings.word_correction_threshold = 0.75;
        let snap = settings.transcription_policy_snapshot(None);
        assert_eq!(snap.word_correction_threshold, 0.75);

        settings.word_correction_threshold = 0.1;
        // Frozen: a later mutation must not leak into the already-taken snapshot.
        assert_eq!(snap.word_correction_threshold, 0.75);
    }

    #[test]
    fn transcription_policy_snapshot_falls_back_to_selected_model() {
        let mut settings = get_default_settings();
        settings.selected_model = "whisper-base".to_string();

        // Empty override string (e.g. an unset `translator_model`) falls back,
        // same as no override at all.
        assert_eq!(
            settings.transcription_policy_snapshot(Some("")).model,
            "whisper-base"
        );
        assert_eq!(
            settings.transcription_policy_snapshot(None).model,
            "whisper-base"
        );
    }

    /// The whole point of T-108: once taken, a snapshot must not observe a
    /// settings change made after it (simulating a mid-job toggle of
    /// Translate-to-English while a multi-segment job is running).
    #[test]
    fn transcription_policy_snapshot_is_frozen_after_later_settings_mutation() {
        let mut settings = get_default_settings();
        settings.translate_to_english = false;
        settings.selected_language = "en".to_string();
        settings.word_correction_threshold = 0.3;

        let snap = settings.transcription_policy_snapshot(None);

        settings.translate_to_english = true;
        settings.selected_language = "fr".to_string();
        settings.word_correction_threshold = 0.9;

        assert!(!snap.translate_to_english);
        assert_eq!(snap.language, "en");
        assert_eq!(snap.word_correction_threshold, 0.3);
    }

    // --- T-212: GPU device selection --------------------------------------

    #[test]
    fn transcribe_gpu_device_defaults_to_auto() {
        assert_eq!(get_default_settings().transcribe_gpu_device, -1);
    }

    #[test]
    fn transcribe_gpu_device_round_trips_through_json() {
        let mut settings = get_default_settings();
        settings.transcribe_gpu_device = 3;
        let json = serde_json::to_string(&settings).unwrap();
        let restored: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.transcribe_gpu_device, 3);
    }

    #[test]
    fn transcribe_gpu_device_defaults_when_missing_from_stored_json() {
        // Migration safety: a settings file saved before T-212 has no
        // `transcribe_gpu_device` key at all. Deserializing it must fall back
        // to Auto (-1) via `#[serde(default = "default_transcribe_gpu_device")]`
        // rather than failing to load or silently zeroing (which would read
        // as "device 0" instead of Auto).
        let mut value = serde_json::to_value(get_default_settings()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("transcribe_gpu_device");
        let restored: AppSettings = serde_json::from_value(value).unwrap();
        assert_eq!(restored.transcribe_gpu_device, -1);
    }

    #[test]
    fn transcribe_gpu_device_persists_cpu_forcing_sentinel() {
        let mut settings = get_default_settings();
        settings.transcribe_gpu_device = -2;
        let json = serde_json::to_string(&settings).unwrap();
        let restored: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.transcribe_gpu_device, -2);
    }
}
