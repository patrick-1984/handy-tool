//! Keyboard Typer: simulates typing arbitrary text into the focused window.
//!
//! Used for environments where copy/paste is unavailable (remote sessions,
//! VM consoles, password prompts). The text lives only in memory — it is
//! never written to settings or disk, so passwords don't persist.

use enigo::{Direction, Key, Keyboard};
use log::{info, warn};
use serde::Serialize;
use specta::Type;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::input::EnigoState;
use crate::settings;

pub const TYPING_STATUS_EVENT: &str = "typing-status";

/// Grace period after the countdown ends, so the modifier keys of the
/// triggering shortcut (or the mouse click on Go) are released before the
/// first simulated keystroke — otherwise a held Ctrl/Alt would turn typed
/// characters into hotkeys in the target application.
const MODIFIER_RELEASE_GRACE_MS: u64 = 600;

/// Delay used when typing is triggered via the global shortcut: the target
/// window already has focus, so only a short countdown is needed.
const SHORTCUT_TRIGGER_DELAY_SECS: u32 = 1;

#[derive(Serialize, Clone, Type)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TypingStatus {
    Countdown { seconds_left: u32 },
    Typing { typed: u32, total: u32 },
    Done { total: u32 },
    Cancelled,
    Error { message: String },
}

/// In-memory typing state managed by Tauri. The text is intentionally not
/// part of AppSettings so it can never be persisted.
pub struct TypingState {
    text: Mutex<String>,
    /// Bumping the generation cancels any in-flight session.
    generation: Arc<AtomicU64>,
    running: Arc<AtomicBool>,
}

impl TypingState {
    pub fn new() -> Self {
        Self {
            text: Mutex::new(String::new()),
            generation: Arc::new(AtomicU64::new(0)),
            running: Arc::new(AtomicBool::new(false)),
        }
    }
}

pub fn is_running(app: &AppHandle) -> bool {
    app.try_state::<TypingState>()
        .map(|s| s.running.load(Ordering::SeqCst))
        .unwrap_or(false)
}

pub fn cancel(app: &AppHandle) {
    if let Some(state) = app.try_state::<TypingState>() {
        if state.running.load(Ordering::SeqCst) {
            state.generation.fetch_add(1, Ordering::SeqCst);
            info!("Typing session cancelled");
        }
    }
}

/// Toggle handler for the global `type_text` shortcut: cancels a running
/// session, otherwise starts one with a short countdown (the target window
/// already has focus when the shortcut fires).
pub fn toggle_from_shortcut(app: &AppHandle) {
    if is_running(app) {
        cancel(app);
    } else if let Err(e) = start_session(app, SHORTCUT_TRIGGER_DELAY_SECS) {
        warn!("Could not start typing session from shortcut: {}", e);
    }
}

#[tauri::command]
#[specta::specta]
pub fn set_typing_text(app: AppHandle, text: String) -> Result<(), String> {
    let state = app.state::<TypingState>();
    let mut guard = state.text.lock().map_err(|_| "typing state poisoned")?;
    *guard = text;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn start_typing(app: AppHandle, delay_secs: u32) -> Result<(), String> {
    start_session(&app, delay_secs)
}

#[tauri::command]
#[specta::specta]
pub fn cancel_typing(app: AppHandle) -> Result<(), String> {
    cancel(&app);
    Ok(())
}

fn start_session(app: &AppHandle, delay_secs: u32) -> Result<(), String> {
    let state = app.state::<TypingState>();

    let text = state
        .text
        .lock()
        .map_err(|_| "typing state poisoned")?
        .clone();
    if text.is_empty() {
        return Err("No text to type".to_string());
    }

    // Make sure the input system exists before committing to a session.
    if app.try_state::<EnigoState>().is_none() {
        crate::commands::initialize_enigo(app.clone())?;
    }

    if state.running.swap(true, Ordering::SeqCst) {
        return Err("A typing session is already running".to_string());
    }
    let generation = state.generation.fetch_add(1, Ordering::SeqCst) + 1;

    // Allow Escape (the cancel binding) to abort while counting down/typing.
    // Skip if recording is in progress — the recorder owns the binding then.
    let recording = app
        .try_state::<Arc<crate::managers::audio::AudioRecordingManager>>()
        .map(|m| m.is_recording())
        .unwrap_or(false);
    if !recording {
        crate::shortcut::register_cancel_shortcut(app);
    }

    let app = app.clone();
    std::thread::spawn(move || {
        run_session(&app, generation, &text, delay_secs);

        let state = app.state::<TypingState>();
        state.running.store(false, Ordering::SeqCst);

        let still_recording = app
            .try_state::<Arc<crate::managers::audio::AudioRecordingManager>>()
            .map(|m| m.is_recording())
            .unwrap_or(false);
        if !still_recording {
            crate::shortcut::unregister_cancel_shortcut(&app);
        }
    });

    Ok(())
}

fn emit_status(app: &AppHandle, status: TypingStatus) {
    let _ = app.emit(TYPING_STATUS_EVENT, status);
}

fn cancelled(app: &AppHandle, generation: u64) -> bool {
    let state = app.state::<TypingState>();
    state.generation.load(Ordering::SeqCst) != generation
}

fn run_session(app: &AppHandle, generation: u64, text: &str, delay_secs: u32) {
    // Countdown, polling for cancellation every 100 ms.
    for seconds_left in (1..=delay_secs).rev() {
        emit_status(app, TypingStatus::Countdown { seconds_left });
        for _ in 0..10 {
            if cancelled(app, generation) {
                emit_status(app, TypingStatus::Cancelled);
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    std::thread::sleep(Duration::from_millis(MODIFIER_RELEASE_GRACE_MS));
    if cancelled(app, generation) {
        emit_status(app, TypingStatus::Cancelled);
        return;
    }

    let key_delay_ms = settings::get_settings(app).typing_key_delay_ms as u64;
    let total = text.chars().count() as u32;
    let mut typed: u32 = 0;
    emit_status(app, TypingStatus::Typing { typed, total });

    let enigo_state = app.state::<EnigoState>();
    for ch in text.chars() {
        if cancelled(app, generation) {
            emit_status(app, TypingStatus::Cancelled);
            return;
        }

        let result = {
            let mut enigo = match enigo_state.0.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            match ch {
                // CRLF: the \n press covers the line break, skip the \r.
                '\r' => Ok(()),
                '\n' => enigo
                    .key(Key::Return, Direction::Click)
                    .map_err(|e| e.to_string()),
                '\t' => enigo
                    .key(Key::Tab, Direction::Click)
                    .map_err(|e| e.to_string()),
                _ => enigo.text(&ch.to_string()).map_err(|e| e.to_string()),
            }
        };

        if let Err(message) = result {
            warn!("Typing failed at character {}: {}", typed, message);
            emit_status(app, TypingStatus::Error { message });
            return;
        }

        typed += 1;
        if typed % 5 == 0 || typed == total {
            emit_status(app, TypingStatus::Typing { typed, total });
        }
        std::thread::sleep(Duration::from_millis(key_delay_ms));
    }

    info!("Typing session completed ({} characters)", total);
    emit_status(app, TypingStatus::Done { total });
}
