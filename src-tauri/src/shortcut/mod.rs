//! Keyboard shortcut management module
//!
//! This module provides a unified interface for keyboard shortcuts with
//! multiple backend implementations:
//!
//! - `tauri`: Uses Tauri's built-in global-shortcut plugin
//! - `handy_keys`: Uses the handy-keys library for more control
//!
//! The active implementation is determined by the `keyboard_implementation`
//! setting and can be changed at runtime.

mod handler;
pub mod handy_keys;
mod tauri_impl;

use log::{error, info, warn};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;

use crate::settings::{
    self, AnchorAction, AutoSubmitKey, ClipboardHandling, ClipboardRestoreDelay, CursorMode,
    JumperPasteDelay, JumperSubmitDelay, KeyboardImplementation, LLMPrompt, ModelUnloadTimeout,
    OverlayPosition, PasteMethod, ShortcutBinding, SoundTheme, SubmitIdleBehavior, Theme,
    TranscriptionMode, TypingTool, get_settings,
};
use crate::tray;

// Note: Commands are accessed via shortcut::handy_keys:: in lib.rs

/// Initialize shortcuts using the configured implementation
pub fn init_shortcuts(app: &AppHandle) {
    let user_settings = settings::load_or_create_app_settings(app);

    // Check which implementation to use
    match user_settings.keyboard_implementation {
        KeyboardImplementation::Tauri => {
            tauri_impl::init_shortcuts(app);
        }
        KeyboardImplementation::HandyKeys => {
            if let Err(e) = handy_keys::init_shortcuts(app) {
                error!("Failed to initialize handy-keys shortcuts: {}", e);
                // Fall back to Tauri implementation and persist this fallback
                warn!(
                    "Falling back to Tauri global shortcut implementation and saving fallback to settings"
                );

                // Update settings to persist the fallback so we don't retry HandyKeys on next launch
                let mut settings = settings::get_settings(app);
                settings.keyboard_implementation = KeyboardImplementation::Tauri;
                settings::write_settings(app, settings);

                tauri_impl::init_shortcuts(app);
            }
        }
    }
}

/// Register the cancel shortcut (called when recording starts)
pub fn register_cancel_shortcut(app: &AppHandle) {
    let settings = get_settings(app);
    match settings.keyboard_implementation {
        KeyboardImplementation::Tauri => tauri_impl::register_cancel_shortcut(app),
        KeyboardImplementation::HandyKeys => handy_keys::register_cancel_shortcut(app),
    }
}

/// Unregister the cancel shortcut (called when recording stops)
pub fn unregister_cancel_shortcut(app: &AppHandle) {
    let settings = get_settings(app);
    match settings.keyboard_implementation {
        KeyboardImplementation::Tauri => tauri_impl::unregister_cancel_shortcut(app),
        KeyboardImplementation::HandyKeys => handy_keys::unregister_cancel_shortcut(app),
    }
}

/// Register a shortcut using the appropriate implementation
pub fn register_shortcut(app: &AppHandle, binding: ShortcutBinding) -> Result<(), String> {
    let settings = get_settings(app);
    let id = binding.id.clone();
    let current = binding.current_binding.clone();
    let result = match settings.keyboard_implementation {
        KeyboardImplementation::Tauri => tauri_impl::register_shortcut(app, binding),
        KeyboardImplementation::HandyKeys => handy_keys::register_shortcut(app, binding),
    };
    record_registration_result(&id, &current, &result);
    result
}

/// A shortcut that failed to register (startup or implementation switch). Kept
/// so the frontend can surface it — a silently dead hotkey looks identical to a
/// working one in the settings UI otherwise.
#[derive(Clone, serde::Serialize, specta::Type)]
pub struct RegistrationFailure {
    pub id: String,
    pub binding: String,
    pub error: String,
}

static REGISTRATION_FAILURES: once_cell::sync::Lazy<std::sync::Mutex<Vec<RegistrationFailure>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(Vec::new()));

/// Record the outcome of a registration attempt: a failure is remembered (and
/// shown by the frontend); a later success for the same binding clears it.
pub(crate) fn record_registration_result(id: &str, binding: &str, result: &Result<(), String>) {
    if let Ok(mut failures) = REGISTRATION_FAILURES.lock() {
        failures.retain(|f| f.id != id);
        if let Err(e) = result {
            failures.push(RegistrationFailure {
                id: id.to_string(),
                binding: binding.to_string(),
                error: e.clone(),
            });
        }
    }
}

/// Shortcuts that failed to register, for the frontend to display on startup.
#[tauri::command]
#[specta::specta]
pub fn get_shortcut_registration_failures() -> Vec<RegistrationFailure> {
    REGISTRATION_FAILURES
        .lock()
        .map(|f| f.clone())
        .unwrap_or_default()
}

/// Jumper bindings (hot anchor + static slots) are Windows-only — they must
/// not claim global hotkeys on other platforms.
pub(crate) fn is_jumper_binding(id: &str) -> bool {
    id == "anchor_set"
        || id == "anchor_jump"
        || id == "anchor_set_2"
        || id == "anchor_jump_2"
        || id.starts_with("jump_slot_")
        || id.starts_with("jump_set_slot_")
}

/// A binding being unregistered can never deliver its pending key release. If a
/// recording is active, synthesize the release so a held PTT is stopped instead
/// of stranding the coordinator in Recording with a hot mic. (The coordinator
/// ignores releases for bindings that aren't the active recording, so this is a
/// no-op in every other state.)
fn synthesize_release_if_recording(app: &AppHandle, binding_id: &str) {
    let recording = app
        .try_state::<std::sync::Arc<crate::managers::audio::AudioRecordingManager>>()
        .map(|rm| rm.is_recording())
        .unwrap_or(false);
    if recording {
        if let Some(coordinator) = app.try_state::<crate::TranscriptionCoordinator>() {
            log::debug!(
                "Unregistering '{}' while recording — synthesizing key release",
                binding_id
            );
            coordinator.send_input(binding_id, "", false);
        }
    }
}

/// Unregister a shortcut using the appropriate implementation
pub fn unregister_shortcut(app: &AppHandle, binding: ShortcutBinding) -> Result<(), String> {
    synthesize_release_if_recording(app, &binding.id);
    let settings = get_settings(app);
    match settings.keyboard_implementation {
        KeyboardImplementation::Tauri => tauri_impl::unregister_shortcut(app, binding),
        KeyboardImplementation::HandyKeys => handy_keys::unregister_shortcut(app, binding),
    }
}

// ============================================================================
// Binding Management Commands
// ============================================================================

