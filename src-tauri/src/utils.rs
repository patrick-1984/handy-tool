use crate::TranscriptionCoordinator;
use crate::managers::audio::AudioRecordingManager;
use crate::managers::transcription::TranscriptionManager;
use crate::shortcut;
use log::info;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

// Re-export all utility modules for easy access
// pub use crate::audio_feedback::*;
pub use crate::clipboard::*;
pub use crate::overlay::*;
pub use crate::tray::*;

/// Centralized cancellation function that can be called from anywhere in the app.
/// Handles cancelling both recording and transcription operations and updates UI state.
pub fn cancel_current_operation(app: &AppHandle) {
    info!("Initiating operation cancellation...");

    // Unregister the cancel shortcut asynchronously
    shortcut::unregister_cancel_shortcut(app);

    // Cancel any ongoing recording
    let audio_manager = app.state::<Arc<AudioRecordingManager>>();
    let recording_was_active = audio_manager.is_recording();
    audio_manager.cancel_recording();

    // Finding 7 (T-101 follow-up): bump the take-cancellation generation so
    // an in-flight pipeline — already past start(), possibly mid-transcribe
    // or mid-post-process, and already holding its OWN copies of
    // `delivery_intent`/`post_take_action` by value — skips its paste/
    // post-take-action dispatch instead of running them after the user
    // cancelled. Clearing the globals below only protects the NEXT take; a
    // take whose intent was already captured by value at stop() time is
    // untouched by that clear, which is exactly the gap this closes.
    crate::actions::cancel_take_generation();

    // A cancelled take's anchored-delivery request and deferred on-finish
    // action must die with it — stranded, they would hijack a later paste.
    crate::anchor::clear_delivery_request();
    crate::anchor::clear_post_take_action();

    // Tear down any chunked-recording session (clears the chunk callback and
    // re-enables model unloading before maybe_unload_immediately below).
    crate::actions::end_chunked_session(app);

    // Update tray icon and hide overlay
    change_tray_icon(app, crate::tray::TrayIconState::Idle);
    hide_recording_overlay(app);

    // Unload model if immediate unload is enabled
    let tm = app.state::<Arc<TranscriptionManager>>();
    tm.maybe_unload_immediately("cancellation");

    // Notify coordinator so it can keep lifecycle state coherent.
    if let Some(coordinator) = app.try_state::<TranscriptionCoordinator>() {
        coordinator.notify_cancel(recording_was_active);
    }

    info!("Operation cancellation completed - returned to idle state");
}

/// Check if using the Wayland display server protocol
#[cfg(target_os = "linux")]
pub fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v.to_lowercase() == "wayland")
            .unwrap_or(false)
}

/// Check if running on KDE Plasma desktop environment
#[cfg(target_os = "linux")]
pub fn is_kde_plasma() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .map(|v| v.to_uppercase().contains("KDE"))
        .unwrap_or(false)
        || std::env::var("KDE_SESSION_VERSION").is_ok()
}

/// Check if running on KDE Plasma with Wayland
#[cfg(target_os = "linux")]
pub fn is_kde_wayland() -> bool {
    is_wayland() && is_kde_plasma()
}
