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

/// X11 auto-repeat coalescing (T-209): a held key under X11 auto-repeat is
/// delivered as synthetic release→press pairs, not one long press. A PTT
/// release is parked for this grace window and cancelled by the matching
/// auto-repeat press; only a release that survives the window (a genuine
/// key-up) actually stops the recording.
#[cfg(target_os = "linux")]
const PTT_RELEASE_GRACE: Duration = Duration::from_millis(40);

/// Commands processed sequentially by the coordinator thread.
enum Command {
    Input {
        binding_id: String,
        hotkey_string: String,
        is_pressed: bool,
        /// T-113 (finding 9): timestamped at `send_input()` — enqueue time —
        /// so the coordinator can log the QUEUE delay at dequeue, closing
        /// the previously-uninstrumented shortcut→coordinator latency gap
        /// (the existing timing only started AFTER dequeue).
        enqueued_at: Instant,
    },
    Cancel {
        recording_was_active: bool,
    },
    ProcessingFinished,
    /// Commit a parked PTT release whose X11 auto-repeat grace window expired
    /// without a matching press (T-209). Stale if `generation` no longer
    /// matches the parked entry.
    #[cfg(target_os = "linux")]
    CommitPttRelease {
        binding_id: String,
        hotkey_string: String,
        generation: u64,
    },
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

        // The coordinator thread sends itself delayed CommitPttRelease
        // commands for parked releases (T-209).
        #[cfg(target_os = "linux")]
        let thread_tx = tx.clone();

        thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut stage = Stage::Idle;
                // Per-binding: a rapid press of binding B must not be eaten by
                // a just-processed press of binding A.
                let mut last_press: std::collections::HashMap<String, Instant> =
                    std::collections::HashMap::new();
                // Parked PTT releases awaiting their grace window, keyed by
                // binding, valued by generation so a stale delayed commit
                // can never fire a newer parked release (T-209).
                #[cfg(target_os = "linux")]
                let mut parked_ptt_release: std::collections::HashMap<String, u64> =
                    std::collections::HashMap::new();
                #[cfg(target_os = "linux")]
                let mut ptt_release_generation: u64 = 0;

