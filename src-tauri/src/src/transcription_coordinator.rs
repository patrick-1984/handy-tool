use crate::actions::ACTION_MAP;
use crate::managers::audio::AudioRecordingManager;
use crate::settings::{AnchorAction, SubmitIdleBehavior};
use log::{debug, error, warn};
use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

const DEBOUNCE: Duration = Duration::from_millis(30);

/// Commands processed sequentially by the coordinator thread.
enum Command {
    Input {
        binding_id: String,
        hotkey_string: String,
        is_pressed: bool,
    },
    Cancel {
        recording_was_active: bool,
    },
    ProcessingFinished,
}

/// Pipeline lifecycle, owned exclusively by the coordinator thread.
enum Stage {
    Idle,
    Recording(String), // binding_id
    Processing,
}

/// Mirror of the coordinator's current stage, readable from any thread —
/// 0 = Idle, 1 = Recording, 2 = Processing. The Translator's batch worker
/// uses this to yield the engine to live dictation. Advisory only: it can
/// lag the coordinator by an instruction, so engine calls must still
/// serialize through `actions::CHUNK_TRANSCRIBE_LOCK`.
static PIPELINE_STAGE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub const STAGE_IDLE: u8 = 0;
pub const STAGE_RECORDING: u8 = 1;
pub const STAGE_PROCESSING: u8 = 2;

pub fn pipeline_stage() -> u8 {
    PIPELINE_STAGE.load(std::sync::atomic::Ordering::SeqCst)
}

fn publish_stage(stage: &Stage) {
    let v = match stage {
        Stage::Idle => STAGE_IDLE,
        Stage::Recording(_) => STAGE_RECORDING,
        Stage::Processing => STAGE_PROCESSING,
    };
    PIPELINE_STAGE.store(v, std::sync::atomic::Ordering::SeqCst);
}

/// Serialises all transcription lifecycle events through a single thread
/// to eliminate race conditions between keyboard shortcuts, signals, and
/// the async transcribe-paste pipeline.
pub struct TranscriptionCoordinator {
    tx: Sender<Command>,
}

pub fn is_transcribe_binding(id: &str) -> bool {
    id == "transcribe"
        || id == "transcribe_ptt"
        || id == "transcribe_with_post_process"
        || id == "transcribe_and_submit"
}

fn is_ptt_binding(id: &str) -> bool {
    id == "transcribe_ptt"
}

