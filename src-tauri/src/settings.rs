use log::{debug, warn};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;
use std::collections::HashMap;
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
    #[serde(default)]
    pub post_process_disable_thinking: bool,
    /// Registry provider id (kind `openrouter`) supplying the base URL + API key
    /// for OpenRouter transcription.
    #[serde(default)]
    pub openrouter_transcription_provider_ref: String,
    /// Transcription model id (e.g. `openai/whisper-large-v3` or
    /// `google/gemini-2.5-flash`).
    #[serde(default)]
    pub openrouter_transcription_model: String,
    #[serde(default)]
    pub openrouter_transcription_route: OpenRouterTranscriptionRoute,
    #[serde(default)]
    pub openrouter_transcription_audio_format: TranscriptionAudioFormat,
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
    /// Hot slot: keep the anchor armed after a successful delivery
    /// (default one-shot). Static slots have their own per-slot flags below.
    #[serde(default)]
    pub anchor_keep: bool,
    /// Hot slot: return focus to the previous window after delivering.
    #[serde(default = "default_anchor_return_focus")]
    pub anchor_return_focus: bool,
    /// Static slots 1–4: keep the slot after a delivery into it (index 0 =
    /// slot 1). Default true — statics are durable bookmarks; turning one off
    /// makes that slot one-shot like the hot anchor.
    #[serde(default = "default_static_slot_keep")]
    pub jumper_slot_keep: Vec<bool>,
    /// Static slots 1–4: return focus after delivering into the slot.
    #[serde(default = "default_static_slot_return_focus")]
    pub jumper_slot_return_focus: Vec<bool>,
    /// Persist jump-slot targets across restarts (opt-in). Handles can't
    /// survive a reboot, so the saved identity is re-resolved against live
    /// windows; unresolved slots show red until their app reappears.
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
    /// Track-last-output: after this flow pastes, the HOT slot auto-captures
    /// where the text landed (before any focus return). Default off.
    #[serde(default)]
    pub jumper_track_output: bool,
    #[serde(default)]
    pub jumper_track_submit: bool,
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

fn default_anchor_return_focus() -> bool {
    true
}

fn default_static_slot_keep() -> Vec<bool> {
    vec![true; 4]
}

fn default_static_slot_return_focus() -> Vec<bool> {
    vec![true; 4]
}

fn default_jumper_saved_slots() -> Vec<Option<SavedJumpSlot>> {
    vec![None; 5]
}

fn default_model_unload_custom_seconds() -> u64 {
    300
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
        prompt: "You are a transcription post-processor. The user dictated text by voice. Return a clean, well-structured version of it WITHOUT changing their meaning or word choices.\n\nFIRST, check for a processing directive. If the transcript opens with wording like \"instructions\", \"processing\", \"processing info\", or \"processing instruction(s)\" followed by directions, treat that opening as INSTRUCTIONS to follow and treat the remainder as the text to process. Apply the instructions to the remainder and return the result. Do not echo the directive itself.\n\nOTHERWISE, apply DEFAULT STRUCTURING:\n1. Begin with a one- to two-sentence summary of the text in italics, under a short \"Summary\" heading.\n2. Then output the cleaned text, formatted to fit the content: free-flowing speech becomes clean paragraphs; a list, steps, or enumerated points become a numbered (1, 2, 3) or bulleted list.\n3. Preserve the user's wording. Fix only spelling, capitalization, punctuation, obvious transcription errors, and filler words (um, uh). Do NOT paraphrase, reorder, or rephrase.\n4. Keep the original language (if it was French, keep it in French).\n5. Where the transcription is likely wrong and you are NOT confident of the intended word (e.g. a garbled name or technical term), do not silently guess — flag it inline as !!! your-best-guess — confirm? so it stands out.\n\nReturn only the processed text (summary + body). No preamble, no explanation.\n\nTranscript:\n${output}".to_string(),
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

    // Jumper static slots 1–4 (Windows-only feature; registration skipped
    // elsewhere). Digits avoid AltGr letter collisions on European layouts.
    for i in 1..=4u8 {
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
        always_on_microphone: false,
        selected_microphone: None,
        clamshell_microphone: None,
        selected_output_device: None,
        translate_to_english: false,
        selected_language: "auto".to_string(),
        overlay_position: default_overlay_position(),
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
        post_process_disable_thinking: false,
        openrouter_transcription_provider_ref: String::new(),
        openrouter_transcription_model: String::new(),
        openrouter_transcription_route: OpenRouterTranscriptionRoute::default(),
        openrouter_transcription_audio_format: TranscriptionAudioFormat::default(),
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
        anchor_keep: false,
        anchor_return_focus: default_anchor_return_focus(),
        jumper_slot_keep: default_static_slot_keep(),
        jumper_slot_return_focus: default_static_slot_return_focus(),
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
        jumper_track_output: false,
        jumper_track_submit: false,
        translator_enabled: false,
        translator_folders: Vec::new(),
        translator_seeded: false,
        translator_priority: default_translator_priority(),
        translator_model: String::new(),
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
        .store(SETTINGS_STORE_PATH)
        .expect("Failed to initialize store");

    let mut settings = if let Some(settings_value) = store.get("settings") {
        // Parse the entire settings object
        match serde_json::from_value::<AppSettings>(settings_value) {
            Ok(mut settings) => {
                // Never Debug-dump the whole settings struct: it embeds LLM
                // provider API keys, the API-transcription key, and the MCP
                // bearer token, and dev builds write DEBUG logs to disk.
                debug!(
                    "Found existing settings ({} bindings, {} providers)",
                    settings.bindings.len(),
                    settings.llm_providers.len()
                );
                if ensure_default_bindings(&mut settings) {
                    debug!("Settings updated with new bindings");
                    store.set("settings", serde_json::to_value(&settings).unwrap());
                }

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

    if ensure_llm_defaults(&mut settings) {
        store.set("settings", serde_json::to_value(&settings).unwrap());
    }

    settings
}

pub fn get_settings(app: &AppHandle) -> AppSettings {
    let store = app
        .store(SETTINGS_STORE_PATH)
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

    let bindings_updated = ensure_default_bindings(&mut settings);
    if ensure_llm_defaults(&mut settings) || bindings_updated {
        store.set("settings", serde_json::to_value(&settings).unwrap());
    }

    settings
}

pub fn write_settings(app: &AppHandle, settings: AppSettings) {
    let store = app
        .store(SETTINGS_STORE_PATH)
        .expect("Failed to initialize store");

    store.set("settings", serde_json::to_value(&settings).unwrap());
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
    fn default_settings_disable_auto_submit() {
        let settings = get_default_settings();
        assert!(!settings.auto_submit);
        assert_eq!(settings.auto_submit_key, AutoSubmitKey::Enter);
    }
}