#[derive(Serialize, Type)]
pub struct BindingResponse {
    success: bool,
    binding: Option<ShortcutBinding>,
    error: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub fn change_binding(
    app: AppHandle,
    id: String,
    binding: String,
) -> Result<BindingResponse, String> {
    // Reject empty bindings — every shortcut should have a value
    if binding.trim().is_empty() {
        return Err("Binding cannot be empty".to_string());
    }

    let mut settings = settings::get_settings(&app);

    // Get the binding to modify, or create it from defaults if it doesn't exist
    let binding_to_modify = match settings.bindings.get(&id) {
        Some(binding) => binding.clone(),
        None => {
            // Try to get the default binding for this id
            let default_settings = settings::get_default_settings();
            match default_settings.bindings.get(&id) {
                Some(default_binding) => {
                    warn!(
                        "Binding '{}' not found in settings, creating from defaults",
                        id
                    );
                    default_binding.clone()
                }
                None => {
                    let error_msg = format!("Binding with id '{}' not found in defaults", id);
                    warn!("change_binding error: {}", error_msg);
                    return Ok(BindingResponse {
                        success: false,
                        binding: None,
                        error: Some(error_msg),
                    });
                }
            }
        }
    };

    // If this is the cancel binding, just update the settings and return.
    // It's managed dynamically (registered only while recording) — but if a
    // recording is active RIGHT NOW, the old accelerator is registered and
    // must be swapped, or it stays globally swallowed until app exit. The swap
    // uses the SYNCHRONOUS explicit-binding paths: the async cancel helpers
    // re-read settings at execution time and can race the write below
    // (unregistering the new chord instead of the old one).
    if id == "cancel" {
        if let Some(mut b) = settings.bindings.get(&id).cloned() {
            let recording = app
                .try_state::<std::sync::Arc<crate::managers::audio::AudioRecordingManager>>()
                .map(|rm| rm.is_recording())
                .unwrap_or(false);
            let old = b.clone();
            b.current_binding = binding;
            if recording {
                if let Err(e) = unregister_shortcut(&app, old) {
                    warn!("cancel swap: failed to unregister old chord: {}", e);
                }
                if let Err(e) = register_shortcut(&app, b.clone()) {
                    // register_shortcut recorded the failure for the UI; the
                    // new chord is persisted but currently inactive.
                    settings.bindings.insert(id.clone(), b.clone());
                    settings::write_settings(&app, settings);
                    return Ok(BindingResponse {
                        success: false,
                        binding: Some(b),
                        error: Some(e),
                    });
                }
            }
            settings.bindings.insert(id.clone(), b.clone());
            settings::write_settings(&app, settings);
            return Ok(BindingResponse {
                success: true,
                binding: Some(b.clone()),
                error: None,
            });
        }
    }

    // Unregister the existing binding
    if let Err(e) = unregister_shortcut(&app, binding_to_modify.clone()) {
        let error_msg = format!("Failed to unregister shortcut: {}", e);
        error!("change_binding error: {}", error_msg);
    }

    // Validate the new shortcut for the current keyboard implementation
    if let Err(e) = validate_shortcut_for_implementation(&binding, settings.keyboard_implementation)
    {
        warn!("change_binding validation error: {}", e);
        return Err(e);
    }

    // Create an updated binding
    let mut updated_binding = binding_to_modify;
    updated_binding.current_binding = binding;

    // Register the new binding
    if let Err(e) = register_shortcut(&app, updated_binding.clone()) {
        let error_msg = format!("Failed to register shortcut: {}", e);
        error!("change_binding error: {}", error_msg);
        return Ok(BindingResponse {
            success: false,
            binding: None,
            error: Some(error_msg),
        });
    }

    // Update the binding in the settings
    settings.bindings.insert(id, updated_binding.clone());

    // Save the settings
    settings::write_settings(&app, settings);

    // Return the updated binding
    Ok(BindingResponse {
        success: true,
        binding: Some(updated_binding),
        error: None,
    })
}

#[tauri::command]
#[specta::specta]
pub fn reset_binding(app: AppHandle, id: String) -> Result<BindingResponse, String> {
    let binding = settings::get_stored_binding(&app, &id);
    change_binding(app, id, binding.default_binding)
}

/// Temporarily unregister a binding while the user is editing it in the UI.
/// This avoids firing the action while keys are being recorded.
#[tauri::command]
#[specta::specta]
pub fn suspend_binding(app: AppHandle, id: String) -> Result<(), String> {
    if let Some(b) = settings::get_bindings(&app).get(&id).cloned() {
        if let Err(e) = unregister_shortcut(&app, b) {
            error!("suspend_binding error for id '{}': {}", id, e);
            return Err(e);
        }
    }
    Ok(())
}

/// Re-register the binding after the user has finished editing.
#[tauri::command]
#[specta::specta]
pub fn resume_binding(app: AppHandle, id: String) -> Result<(), String> {
    if let Some(b) = settings::get_bindings(&app).get(&id).cloned() {
        if let Err(e) = register_shortcut(&app, b) {
            error!("resume_binding error for id '{}': {}", id, e);
            return Err(e);
        }
    }
    Ok(())
}

// ============================================================================
// Keyboard Implementation Switching
// ============================================================================

/// Result of changing keyboard implementation
#[derive(Serialize, Type)]
pub struct ImplementationChangeResult {
    pub success: bool,
    /// List of binding IDs that were reset to defaults due to incompatibility
    pub reset_bindings: Vec<String>,
}

/// Change the keyboard implementation with runtime switching.
/// This will unregister all shortcuts from the old implementation,
/// validate shortcuts for the new implementation (resetting invalid ones to defaults),
/// and register them with the new implementation.
#[tauri::command]
#[specta::specta]
pub fn change_keyboard_implementation_setting(
    app: AppHandle,
    implementation: String,
) -> Result<ImplementationChangeResult, String> {
    let current_settings = settings::get_settings(&app);
    let current_impl = current_settings.keyboard_implementation;
    let new_impl = parse_keyboard_implementation(&implementation);

    // If same implementation, nothing to do
    if current_impl == new_impl {
        return Ok(ImplementationChangeResult {
            success: true,
            reset_bindings: vec![],
        });
    }

    info!(
        "Switching keyboard implementation from {:?} to {:?}",
        current_impl, new_impl
    );

    // Unregister all shortcuts from the current implementation
    unregister_all_shortcuts(&app, current_impl);

    // Update the setting
    let mut settings = settings::get_settings(&app);
    settings.keyboard_implementation = new_impl;
    settings::write_settings(&app, settings);

    // Initialize new implementation if needed (HandyKeys needs state)
    if new_impl == KeyboardImplementation::HandyKeys {
        if initialize_handy_keys_with_rollback(&app)? {
            // Shortcuts already registered during init
            return Ok(ImplementationChangeResult {
                success: true,
                reset_bindings: vec![],
            });
        }
    }

    // Register all shortcuts with new implementation, resetting invalid ones
    let reset_bindings = register_all_shortcuts_for_implementation(&app, new_impl);

    // Emit event to notify frontend of the change
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({
            "setting": "keyboard_implementation",
            "value": implementation,
            "reset_bindings": reset_bindings
        }),
    );

