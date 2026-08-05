use crate::TranscriptionCoordinator;
use crate::managers::audio::AudioRecordingManager;
use crate::managers::transcription::TranscriptionManager;
use crate::shortcut;
use log::{info, warn};
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

    // Commit take-cancellation BEFORE tearing down the recording device (T-306
    // defense-in-depth): cancel_recording stops the mic stream and joins the
    // recorder worker; committing the generation bump + intent clears first
    // guarantees an in-flight pipeline skips its dispatch even if that teardown
    // ever stalls.
    //
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

    // Everything above is the SUPPRESSION PROLOGUE and runs for BOTH cancel
    // behaviors, synchronously on this thread, before anything can be queued.
    // That ordering is what makes `FinishSilently` race-free: whatever the
    // coordinator is doing — even if an Input command already queued ahead of
    // us moves the take to Processing first — the generation bump above has
    // already invalidated that take's snapshot, so its paste and deferred
    // action are suppressed no matter which stage the command below lands in.
    //
    // `FinishSilently` (default since 0.63.0): don't throw the take away —
    // finish it like a normal press so it is transcribed and saved to history,
    // and deliver nothing. The coordinator owns the recording lifecycle and is
    // the only component that can read `Stage` coherently, so hand off to it
    // and do NOT run any of the discard teardown below (cancelling the
    // recorder, ending the chunked session, resetting tray/overlay or
    // unloading the model would all sabotage the very take we are finishing).
    if crate::settings::get_settings(app).cancel_behavior
        == crate::settings::CancelBehavior::FinishSilently
    {
        match app.try_state::<TranscriptionCoordinator>() {
            Some(coordinator) => {
                info!("Cancel → finish silently: keeping the transcript, delivering nothing");
                // Arm the "deliver nothing" marker HERE, synchronously, BEFORE
                // the command is queued — this ordering is the fix for a real
                // fail-open race and must not be moved into the coordinator's
                // FinishSilently arm.
                //
                // A normal finishing Input (a Transcribe press, a PTT release)
                // can already be sitting in the coordinator's queue AHEAD of
                // our command. The coordinator would run that first: it calls
                // the ordinary `stop(..., silent = false)`, whose
                // `snapshot_take_generation()` runs AFTER the bump above and
                // therefore ABSORBS it (the "finding 7(a)" trap documented at
                // that snapshot). Our FinishSilently command would then arrive
                // to find `Stage::Processing` and do nothing — and the take the
                // user just cancelled would paste, submit and jump.
                //
                // Arming before queueing removes the ordering dependency
                // entirely: whichever `stop()` the coordinator reaches next
                // consumes the marker and finishes silently.
                crate::actions::arm_silent_take();
                if coordinator.notify_finish_silently() {
                    return;
                }
                // The coordinator is gone (channel closed, e.g. its thread
                // panicked). Nothing will consume the marker or stop the
                // recorder, so fall through to the discard teardown rather
                // than leave the microphone running forever.
                warn!(
                    "Cancel → finish silently could not reach the coordinator; discarding this take instead"
                );
                crate::actions::clear_silent_take();
            }
            None => {
                // No coordinator (should not happen once setup has run) — fall
                // through to the discard path rather than leaving a recording
                // running forever.
                warn!(
                    "Cancel → finish silently requested but the coordinator is unavailable; discarding instead"
                );
            }
        }
    }

    // Cancel any ongoing recording
    let audio_manager = app.state::<Arc<AudioRecordingManager>>();
    let recording_was_active = audio_manager.is_recording();
    audio_manager.cancel_recording();

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
