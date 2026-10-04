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
    let rm = app.state::<Arc<AudioRecordingManager>>();
    let new_mode = if always_on {
        MicrophoneMode::AlwaysOn
    } else {
        MicrophoneMode::OnDemand
    };

    // Switch first, save after: an always-on setting saved while the microphone
    // cannot open would be tried again at every start.
    rm.update_mode(new_mode)
        .map_err(|e| format!("Failed to update microphone mode: {}", e))?;
    let mut settings = get_settings(&app);
    settings.always_on_microphone = always_on;
    write_settings(&app, settings);
    Ok(())
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

/// After a cold start, keep "Starting mic..." on the pill until the microphone
/// has warmed up (the wait measured on its recent cold starts).
#[tauri::command]
#[specta::specta]
pub fn change_mic_warmup_wait_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.mic_warmup_wait = enabled;
    write_settings(&app, settings);
    Ok(())
}

/// Show "Too quiet" on the pill when speech is too quiet to be kept.
#[tauri::command]
#[specta::specta]
pub fn change_too_quiet_hint_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.too_quiet_hint = enabled;
    write_settings(&app, settings);
    Ok(())
}

/// Show "Too quiet" in a box of its own under the pill instead of inside it.
#[tauri::command]
#[specta::specta]
pub fn change_too_quiet_hint_box_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.too_quiet_hint_box = enabled;
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

/// The pointer came onto the recording pill, or pressed it: note the window the
/// user dictates into (see `overlay::note_focus_before_pill`). Windows only.
#[tauri::command]
#[specta::specta]
pub fn overlay_note_focus() {
    crate::overlay::note_focus_before_pill();
}

/// A click or a right-click menu on the recording pill is done: hand the
/// foreground back if the pill took it. Windows only.
#[tauri::command]
#[specta::specta]
pub fn overlay_restore_focus() {
    crate::overlay::restore_focus_after_pill();
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

/// Show the live text box at stop for a take without it, with the transcript
/// typed in as it comes in.
#[tauri::command]
#[specta::specta]
pub fn change_live_text_after_stop_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.live_text_after_stop = enabled;
    write_settings(&app, settings);
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

/// The live text box's text size, in logical pixels.
#[tauri::command]
#[specta::specta]
pub fn change_live_text_font_size_setting(app: AppHandle, size: u32) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.live_text_font_size = size.clamp(11, 28);
    write_settings(&app, settings);
    crate::overlay::refresh_live_text_window(&app);
    Ok(())
}

/// The recording pill's size, in percent of its normal size.
#[tauri::command]
#[specta::specta]
pub fn change_pill_scale_setting(app: AppHandle, scale: u32) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.pill_scale = scale.clamp(100, 150);
    write_settings(&app, settings);
    crate::overlay::update_overlay_position(&app);
    Ok(())
}

/// How the pill shows the transcription's progress.
#[tauri::command]
#[specta::specta]
pub fn change_progress_style_setting(
    app: AppHandle,
    style: crate::settings::ProgressStyle,
) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.progress_style = style;
    write_settings(&app, settings);
    Ok(())
}

/// How strong the progress glow is, in percent of its normal strength.
#[tauri::command]
#[specta::specta]
pub fn change_progress_glow_setting(app: AppHandle, strength: u32) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.progress_glow = strength.min(200);
    write_settings(&app, settings);
    Ok(())
}

/// The progress light's colour ("#rrggbb", or empty for the default cyan).
#[tauri::command]
#[specta::specta]
pub fn change_progress_color_setting(app: AppHandle, color: String) -> Result<(), String> {
    let valid = color.is_empty()
        || (color.len() == 7
            && color.starts_with('#')
            && color[1..].chars().all(|c| c.is_ascii_hexdigit()));
    if !valid {
        return Err(format!("Not a colour: {color}"));
    }
    let mut settings = get_settings(&app);
    settings.progress_color = color.to_ascii_lowercase();
    write_settings(&app, settings);
    Ok(())
}

/// Wider sound bars on the pill.
#[tauri::command]
#[specta::specta]
pub fn change_sound_bars_wide_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.sound_bars_wide = enabled;
    write_settings(&app, settings);
    Ok(())
}

/// Progress Style: line - whether the line glows too.
#[tauri::command]
#[specta::specta]
pub fn change_progress_line_glow_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.progress_line_glow = enabled;
    write_settings(&app, settings);
    Ok(())
}

/// "Whole text": how many lines the live text box shows.
#[tauri::command]
#[specta::specta]
pub fn change_live_text_lines_setting(app: AppHandle, lines: u32) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.live_text_lines = lines.clamp(2, 30);
    write_settings(&app, settings);
    crate::overlay::refresh_live_text_window(&app);
    Ok(())
}

/// Open the app on the page that was open when it was closed.
#[tauri::command]
#[specta::specta]
pub fn change_reopen_last_page_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.reopen_last_page = enabled;
    write_settings(&app, settings);
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