    info!("Keyboard implementation switched to {:?}", new_impl);

    Ok(ImplementationChangeResult {
        success: true,
        reset_bindings,
    })
}

/// Get the current keyboard implementation
#[tauri::command]
#[specta::specta]
pub fn get_keyboard_implementation(app: AppHandle) -> String {
    let settings = settings::get_settings(&app);
    match settings.keyboard_implementation {
        KeyboardImplementation::Tauri => "tauri".to_string(),
        KeyboardImplementation::HandyKeys => "handy_keys".to_string(),
    }
}

// ============================================================================
// Validation Helpers
// ============================================================================

/// Validate a shortcut for a specific implementation
fn validate_shortcut_for_implementation(
    raw: &str,
    implementation: KeyboardImplementation,
) -> Result<(), String> {
    match implementation {
        KeyboardImplementation::Tauri => tauri_impl::validate_shortcut(raw),
        KeyboardImplementation::HandyKeys => handy_keys::validate_shortcut(raw),
    }
}

/// Parse a keyboard implementation string into the enum
fn parse_keyboard_implementation(s: &str) -> KeyboardImplementation {
    match s {
        "tauri" => KeyboardImplementation::Tauri,
        "handy_keys" => KeyboardImplementation::HandyKeys,
        other => {
            warn!(
                "Invalid keyboard implementation '{}', defaulting to tauri",
                other
            );
            KeyboardImplementation::Tauri
        }
    }
}

/// Unregister all shortcuts for the current implementation
fn unregister_all_shortcuts(app: &AppHandle, implementation: KeyboardImplementation) {
    let bindings = settings::get_bindings(app);

    for (id, binding) in bindings {
        // Skip cancel shortcut as it's dynamically registered
        if id == "cancel" {
            continue;
        }

        synthesize_release_if_recording(app, &id);
        let result = match implementation {
            KeyboardImplementation::Tauri => tauri_impl::unregister_shortcut(app, binding),
            KeyboardImplementation::HandyKeys => handy_keys::unregister_shortcut(app, binding),
        };

        if let Err(e) = result {
            warn!(
                "Failed to unregister shortcut '{}' during switch: {}",
                id, e
            );
        }
    }

    // The cancel binding is dynamically registered only while recording — if a
    // recording is active it IS registered on THIS backend and must be removed
    // here; the eventual stop() would otherwise unregister via the NEW backend
    // and leave this one's registration swallowing the key until app exit.
    let recording = app
        .try_state::<std::sync::Arc<crate::managers::audio::AudioRecordingManager>>()
        .map(|rm| rm.is_recording())
        .unwrap_or(false);
    if recording {
        if let Some(cancel) = settings::get_bindings(app).get("cancel").cloned() {
            let _ = match implementation {
                KeyboardImplementation::Tauri => tauri_impl::unregister_shortcut(app, cancel),
                KeyboardImplementation::HandyKeys => handy_keys::unregister_shortcut(app, cancel),
            };
        }
    }
}

/// Register all shortcuts for a specific implementation, validating and resetting invalid ones
fn register_all_shortcuts_for_implementation(
    app: &AppHandle,
    implementation: KeyboardImplementation,
) -> Vec<String> {
    let mut reset_bindings = Vec::new();
    let default_bindings = settings::get_default_settings().bindings;
    let mut current_settings = settings::get_settings(app);

    for (id, default_binding) in &default_bindings {
        // Skip cancel shortcut as it's dynamically registered
        if id == "cancel" {
            continue;
        }

        // Skip post-processing shortcut when the feature is disabled
        if id == "transcribe_with_post_process" && !current_settings.post_process_enabled {
            continue;
        }
        // The Jumper is Windows-only — don't claim its hotkeys elsewhere.
        if is_jumper_binding(id) && !cfg!(windows) {
            continue;
        }

        let mut binding = current_settings
            .bindings
            .get(id)
            .cloned()
            .unwrap_or_else(|| default_binding.clone());

        // Validate the shortcut for the target implementation
        if let Err(e) =
            validate_shortcut_for_implementation(&binding.current_binding, implementation)
        {
            info!(
                "Shortcut '{}' ({}) is invalid for {:?}: {}. Resetting to default.",
                id, binding.current_binding, implementation, e
            );

            // Reset to default
            binding.current_binding = default_binding.current_binding.clone();
            current_settings
                .bindings
                .insert(id.clone(), binding.clone());
            reset_bindings.push(id.clone());
        }

        // Register with the appropriate implementation
        let current = binding.current_binding.clone();
        let result = match implementation {
            KeyboardImplementation::Tauri => tauri_impl::register_shortcut(app, binding),
            KeyboardImplementation::HandyKeys => handy_keys::register_shortcut(app, binding),
        };
        record_registration_result(id, &current, &result);

        if let Err(e) = result {
            error!(
                "Failed to register shortcut '{}' for {:?}: {}",
                id, implementation, e
            );
        }
    }

    // Save settings if any bindings were reset
    if !reset_bindings.is_empty() {
        settings::write_settings(app, current_settings);
    }

    reset_bindings
}

/// Initialize HandyKeys if not already initialized, with rollback on failure
fn initialize_handy_keys_with_rollback(app: &AppHandle) -> Result<bool, String> {
    if app.try_state::<handy_keys::HandyKeysState>().is_some() {
        return Ok(false); // Already initialized, caller should continue
    }

    if let Err(e) = handy_keys::init_shortcuts(app) {
        error!("Failed to initialize HandyKeys: {}", e);
        // Rollback to Tauri
        let mut settings = settings::get_settings(app);
        settings.keyboard_implementation = KeyboardImplementation::Tauri;
        settings::write_settings(app, settings);
        tauri_impl::init_shortcuts(app);
        return Err(format!(
            "Failed to initialize HandyKeys: {}. Reverted to Tauri.",
            e
        ));
    }

    // init_shortcuts already registered shortcuts
    Ok(true)
}

// ============================================================================
// General Settings Commands
// ============================================================================