                while let Ok(cmd) = rx.recv() {
                    match cmd {
                        Command::Input {
                            binding_id,
                            hotkey_string,
                            is_pressed,
                            enqueued_at,
                        } => {
                            // T-113 (finding 9): queue delay from send_input()
                            // to this dequeue — the gap upstream of every
                            // other timing point already in this file.
                            debug!(
                                "T-113: coordinator dequeued Input for '{binding_id}' (pressed={is_pressed}) after {:?} queue delay",
                                enqueued_at.elapsed()
                            );
                            let push_to_talk = is_ptt_binding(&binding_id);
                            // A press for a binding whose release is parked is
                            // X11 auto-repeat: cancel the parked release and
                            // swallow the press — the recording simply keeps
                            // going, no start actions replay (T-209). Checked
                            // BEFORE the press debounce so a debounced repeat
                            // press still cancels its parked release.
                            #[cfg(target_os = "linux")]
                            if push_to_talk
                                && is_pressed
                                && matches!(&stage, Stage::Recording(id) if id == &binding_id)
                                && parked_ptt_release.remove(&binding_id).is_some()
                            {
                                debug!("Coalesced X11 auto-repeat press for '{binding_id}'");
                                continue;
                            }
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
                                    timed_idle_start(
                                        &app,
                                        &mut stage,
                                        &binding_id,
                                        &hotkey_string,
                                        s.anchor_action_output_idle,
                                        s.anchor_action_output_idle_slot,
                                        &s.jumper_save_cursor_slots,
                                    );
                                } else if !is_pressed
                                    && matches!(&stage, Stage::Recording(id) if id == &binding_id)
                                {
                                    // X11: park the release instead of stopping.
                                    // A matching auto-repeat press within the
                                    // grace window cancels it; otherwise it
                                    // commits via CommitPttRelease (T-209).
                                    #[cfg(target_os = "linux")]
                                    {
                                        ptt_release_generation += 1;
                                        parked_ptt_release
                                            .insert(binding_id.clone(), ptt_release_generation);
                                        let tx = thread_tx.clone();
                                        let generation = ptt_release_generation;
                                        let binding_id = binding_id.clone();
                                        let hotkey_string = hotkey_string.clone();
                                        thread::spawn(move || {
                                            thread::sleep(PTT_RELEASE_GRACE);
                                            let _ = tx.send(Command::CommitPttRelease {
                                                binding_id,
                                                hotkey_string,
                                                generation,
                                            });
                                        });
                                    }
                                    #[cfg(not(target_os = "linux"))]
                                    {
                                        let s = crate::settings::get_settings(&app);
                                        perform_anchor_action(
                                            &app,
                                            s.anchor_action_output_stop,
                                            s.anchor_action_output_stop_slot,
                                            true,
                                            &s.jumper_save_cursor_slots,
                                        );
                                        stop(&app, &mut stage, &binding_id, &hotkey_string);
                                    }
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
                                            // T-302 #3: the on-finish jump/anchor action fires
                                            // only when the take STARTED and FINISHED via the
                                            // same flow. Started plain Transcribe, finished
                                            // Transcribe&Submit → skip the jump (but still
                                            // submit) when require-same-flow is on.
                                            let same_flow = rec_id == "transcribe_and_submit";
                                            if !(s.anchor_on_finish_require_same_flow && !same_flow)
                                            {
                                                perform_anchor_action(
                                                    &app,
                                                    s.anchor_action_submit_stop,
                                                    s.anchor_action_submit_stop_slot,
                                                    true,
                                                    &s.jumper_save_cursor_slots,
                                                );
                                            }
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
                                            // T-113: time from dispatch, since the
                                            // anchor side-action runs even under
                                            // DoNothing — it's what makes the key useful
                                            // as a pure jump/anchor button — and is
                                            // suspect #4 for start-latency reports
                                            // (Jump's activate_verified poll ladder).
                                            let t0 = Instant::now();
                                            perform_anchor_action(
                                                &app,
                                                s.anchor_action_submit_idle,
                                                s.anchor_action_submit_idle_slot,
                                                false,
                                                &s.jumper_save_cursor_slots,
                                            );
                                            debug!(
                                                "T-113: perform_anchor_action (submit-idle) took {:?}",
                                                t0.elapsed()
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
                                                debug!(
                                                    "T-113: Idle→start dispatch for '{binding_id}' took {:?} total",
                                                    t0.elapsed()
                                                );
                                            }
                                        }
                                        None => {
                                            debug!("Ignoring submit press: pipeline busy");
                                            crate::audio_feedback::play_busy_beep(&app);
                                        }
                                    }
                                } else {
                                    // Plain Transcribe (or post-process Transcribe): FINISH a
                                    // recording started by ANY binding, mirroring the Transcribe &
                                    // Submit branch above — so a take started in Transcribe & Submit
                                    // can be finished with plain Transcribe (and vice-versa). The
                                    // old `Recording(id) if id == &binding_id` only matched the
                                    // SAME binding, so a submit-started take fell into the
                                    // "pipeline busy" arm here.
                                    let active = if let Stage::Recording(id) = &stage {
                                        Some(id.clone())
                                    } else {
                                        None
                                    };
                                    match active {
                                        Some(rec_id) => {
                                            let s = crate::settings::get_settings(&app);
                                            // On-finish jump/anchor only when started AND finished
                                            // via the same flow (T-302 #3). Output-style bindings
                                            // (plain / post-process / PTT) are the "output" flow;
                                            // only a submit-started take differs.
                                            let same_flow = rec_id != "transcribe_and_submit";
                                            if !(s.anchor_on_finish_require_same_flow && !same_flow)
                                            {
                                                perform_anchor_action(
                                                    &app,
                                                    s.anchor_action_output_stop,
                                                    s.anchor_action_output_stop_slot,
                                                    true,
                                                    &s.jumper_save_cursor_slots,
                                                );
                                            }
                                            // MUST stop the recorder's ACTUAL owner: stop_recording
                                            // only stops when the id matches the one stored at
                                            // start (audio.rs), else the recorder keeps running
                                            // while we move to Processing. No submit override is
                                            // armed (start() cleared it), so this is a plain output
                                            // paste with no submit key.
                                            stop(&app, &mut stage, &rec_id, &hotkey_string);
                                        }
                                        None if matches!(stage, Stage::Idle) => {
                                            let s = crate::settings::get_settings(&app);
                                            timed_idle_start(
                                                &app,
                                                &mut stage,
                                                &binding_id,
                                                &hotkey_string,
                                                s.anchor_action_output_idle,
                                                s.anchor_action_output_idle_slot,
                                                &s.jumper_save_cursor_slots,
                                            );
                                        }
                                        None => {
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
                            // A parked release belongs to the recording that just
                            // ended — dropping it here keeps a stale sleeper commit
                            // from stopping the NEXT recording (its generation
                            // would otherwise still match).
                            #[cfg(target_os = "linux")]
                            parked_ptt_release.clear();
                            schedule_unmute_cleanup(&app);
                        }
                        Command::ProcessingFinished => {
                            stage = Stage::Idle;
                            publish_stage(&stage);
                            #[cfg(target_os = "linux")]
                            parked_ptt_release.clear();
                            schedule_unmute_cleanup(&app);
                        }
                        #[cfg(target_os = "linux")]
                        Command::CommitPttRelease {
                            binding_id,
                            hotkey_string,
                            generation,
                        } => {
                            // Superseded: a matching auto-repeat press cancelled
                            // this parked release, or a newer release re-parked.
                            if parked_ptt_release.get(&binding_id) != Some(&generation) {
                                debug!("Ignoring stale parked PTT release for '{binding_id}'");
                                continue;
                            }
                            parked_ptt_release.remove(&binding_id);
                            // Genuine key-up: run the exact stop path a direct
                            // release would have taken (anchor action + stop),
                            // guarded so it stops the matching recording once.
                            if matches!(&stage, Stage::Recording(id) if id == &binding_id) {
                                let s = crate::settings::get_settings(&app);
                                perform_anchor_action(
                                    &app,
                                    s.anchor_action_output_stop,
                                    s.anchor_action_output_stop_slot,
                                    true,
                                    &s.jumper_save_cursor_slots,
                                );
                                stop(&app, &mut stage, &binding_id, &hotkey_string);
                            }
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
                enqueued_at: Instant::now(),
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

/// Run the flow's configured Jumper side-action, against the configured slot
/// (0 = hot, 1–4 static). Two moments:
/// - ON START (`at_finish == false`, the press that begins a sequence):
///   the action runs immediately — Jump navigates NOW.
/// - ON FINISH (`at_finish == true`, the press that ends the take): Jump arms
///   verified DELIVERY (the paste itself IS the finish); Set/Clear are
///   DEFERRED and run only after the paste has fully completed — "finished"
///   means text delivered and clipboard restored, not "stop was pressed".
fn perform_anchor_action(
    app: &AppHandle,
    action: AnchorAction,
    slot: u8,
    at_finish: bool,
    save_cursor_slots: &[bool],
) {
    let slot = (slot as usize).min(crate::anchor::SLOT_COUNT - 1);
    let save_cursor = crate::anchor::slot_save_cursor(save_cursor_slots, slot);
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
            if at_finish {
                // T-301: carry the DRIVING flow's cursor policy (resolved by
                // the caller — submit flow → submit toggle, output flow →
                // output toggle) into the deferred on-finish Set.
                crate::anchor::arm_post_take_action(
                    crate::anchor::PostTakeAction::Set,
                    slot,
                    save_cursor,
                );
            } else if let Err(e) = crate::anchor::set_slot(app, slot) {
                warn!("Jumper action set failed: {}", e);
            }
        }
        AnchorAction::Clear => {
            if at_finish {
                crate::anchor::arm_post_take_action(
                    crate::anchor::PostTakeAction::Clear,
                    slot,
                    save_cursor,
                );
            } else {
                crate::anchor::clear(app, slot);
            }
        }
    }
}

/// T-113 instrumentation: the Idle→Recording dispatch, timed end-to-end —
/// the configured on-START anchor side-action (suspect #4: Jump's
/// `activate_verified` poll ladder can synchronously cost up to ~700ms ×3
/// escalation steps) followed by `start()` itself (which times its own
/// stages down into `try_start_recording`/mic-open — see managers/audio.rs
/// and actions.rs). DEBUG-only logging; adds no blocking work of its own.
fn timed_idle_start(
    app: &AppHandle,
    stage: &mut Stage,
    binding_id: &str,
    hotkey_string: &str,
    action: AnchorAction,
    slot: u8,
    save_cursor_slots: &[bool],
) {
    let t0 = Instant::now();
    perform_anchor_action(app, action, slot, false, save_cursor_slots);
    debug!(
        "T-113: perform_anchor_action (start, '{binding_id}') took {:?}",
        t0.elapsed()
    );
    start(app, stage, binding_id, hotkey_string);
    debug!(
        "T-113: Idle→start dispatch for '{binding_id}' took {:?} total",
        t0.elapsed()
    );
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
    // skipped because the transcription came back empty), nor a stale deferred
    // on-finish action.
    crate::anchor::clear_delivery_request();
    crate::anchor::clear_post_take_action();
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