impl TranscriptionCoordinator {
    pub fn new(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut stage = Stage::Idle;
                // Per-binding: a rapid press of binding B must not be eaten by
                // a just-processed press of binding A.
                let mut last_press: std::collections::HashMap<String, Instant> =
                    std::collections::HashMap::new();

                while let Ok(cmd) = rx.recv() {
                    match cmd {
                        Command::Input {
                            binding_id,
                            hotkey_string,
                            is_pressed,
                        } => {
                            let push_to_talk = is_ptt_binding(&binding_id);
                            // Debounce rapid-fire press events (key repeat / double-tap).
                            // Releases always pass through for push-to-talk.
                            if is_pressed {
                                let now = Instant::now();
                                if last_press
                                    .get(&binding_id)
                                    .map_or(false, |t| now.duration_since(*t) < DEBOUNCE)
                                {
                                    debug!("Debounced press for '{binding_id}'");
                                    continue;
                                }
                                last_press.insert(binding_id.clone(), now);
                            }

                            if push_to_talk {
                                if is_pressed && matches!(stage, Stage::Idle) {
                                    // PTT belongs to the typical-output flow —
                                    // same anchor side-actions as the toggle.
                                    let s = crate::settings::get_settings(&app);
                                    perform_anchor_action(
                                        &app,
                                        s.anchor_action_output_idle,
                                        s.anchor_action_output_idle_slot,
                                        false,
                                    );
                                    start(&app, &mut stage, &binding_id, &hotkey_string);
                                } else if !is_pressed
                                    && matches!(&stage, Stage::Recording(id) if id == &binding_id)
                                {
                                    let s = crate::settings::get_settings(&app);
                                    perform_anchor_action(
                                        &app,
                                        s.anchor_action_output_stop,
                                        s.anchor_action_output_stop_slot,
                                        true,
                                    );
                                    stop(&app, &mut stage, &binding_id, &hotkey_string);
                                } else if is_pressed {
                                    // Held PTT while the pipeline is busy: the
                                    // utterance will NOT be recorded — say so.
                                    debug!("PTT press while busy — beeping");
                                    crate::audio_feedback::play_busy_beep(&app);
                                }
                            } else if is_pressed {
                                if binding_id == "transcribe_and_submit" {
                                    // FINISH a recording started by ANY binding, or (when
                                    // idle) act per `submit_idle_behavior`.
                                    let active = if let Stage::Recording(id) = &stage {
                                        Some(id.clone())
                                    } else {
                                        None
                                    };
                                    match active {
                                        Some(rec_id) => {
                                            // Finishing via this shortcut is its whole contract:
                                            // ALWAYS paste with its configured method, press its
                                            // submit key, and apply its clipboard handling — no
                                            // matter which binding started the recording.
                                            let s = crate::settings::get_settings(&app);
                                            perform_anchor_action(
                                                &app,
                                                s.anchor_action_submit_stop,
                                                s.anchor_action_submit_stop_slot,
                                                true,
                                            );
                                            crate::clipboard::set_submit_override(
                                                crate::clipboard::SubmitOverride {
                                                    submit: Some((
                                                        s.submit_paste_method,
                                                        s.submit_key,
                                                    )),
                                                    clipboard: Some(s.submit_clipboard_handling),
                                                    restore_extra_ms: s
                                                        .submit_clipboard_restore_delay
                                                        .to_ms(),
                                                },
                                            );
                                            stop(&app, &mut stage, &rec_id, &hotkey_string);
                                        }
                                        None if matches!(stage, Stage::Idle) => {
                                            let s = crate::settings::get_settings(&app);
                                            // The anchor side-action runs even under
                                            // DoNothing — it's what makes the key useful
                                            // as a pure jump/anchor button.
                                            perform_anchor_action(
                                                &app,
                                                s.anchor_action_submit_idle,
                                                s.anchor_action_submit_idle_slot,
                                                false,
                                            );
                                            if s.submit_idle_behavior
                                                == SubmitIdleBehavior::DoNothing
                                            {
                                                debug!("Submit press while idle: doing nothing");
                                            } else {
                                                start(
                                                    &app,
                                                    &mut stage,
                                                    &binding_id,
                                                    &hotkey_string,
                                                );
                                            }
                                        }
                                        None => {
                                            debug!("Ignoring submit press: pipeline busy");
                                            crate::audio_feedback::play_busy_beep(&app);
                                        }
                                    }
                                } else {
                                    match &stage {
                                        Stage::Idle => {
                                            let s = crate::settings::get_settings(&app);
                                            perform_anchor_action(
                                                &app,
                                                s.anchor_action_output_idle,
                                                s.anchor_action_output_idle_slot,
                                                false,
                                            );
                                            start(&app, &mut stage, &binding_id, &hotkey_string);
                                        }
                                        Stage::Recording(id) if id == &binding_id => {
                                            let s = crate::settings::get_settings(&app);
                                            perform_anchor_action(
                                                &app,
                                                s.anchor_action_output_stop,
                                                s.anchor_action_output_stop_slot,
                                                true,
                                            );
                                            stop(&app, &mut stage, &binding_id, &hotkey_string);
                                        }
                                        _ => {
                                            debug!(
                                                "Ignoring press for '{binding_id}': pipeline busy"
                                            );
                                            crate::audio_feedback::play_busy_beep(&app);
                                        }
                                    }
                                }
                            }
                        }
                        Command::Cancel {
                            recording_was_active,
                        } => {
                            // Don't reset during processing — wait for the pipeline to finish.
                            if !matches!(stage, Stage::Processing)
                                && (recording_was_active || matches!(stage, Stage::Recording(_)))
                            {
                                stage = Stage::Idle;
                                publish_stage(&stage);
                            }
                            schedule_unmute_cleanup(&app);
                        }
                        Command::ProcessingFinished => {
                            stage = Stage::Idle;
                            publish_stage(&stage);
                            schedule_unmute_cleanup(&app);
                        }
                    }
                }
                debug!("Transcription coordinator exited");
            }));
            if let Err(e) = result {
                error!("Transcription coordinator panicked: {e:?}");
            }
        });

        Self { tx }
    }

    /// Send a keyboard/signal input event for a transcribe binding.
    /// PTT behavior is derived from the binding_id (transcribe_ptt = PTT mode).
    pub fn send_input(&self, binding_id: &str, hotkey_string: &str, is_pressed: bool) {
        if self
            .tx
            .send(Command::Input {
                binding_id: binding_id.to_string(),
                hotkey_string: hotkey_string.to_string(),
                is_pressed,
            })
            .is_err()
        {
            warn!("Transcription coordinator channel closed");
        }
    }

    pub fn notify_cancel(&self, recording_was_active: bool) {
        if self
            .tx
            .send(Command::Cancel {
                recording_was_active,
            })
            .is_err()
        {
            warn!("Transcription coordinator channel closed");
        }
    }

    pub fn notify_processing_finished(&self) {
        if self.tx.send(Command::ProcessingFinished).is_err() {
            warn!("Transcription coordinator channel closed");
        }
    }
}