#[tauri::command]
#[specta::specta]
pub fn change_ptt_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.push_to_talk = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_audio_feedback_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.audio_feedback = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_audio_feedback_volume_setting(app: AppHandle, volume: f32) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.audio_feedback_volume = volume;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_sound_theme_setting(app: AppHandle, theme: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match theme.as_str() {
        "marimba" => SoundTheme::Marimba,
        "pop" => SoundTheme::Pop,
        "custom" => SoundTheme::Custom,
        other => {
            warn!("Invalid sound theme '{}', defaulting to marimba", other);
            SoundTheme::Marimba
        }
    };
    settings.sound_theme = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_translate_to_english_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.translate_to_english = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_selected_language_setting(app: AppHandle, language: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.selected_language = language;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_overlay_position_setting(app: AppHandle, position: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match position.as_str() {
        "none" => OverlayPosition::None,
        "top" => OverlayPosition::Top,
        "bottom" => OverlayPosition::Bottom,
        other => {
            warn!("Invalid overlay position '{}', defaulting to bottom", other);
            OverlayPosition::Bottom
        }
    };
    settings.overlay_position = parsed;
    settings::write_settings(&app, settings);

    // Update overlay position without recreating window
    crate::utils::update_overlay_position(&app);

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_app_theme_setting(app: AppHandle, theme: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.app_theme = match theme.as_str() {
        "system" => Theme::System,
        "light" => Theme::Light,
        "dark" => Theme::Dark,
        other => {
            warn!("Invalid appearance theme '{}', defaulting to system", other);
            Theme::System
        }
    };
    let resolved = settings.app_theme;
    info!("Appearance theme changed to: {:?}", resolved);
    settings::write_settings(&app, settings);

    // The main window resolves + applies its own theme via React (including
    // tracking OS changes live when System is selected). The overlay and
    // floating windows have no settings store of their own, so push the
    // resolved choice into them directly here.
    crate::apply_theme_to_aux_windows(&app, resolved);

    let _ = app.emit(
        "settings-changed",
        serde_json::json!({
            "setting": "app_theme",
            "value": theme
        }),
    );

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_debug_mode_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.debug_mode = enabled;
    settings::write_settings(&app, settings);

    // Emit event to notify frontend of debug mode change
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({
            "setting": "debug_mode",
            "value": enabled
        }),
    );

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_start_hidden_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.start_hidden = enabled;
    settings::write_settings(&app, settings);

    // Notify frontend
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({
            "setting": "start_hidden",
            "value": enabled
        }),
    );

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_autostart_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    // Portable mode never registers/unregisters the autostart Run-key entry
    // (T-114 gap #3, mirrors the startup-time skip in
    // `lib.rs::initialize_core_logic`) — it's machine/user-profile state
    // that would outlive the portable folder, and could stomp an installed
    // copy's autostart entry sharing the same Run key. Reject up front
    // rather than silently no-op, so the settings UI can surface why the
    // toggle had no effect.
    if crate::portable::portable_data_dir().is_some() {
        warn!(
            "Portable mode: ignoring change_autostart_setting({enabled}) — autostart is disabled in portable mode"
        );
        return Err(
            "Autostart is disabled in portable mode (it would register a machine-wide Run key entry outside this folder)"
                .to_string(),
        );
    }

    let mut settings = settings::get_settings(&app);
    settings.autostart_enabled = enabled;
    settings::write_settings(&app, settings);

    // Apply the autostart setting immediately
    let autostart_manager = app.autolaunch();
    if enabled {
        let _ = autostart_manager.enable();
    } else {
        let _ = autostart_manager.disable();
    }

    // Notify frontend
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({
            "setting": "autostart_enabled",
            "value": enabled
        }),
    );

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn update_llm_providers(
    app: AppHandle,
    providers: Vec<settings::LlmProvider>,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.llm_providers = providers.clone();
    settings::write_settings(&app, settings);

    let _ = app.emit(
        "settings-changed",
        serde_json::json!({
            "setting": "llm_providers",
            "value": providers
        }),
    );

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_typing_start_delay_setting(app: AppHandle, delay: u32) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.typing_start_delay_secs = delay;
    settings::write_settings(&app, settings);

    let _ = app.emit(
        "settings-changed",
        serde_json::json!({
            "setting": "typing_start_delay_secs",
            "value": delay
        }),
    );

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_typing_key_delay_setting(app: AppHandle, delay: u32) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.typing_key_delay_ms = delay;
    settings::write_settings(&app, settings);

    let _ = app.emit(
        "settings-changed",
        serde_json::json!({
            "setting": "typing_key_delay_ms",
            "value": delay
        }),
    );

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn update_custom_words(app: AppHandle, words: Vec<String>) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.custom_words = words;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_word_correction_threshold_setting(
    app: AppHandle,
    threshold: f64,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.word_correction_threshold = threshold;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_paste_method_setting(app: AppHandle, method: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match method.as_str() {
        "ctrl_v" => PasteMethod::CtrlV,
        "direct" => PasteMethod::Direct,
        "none" => PasteMethod::None,
        "shift_insert" => PasteMethod::ShiftInsert,
        "ctrl_shift_v" => PasteMethod::CtrlShiftV,
        "external_script" => PasteMethod::ExternalScript,
        other => {
            warn!("Invalid paste method '{}', defaulting to ctrl_v", other);
            PasteMethod::CtrlV
        }
    };
    settings.paste_method = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_paste_method_ptt_setting(app: AppHandle, method: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match method.as_str() {
        "ctrl_v" => PasteMethod::CtrlV,
        "direct" => PasteMethod::Direct,
        "none" => PasteMethod::None,
        "shift_insert" => PasteMethod::ShiftInsert,
        "ctrl_shift_v" => PasteMethod::CtrlShiftV,
        "external_script" => PasteMethod::ExternalScript,
        other => {
            warn!("Invalid PTT paste method '{}', defaulting to ctrl_v", other);
            PasteMethod::CtrlV
        }
    };
    settings.paste_method_ptt = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_available_typing_tools() -> Vec<String> {
    #[cfg(target_os = "linux")]
    {
        crate::clipboard::get_available_typing_tools()
    }
    #[cfg(not(target_os = "linux"))]
    {
        vec!["auto".to_string()]
    }
}

#[tauri::command]
#[specta::specta]
pub fn change_typing_tool_setting(app: AppHandle, tool: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match tool.as_str() {
        "auto" => TypingTool::Auto,
        "wtype" => TypingTool::Wtype,
        "kwtype" => TypingTool::Kwtype,
        "dotool" => TypingTool::Dotool,
        "ydotool" => TypingTool::Ydotool,
        "xdotool" => TypingTool::Xdotool,
        other => {
            warn!("Invalid typing tool '{}', defaulting to auto", other);
            TypingTool::Auto
        }
    };
    settings.typing_tool = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_external_script_path_setting(
    app: AppHandle,
    path: Option<String>,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.external_script_path = path;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_clipboard_handling_setting(app: AppHandle, handling: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match handling.as_str() {
        "dont_modify" => ClipboardHandling::DontModify,
        "copy_to_clipboard" => ClipboardHandling::CopyToClipboard,
        other => {
            warn!(
                "Invalid clipboard handling '{}', defaulting to dont_modify",
                other
            );
            ClipboardHandling::DontModify
        }
    };
    settings.clipboard_handling = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_auto_submit_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.auto_submit = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_auto_submit_key_setting(app: AppHandle, key: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match key.as_str() {
        "enter" => AutoSubmitKey::Enter,
        "ctrl_enter" => AutoSubmitKey::CtrlEnter,
        "cmd_enter" => AutoSubmitKey::CmdEnter,
        other => {
            warn!("Invalid auto submit key '{}', defaulting to enter", other);
            AutoSubmitKey::Enter
        }
    };
    settings.auto_submit_key = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_submit_paste_method_setting(app: AppHandle, method: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match method.as_str() {
        "ctrl_v" => PasteMethod::CtrlV,
        "direct" => PasteMethod::Direct,
        "none" => PasteMethod::None,
        "shift_insert" => PasteMethod::ShiftInsert,
        "ctrl_shift_v" => PasteMethod::CtrlShiftV,
        "external_script" => PasteMethod::ExternalScript,
        other => {
            warn!(
                "Invalid submit paste method '{}', defaulting to ctrl_v",
                other
            );
            PasteMethod::CtrlV
        }
    };
    settings.submit_paste_method = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_paste_last_paste_method_setting(
    app: AppHandle,
    method: String,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match method.as_str() {
        "ctrl_v" => PasteMethod::CtrlV,
        "direct" => PasteMethod::Direct,
        "none" => PasteMethod::None,
        "shift_insert" => PasteMethod::ShiftInsert,
        "ctrl_shift_v" => PasteMethod::CtrlShiftV,
        "external_script" => PasteMethod::ExternalScript,
        other => {
            warn!(
                "Invalid paste-last paste method '{}', defaulting to ctrl_v",
                other
            );
            PasteMethod::CtrlV
        }
    };
    settings.paste_last_paste_method = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_paste_last_clipboard_handling_setting(
    app: AppHandle,
    handling: String,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match handling.as_str() {
        "dont_modify" => ClipboardHandling::DontModify,
        "copy_to_clipboard" => ClipboardHandling::CopyToClipboard,
        other => {
            warn!(
                "Invalid paste-last clipboard handling '{}', defaulting to dont_modify",
                other
            );
            ClipboardHandling::DontModify
        }
    };
    settings.paste_last_clipboard_handling = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

fn parse_clipboard_restore_delay(value: &str) -> ClipboardRestoreDelay {
    match value {
        "none" => ClipboardRestoreDelay::None,
        "ms250" => ClipboardRestoreDelay::Ms250,
        "ms500" => ClipboardRestoreDelay::Ms500,
        "ms1000" => ClipboardRestoreDelay::Ms1000,
        "ms2500" => ClipboardRestoreDelay::Ms2500,
        "ms5000" => ClipboardRestoreDelay::Ms5000,
        other => {
            warn!(
                "Invalid clipboard restore delay '{}', defaulting to none",
                other
            );
            ClipboardRestoreDelay::None
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn change_clipboard_restore_delay_setting(app: AppHandle, delay: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.clipboard_restore_delay = parse_clipboard_restore_delay(&delay);
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_submit_clipboard_restore_delay_setting(
    app: AppHandle,
    delay: String,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.submit_clipboard_restore_delay = parse_clipboard_restore_delay(&delay);
    settings::write_settings(&app, settings);
    Ok(())
}

fn parse_jumper_submit_delay(value: &str) -> JumperSubmitDelay {
    match value {
        "none" => JumperSubmitDelay::None,
        "ms100" => JumperSubmitDelay::Ms100,
        "ms250" => JumperSubmitDelay::Ms250,
        "ms500" => JumperSubmitDelay::Ms500,
        "ms1000" => JumperSubmitDelay::Ms1000,
        "ms2000" => JumperSubmitDelay::Ms2000,
        other => {
            warn!(
                "Invalid jumper submit delay '{}', defaulting to ms250",
                other
            );
            JumperSubmitDelay::Ms250
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn change_jumper_submit_delay_setting(app: AppHandle, delay: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.jumper_submit_delay = parse_jumper_submit_delay(&delay);
    settings::write_settings(&app, settings);
    Ok(())
}

fn parse_jumper_paste_delay(value: &str) -> JumperPasteDelay {
    match value {
        "none" => JumperPasteDelay::None,
        "ms100" => JumperPasteDelay::Ms100,
        "ms250" => JumperPasteDelay::Ms250,
        "ms500" => JumperPasteDelay::Ms500,
        "ms1000" => JumperPasteDelay::Ms1000,
        "ms2000" => JumperPasteDelay::Ms2000,
        other => {
            warn!(
                "Invalid jumper paste delay '{}', defaulting to ms250",
                other
            );
            JumperPasteDelay::Ms250
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn change_jumper_paste_delay_setting(app: AppHandle, delay: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.jumper_paste_delay = parse_jumper_paste_delay(&delay);
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_jumper_submit_delay_remote_setting(
    app: AppHandle,
    delay: String,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.jumper_submit_delay_remote = parse_jumper_submit_delay(&delay);
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_jumper_paste_delay_remote_setting(
    app: AppHandle,
    delay: String,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.jumper_paste_delay_remote = parse_jumper_paste_delay(&delay);
    settings::write_settings(&app, settings);
    Ok(())
}

/// Replace the remote-desktop classifier list. Each entry is trimmed; blank
/// entries and case-insensitive duplicates are dropped so the stored list is
/// clean (the matcher ignores blanks anyway, but a tidy list keeps the UI
/// honest). An empty result is allowed — it simply disables remote
/// classification (all jumps use the local delays).
#[tauri::command]
#[specta::specta]
pub fn set_jumper_remote_match_strings(app: AppHandle, strings: Vec<String>) -> Result<(), String> {
    let mut seen: Vec<String> = Vec::new();
    let mut cleaned: Vec<String> = Vec::new();
    for raw in strings {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if seen.contains(&lower) {
            continue;
        }
        seen.push(lower);
        cleaned.push(trimmed.to_string());
    }
    let mut settings = settings::get_settings(&app);
    settings.jumper_remote_match_strings = cleaned;
    settings::write_settings(&app, settings);
    // Refresh the Jumper UI so the "Remote ✓" badges reflect the new list
    // immediately, without waiting for a slot mutation.
    crate::anchor::emit_anchor_changed(&app);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_submit_key_setting(app: AppHandle, key: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match key.as_str() {
        "enter" => AutoSubmitKey::Enter,
        "ctrl_enter" => AutoSubmitKey::CtrlEnter,
        "cmd_enter" => AutoSubmitKey::CmdEnter,
        other => {
            warn!("Invalid submit key '{}', defaulting to enter", other);
            AutoSubmitKey::Enter
        }
    };
    settings.submit_key = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

/// Set one of the four per-flow anchor actions. `key` selects the slot
/// (output_idle | output_stop | submit_idle | submit_stop).
#[tauri::command]
#[specta::specta]
pub fn change_anchor_action_setting(
    app: AppHandle,
    key: String,
    action: String,
) -> Result<(), String> {
    let parsed = match action.as_str() {
        "none" => AnchorAction::None,
        "jump" => AnchorAction::Jump,
        "set" => AnchorAction::Set,
        "clear" => AnchorAction::Clear,
        other => {
            warn!("Invalid anchor action '{}', defaulting to none", other);
            AnchorAction::None
        }
    };
    let mut settings = settings::get_settings(&app);
    match key.as_str() {
        "output_idle" => settings.anchor_action_output_idle = parsed,
        "output_stop" => settings.anchor_action_output_stop = parsed,
        "submit_idle" => settings.anchor_action_submit_idle = parsed,
        "submit_stop" => settings.anchor_action_submit_stop = parsed,
        other => return Err(format!("Unknown anchor action key '{}'", other)),
    }
    settings::write_settings(&app, settings);
    Ok(())
}

/// Set the slot (0 = hot, 1–4 static) an event action targets.
#[tauri::command]
#[specta::specta]
pub fn change_anchor_action_slot_setting(
    app: AppHandle,
    key: String,
    slot: u32,
) -> Result<(), String> {
    let slot = slot.min((crate::anchor::SLOT_COUNT - 1) as u32) as u8;
    let mut settings = settings::get_settings(&app);
    match key.as_str() {
        "output_idle" => settings.anchor_action_output_idle_slot = slot,
        "output_stop" => settings.anchor_action_output_stop_slot = slot,
        "submit_idle" => settings.anchor_action_submit_idle_slot = slot,
        "submit_stop" => settings.anchor_action_submit_stop_slot = slot,
        other => return Err(format!("Unknown anchor action key '{}'", other)),
    }
    settings::write_settings(&app, settings);
    Ok(())
}

/// Toggle track-last-output for a flow ("output" = dictate, "submit" =
/// Transcribe & Submit). The two flows are independent (mirrors
/// `change_return_focus_setting`).
#[tauri::command]
#[specta::specta]
pub fn change_jumper_track_setting(
    app: AppHandle,
    flow: String,
    enabled: bool,
) -> Result<(), String> {
    // Validate BEFORE the mutation closure so an unknown flow can't half-apply,
    // and route through the serialized RMW helper (T-111) so the two per-flow
    // switches — and rapid enable/slot writes — can't overwrite one another.
    if flow != "output" && flow != "submit" {
        return Err(format!("Unknown track-output flow '{}'", flow));
    }
    settings::update_settings(&app, |s| {
        if flow == "output" {
            s.jumper_track_output_enabled = enabled;
        } else {
            s.jumper_track_submit_enabled = enabled;
        }
    });
    Ok(())
}

/// Pick which slot receives the tracked last-output location (0 = hot) for a
/// flow ("output" or "submit").
#[tauri::command]
#[specta::specta]
pub fn change_jumper_track_slot_setting(
    app: AppHandle,
    flow: String,
    slot: u32,
) -> Result<(), String> {
    if flow != "output" && flow != "submit" {
        return Err(format!("Unknown track-output flow '{}'", flow));
    }
    if slot as usize >= crate::anchor::SLOT_COUNT {
        return Err(format!("invalid jump slot {slot}"));
    }
    settings::update_settings(&app, |s| {
        if flow == "output" {
            s.jumper_track_output_slot = slot as u8;
        } else {
            s.jumper_track_submit_slot = slot as u8;
        }
    });
    Ok(())
}

/// Toggle save/restore-cursor for a single jump slot (index 0 = hot, 1-4 =
/// static). Per-slot config lives on the Jumper page; capture gating is by
/// target slot index everywhere. T-302.
#[tauri::command]
#[specta::specta]
pub fn change_jumper_save_cursor_slot(
    app: AppHandle,
    slot: u8,
    enabled: bool,
) -> Result<(), String> {
    // Validate BEFORE the mutation closure so an out-of-range slot can't
    // half-apply, and route through the serialized RMW helper so concurrent
    // per-slot toggles can't overwrite one another.
    if slot as usize >= crate::anchor::SLOT_COUNT {
        return Err(format!("invalid jump slot {slot}"));
    }
    settings::update_settings(&app, |s| {
        // Defensively normalize the vec to SLOT_COUNT before indexing so an
        // old/short persisted value can't panic.
        if s.jumper_save_cursor_slots.len() != crate::anchor::SLOT_COUNT {
            s.jumper_save_cursor_slots
                .resize(crate::anchor::SLOT_COUNT, false);
        }
        s.jumper_save_cursor_slots[slot as usize] = enabled;
    });
    Ok(())
}

/// Toggle whether the on-finish Jumper jump/anchor action fires only when the
/// take's START flow matches its FINISH flow (start plain Transcribe + finish
/// Transcribe & Submit → no jump). Default off. T-302.
#[tauri::command]
#[specta::specta]
pub fn change_anchor_require_same_flow(app: AppHandle, enabled: bool) -> Result<(), String> {
    settings::update_settings(&app, |s| {
        s.anchor_on_finish_require_same_flow = enabled;
    });
    Ok(())
}

/// Set the coordinate mode used when restoring the cursor for a SINGLE jump
/// slot (index 0 = Hot 1, 1-4 = static, 5 = Hot 2): "AppRelative" (same spot
/// inside the app; default) or "ScreenAbsolute" (fixed monitor pixel). T-304
/// (replaces the pre-0.51 shared cursor-mode command). Takes a
/// String parsed manually per the enum-arg convention; the strings are the
/// `CursorMode` variant names. Mirrors `change_jumper_save_cursor_slot`:
/// validate the slot BEFORE the mutation closure and normalize the vec to
/// SLOT_COUNT before indexing so an old/short persisted value can't panic.
#[tauri::command]
#[specta::specta]
pub fn change_jumper_cursor_mode_slot(
    app: AppHandle,
    slot: u8,
    mode: String,
) -> Result<(), String> {
    if slot as usize >= crate::anchor::SLOT_COUNT {
        return Err(format!("invalid jump slot {slot}"));
    }
    let parsed = match mode.as_str() {
        "AppRelative" => CursorMode::AppRelative,
        "ScreenAbsolute" => CursorMode::ScreenAbsolute,
        other => return Err(format!("Unknown cursor mode '{}'", other)),
    };
    settings::update_settings(&app, |s| {
        if s.jumper_cursor_mode_slots.len() != crate::anchor::SLOT_COUNT {
            // Seed any padded entries from the legacy global, matching the
            // ensure_jumper_v2 migration, so a first write pre-normalization
            // can't reset other slots to the type default.
            let seed = s.jumper_cursor_mode;
            s.jumper_cursor_mode_slots
                .resize(crate::anchor::SLOT_COUNT, seed);
        }
        s.jumper_cursor_mode_slots[slot as usize] = parsed;
    });
    Ok(())
}

/// Set the Translator engine's idle-unload timeout (independent of the main
/// model's `model_unload_timeout`). Mirrors `set_model_unload_timeout` but
/// takes a String parsed manually per the enum-arg convention; parsing via
/// serde guarantees it accepts exactly the `ModelUnloadTimeout` wire repr. T-36.
#[tauri::command]
#[specta::specta]
pub fn change_translator_model_unload_timeout(
    app: AppHandle,
    timeout: String,
) -> Result<(), String> {
    let parsed: ModelUnloadTimeout =
        serde_json::from_value(serde_json::Value::String(timeout.clone()))
            .map_err(|_| format!("Invalid translator model unload timeout '{}'", timeout))?;
    settings::update_settings(&app, |s| {
        s.translator_model_unload_timeout = parsed;
    });
    Ok(())
}

/// Idle seconds for the Translator's `Custom` unload timeout. Mirrors
/// `set_model_unload_custom_seconds` (clamps to >= 1). T-36.
#[tauri::command]
#[specta::specta]
pub fn change_translator_model_unload_custom_seconds(
    app: AppHandle,
    seconds: u64,
) -> Result<(), String> {
    let seconds = seconds.max(1);
    settings::update_settings(&app, |s| {
        s.translator_model_unload_custom_seconds = seconds;
    });
    Ok(())
}

/// Toggle return-focus-after-delivery for a flow ("output" or "submit").
/// The location returned to is captured automatically at delivery start.
#[tauri::command]
#[specta::specta]
pub fn change_return_focus_setting(
    app: AppHandle,
    flow: String,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    match flow.as_str() {
        "output" => settings.return_focus_output = enabled,
        "submit" => settings.return_focus_submit = enabled,
        other => return Err(format!("Unknown return-focus flow '{}'", other)),
    }
    settings::write_settings(&app, settings);
    Ok(())
}

/// Persist jump slots across restarts. Turning it ON snapshots the current
/// live slots so they survive; turning it OFF wipes the saved identities.
#[tauri::command]
#[specta::specta]
pub fn change_jumper_persist_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    // Finding 11 (v0.42.0 SECOND adversarial review): the settings flag flip
    // and the follow-on snapshot/delete step must run as ONE indivisible
    // sequence relative to a concurrent call of this same function — a
    // second `change_jumper_persist_setting` call (enable racing disable, or
    // vice versa) interleaving between the flag flip and the snapshot/delete
    // below could resurrect cleared identities, flip `jumper_persist` back,
    // or recreate the hints sidecar right after it was deleted. See
    // `anchor::with_persist_toggle_lock`'s doc for the full lock order this
    // composes with (`snapshot_all`'s own `PERSIST_LOCK`/
    // `SETTINGS_MUTATION_LOCK` nesting).
    crate::anchor::with_persist_toggle_lock(|| {
        if enabled {
            settings::update_settings(&app, |settings| {
                settings.jumper_persist = true;
            });
            #[cfg(windows)]
            crate::anchor::snapshot_slots(&app);
        } else {
            // Flag flip + identity clear + hints-sidecar delete run as ONE
            // operation under PERSIST_LOCK (finding 11, v0.42.0 3rd review) so
            // an in-flight `persist_slot` — which holds the SAME lock — can
            // never recreate a hint or resurrect an identity after the clear.
            crate::anchor::disable_persistence(&app);
        }
    });
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_submit_idle_behavior_setting(app: AppHandle, behavior: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match behavior.as_str() {
        "start_normal" => SubmitIdleBehavior::StartNormal,
        "do_nothing" => SubmitIdleBehavior::DoNothing,
        "start_and_submit" => SubmitIdleBehavior::StartAndSubmit,
        other => {
            warn!(
                "Invalid submit idle behavior '{}', defaulting to start_normal",
                other
            );
            SubmitIdleBehavior::StartNormal
        }
    };
    settings.submit_idle_behavior = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_submit_clipboard_handling_setting(
    app: AppHandle,
    handling: String,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match handling.as_str() {
        "dont_modify" => ClipboardHandling::DontModify,
        "copy_to_clipboard" => ClipboardHandling::CopyToClipboard,
        other => {
            warn!(
                "Invalid submit clipboard handling '{}', defaulting to dont_modify",
                other
            );
            ClipboardHandling::DontModify
        }
    };
    settings.submit_clipboard_handling = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_post_process_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.post_process_enabled = enabled;
    settings::write_settings(&app, settings.clone());

    // Register or unregister the post-processing shortcut
    if let Some(binding) = settings
        .bindings
        .get("transcribe_with_post_process")
        .cloned()
    {
        if enabled {
            let _ = register_shortcut(&app, binding);
        } else {
            let _ = unregister_shortcut(&app, binding);
        }
    }

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_experimental_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.experimental_enabled = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_transcription_mode_setting(app: AppHandle, mode: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.transcription_mode = match mode.as_str() {
        "live" => TranscriptionMode::Live,
        "post_recording" => TranscriptionMode::PostRecording,
        _ => {
            log::warn!(
                "Invalid transcription mode '{}', defaulting to post_recording",
                mode
            );
            TranscriptionMode::PostRecording
        }
    };
    log::info!(
        "Transcription mode changed to: {:?}",
        settings.transcription_mode
    );
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_transcription_mode_ptt_setting(app: AppHandle, mode: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.transcription_mode_ptt = match mode.as_str() {
        "live" => TranscriptionMode::Live,
        "post_recording" => TranscriptionMode::PostRecording,
        _ => {
            log::warn!(
                "Invalid PTT transcription mode '{}', defaulting to live",
                mode
            );
            TranscriptionMode::Live
        }
    };
    log::info!(
        "PTT transcription mode changed to: {:?}",
        settings.transcription_mode_ptt
    );
    settings::write_settings(&app, settings);
    Ok(())
}

/// T-212: persists the local Whisper GPU-device selection. Sentinel-encoded
/// (`-1` Auto, `-2` force CPU, `>= 0` explicit Vulkan device index — see the
/// `transcribe_gpu_device` doc comment in settings.rs), so this is a plain
/// `i32` setter rather than an enum-parsing command; validation/fallback for
/// a stale or disappeared device happens at model load time
/// (`managers/transcription.rs`), not here.
///
/// Adversarial review finding 7 (T-212 follow-up): routes through
/// `settings::update_settings` (the T-111 read-modify-write helper) instead
/// of the bare `get_settings`/mutate/`write_settings` pattern, which raced
/// other concurrent settings writers.
#[tauri::command]
#[specta::specta]
pub fn change_transcribe_gpu_device_setting(app: AppHandle, device: i32) -> Result<(), String> {
    settings::update_settings(&app, |settings| {
        settings.transcribe_gpu_device = device;
    });
    log::info!("Transcribe GPU device setting changed to: {}", device);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_api_transcription_url_setting(app: AppHandle, url: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.api_transcription_url = url;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_api_transcription_key_setting(app: AppHandle, key: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.api_transcription_key = key;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_api_transcription_model_setting(app: AppHandle, model: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.api_transcription_model = model;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_openrouter_transcription_url_setting(
    app: AppHandle,
    url: String,
) -> Result<(), String> {
    settings::update_settings(&app, |s| s.openrouter_transcription_url = url);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_openrouter_transcription_key_setting(
    app: AppHandle,
    key: String,
) -> Result<(), String> {
    settings::update_settings(&app, |s| s.openrouter_transcription_key = key);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_openrouter_transcription_model_setting(
    app: AppHandle,
    model: String,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.openrouter_transcription_model = model;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_openrouter_transcription_route_setting(
    app: AppHandle,
    route: String,
) -> Result<(), String> {
    use settings::OpenRouterTranscriptionRoute;
    let mut settings = settings::get_settings(&app);
    settings.openrouter_transcription_route = match route.as_str() {
        "chat" => OpenRouterTranscriptionRoute::Chat,
        "stt" => OpenRouterTranscriptionRoute::Stt,
        other => {
            warn!(
                "Invalid OpenRouter transcription route '{}', defaulting to stt",
                other
            );
            OpenRouterTranscriptionRoute::Stt
        }
    };
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn update_model_test_library(
    app: AppHandle,
    library: settings::ModelTestLibrary,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.model_test_library = library;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_openrouter_transcription_audio_format_setting(
    app: AppHandle,
    format: String,
) -> Result<(), String> {
    use settings::TranscriptionAudioFormat;
    let mut settings = settings::get_settings(&app);
    settings.openrouter_transcription_audio_format = match format.as_str() {
        "wav" => TranscriptionAudioFormat::Wav,
        "opus" => TranscriptionAudioFormat::Opus,
        other => {
            warn!(
                "Invalid transcription audio format '{}', defaulting to opus",
                other
            );
            TranscriptionAudioFormat::Opus
        }
    };
    settings::write_settings(&app, settings);
    Ok(())
}

/// Select which registered LLM provider (by stable id) post-processing uses.
/// An empty id clears the selection.
#[tauri::command]
#[specta::specta]
pub fn set_post_process_provider_ref(app: AppHandle, provider_id: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    if !provider_id.is_empty() && settings.llm_provider(&provider_id).is_none() {
        return Err(format!("Provider '{}' not found", provider_id));
    }
    settings.post_process_provider_ref = provider_id;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_post_process_temperature_setting(
    app: AppHandle,
    temperature: f32,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.post_process_temperature = temperature.clamp(0.0, 2.0);
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn add_post_process_prompt(
    app: AppHandle,
    name: String,
    prompt: String,
) -> Result<LLMPrompt, String> {
    let mut settings = settings::get_settings(&app);

    // Generate unique ID using timestamp and random component
    let id = format!("prompt_{}", chrono::Utc::now().timestamp_millis());

    let new_prompt = LLMPrompt {
        id: id.clone(),
        name,
        prompt,
    };

    settings.post_process_prompts.push(new_prompt.clone());
    settings::write_settings(&app, settings);

    Ok(new_prompt)
}

#[tauri::command]
#[specta::specta]
pub fn update_post_process_prompt(
    app: AppHandle,
    id: String,
    name: String,
    prompt: String,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);

    if let Some(existing_prompt) = settings
        .post_process_prompts
        .iter_mut()
        .find(|p| p.id == id)
    {
        existing_prompt.name = name;
        existing_prompt.prompt = prompt;
        settings::write_settings(&app, settings);
        Ok(())
    } else {
        Err(format!("Prompt with id '{}' not found", id))
    }
}

#[tauri::command]
#[specta::specta]
pub fn delete_post_process_prompt(app: AppHandle, id: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);

    // Don't allow deleting the last prompt
    if settings.post_process_prompts.len() <= 1 {
        return Err("Cannot delete the last prompt".to_string());
    }

    // Find and remove the prompt
    let original_len = settings.post_process_prompts.len();
    settings.post_process_prompts.retain(|p| p.id != id);

    if settings.post_process_prompts.len() == original_len {
        return Err(format!("Prompt with id '{}' not found", id));
    }

    // If the deleted prompt was selected, select the first one or None
    if settings.post_process_selected_prompt_id.as_ref() == Some(&id) {
        settings.post_process_selected_prompt_id =
            settings.post_process_prompts.first().map(|p| p.id.clone());
    }

    settings::write_settings(&app, settings);
    Ok(())
}

// Model listing for any registered provider is handled by the kind-aware
// `token_count::list_provider_models` command (used by the registry UI).

#[tauri::command]
#[specta::specta]
pub fn set_post_process_selected_prompt(app: AppHandle, id: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);

    // Verify the prompt exists
    if !settings.post_process_prompts.iter().any(|p| p.id == id) {
        return Err(format!("Prompt with id '{}' not found", id));
    }

    settings.post_process_selected_prompt_id = Some(id);
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_mute_while_recording_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.mute_while_recording = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_append_trailing_space_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.append_trailing_space = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_crash_resilient_recording_setting(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.crash_resilient_recording = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_app_language_setting(app: AppHandle, language: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.app_language = language.clone();
    settings::write_settings(&app, settings);

    // Refresh the tray menu with the new language
    tray::update_tray_menu(&app, &tray::TrayIconState::Idle, Some(&language));

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_post_process_disable_thinking_setting(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.post_process_disable_thinking = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_show_tray_icon_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.show_tray_icon = enabled;
    settings::write_settings(&app, settings);

    // Apply change immediately
    tray::set_tray_visibility(&app, enabled);

    Ok(())
}
