//! Tauri global-shortcut implementation
//!
//! This module provides shortcut functionality using Tauri's built-in
//! global-shortcut plugin.

use log::{error, warn};
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::settings::{self, ShortcutBinding, get_settings};

use super::handler::handle_shortcut_event;

/// Every binding this backend holds right now, with the chord it holds, kept by
/// register_shortcut / unregister_shortcut so every path (init, rebinding, take
/// start and end, a chord changed mid-take) agrees. Shortcut Keeper keeps only
/// these. For the take-only bindings (Cancel, Pause, Undo word) it also guards
/// unregistering: that goes by chord, so unregistering one whose registration
/// failed — its chord taken by another binding — would remove that other
/// binding's shortcut.
static REGISTERED: once_cell::sync::Lazy<
    std::sync::Mutex<std::collections::HashMap<String, String>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// (binding id, chord) of every binding this backend holds right now.
pub(crate) fn registered_bindings() -> Vec<(String, String)> {
    REGISTERED
        .lock()
        .map(|held| {
            held.iter()
                .map(|(id, chord)| (id.clone(), chord.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// Initialize shortcuts using Tauri's global-shortcut plugin
pub fn init_shortcuts(app: &AppHandle) {
    let default_bindings = settings::get_default_settings().bindings;
    let user_settings = settings::load_or_create_app_settings(app);

    // Register all default shortcuts, applying user customizations
    for (id, default_binding) in default_bindings {
        if super::is_take_binding(&id) {
            continue; // Skip cancel shortcut, it will be registered dynamically
        }
        // The Jumper is Windows-only — don't claim its hotkeys elsewhere.
        if super::is_jumper_binding(&id) && !cfg!(windows) {
            continue;
        }
        // Skip post-processing shortcut when the feature is disabled
        if id == "transcribe_with_post_process" && !user_settings.post_process_enabled {
            continue;
        }
        let binding = user_settings
            .bindings
            .get(&id)
            .cloned()
            .unwrap_or(default_binding);

        let current = binding.current_binding.clone();
        let result = register_shortcut(app, binding);
        super::record_registration_result(app, &id, &current, &result);
        if let Err(e) = result {
            error!("Failed to register shortcut {} during init: {}", id, e);
        }
    }
}

/// Validate a shortcut string for the Tauri global-shortcut implementation.
/// Tauri requires at least one non-modifier key and doesn't support the fn key.
pub fn validate_shortcut(raw: &str) -> Result<(), String> {
    if raw.trim().is_empty() {
        return Err("Shortcut cannot be empty".into());
    }

    let modifiers = [
        "ctrl", "control", "shift", "alt", "option", "meta", "command", "cmd", "super", "win",
        "windows",
    ];

    // Check for fn key which Tauri doesn't support
    let parts: Vec<String> = raw.split('+').map(|p| p.trim().to_lowercase()).collect();
    for part in &parts {
        if part == "fn" || part == "function" {
            return Err("The 'fn' key is not supported by Tauri global shortcuts".into());
        }
    }

    // Check for at least one non-modifier key
    let has_non_modifier = parts.iter().any(|part| !modifiers.contains(&part.as_str()));

    if has_non_modifier {
        Ok(())
    } else {
        Err("Tauri shortcuts must include a main key (letter, number, F-key, etc.) in addition to modifiers".into())
    }
}

/// Register a shortcut using Tauri's global-shortcut plugin
pub fn register_shortcut(app: &AppHandle, binding: ShortcutBinding) -> Result<(), String> {
    // Switched off by the user ("None"): nothing to register.
    if super::is_unbound(&binding.current_binding) {
        return Ok(());
    }

    // Validate for Tauri requirements
    if let Err(e) = validate_shortcut(&binding.current_binding) {
        warn!(
            "register_tauri_shortcut validation error for binding '{}': {}",
            binding.current_binding, e
        );
        return Err(e);
    }

    // Parse shortcut and return error if it fails
    let shortcut = match binding.current_binding.parse::<Shortcut>() {
        Ok(s) => s,
        Err(e) => {
            let error_msg = format!(
                "Failed to parse shortcut '{}': {}",
                binding.current_binding, e
            );
            error!("register_tauri_shortcut parse error: {}", error_msg);
            return Err(error_msg);
        }
    };

    // Prevent duplicate registrations that would silently shadow one another
    if app.global_shortcut().is_registered(shortcut) {
        let error_msg = format!("Shortcut '{}' is already in use", binding.current_binding);
        warn!("register_tauri_shortcut duplicate error: {}", error_msg);
        return Err(error_msg);
    }

    // Clone binding.id for use in the closure
    let binding_id_for_closure = binding.id.clone();

    app.global_shortcut()
        .on_shortcut(shortcut, move |app_handle, scut, event| {
            if scut == &shortcut {
                let shortcut_string = scut.into_string();
                let is_pressed = event.state == ShortcutState::Pressed;
                handle_shortcut_event(
                    app_handle,
                    &binding_id_for_closure,
                    &shortcut_string,
                    is_pressed,
                );
            }
        })
        .map_err(|e| {
            let error_msg = format!(
                "Couldn't register shortcut '{}': {}",
                binding.current_binding, e
            );
            error!("register_tauri_shortcut registration error: {}", error_msg);
            error_msg
        })?;

    if let Ok(mut held) = REGISTERED.lock() {
        held.insert(binding.id.clone(), binding.current_binding.clone());
    }
    crate::remote_keys::notify_bindings_changed();
    Ok(())
}

/// Unregister a shortcut from Tauri's global-shortcut plugin
pub fn unregister_shortcut(app: &AppHandle, binding: ShortcutBinding) -> Result<(), String> {
    // Switched off by the user ("None"): it was never registered.
    if super::is_unbound(&binding.current_binding) {
        return Ok(());
    }
    // Ownership goes first, before the chord is released: a newer
    // registration of this binding (a take starting while the last one's
    // cleanup still runs) can only succeed after that, so its entry is never
    // the one removed here. A take-only binding this backend does not hold (its
    // registration failed, or no take is running) must not unregister its
    // chord: another binding may own it.
    let held = REGISTERED
        .lock()
        .map(|mut held| held.remove(&binding.id).is_some())
        .unwrap_or(false);
    if super::is_take_binding(&binding.id) && !held {
        return Ok(());
    }

    let shortcut = match binding.current_binding.parse::<Shortcut>() {
        Ok(s) => s,
        Err(e) => {
            let error_msg = format!(
                "Failed to parse shortcut '{}' for unregistration: {}",
                binding.current_binding, e
            );
            error!("unregister_tauri_shortcut parse error: {}", error_msg);
            return Err(error_msg);
        }
    };

    app.global_shortcut().unregister(shortcut).map_err(|e| {
        let error_msg = format!(
            "Failed to unregister shortcut '{}': {}",
            binding.current_binding, e
        );
        error!("unregister_tauri_shortcut error: {}", error_msg);
        error_msg
    })?;

    crate::remote_keys::notify_bindings_changed();
    Ok(())
}

/// Register a take-only binding (Cancel, Pause, Undo word) when a take starts.
pub fn register_dynamic_shortcut(app: &AppHandle, id: &'static str) {
    // Cancel shortcut is disabled on Linux due to instability with dynamic shortcut registration
    #[cfg(target_os = "linux")]
    {
        let _ = app;
        return;
    }

    #[cfg(not(target_os = "linux"))]
    {
        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Some(take_binding) = get_settings(&app_clone).bindings.get(id).cloned() {
                if let Err(e) = register_shortcut(&app_clone, take_binding) {
                    error!("Failed to register {} shortcut: {}", id, e);
                }
            }
        });
    }
}

/// Unregister a take-only binding when the take ends.
pub fn unregister_dynamic_shortcut(app: &AppHandle, id: &'static str) {
    // Cancel shortcut is disabled on Linux due to instability with dynamic shortcut registration
    #[cfg(target_os = "linux")]
    {
        let _ = app;
        return;
    }

    #[cfg(not(target_os = "linux"))]
    {
        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Some(take_binding) = get_settings(&app_clone).bindings.get(id).cloned() {
                // We ignore errors here as it might already be unregistered
                let _ = unregister_shortcut(&app_clone, take_binding);
            }
        });
    }
}
