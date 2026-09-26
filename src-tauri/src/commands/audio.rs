use crate::audio_feedback;
use crate::audio_toolkit::audio::{list_input_devices, list_output_devices};
use crate::managers::audio::{AudioRecordingManager, MicrophoneMode};
use crate::settings::{LiveTextMode, get_settings, write_settings};
use log::warn;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Serialize, Type)]
pub struct CustomSounds {
    start: bool,
    stop: bool,
}

fn custom_sound_exists(app: &AppHandle, sound_type: &str) -> bool {
    // T-114 finding #5: route through the portable-aware resolver instead of
    // resolving BaseDirectory::AppData directly, so a portable launch checks
    // its own `data\custom_*.wav` files instead of always looking in
    // %APPDATA%\pr.handy (which would both ignore a portable copy's own
    // custom sounds and pick up an installed copy's, if present).
    match crate::portable::resolve_app_data_dir(app) {
        Ok(dir) => dir.join(format!("custom_{}.wav", sound_type)).exists(),
        Err(_) => false,
    }
}

#[tauri::command]
#[specta::specta]
pub fn check_custom_sounds(app: AppHandle) -> CustomSounds {
    CustomSounds {
        start: custom_sound_exists(&app, "start"),
        stop: custom_sound_exists(&app, "stop"),
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct AudioDevice {
    pub index: String,
    pub name: String,
    pub is_default: bool,
}

#[tauri::command]
#[specta::specta]
pub fn update_microphone_mode(app: AppHandle, always_on: bool) -> Result<(), String> {
    // Update settings
    let mut settings = get_settings(&app);
    settings.always_on_microphone = always_on;
    write_settings(&app, settings);

    // Update the audio manager mode
    let rm = app.state::<Arc<AudioRecordingManager>>();
    let new_mode = if always_on {
        MicrophoneMode::AlwaysOn
    } else {
        MicrophoneMode::OnDemand
    };

    rm.update_mode(new_mode)
        .map_err(|e| format!("Failed to update microphone mode: {}", e))
}

/// Show the pause/resume button on the recording overlay (and allow the Pause shortcut).
#[tauri::command]
#[specta::specta]
pub fn change_pause_button_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.pause_button_enabled = enabled;
    write_settings(&app, settings);
    Ok(())
}

/// Allow the Undo-last-word shortcut during live takes.
#[tauri::command]
#[specta::specta]
pub fn change_undo_word_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.undo_word_enabled = enabled;
    write_settings(&app, settings);
    Ok(())
}

/// Switch the live text box on or off (from settings or the overlay's T button).
/// While on, every take runs live. Returns the new state.
#[tauri::command]
#[specta::specta]
pub fn change_live_text_box_setting(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let mut settings = get_settings(&app);
    // Remote engines get the audio only after stop, so a take with them is never
    // live; the settings page explains this and lists the models that work.
    if enabled
        && matches!(
            settings.selected_model.as_str(),
            "api-whisper" | "openrouter-transcription"
        )
    {
        return Err("The live text box needs a model that runs on this PC".to_string());
    }
    settings.live_text_box_enabled = enabled;
    write_settings(&app, settings);
    crate::overlay::refresh_live_text_window(&app);
    // Switched on during a take that started without it: that take goes live.
    if enabled {
        crate::actions::make_take_live(&app);
    }
    let _ = app.emit("live-text-box-changed", enabled);
    Ok(enabled)
}

/// Flip the live text box (the overlay's T button). Returns the new state.
#[tauri::command]
#[specta::specta]
pub fn toggle_live_text_box(app: AppHandle) -> Result<bool, String> {
    let enabled = !get_settings(&app).live_text_box_enabled;
    change_live_text_box_setting(app, enabled)
}

/// What the live text box shows: the last words, or the whole text so far.
#[tauri::command]
#[specta::specta]
pub fn change_live_text_mode_setting(app: AppHandle, mode: LiveTextMode) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.live_text_mode = mode;
    write_settings(&app, settings);
    crate::overlay::refresh_live_text_window(&app);
    Ok(())
}

/// Whether the live text box fades out after you stop talking.
#[tauri::command]
#[specta::specta]
pub fn change_live_text_fade_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.live_text_fade = enabled;
    write_settings(&app, settings);
    crate::overlay::refresh_live_text_window(&app);
    Ok(())
}

/// How wide the live text box is (logical pixels).
#[tauri::command]
#[specta::specta]
pub fn change_live_text_width_setting(app: AppHandle, width: u32) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.live_text_width = width.clamp(240, 1200);
    write_settings(&app, settings);
    crate::overlay::refresh_live_text_window(&app);
    Ok(())
}