/// Run the flow's configured Jumper side-action for this press, against the
/// configured slot (0 = hot, 1–4 static). `at_finish` distinguishes the two
/// moments: on an idle press, Jump navigates NOW; on the finish press, Jump
/// arms verified DELIVERY into that slot's target for the upcoming paste.
fn perform_anchor_action(app: &AppHandle, action: AnchorAction, slot: u8, at_finish: bool) {
    let slot = (slot as usize).min(crate::anchor::SLOT_COUNT - 1);
    match action {
        AnchorAction::None => {}
        AnchorAction::Jump => {
            if at_finish {
                crate::anchor::request_delivery(slot);
            } else if let Err(e) = crate::anchor::jump(app, slot) {
                warn!("Jumper action jump failed: {}", e);
            }
        }
        AnchorAction::Set => {
            if let Err(e) = crate::anchor::set_slot(app, slot) {
                warn!("Jumper action set failed: {}", e);
            }
        }
        AnchorAction::Clear => crate::anchor::clear(app, slot),
    }
}

/// The always-on start-feedback thread applies the mute AFTER its sound
/// finishes; a quick tap can land that mute just after stop() already unmuted
/// (TOCTOU), stranding system audio muted. Once a recording's lifecycle is
/// over, definitively unmute after any feedback sound could have finished.
fn schedule_unmute_cleanup(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        if let Some(rm) = app.try_state::<Arc<AudioRecordingManager>>() {
            if !rm.is_recording() {
                rm.remove_mute();
            }
        }
    });
}

fn start(app: &AppHandle, stage: &mut Stage, binding_id: &str, hotkey_string: &str) {
    // A fresh recording carries no submit intent until a submit shortcut finishes it.
    crate::clipboard::clear_submit_override();
    // Nor a stale anchored-delivery request (e.g. from a take whose paste was
    // skipped because the transcription came back empty).
    crate::anchor::clear_delivery_request();
    let Some(action) = ACTION_MAP.get(binding_id) else {
        warn!("No action in ACTION_MAP for '{binding_id}'");
        return;
    };
    action.start(app, binding_id, hotkey_string);
    if app
        .try_state::<Arc<AudioRecordingManager>>()
        .map_or(false, |a| a.is_recording())
    {
        *stage = Stage::Recording(binding_id.to_string());
        publish_stage(stage);
    } else {
        debug!("Start for '{binding_id}' did not begin recording; staying idle");
    }
}

fn stop(app: &AppHandle, stage: &mut Stage, binding_id: &str, hotkey_string: &str) {
    let Some(action) = ACTION_MAP.get(binding_id) else {
        warn!("No action in ACTION_MAP for '{binding_id}'");
        return;
    };
    action.stop(app, binding_id, hotkey_string);
    *stage = Stage::Processing;
    publish_stage(stage);
}