/// Show the speed chip on the recording pill (it widens the pill to fit).
#[tauri::command]
#[specta::specta]
pub fn change_speed_indicator_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.speed_indicator_enabled = enabled;
    write_settings(&app, settings);
    crate::overlay::update_overlay_position(&app);
    Ok(())
}

/// Pause the take in progress, or resume it (the overlay's pause button).
#[tauri::command]
#[specta::specta]
pub fn toggle_pause_recording(app: AppHandle) {
    crate::actions::toggle_pause(&app);
}

/// Minutes the microphone stays open after a take in on-demand mode (0 = off).
#[tauri::command]
#[specta::specta]
pub fn change_mic_keep_warm_setting(app: AppHandle, minutes: u32) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.mic_keep_warm_minutes = minutes;
    write_settings(&app, settings);
    if minutes == 0 {
        // Don't leave a warm microphone open for the rest of its old window.
        app.state::<Arc<AudioRecordingManager>>()
            .release_warm_microphone();
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_microphone_mode(app: AppHandle) -> Result<bool, String> {
    let settings = get_settings(&app);
    Ok(settings.always_on_microphone)
}

#[tauri::command]
#[specta::specta]
pub fn get_available_microphones() -> Result<Vec<AudioDevice>, String> {
    let devices =
        list_input_devices().map_err(|e| format!("Failed to list audio devices: {}", e))?;

    let mut result = vec![AudioDevice {
        index: "default".to_string(),
        name: "Default".to_string(),
        is_default: true,
    }];

    result.extend(devices.into_iter().map(|d| AudioDevice {
        index: d.index,
        name: d.name,
        is_default: false, // The explicit default is handled separately
    }));

    Ok(result)
}

#[tauri::command]
#[specta::specta]
pub fn set_selected_microphone(app: AppHandle, device_name: String) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.selected_microphone = if device_name == "default" {
        None
    } else {
        Some(device_name)
    };
    write_settings(&app, settings);

    // Update the audio manager to use the new device
    let rm = app.state::<Arc<AudioRecordingManager>>();
    rm.update_selected_device()
        .map_err(|e| format!("Failed to update selected device: {}", e))?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_selected_microphone(app: AppHandle) -> Result<String, String> {
    let settings = get_settings(&app);
    Ok(settings
        .selected_microphone
        .unwrap_or_else(|| "default".to_string()))
}

#[tauri::command]
#[specta::specta]
pub fn get_available_output_devices() -> Result<Vec<AudioDevice>, String> {
    let devices =
        list_output_devices().map_err(|e| format!("Failed to list output devices: {}", e))?;

    let mut result = vec![AudioDevice {
        index: "default".to_string(),
        name: "Default".to_string(),
        is_default: true,
    }];

    result.extend(devices.into_iter().map(|d| AudioDevice {
        index: d.index,
        name: d.name,
        is_default: false, // The explicit default is handled separately
    }));

    Ok(result)
}

#[tauri::command]
#[specta::specta]
pub fn set_selected_output_device(app: AppHandle, device_name: String) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.selected_output_device = if device_name == "default" {
        None
    } else {
        Some(device_name)
    };
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_selected_output_device(app: AppHandle) -> Result<String, String> {
    let settings = get_settings(&app);
    Ok(settings
        .selected_output_device
        .unwrap_or_else(|| "default".to_string()))
}

#[tauri::command]
#[specta::specta]
pub async fn play_test_sound(app: AppHandle, sound_type: String) {
    let sound = match sound_type.as_str() {
        "start" => audio_feedback::SoundType::Start,
        "stop" => audio_feedback::SoundType::Stop,
        _ => {
            warn!("Unknown sound type: {}", sound_type);
            return;
        }
    };
    audio_feedback::play_test_sound(&app, sound);
}

#[tauri::command]
#[specta::specta]
pub fn set_clamshell_microphone(app: AppHandle, device_name: String) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.clamshell_microphone = if device_name == "default" {
        None
    } else {
        Some(device_name)
    };
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_clamshell_microphone(app: AppHandle) -> Result<String, String> {
    let settings = get_settings(&app);
    Ok(settings
        .clamshell_microphone
        .unwrap_or_else(|| "default".to_string()))
}

#[tauri::command]
#[specta::specta]
pub fn is_recording(app: AppHandle) -> bool {
    let audio_manager = app.state::<Arc<AudioRecordingManager>>();
    audio_manager.is_recording()
}
