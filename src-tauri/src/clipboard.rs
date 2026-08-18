use crate::input::{self, EnigoState};
#[cfg(target_os = "linux")]
use crate::settings::TypingTool;
use crate::settings::{AutoSubmitKey, ClipboardHandling, PasteMethod, get_settings};
use enigo::{Direction, Enigo, Key, Keyboard};
use log::info;
use once_cell::sync::Lazy;
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

#[cfg(target_os = "linux")]
use crate::utils::{is_kde_wayland, is_wayland};

/// One-shot paste override armed when the "Transcribe & Submit" shortcut finishes
/// a recording.
///
/// T-116 (same take-ownership treatment T-101 gave `DeliveryIntent`): this
/// static is now ONLY the arming mailbox between the coordinator's finishing
/// press and the take's `stop()` — `actions.rs` consumes it into an owned,
/// take-scoped value via `take_submit_override()` synchronously at the SAME
/// point it captures `delivery_intent`/`post_take_action`/`take_gen` (before
/// the async pipeline spawns), and threads it BY VALUE through `paste()` into
/// `paste_inner`. Previously `paste_inner` itself called `take_submit_override()`
/// lazily, at actual-paste time — a process-global one-shot consumed whenever
/// the NEXT `paste()` happened to run. That let a pathologically delayed OLD
/// take's paste consume a NEWER take's override (pasting with the wrong
/// method/submit key), and let a new take's `start()`-time
/// `clear_submit_override()` strip an already-stopped take's override out from
/// under it. Capturing by value at stop() closes both: nothing is left in the
/// global for a later take to pick up, and a take that already owns its copy
/// can't have it erased by a later clear.
///
/// - `submit`: when `Some`, force this paste method and always send the submit key
///   (independent of the global `auto_submit`).
/// - `clipboard`: when `Some`, override clipboard handling for this paste.
/// - `restore_extra_ms`: extra wait before restoring the original clipboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubmitOverride {
    pub submit: Option<(PasteMethod, AutoSubmitKey)>,
    pub clipboard: Option<ClipboardHandling>,
    pub restore_extra_ms: u64,
}

/// Monotonic paste generation. Every clipboard paste bumps it; a pending delayed
/// clipboard restore only fires if no newer paste has superseded it.
static RESTORE_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// What the clipboard held immediately BEFORE Handy's most recent transient
/// (`DontModify`) write, together with the generation of that write and the
/// exact text we put there.
///
/// This lives inside `CLIPBOARD_WRITE_LOCK`'s payload rather than in a static
/// of its own precisely so it is unreachable outside the critical section that
/// already owns every clipboard write + generation bump: no second lock, no
/// lock-ordering rule to get wrong, and no way to observe a
/// (generation, original) pair that do not belong together.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingRestore {
    generation: u64,
    /// Exactly what we wrote — used to prove the clipboard is still OURS
    /// before restoring, and to recognise our own text on a later capture.
    written: String,
    original: ClipboardSnapshot,
}

/// What we captured of the pre-paste clipboard, and therefore what "restore"
/// means. `read_text()` is text-ONLY: an image or a file list reads as `Err`,
/// and the old `unwrap_or_default()` turned that into `""`, so the later
/// restore wrote an empty string over it. Modelling it explicitly lets the
/// restore CLEAR instead of leaving a phantom empty-text item, and lets us warn
/// that this paste is not losslessly reversible.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ClipboardSnapshot {
    Text(String),
    NonText,
}

/// Serializes every clipboard WRITE with the generation bump that guards it
/// (T-103, finding 3). Without this, the delayed restore thread could read
/// `RESTORE_GEN`, find it still matches its own generation, and then — in the
/// gap before its own `write_text` call — lose a race to a `park_text`/paste
/// that bumps the generation and writes AFTER the restore's check but BEFORE
/// its write, so the restore's write (now stale) lands last and clobbers the
/// just-parked text. Held ONLY around the bump+write pair itself — never
/// across the paste keystroke, any sleep, or other blocking work.
///
/// INVARIANT: every site that bumps `RESTORE_GEN` must, in the SAME critical
/// section, set this payload to `Some(..)` (a transient write we will undo) or
/// `None` (a permanent write — a park, or `CopyToClipboard`). A stale entry
/// left behind a bump is exactly the restore-chain poisoning bug.
static CLIPBOARD_WRITE_LOCK: Lazy<Mutex<Option<PendingRestore>>> = Lazy::new(|| Mutex::new(None));

static SUBMIT_OVERRIDE: Lazy<Mutex<Option<SubmitOverride>>> = Lazy::new(|| Mutex::new(None));

/// Sentinel error returned internally when a per-keystroke anchor re-check
/// (T-103, finding 1) finds the delivery target no longer foreground/focused
/// mid-paste. Distinct from an ordinary paste failure so `paste_inner` routes
/// it through the SAME fail-closed park path as a `begin_delivery`/TOCTOU
/// verification failure — never the generic paste-failure toast. Never
/// surfaced to the user as-is.
///
/// `input::paste_text_direct` raises the SAME sentinel from its per-batch
/// re-check, so it is defined once there and aliased here — two independent
/// copies of a magic string that must stay equal is a defect waiting to happen.
const ANCHOR_FOCUS_LOST: &str = input::DELIVERY_ABORTED;

/// The text was delivered in full, but focus moved during the pre-submit settle
/// so the submit key was withheld.
///
/// Distinct from [`ANCHOR_FOCUS_LOST`] because the recovery advice is opposite:
/// nothing needs re-inserting, and telling the user otherwise makes them paste
/// the transcript a second time. 1.3.0 lengthened the no-jump remote settle from
/// 50 ms to the remote submit delay, which makes this outcome markedly more
/// reachable than it was.
const ANCHOR_SUBMIT_ABORTED: &str = "__anchor_submit_aborted_text_delivered__";

/// As [`ANCHOR_FOCUS_LOST`], but part of the transcript had ALREADY been typed
/// into the target. Handled by the same fail-closed path, with different user
/// messaging -- see the abort handler in `paste_inner`.
const ANCHOR_FOCUS_LOST_PARTIAL: &str = input::DELIVERY_ABORTED_PARTIAL;

/// Internal (non-configurable) grace kept AFTER the auto-submit key is injected
/// and BEFORE `finish_delivery` may return focus — but ONLY when the delivery
/// jumped the foreground and the flow will return focus. Windows `SendInput`
/// only INSERTS the Enter into the target's input queue; returning focus to the
/// previous window immediately (the default) can race that queued Enter so the
/// target never processes it. This keeps the jumped-to target deliberately
/// foreground just long enough to consume the Enter.
#[cfg(windows)]
const POST_SUBMIT_RETURN_FOCUS_GRACE_MS: u64 = 100;

/// Arm the mailbox (T-116): the coordinator calls this synchronously on the
/// Transcribe & Submit finishing press, BEFORE calling `stop()` — which is
/// exactly where `take_submit_override()` below consumes it into the take's
/// owned pipeline value.
pub fn set_submit_override(over: SubmitOverride) {
    if let Ok(mut guard) = SUBMIT_OVERRIDE.lock() {
        *guard = Some(over);
    }
}

/// Park text on the clipboard as the delivery of last resort. Bumps the paste
/// generation and writes under `CLIPBOARD_WRITE_LOCK` (T-103, finding 3) so a
/// pending delayed clipboard-restore (from an earlier paste) can never land
/// between the bump and the write and overwrite what was just parked. Returns
/// whether the write stuck.
pub fn park_text(app: &AppHandle, text: &str) -> bool {
    let mut pending = CLIPBOARD_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // Write FIRST, commit SECOND. Bumping before the write meant a park that
    // FAILED (clipboard contention) still superseded an older transient
    // transcript's pending restore — so that restore saw itself as stale and
    // exited, stranding ITS transcript on the clipboard forever for a write
    // that never happened.
    let ok = app.clipboard().write_text(text).is_ok();
    if ok {
        // A park is a PERMANENT write: there is nothing left to restore, and
        // the pending entry must be dropped so a later capture cannot inherit
        // it. Only true once the write actually landed.
        RESTORE_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        *pending = None;
    }
    ok
}

/// Whether a fail-closed rescue may leave the transcript on the clipboard.
///
/// The rescue itself is NOT optional — losing a take is strictly worse than
/// the leak — but the clipboard is not the only place the take survives:
/// `save_history` runs BEFORE delivery is dispatched, and
/// `actions::set_last_transcription` is called synchronously before the paste,
/// so "Paste Last Transcription" can always re-deliver it. Under `DontModify`
/// the user has explicitly said their clipboard is not ours to keep, so the
/// rescue withholds the write and the toast points at History + Paste Last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParkDecision {
    Park,
    Withhold,
}

pub(crate) fn park_decision(handling: ClipboardHandling) -> ParkDecision {
    match handling {
        ClipboardHandling::CopyToClipboard => ParkDecision::Park,
        ClipboardHandling::DontModify => ParkDecision::Withhold,
    }
}

/// Wire value for the failure toasts — what actually happened to the clipboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RescueOutcome {
    Parked,
    Withheld,
    ParkFailed,
}

impl RescueOutcome {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Parked => "parked",
            Self::Withheld => "withheld",
            Self::ParkFailed => "failed",
        }
    }
}

/// Fail-closed rescue for a delivery that could not be verified.
///
/// * `CopyToClipboard` → Park: bump+write under `CLIPBOARD_WRITE_LOCK`.
/// * `DontModify` → Withhold: touch NOTHING. Critically this means NOT bumping
///   `RESTORE_GEN`, so the restore armed by the paste that just failed still
///   fires and takes OUR transcript back off the clipboard. Withholding is an
///   ACTIVE instruction, not a no-op — it is only correct because
///   `paste_via_clipboard` now arms that restore unconditionally on write
///   success. On a path that never reached a clipboard write (a
///   `begin_delivery` failure) this is simply a no-op.
///
/// The take survives either way; the caller MUST surface the returned outcome
/// so the user is told where it went.
pub(crate) fn park_for_rescue(
    app: &AppHandle,
    text: &str,
    handling: ClipboardHandling,
) -> RescueOutcome {
    match park_decision(handling) {
        ParkDecision::Withhold => {
            log::info!(
                "Rescue: withholding clipboard park of {} chars — clipboard handling is \
                 DontModify; the take is in History and re-pastable with Paste Last",
                text.len()
            );
            RescueOutcome::Withheld
        }
        ParkDecision::Park => {
            if park_text(app, text) {
                log::info!("Rescue: parked {} chars on the clipboard", text.len());
                RescueOutcome::Parked
            } else {
                log::warn!(
                    "Rescue: could not park {} chars on the clipboard",
                    text.len()
                );
                RescueOutcome::ParkFailed
            }
        }
    }
}

/// Single source of truth for a paste's effective clipboard handling: submit
/// override, then manual (Paste Last) override, then the global setting.
/// Extracted so `actions::report_paste_failure` resolves the SAME value the
/// paste itself used — a rescue that disagreed would leak under `DontModify`
/// or withhold under `CopyToClipboard`.
pub(crate) fn effective_clipboard_handling(
    submit_override: Option<SubmitOverride>,
    manual_override: Option<ClipboardHandling>,
    global: ClipboardHandling,
) -> ClipboardHandling {
    submit_override
        .and_then(|o| o.clipboard)
        .or(manual_override)
        .unwrap_or(global)
}

/// How long to wait after delivering the text and before pressing the submit key.
///
/// 1.2.0 gated this on `jumped` alone, so an already-focused remote target got
/// ZERO settle: the user's logs show `foreground_switched=false,
/// pre_submit_extra_ms=0` and Enter arriving ~50 ms after the text, while it was
/// still draining through the RDP virtual channel. The submitted message was
/// therefore truncated or split. That is the defect this closes.
///
/// |          | remote                     | local                |
/// |----------|----------------------------|----------------------|
/// | jump     | `submit_remote_ms`         | `submit_local_ms`    |
/// | no jump  | `submit_remote_ms` (fixed) | **0**                |
///
/// The no-jump/local cell MUST stay 0. That is the overwhelmingly common case —
/// ordinary dictation into the window you are already looking at — and charging
/// it the local delay would add a quarter to half a second to every single take.
///
/// Reusing `submit_remote_ms` for no-jump/remote is safe by construction: it
/// already has to cover activation PLUS transport, so it is a strict upper bound
/// on the transport-only wait needed here. It can be too generous; it cannot be
/// too short.
/// Total settle before the submit key: the fixed base, the target-aware pacing,
/// and — for `Direct` only — one more inter-injection pause.
///
/// A `SendInput` returns as soon as the LOCAL input queue accepts it, not when
/// the target has rendered anything. Typing therefore has a tail that a single
/// paste chord does not, and the Enter is simply the next injection, so it
/// deserves at least the same pause every chunk got. Without this a local
/// `Direct` + auto-submit fires Enter at zero delay after the final chunk.
fn pre_submit_total_ms(
    base_ms: u64,
    pacing_extra_ms: u64,
    method: PasteMethod,
    chunk_delay_ms: u64,
) -> u64 {
    let typing_tail = if method == PasteMethod::Direct {
        chunk_delay_ms
    } else {
        0
    };
    base_ms + pacing_extra_ms + typing_tail
}

/// Extra wait before restoring the original clipboard.
///
/// `remote_override` is `None` for "inherit", so a user who has never touched
/// the remote control keeps byte-identical behaviour. A per-flow submit override
/// still wins over both, because that is an explicit per-shortcut choice.
fn restore_extra_for_target(
    submit_override_ms: Option<u64>,
    remote: bool,
    remote_override_ms: Option<u64>,
    global_ms: u64,
) -> u64 {
    if let Some(ms) = submit_override_ms {
        return ms;
    }
    match (remote, remote_override_ms) {
        (true, Some(ms)) => ms,
        _ => global_ms,
    }
}

fn pre_submit_extra_ms(
    jumped: bool,
    remote: bool,
    submit_local_ms: u64,
    submit_remote_ms: u64,
) -> u64 {
    match (jumped, remote) {
        (_, true) => submit_remote_ms,
        (true, false) => submit_local_ms,
        (false, false) => 0,
    }
}

/// Is the pinned unanchored delivery target still the foreground window?
///
/// `None` means the foreground could not be identified when the delivery was
/// classified (or this is not Windows). That imposes NO constraint rather than
/// blocking the delivery: refusing every paste on a machine where the query
/// fails would be a far worse failure than the narrow race this closes.
fn unanchored_target_intact(pinned: Option<isize>) -> bool {
    match pinned {
        Some(expected) => crate::anchor::foreground_window_id() == Some(expected),
        None => true,
    }
}

/// Build the per-keystroke "may delivery still proceed?" predicate.
///
/// Anchored deliveries re-verify their `DeliveryGuard`; unanchored ones
/// re-verify the foreground window pinned when the delivery was classified.
/// Both are strict and immediate -- any bounded settling wait belongs to the
/// pre-flight check, not to the gaps between injected batches.
fn still_valid_for(
    guard: Option<&crate::anchor::DeliveryGuard>,
    pinned: Option<isize>,
) -> impl Fn() -> bool + '_ {
    move || match guard {
        Some(g) => crate::anchor::await_guard_ready(g, false),
        None => unanchored_target_intact(pinned),
    }
}

/// Decide what this paste must treat as "the user's clipboard".
///
/// The bug: paste #1 writes T1 and arms a restore; paste #2 starts inside that
/// window, reads the clipboard, gets T1, and later faithfully "restores" T1 —
/// making a PAST transcript the permanent clipboard content and destroying the
/// user's real data. Inherit paste #1's captured original instead, but ONLY
/// when custody is provable two ways:
///   * `pending.generation == current_generation` — nothing (park, paste,
///     CopyToClipboard) has superseded that write, AND
///   * the live read is byte-identical to `pending.written` — the user has not
///     copied something new since. A real user copy must always win; it is the
///     most recent thing they intended to have.
///
/// Returns the snapshot to use and whether it was inherited (for logging).
fn resolve_capture(
    pending: Option<&PendingRestore>,
    current_generation: u64,
    live: ClipboardSnapshot,
) -> (ClipboardSnapshot, bool) {
    match pending {
        Some(p)
            if p.generation == current_generation
                && live == ClipboardSnapshot::Text(p.written.clone()) =>
        {
            (p.original.clone(), true)
        }
        _ => (live, false),
    }
}

/// Whether the clipboard still holds exactly what we wrote, i.e. Handy still
/// owns it. `RESTORE_GEN` only detects Handy-originated writes; if the USER
/// copied something during the restore delay, restoring would silently clobber
/// the thing they just copied. Checked immediately before the restore write.
fn clipboard_still_ours(live: &ClipboardSnapshot, written: &str) -> bool {
    matches!(live, ClipboardSnapshot::Text(t) if t == written)
}

/// Backoff for a contended restore. `None` = give up and warn. Windows
/// clipboard contention (RDP/Citrix redirection, clipboard managers) is a real
/// field condition, and a single failed attempt would mean a permanent leak.
/// Pure so the retry budget is unit-testable without sleeping.
fn restore_retry_delay_ms(attempt: u32) -> Option<u64> {
    match attempt {
        0 => Some(60),
        1 => Some(150),
        _ => None,
    }
}

/// Pure decision helper for the restore-vs-park/paste race (T-103, finding
/// 3): given the generation a delayed restore was armed for and the CURRENT
/// generation read INSIDE the same lock as the restore's write, should the
/// restore proceed? Extracted so the ordering logic is unit-testable without
/// real clipboard I/O, timing, or touching the shared `RESTORE_GEN`/
/// `CLIPBOARD_WRITE_LOCK` statics from a test.
fn should_restore(armed_generation: u64, current_generation: u64) -> bool {
    armed_generation == current_generation
}

#[cfg(test)]
mod restore_race_tests {
    use super::*;

    #[test]
    fn restore_proceeds_when_no_newer_write_landed() {
        assert!(should_restore(3, 3));
    }

    #[test]
    fn restore_is_skipped_when_a_newer_generation_landed_under_the_lock() {
        // Simulates a park_text/paste bumping the generation between the
        // restore thread's spawn and its lock acquisition — the classic
        // T-103 finding 3 race. Reading the CURRENT generation inside the
        // same lock as the write (rather than before it) is what lets the
        // restore see this fresh value and skip.
        assert!(!should_restore(1, 2));
    }

    fn pending(generation: u64) -> PendingRestore {
        PendingRestore {
            generation,
            written: "TRANSCRIPT 1".to_string(),
            original: ClipboardSnapshot::Text("user data".to_string()),
        }
    }

    #[test]
    fn capture_inherits_the_original_instead_of_our_own_transcript() {
        // The restore-chain poisoning bug: paste #2 starts while paste #1's
        // transcript is still on the clipboard. Without this, paste #2 would
        // snapshot TRANSCRIPT 1 as "the user's clipboard" and faithfully
        // restore it forever, destroying the real content.
        let p = pending(7);
        let (snapshot, inherited) = resolve_capture(
            Some(&p),
            7,
            ClipboardSnapshot::Text("TRANSCRIPT 1".to_string()),
        );
        assert_eq!(snapshot, ClipboardSnapshot::Text("user data".to_string()));
        assert!(inherited);
    }

    #[test]
    fn capture_does_not_inherit_when_the_user_copied_something_new() {
        // A genuine user copy must always win — it is the most recent thing
        // they intended to have on the clipboard.
        let p = pending(7);
        let (snapshot, inherited) = resolve_capture(
            Some(&p),
            7,
            ClipboardSnapshot::Text("fresh user copy".to_string()),
        );
        assert_eq!(
            snapshot,
            ClipboardSnapshot::Text("fresh user copy".to_string())
        );
        assert!(!inherited);
    }

    #[test]
    fn capture_does_not_inherit_across_a_superseding_write() {
        // A park or a newer paste bumped the generation: that entry no longer
        // describes the clipboard, so it must not be trusted.
        let p = pending(7);
        let (_, inherited) = resolve_capture(
            Some(&p),
            9,
            ClipboardSnapshot::Text("TRANSCRIPT 1".to_string()),
        );
        assert!(!inherited);
    }

    #[test]
    fn capture_with_no_pending_entry_uses_the_live_clipboard() {
        let (snapshot, inherited) =
            resolve_capture(None, 3, ClipboardSnapshot::Text("user data".to_string()));
        assert_eq!(snapshot, ClipboardSnapshot::Text("user data".to_string()));
        assert!(!inherited);
    }

    #[test]
    fn restore_is_skipped_when_the_user_changed_the_clipboard() {
        // RESTORE_GEN only sees Handy's own writes. If the USER copied during
        // the restore delay, restoring would clobber what they just copied.
        assert!(clipboard_still_ours(
            &ClipboardSnapshot::Text("ours".to_string()),
            "ours"
        ));
        assert!(!clipboard_still_ours(
            &ClipboardSnapshot::Text("user copied this".to_string()),
            "ours"
        ));
        assert!(!clipboard_still_ours(&ClipboardSnapshot::NonText, "ours"));
    }

    #[test]
    fn park_failure_must_not_supersede_an_older_pending_restore() {
        // Regression guard for the ordering in `park_text`: the generation is
        // bumped ONLY when the write lands. A park that failed used to bump
        // anyway, so an older transient transcript's restore saw itself as
        // superseded and exited, stranding that transcript forever.
        // gen unchanged after a failed park => the older restore still matches.
        assert!(should_restore(5, 5));
        // and a SUCCESSFUL later write does supersede it.
        assert!(!should_restore(5, 6));
    }

    #[test]
    fn restore_retry_budget_is_bounded() {
        assert_eq!(restore_retry_delay_ms(0), Some(60));
        assert_eq!(restore_retry_delay_ms(1), Some(150));
        assert_eq!(restore_retry_delay_ms(2), None);
    }

    // ---- pre-submit pacing matrix (1.3.0) ----
    //
    // The bug these encode: 1.2.0 gated the settle on `jumped` alone, so an
    // already-focused RDP target got 0 ms and Enter raced the text through the
    // virtual channel.

    #[test]
    fn no_jump_remote_pays_the_remote_submit_delay() {
        // THE regression these tests exist for. Reported symptom: text truncated
        // or split because Enter fired ~50 ms after the last keystroke.
        assert_eq!(pre_submit_extra_ms(false, true, 300, 600), 600);
    }

    #[test]
    fn local_no_jump_delivery_pays_no_pacing_at_all() {
        // DO NOT DELETE. This is the guard against "fixing" RDP by charging
        // every ordinary dictation 300-600 ms it never needed.
        assert_eq!(pre_submit_extra_ms(false, false, 300, 600), 0);
    }

    #[test]
    fn a_remote_jump_is_not_double_charged() {
        // A remote jump already paid the post-activation settle; it must get the
        // same remote value here, not that value twice.
        assert_eq!(pre_submit_extra_ms(true, true, 300, 600), 600);
    }

    #[test]
    fn a_local_jump_keeps_the_local_delay() {
        assert_eq!(pre_submit_extra_ms(true, false, 300, 600), 300);
    }

    #[test]
    fn pre_submit_pacing_honours_a_zero_setting() {
        // "Off" must mean off in every cell, not silently floored.
        for (jumped, remote) in [(false, false), (false, true), (true, false), (true, true)] {
            assert_eq!(pre_submit_extra_ms(jumped, remote, 0, 0), 0);
        }
    }

    #[test]
    fn pre_submit_total_adds_a_typing_tail_only_for_direct() {
        // A SendInput returns when the LOCAL queue accepts it, not when the
        // target rendered it, so typing has a tail a single chord does not.
        assert_eq!(
            pre_submit_total_ms(50, 600, PasteMethod::Direct, 15),
            50 + 600 + 15
        );
        for m in [
            PasteMethod::CtrlV,
            PasteMethod::CtrlShiftV,
            PasteMethod::ShiftInsert,
            PasteMethod::ExternalScript,
        ] {
            assert_eq!(pre_submit_total_ms(50, 600, m, 15), 650, "{m:?}");
        }
        // A zero chunk delay must not invent a tail.
        assert_eq!(pre_submit_total_ms(50, 0, PasteMethod::Direct, 0), 50);
    }

    #[test]
    fn remote_restore_override_is_inert_until_set() {
        // The whole point of Option: a user who never touches the remote control
        // keeps byte-identical behaviour.
        assert_eq!(restore_extra_for_target(None, true, None, 250), 250);
        assert_eq!(restore_extra_for_target(None, false, None, 250), 250);
    }

    #[test]
    fn remote_restore_override_applies_only_to_remote_targets() {
        assert_eq!(restore_extra_for_target(None, true, Some(1000), 250), 1000);
        assert_eq!(restore_extra_for_target(None, false, Some(1000), 250), 250);
    }

    #[test]
    fn a_per_flow_submit_override_beats_both() {
        // An explicit per-shortcut choice is the most specific intent there is.
        assert_eq!(
            restore_extra_for_target(Some(50), true, Some(1000), 250),
            50
        );
        assert_eq!(restore_extra_for_target(Some(0), true, Some(1000), 250), 0);
    }

    #[test]
    fn unanchored_target_absent_imposes_no_constraint() {
        // "Could not identify the foreground window" must not block delivery.
        assert!(unanchored_target_intact(None));
    }

    #[test]
    fn dont_modify_withholds_the_rescue_park() {
        // The reported bug: a failed delivery parked the transcript on the
        // clipboard even though the user had asked us not to touch it.
        assert_eq!(
            park_decision(ClipboardHandling::DontModify),
            ParkDecision::Withhold
        );
        assert_eq!(
            park_decision(ClipboardHandling::CopyToClipboard),
            ParkDecision::Park
        );
    }

    #[test]
    fn rescue_outcome_wire_values_match_the_frontend_contract() {
        // src/App.tsx branches on these exact strings.
        assert_eq!(RescueOutcome::Parked.as_str(), "parked");
        assert_eq!(RescueOutcome::Withheld.as_str(), "withheld");
        assert_eq!(RescueOutcome::ParkFailed.as_str(), "failed");
    }

    #[test]
    fn effective_clipboard_handling_precedence() {
        let global = ClipboardHandling::DontModify;
        let submit_with = |c: Option<ClipboardHandling>| SubmitOverride {
            submit: None,
            clipboard: c,
            restore_extra_ms: 0,
        };

        // Submit override wins over everything.
        assert_eq!(
            effective_clipboard_handling(
                Some(submit_with(Some(ClipboardHandling::CopyToClipboard))),
                Some(ClipboardHandling::DontModify),
                global
            ),
            ClipboardHandling::CopyToClipboard
        );
        // A submit override carrying no clipboard choice falls through.
        assert_eq!(
            effective_clipboard_handling(
                Some(submit_with(None)),
                Some(ClipboardHandling::CopyToClipboard),
                global
            ),
            ClipboardHandling::CopyToClipboard
        );
        // No overrides → the global setting.
        assert_eq!(
            effective_clipboard_handling(None, None, global),
            ClipboardHandling::DontModify
        );
    }

    #[test]
    fn non_text_clipboard_is_distinguished_from_empty_text() {
        // An image/file clipboard must be CLEARED, not overwritten with "",
        // which would leave a phantom empty item where the image used to be.
        assert_ne!(
            ClipboardSnapshot::NonText,
            ClipboardSnapshot::Text(String::new())
        );
    }
}

/// Clear the mailbox (called when a new recording starts, mirroring
/// `anchor::clear_delivery_request`/`clear_post_take_action`). T-116: this only
/// protects a LATER take now — a take whose `stop()` already captured the
/// override into its own pipeline by value is untouched by this clear, since
/// `take_submit_override()` is the ONLY reader of the global and it already
/// ran for that take.
pub fn clear_submit_override() {
    if let Ok(mut guard) = SUBMIT_OVERRIDE.lock() {
        *guard = None;
    }
}

/// Consume the mailbox into an owned, take-scoped value. Call ONCE per take,
/// synchronously, at the SAME `stop()`-time point `anchor::take_delivery_intent()`
/// / `anchor::take_post_take_action()` / `actions::snapshot_take_generation()`
/// are called — while the coordinator thread still serializes everything, so
/// no other take can be starting concurrently. This is now the ONLY place that
/// reads `SUBMIT_OVERRIDE`; `paste_inner` no longer reads the global itself,
/// it receives this take's captured value as a parameter.
pub(crate) fn take_submit_override() -> Option<SubmitOverride> {
    SUBMIT_OVERRIDE
        .lock()
        .ok()
        .and_then(|mut guard| guard.take())
}

/// Pastes text using the clipboard: saves current content, writes text, sends
/// the paste keystroke, then (when `restore_after_ms` is `Some`) restores the
/// original clipboard after that delay on a background thread.
///
/// `anchor_guard`, when `Some`, is the active anchored delivery's TOCTOU
/// guard (T-103, finding 1): re-verified immediately before the synthesized
/// keystroke below — the write-clipboard delay (`paste_delay_ms`) and the
/// native-tool-vs-enigo dispatch are exactly the kind of gap a focus-stealing
/// popup can land in between `begin_delivery`'s own check and this one. `None`
/// for every non-anchored paste (MCP/CLI, no delivery intent, paste disabled)
/// — those are completely unaffected by this check.
fn paste_via_clipboard(
    enigo: &mut Enigo,
    text: &str,
    app_handle: &AppHandle,
    paste_method: &PasteMethod,
    paste_delay_ms: u64,
    restore_after_ms: Option<u64>,
    anchor_guard: Option<&crate::anchor::DeliveryGuard>,
    unanchored_target: Option<isize>,
    remote_retry: bool,
) -> Result<(), String> {
    let clipboard = app_handle.clipboard();

    // Capture + write + bump, all inside the ONE lock that owns clipboard
    // writes (T-103, finding 3). The capture MUST be in here too: read outside
    // the lock and this paste can snapshot a transcript another paste is about
    // to supersede, which is the restore-chain poisoning bug.
    let armed: Option<PendingRestore> = {
        let mut pending = CLIPBOARD_WRITE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let live = match clipboard.read_text() {
            Ok(t) => ClipboardSnapshot::Text(t),
            Err(e) => {
                log::debug!("Clipboard held no readable text ({e}) — capturing as non-text");
                ClipboardSnapshot::NonText
            }
        };
        let current_gen = RESTORE_GEN.load(std::sync::atomic::Ordering::SeqCst);
        let (original, inherited) = resolve_capture(pending.as_ref(), current_gen, live);
        if inherited {
            log::warn!(
                "Clipboard capture inherited pending restore (gen {current_gen}) — a previous \
                 paste's transcript was still on the clipboard when this paste started; \
                 restoring the ORIGINAL content instead of that transcript"
            );
        }
        if original == ClipboardSnapshot::NonText && restore_after_ms.is_some() {
            log::warn!(
                "Clipboard held non-text content (image/file) — it cannot be restored and will \
                 be CLEARED after this paste"
            );
        }

        // Write FIRST, bump SECOND. If the write fails nothing is bumped and
        // nothing is registered, so an older pending restore stays valid and
        // still fires — bumping first would strand it and leave ITS transcript
        // on the clipboard forever.
        #[cfg(target_os = "linux")]
        let write_result = if is_wayland() && is_wl_copy_available() {
            info!("Using wl-copy for clipboard write on Wayland");
            write_clipboard_via_wl_copy(text)
        } else {
            clipboard
                .write_text(text)
                .map_err(|e| format!("Failed to write to clipboard: {}", e))
        };

        #[cfg(not(target_os = "linux"))]
        let write_result = clipboard
            .write_text(text)
            .map_err(|e| format!("Failed to write to clipboard: {}", e));

        write_result?;

        let generation = RESTORE_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        // INVARIANT (see CLIPBOARD_WRITE_LOCK): `Some` for a transient write we
        // will undo; `None` when `restore_after_ms` is `None`, i.e. the
        // CopyToClipboard case where this write is deliberately permanent.
        let entry = restore_after_ms.map(|_| PendingRestore {
            generation,
            written: text.to_string(),
            original,
        });
        *pending = entry.clone();
        entry
    };

    // SINGLE EXIT so the arming below runs on EVERY path once the write
    // succeeded. Four early returns used to sit between the write and the
    // arming — the anchor re-check, the Linux native combo `?`, the enigo `?`s
    // and the `_ =>` arm — and each one left the transcript on the clipboard
    // with NO restore ever scheduled. Same idiom as the `delivered` block in
    // `paste_inner`, and for the same reason: an epilogue that must always run.
    let keystroke: Result<(), String> = (|| {
        std::thread::sleep(Duration::from_millis(paste_delay_ms));

        // T-103 (finding 1): re-verify the anchor immediately before the
        // synthesized keystroke — a focus change since `begin_delivery`'s own
        // check (or since the last re-check) must abort rather than paste
        // blind. No-op (`anchor_guard` is `None`) for every non-anchored paste.
        // T-309: on a remote-desktop jump this tolerates the target still
        // settling its inner focus (bounded retry) instead of parking at once.
        if let Some(guard) = anchor_guard {
            if !crate::anchor::await_guard_ready(guard, remote_retry) {
                return Err(ANCHOR_FOCUS_LOST.to_string());
            }
        } else if !unanchored_target_intact(unanchored_target) {
            // Unanchored deliveries had NO check here at all: the transcript
            // was already on the clipboard and the paste chord fired blind
            // after `paste_delay_ms`. Switching windows inside that gap sent
            // it wherever focus had gone -- and if that was an RDP session,
            // redirection carried the transcript to the remote machine's
            // clipboard, which is exactly the leak this release exists to
            // stop. Aborting here leaves the armed restore to take the
            // transcript back off the local clipboard.
            return Err(ANCHOR_FOCUS_LOST.to_string());
        }

        // Send paste key combo
        #[cfg(target_os = "linux")]
        let key_combo_sent = try_send_key_combo_linux(paste_method)?;

        #[cfg(not(target_os = "linux"))]
        let key_combo_sent = false;

        // Fall back to enigo if no native tool handled it
        if !key_combo_sent {
            match paste_method {
                PasteMethod::CtrlV => input::send_paste_ctrl_v(enigo)?,
                PasteMethod::CtrlShiftV => input::send_paste_ctrl_shift_v(enigo)?,
                PasteMethod::ShiftInsert => input::send_paste_shift_insert(enigo)?,
                _ => return Err("Invalid paste method for clipboard paste".into()),
            }
        }
        Ok(())
    })();

    // Arm AFTER the keystroke phase, successful or not. The ordering guarantee
    // is UNCHANGED: the delay clock still starts once the keystroke has been
    // attempted, never before it. A later `park_*` can still suppress this by
    // bumping `RESTORE_GEN`; that discipline is untouched.
    if let (Some(entry), Some(delay_ms)) = (armed, restore_after_ms) {
        arm_delayed_restore(app_handle.clone(), entry, delay_ms);
    }

    keystroke
}

/// Put a captured snapshot back on the clipboard. Text is written back; a
/// non-text original (image/file) cannot be reconstructed — our own write
/// already destroyed it — so the clipboard is CLEARED rather than left holding
/// a phantom empty-text item where the user's image used to be.
fn write_snapshot(app: &AppHandle, original: &ClipboardSnapshot) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    if is_wayland() && is_wl_copy_available() {
        return match original {
            ClipboardSnapshot::Text(t) => write_clipboard_via_wl_copy(t),
            ClipboardSnapshot::NonText => clear_clipboard_via_wl_copy(),
        };
    }

    match original {
        ClipboardSnapshot::Text(t) => app.clipboard().write_text(t).map_err(|e| format!("{e}")),
        ClipboardSnapshot::NonText => app.clipboard().clear().map_err(|e| format!("{e}")),
    }
}

/// Restore the pre-paste clipboard after a delay, off the calling thread.
///
/// Remote sessions (Citrix/RDP) fetch clipboard data on demand AFTER the paste
/// keystroke lands in the remote app; restoring too early hands them the old
/// content. The generation is re-checked INSIDE the SAME `CLIPBOARD_WRITE_LOCK`
/// as the write itself (T-103, finding 3) — a check-then-write without the lock
/// left a gap where a park/paste could bump the generation and write AFTER the
/// check but BEFORE the write, so the stale restore would land last.
fn arm_delayed_restore(app: AppHandle, entry: PendingRestore, delay_ms: u64) {
    log::debug!(
        "Clipboard restore armed: gen {}, delay {} ms, wrote {} chars, original = {}",
        entry.generation,
        delay_ms,
        entry.written.len(),
        match &entry.original {
            ClipboardSnapshot::Text(t) => format!("{} chars", t.len()),
            ClipboardSnapshot::NonText => "non-text (will clear)".to_string(),
        },
    );
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(delay_ms));

        let mut attempt: u32 = 0;
        loop {
            // Three distinct outcomes, modelled explicitly so a transient
            // failure can never be mistaken for "stop trying":
            //   None            -> stop for good (superseded, or the user owns it)
            //   Some(Ok(()))    -> restored
            //   Some(Err(msg))  -> transient, feed the retry budget
            let outcome: Option<Result<(), String>> = {
                let mut pending = CLIPBOARD_WRITE_LOCK
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());

                if !should_restore(
                    entry.generation,
                    RESTORE_GEN.load(std::sync::atomic::Ordering::SeqCst),
                ) {
                    log::debug!(
                        "Clipboard restore gen {} superseded — skipping (a later paste or park \
                         owns the clipboard)",
                        entry.generation
                    );
                    None
                } else {
                    // Ownership: `RESTORE_GEN` only sees Handy's own writes. If
                    // the USER copied something during the delay, restoring
                    // would silently clobber what they just copied.
                    //
                    // A read ERROR is NOT the same as "someone else owns it":
                    // the clipboard is merely unreadable right now (contention
                    // is the norm on Windows with RDP/Citrix redirection).
                    // Treating that as lost ownership abandoned the restore for
                    // good and left our transcript exposed — so it feeds the
                    // retry instead.
                    match app.clipboard().read_text() {
                        Err(e) => Some(Err(format!("clipboard unreadable: {e}"))),
                        Ok(live_text) => {
                            if !clipboard_still_ours(
                                &ClipboardSnapshot::Text(live_text),
                                &entry.written,
                            ) {
                                log::debug!(
                                    "Clipboard changed externally since gen {} — leaving the \
                                     user's content alone instead of restoring",
                                    entry.generation
                                );
                                // Someone else owns it: our entry no longer
                                // describes the clipboard, so drop it.
                                if pending.as_ref().map(|p| p.generation) == Some(entry.generation)
                                {
                                    *pending = None;
                                }
                                None
                            } else {
                                // `pending` is deliberately NOT cleared before
                                // the write succeeds. Clearing it up front meant
                                // a failed attempt dropped the provenance a
                                // concurrent capture needs to avoid inheriting
                                // our own transcript — recreating the very
                                // restore-chain poisoning this change exists to
                                // fix.
                                let res = write_snapshot(&app, &entry.original);
                                if res.is_ok()
                                    && pending.as_ref().map(|p| p.generation)
                                        == Some(entry.generation)
                                {
                                    *pending = None;
                                }
                                Some(res)
                            }
                        }
                    }
                }
            };

            let outcome = match outcome {
                None => return,
                Some(r) => r,
            };

            match outcome {
                Ok(()) => {
                    log::debug!("Clipboard restored (gen {})", entry.generation);
                    return;
                }
                Err(e) => match restore_retry_delay_ms(attempt) {
                    Some(backoff) => {
                        log::debug!(
                            "Clipboard restore attempt {} failed ({e}) — retrying in {} ms",
                            attempt + 1,
                            backoff
                        );
                        attempt += 1;
                        std::thread::sleep(Duration::from_millis(backoff));
                    }
                    None => {
                        log::warn!(
                            "Clipboard restore FAILED (gen {}): {e} — {} chars of transcript may \
                             REMAIN on the clipboard",
                            entry.generation,
                            entry.written.len()
                        );
                        return;
                    }
                },
            }
        }
    });
}

/// Attempts to send a key combination using Linux-native tools.
/// Returns `Ok(true)` if a native tool handled it, `Ok(false)` to fall back to enigo.
#[cfg(target_os = "linux")]
fn try_send_key_combo_linux(paste_method: &PasteMethod) -> Result<bool, String> {
    if is_wayland() {
        // Wayland: prefer wtype (but not on KDE), then dotool, then ydotool
        // Note: wtype doesn't work on KDE (no zwp_virtual_keyboard_manager_v1 support)
        if !is_kde_wayland() && is_wtype_available() {
            info!("Using wtype for key combo");
            send_key_combo_via_wtype(paste_method)?;
            return Ok(true);
        }
        if is_dotool_available() {
            info!("Using dotool for key combo");
            send_key_combo_via_dotool(paste_method)?;
            return Ok(true);
        }
        if is_ydotool_available() {
            info!("Using ydotool for key combo");
            send_key_combo_via_ydotool(paste_method)?;
            return Ok(true);
        }
    } else {
        // X11: prefer xdotool, then ydotool
        if is_xdotool_available() {
            info!("Using xdotool for key combo");
            send_key_combo_via_xdotool(paste_method)?;
            return Ok(true);
        }
        if is_ydotool_available() {
            info!("Using ydotool for key combo");
            send_key_combo_via_ydotool(paste_method)?;
            return Ok(true);
        }
    }

    Ok(false)
}

/// Attempts to type text directly using Linux-native tools.
/// Returns `Ok(true)` if a native tool handled it, `Ok(false)` to fall back to enigo.
#[cfg(target_os = "linux")]
fn try_direct_typing_linux(text: &str, preferred_tool: TypingTool) -> Result<bool, String> {
    // If user specified a tool, try only that one
    if preferred_tool != TypingTool::Auto {
        return match preferred_tool {
            TypingTool::Wtype if is_wtype_available() => {
                info!("Using user-specified wtype");
                type_text_via_wtype(text)?;
                Ok(true)
            }
            TypingTool::Kwtype if is_kwtype_available() => {
                info!("Using user-specified kwtype");
                type_text_via_kwtype(text)?;
                Ok(true)
            }
            TypingTool::Dotool if is_dotool_available() => {
                info!("Using user-specified dotool");
                type_text_via_dotool(text)?;
                Ok(true)
            }
            TypingTool::Ydotool if is_ydotool_available() => {
                info!("Using user-specified ydotool");
                type_text_via_ydotool(text)?;
                Ok(true)
            }
            TypingTool::Xdotool if is_xdotool_available() => {
                info!("Using user-specified xdotool");
                type_text_via_xdotool(text)?;
                Ok(true)
            }
            _ => Err(format!(
                "Typing tool {:?} is not available on this system",
                preferred_tool
            )),
        };
    }

    // Auto mode - existing fallback chain
    if is_wayland() {
        // KDE Wayland: prefer kwtype (uses KDE Fake Input protocol, supports umlauts)
        if is_kde_wayland() && is_kwtype_available() {
            info!("Using kwtype for direct text input on KDE Wayland");
            type_text_via_kwtype(text)?;
            return Ok(true);
        }
        // Wayland: prefer wtype, then dotool, then ydotool
        // Note: wtype doesn't work on KDE (no zwp_virtual_keyboard_manager_v1 support)
        if !is_kde_wayland() && is_wtype_available() {
            info!("Using wtype for direct text input");
            type_text_via_wtype(text)?;
            return Ok(true);
        }
        if is_dotool_available() {
            info!("Using dotool for direct text input");
            type_text_via_dotool(text)?;
            return Ok(true);
        }
        if is_ydotool_available() {
            info!("Using ydotool for direct text input");
            type_text_via_ydotool(text)?;
            return Ok(true);
        }
    } else {
        // X11: prefer xdotool, then ydotool
        if is_xdotool_available() {
            info!("Using xdotool for direct text input");
            type_text_via_xdotool(text)?;
            return Ok(true);
        }
        if is_ydotool_available() {
            info!("Using ydotool for direct text input");
            type_text_via_ydotool(text)?;
            return Ok(true);
        }
    }

    Ok(false)
}

/// Returns the list of available typing tools on this system.
/// Always includes "auto" as the first entry.
#[cfg(target_os = "linux")]
pub fn get_available_typing_tools() -> Vec<String> {
    let mut tools = vec!["auto".to_string()];
    if is_wtype_available() {
        tools.push("wtype".to_string());
    }
    if is_kwtype_available() {
        tools.push("kwtype".to_string());
    }
    if is_dotool_available() {
        tools.push("dotool".to_string());
    }
    if is_ydotool_available() {
        tools.push("ydotool".to_string());
    }
    if is_xdotool_available() {
        tools.push("xdotool".to_string());
    }
    tools
}

/// Check if wtype is available (Wayland text input tool)
#[cfg(target_os = "linux")]
fn is_wtype_available() -> bool {
    Command::new("which")
        .arg("wtype")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Check if dotool is available (another Wayland text input tool)
#[cfg(target_os = "linux")]
fn is_dotool_available() -> bool {
    Command::new("which")
        .arg("dotool")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Check if ydotool is available (uinput-based, works on both Wayland and X11)
#[cfg(target_os = "linux")]
fn is_ydotool_available() -> bool {
    Command::new("which")
        .arg("ydotool")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn is_xdotool_available() -> bool {
    Command::new("which")
        .arg("xdotool")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Check if kwtype is available (KDE Wayland virtual keyboard input tool)
#[cfg(target_os = "linux")]
fn is_kwtype_available() -> bool {
    Command::new("which")
        .arg("kwtype")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Check if wl-copy is available (Wayland clipboard tool)
#[cfg(target_os = "linux")]
fn is_wl_copy_available() -> bool {
    Command::new("which")
        .arg("wl-copy")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Type text directly via wtype on Wayland.
#[cfg(target_os = "linux")]
fn type_text_via_wtype(text: &str) -> Result<(), String> {
    let output = Command::new("wtype")
        .arg("--") // Protect against text starting with -
        .arg(text)
        .output()
        .map_err(|e| format!("Failed to execute wtype: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("wtype failed: {}", stderr));
    }

    Ok(())
}

/// Type text directly via xdotool on X11.
#[cfg(target_os = "linux")]
fn type_text_via_xdotool(text: &str) -> Result<(), String> {
    let output = Command::new("xdotool")
        .arg("type")
        .arg("--clearmodifiers")
        .arg("--")
        .arg(text)
        .output()
        .map_err(|e| format!("Failed to execute xdotool: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("xdotool failed: {}", stderr));
    }

    Ok(())
}

/// Type text directly via dotool (works on both Wayland and X11 via uinput).
#[cfg(target_os = "linux")]
fn type_text_via_dotool(text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new("dotool")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn dotool: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        // dotool uses "type <text>" command
        writeln!(stdin, "type {}", text)
            .map_err(|e| format!("Failed to write to dotool stdin: {}", e))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for dotool: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("dotool failed: {}", stderr));
    }

    Ok(())
}

/// Type text directly via ydotool (uinput-based, requires ydotoold daemon).
#[cfg(target_os = "linux")]
fn type_text_via_ydotool(text: &str) -> Result<(), String> {
    let output = Command::new("ydotool")
        .arg("type")
        .arg("--")
        .arg(text)
        .output()
        .map_err(|e| format!("Failed to execute ydotool: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ydotool failed: {}", stderr));
    }

    Ok(())
}

/// Type text directly via kwtype (KDE Wayland virtual keyboard, uses KDE Fake Input protocol).
#[cfg(target_os = "linux")]
fn type_text_via_kwtype(text: &str) -> Result<(), String> {
    let output = Command::new("kwtype")
        .arg("--")
        .arg(text)
        .output()
        .map_err(|e| format!("Failed to execute kwtype: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("kwtype failed: {}", stderr));
    }

    Ok(())
}

/// Write text to clipboard via wl-copy (Wayland clipboard tool).
/// Uses Stdio::null() to avoid blocking on repeated calls — wl-copy forks a
/// daemon that inherits piped fds, causing read_to_end to hang indefinitely.
#[cfg(target_os = "linux")]
fn write_clipboard_via_wl_copy(text: &str) -> Result<(), String> {
    use std::process::Stdio;
    let status = Command::new("wl-copy")
        .arg("--")
        .arg(text)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("Failed to execute wl-copy: {}", e))?;

    if !status.success() {
        return Err("wl-copy failed".into());
    }

    Ok(())
}

/// Clear the clipboard via wl-copy. Used when the pre-paste clipboard held
/// non-text content that cannot be restored, so we remove our transcript
/// rather than leave it there.
#[cfg(target_os = "linux")]
fn clear_clipboard_via_wl_copy() -> Result<(), String> {
    use std::process::Stdio;
    let status = Command::new("wl-copy")
        .arg("--clear")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("Failed to execute wl-copy --clear: {}", e))?;

    if !status.success() {
        return Err("wl-copy --clear failed".into());
    }

    Ok(())
}

/// Send a key combination (e.g., Ctrl+V) via wtype on Wayland.
#[cfg(target_os = "linux")]
fn send_key_combo_via_wtype(paste_method: &PasteMethod) -> Result<(), String> {
    let args: Vec<&str> = match paste_method {
        PasteMethod::CtrlV => vec!["-M", "ctrl", "-k", "v"],
        PasteMethod::ShiftInsert => vec!["-M", "shift", "-k", "Insert"],
        PasteMethod::CtrlShiftV => vec!["-M", "ctrl", "-M", "shift", "-k", "v"],
        _ => return Err("Unsupported paste method".into()),
    };

    let output = Command::new("wtype")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to execute wtype: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("wtype failed: {}", stderr));
    }

    Ok(())
}

/// Send a key combination (e.g., Ctrl+V) via dotool.
#[cfg(target_os = "linux")]
fn send_key_combo_via_dotool(paste_method: &PasteMethod) -> Result<(), String> {
    let command;
    match paste_method {
        PasteMethod::CtrlV => command = "echo key ctrl+v | dotool",
        PasteMethod::ShiftInsert => command = "echo key shift+insert | dotool",
        PasteMethod::CtrlShiftV => command = "echo key ctrl+shift+v | dotool",
        _ => return Err("Unsupported paste method".into()),
    }
    use std::process::Stdio;
    let status = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("Failed to execute dotool: {}", e))?;
    if !status.success() {
        return Err("dotool failed".into());
    }

    Ok(())
}

/// Send a key combination (e.g., Ctrl+V) via ydotool (requires ydotoold daemon).
#[cfg(target_os = "linux")]
fn send_key_combo_via_ydotool(paste_method: &PasteMethod) -> Result<(), String> {
    // ydotool uses Linux input event keycodes with format <keycode>:<pressed>
    // where pressed is 1 for down, 0 for up. Keycodes: ctrl=29, shift=42, v=47, insert=110
    let args: Vec<&str> = match paste_method {
        PasteMethod::CtrlV => vec!["key", "29:1", "47:1", "47:0", "29:0"],
        PasteMethod::ShiftInsert => vec!["key", "42:1", "110:1", "110:0", "42:0"],
        PasteMethod::CtrlShiftV => vec!["key", "29:1", "42:1", "47:1", "47:0", "42:0", "29:0"],
        _ => return Err("Unsupported paste method".into()),
    };

    let output = Command::new("ydotool")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to execute ydotool: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ydotool failed: {}", stderr));
    }

    Ok(())
}

/// Send a key combination (e.g., Ctrl+V) via xdotool on X11.
#[cfg(target_os = "linux")]
fn send_key_combo_via_xdotool(paste_method: &PasteMethod) -> Result<(), String> {
    let key_combo = match paste_method {
        PasteMethod::CtrlV => "ctrl+v",
        PasteMethod::CtrlShiftV => "ctrl+shift+v",
        PasteMethod::ShiftInsert => "shift+Insert",
        _ => return Err("Unsupported paste method".into()),
    };

    let output = Command::new("xdotool")
        .arg("key")
        .arg("--clearmodifiers")
        .arg(key_combo)
        .output()
        .map_err(|e| format!("Failed to execute xdotool: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("xdotool failed: {}", stderr));
    }

    Ok(())
}

/// Pastes text by invoking an external script.
/// The script receives the text to paste as a single argument.
fn paste_via_external_script(text: &str, script_path: &str) -> Result<(), String> {
    info!("Pasting via external script: {}", script_path);

    let output = Command::new(script_path)
        .arg(text)
        .output()
        .map_err(|e| format!("Failed to execute external script '{}': {}", script_path, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "External script '{}' failed with exit code {:?}. stderr: {}, stdout: {}",
            script_path,
            output.status.code(),
            stderr.trim(),
            stdout.trim()
        ));
    }

    Ok(())
}

/// Types text directly by simulating individual key presses.
#[cfg(target_os = "linux")]
fn paste_direct(
    enigo: &mut Enigo,
    text: &str,
    chunk_chars: usize,
    chunk_delay_ms: u64,
    still_valid: Option<input::DeliveryStillValid<'_>>,
    #[cfg(target_os = "linux")] typing_tool: TypingTool,
) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        if try_direct_typing_linux(text, typing_tool)? {
            return Ok(());
        }
        info!("Falling back to enigo for direct text input");
    }

    input::paste_text_direct(enigo, text, chunk_chars, chunk_delay_ms, still_valid)
}

fn send_return_key(enigo: &mut Enigo, key_type: AutoSubmitKey) -> Result<(), String> {
    match key_type {
        AutoSubmitKey::Enter => {
            enigo
                .key(Key::Return, Direction::Press)
                .map_err(|e| format!("Failed to press Return key: {}", e))?;
            enigo
                .key(Key::Return, Direction::Release)
                .map_err(|e| format!("Failed to release Return key: {}", e))?;
        }
        AutoSubmitKey::CtrlEnter => {
            enigo
                .key(Key::Control, Direction::Press)
                .map_err(|e| format!("Failed to press Control key: {}", e))?;
            enigo
                .key(Key::Return, Direction::Press)
                .map_err(|e| format!("Failed to press Return key: {}", e))?;
            enigo
                .key(Key::Return, Direction::Release)
                .map_err(|e| format!("Failed to release Return key: {}", e))?;
            enigo
                .key(Key::Control, Direction::Release)
                .map_err(|e| format!("Failed to release Control key: {}", e))?;
        }
        AutoSubmitKey::CmdEnter => {
            enigo
                .key(Key::Meta, Direction::Press)
                .map_err(|e| format!("Failed to press Meta/Cmd key: {}", e))?;
            enigo
                .key(Key::Return, Direction::Press)
                .map_err(|e| format!("Failed to press Return key: {}", e))?;
            enigo
                .key(Key::Return, Direction::Release)
                .map_err(|e| format!("Failed to release Return key: {}", e))?;
            enigo
                .key(Key::Meta, Direction::Release)
                .map_err(|e| format!("Failed to release Meta/Cmd key: {}", e))?;
        }
    }

    Ok(())
}

fn should_send_auto_submit(auto_submit: bool, paste_method: PasteMethod) -> bool {
    auto_submit && paste_method != PasteMethod::None
}

/// Flow paste: takes a take-scoped submit override and a take-scoped
/// anchored-delivery intent — BOTH captured by the caller at stop()/take time
/// (`clipboard::take_submit_override()` — T-116 — and
/// `anchor::take_delivery_intent()` — T-101), never read from any global in
/// here — and can auto-track the output location. Used by the transcription
/// flows only.
pub fn paste(
    text: String,
    app_handle: AppHandle,
    is_ptt: bool,
    delivery_intent: crate::anchor::DeliveryIntent,
    submit_override: Option<SubmitOverride>,
) -> Result<(), String> {
    paste_inner(
        text,
        app_handle,
        is_ptt,
        true,
        delivery_intent,
        submit_override,
        None,
    )
}

/// Plain paste at the CURRENT focus with an EXPLICIT paste method + clipboard
/// handling (never a global or a flow one-shot). Used by the "Paste Last
/// Transcription" shortcut: no anchor/jump, no submit key — just re-paste the
/// given text where the user is now, the way they configured.
pub fn paste_manual(
    text: String,
    app_handle: AppHandle,
    paste_method: PasteMethod,
    clipboard_handling: ClipboardHandling,
) -> Result<(), String> {
    paste_inner(
        text,
        app_handle,
        false,
        false,
        crate::anchor::DeliveryIntent::NONE,
        None,
        Some((paste_method, clipboard_handling)),
    )
}

/// Plain paste for non-flow callers (MCP/CLI `keyboard_type`): must NEVER
/// consume or observe flow one-shots — a stale submit override or delivery
/// intent would redirect unrelated text into the submit pipeline or an
/// anchored window. Constructed with `DeliveryIntent::NONE` and `None` by
/// construction, never by reading any global.
pub fn paste_plain(text: String, app_handle: AppHandle) -> Result<(), String> {
    paste_inner(
        text,
        app_handle,
        false,
        false,
        crate::anchor::DeliveryIntent::NONE,
        None,
        None,
    )
}

fn paste_inner(
    text: String,
    app_handle: AppHandle,
    is_ptt: bool,
    flow_paste: bool,
    delivery_intent: crate::anchor::DeliveryIntent,
    submit_override: Option<SubmitOverride>,
    // Explicit (method, clipboard_handling) override for a manual paste — forces
    // both WITHOUT forcing a submit key (unlike `submit_override`). `None` = use
    // the flow/global values.
    manual_override: Option<(PasteMethod, ClipboardHandling)>,
) -> Result<(), String> {
    // The Jumper (and therefore `delivery_intent`) is Windows-only — silence
    // the unused-parameter warning on other platforms rather than threading
    // a `#[cfg]` through every call site.
    #[cfg(not(windows))]
    let _ = delivery_intent;

    let settings = get_settings(&app_handle);
    // T-116: `submit_override` is now the CALLER's already-take-scoped value
    // (captured once, synchronously, at stop() time — see
    // `take_submit_override`). This defensive mask is the same belt-and-
    // braces the `delivery_intent`/`clear_post_take_action` non-flow-paste
    // path already applies: a non-flow paste (MCP/CLI, `flow_paste == false`)
    // must never act on a submit override even if some future caller ever
    // passed one in by mistake — `paste_plain` already guarantees `None` by
    // construction, so this is a no-op there today.
    let submit_override = if flow_paste { submit_override } else { None };
    let forced_submit = submit_override.and_then(|o| o.submit);
    let clipboard_handling = effective_clipboard_handling(
        submit_override,
        manual_override.map(|(_, c)| c),
        settings.clipboard_handling,
    );
    let chosen_paste_method = match forced_submit {
        Some((method, _)) => method,
        None => match manual_override {
            Some((method, _)) => method,
            None if is_ptt => settings.paste_method_ptt,
            None => settings.paste_method,
        },
    };

    let paste_delay_ms = settings.paste_delay_ms;
    // NOTE: `restore_after_ms` is resolved LATER, after the target has been
    // classified local vs remote — the remote override needs that answer and it
    // is not known yet at this point in the function.
    let submit_restore_override_ms: Option<u64> = submit_override.map(|o| o.restore_extra_ms);

    // Append trailing space if setting is enabled
    let text = if settings.append_trailing_space {
        format!("{} ", text)
    } else {
        text
    };

    let first_chars: String = text.chars().take(50).collect();
    let last_chars: String = text
        .chars()
        .rev()
        .take(30)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    // Metadata at info; CONTENT previews only at debug — release builds write
    // info-level file logs, and dictated text can be sensitive.
    info!(
        "Paste method chosen: {:?}, delay: {}ms, text: {} chars",
        chosen_paste_method,
        paste_delay_ms,
        text.len(),
    );
    log::debug!(
        "Paste preview: first {:?}, last {:?}",
        first_chars,
        last_chars
    );

    // Get the managed Enigo instance BEFORE any anchored-delivery activation:
    // every fallible `?` from here on must happen while focus is still where
    // the user left it — after begin_delivery() an early return would strand
    // focus at the target with no finish_delivery() epilogue.
    let enigo_state = app_handle
        .try_state::<EnigoState>()
        .ok_or("Enigo state not initialized")?;
    let mut enigo = enigo_state
        .0
        .lock()
        .map_err(|e| format!("Failed to lock Enigo: {}", e))?;

    // Fail-closed path for an anchored delivery that could not be verified.
    // Shared by a verification failure at `begin_delivery` and the T-103 TOCTOU
    // re-check right before the keystroke — both must behave identically.
    //
    // Under `CopyToClipboard` this parks the text (bump-and-write under
    // CLIPBOARD_WRITE_LOCK, superseding any pending restore). Under
    // `DontModify` it deliberately WITHHOLDS the write and does not bump, so
    // the restore armed by `paste_via_clipboard` still fires and takes our
    // transcript back off the clipboard. The take is not lost either way: it is
    // in History (written before delivery) and re-pastable with Paste Last, and
    // the emitted payload tells the frontend which happened so the toast can
    // name the right recovery.
    #[cfg(windows)]
    let park_anchor_failure_detail = |reason: String, anchored: bool, partial: bool| {
        let outcome = park_for_rescue(&app_handle, &text, clipboard_handling);
        // `anchored`/`partial` are ADDITIVE fields — the existing event name and
        // shape are unchanged, so an older frontend keeps working and a newer
        // one can warn that some text may already have landed.
        let _ = app_handle.emit(
            "anchor-delivery-failed",
            serde_json::json!({
                "reason": reason,
                "clipboard": outcome.as_str(),
                "anchored": anchored,
                "partial": partial,
            }),
        );
    };
    #[cfg(windows)]
    let park_anchor_failure = |reason: String| park_anchor_failure_detail(reason, true, false);

    // Anchored delivery: activate + focus the captured target BEFORE any
    // keystroke, with verification — never paste blind into a surprise
    // location. On failure the text is parked on the clipboard instead
    // (superseding any pending delayed restore) and the anchor is kept for a
    // retry (or cleared if the window is gone). Non-flow pastes (MCP/CLI)
    // never touch delivery — `delivery_intent` is `DeliveryIntent::NONE` by
    // construction for them.
    #[cfg(windows)]
    let mut anchor_guard = if flow_paste && chosen_paste_method != PasteMethod::None {
        match crate::anchor::begin_delivery(&app_handle, delivery_intent) {
            crate::anchor::BeginDelivery::NoAnchor => None,
            crate::anchor::BeginDelivery::Ready(guard) => Some(guard),
            crate::anchor::BeginDelivery::Failed { reason } => {
                info!(
                    "Anchored delivery failed: {} — parking text on clipboard",
                    reason
                );
                park_anchor_failure(reason);
                return Ok(());
            }
        }
    } else {
        // Paste disabled: the take-scoped `delivery_intent` simply drops
        // with this call (nothing global left to leak into the NEXT
        // paste — T-101). Only the deferred on-finish action is a residual
        // global concern: actions.rs already takes ownership of it at
        // stop() time before this call ever runs, so this clear is a
        // defensive no-op today, kept in case a future caller reaches
        // paste_inner without going through that take-ownership point.
        if flow_paste {
            crate::anchor::clear_post_take_action();
        }
        None
    };

    // T-309: classify this delivery's target ONCE. `remote_jump` is true only
    // on a real jump (`foreground_switched`) whose target matches the user's
    // remote-desktop classifier — it selects the longer `*_remote` delays AND
    // the bounded readiness retry in the per-keystroke re-checks below.
    //
    // THREE distinct booleans, deliberately named apart because 1.2.0 conflated
    // them and shipped a bug:
    //   `jumped`           - this delivery actually activated a window.
    //   `target_is_remote` - the target is an RDP/Citrix session, jump or not.
    //   `remote_jump`      - both. Its ONLY job is the bounded readiness retry
    //                        below; it must NOT gate pacing, because a target
    //                        reached without a jump is still remote.
    #[cfg(windows)]
    let jumped: bool = anchor_guard
        .as_ref()
        .map(|g| g.foreground_switched())
        .unwrap_or(false);
    #[cfg(not(windows))]
    let jumped: bool = false;

    // Classified AFTER `begin_delivery` deliberately. An anchored delivery
    // activates its target there, so classifying earlier would read whatever
    // the user happened to be looking at rather than the window the text will
    // land in. Prefer the guard's own captured identity; fall back to the
    // foreground only when there is no anchor, which is the ordinary
    // "dictate straight into the focused RDP window" case that
    // `DeliveryGuard::is_remote` cannot answer.
    //
    // For an UNANCHORED delivery the snapshot ALSO yields the identity of the
    // window the decision was made about (`unanchored_target`), so everything
    // downstream re-validates against that exact window and the classification
    // and the keystrokes can never describe two different windows.
    //
    // NOTE: no setting term. 1.2.0 multiplied this by
    // `direct_typing_for_remote_targets`, which quietly made "is this target
    // remote?" mean "is this target remote AND do we want to type into it?".
    // Remoteness is a property of the target; what we DO about it belongs at
    // the use site.
    #[cfg(windows)]
    let (unanchored_target, target_is_remote) = match anchor_guard.as_ref() {
        Some(g) => (None, g.is_remote(&settings.jumper_remote_match_strings)),
        None => match crate::anchor::foreground_snapshot(&settings.jumper_remote_match_strings) {
            Some((hwnd, remote)) => (Some(hwnd), remote),
            None => (None, false),
        },
    };
    #[cfg(not(windows))]
    let target_is_remote = false;
    #[cfg(not(windows))]
    let unanchored_target: Option<isize> = None;

    let remote_jump: bool = jumped && target_is_remote;

    // Base 50 ms settle + the extra for this target. `CopyToClipboard` leaves
    // the transcript as the final clipboard state, so there is nothing to undo.
    let restore_extra_ms = restore_extra_for_target(
        submit_restore_override_ms,
        target_is_remote,
        settings.clipboard_restore_delay_remote.map(|d| d.to_ms()),
        settings.clipboard_restore_delay.to_ms(),
    );
    let restore_after_ms = if clipboard_handling == ClipboardHandling::CopyToClipboard {
        None
    } else {
        Some(50 + restore_extra_ms)
    };

    // 1.2.0 substituted `PasteMethod::Direct` here whenever the target was
    // remote, to keep the transcript off the remote machine's clipboard. It was
    // WITHDRAWN in 1.3.0: batched Unicode injection mangles text over RDP
    // (missing and jumbled characters), and its unpaced auto-submit Enter fired
    // while the text was still draining through the virtual channel. Keystroke
    // delivery is still available, but only as a deliberate, global choice
    // (`Paste method = Direct`) — never auto-selected for a subset of
    // deliveries the user cannot predict from the UI.
    let paste_method = chosen_paste_method;

    // After a real jump, the freshly-activated target (especially an
    // RDP/Citrix session) may still be transitioning — completing activation,
    // moving focus — when the paste keystroke fires, so the Ctrl+V is
    // swallowed or lands nowhere. `begin_delivery` already settled a fixed
    // ~60 ms; add the user-configurable paste delay ON TOP, ONLY on a real
    // jump (`foreground_switched`) — the longer `*_remote` value for a remote
    // target, else the local one. Runs BEFORE the TOCTOU re-check below so a
    // focus change during this wait is still caught before we paste.
    #[cfg(windows)]
    {
        if let Some(guard) = anchor_guard.as_ref() {
            if guard.foreground_switched() {
                let extra_ms = if remote_jump {
                    settings.jumper_paste_delay_remote.to_ms()
                } else {
                    settings.jumper_paste_delay.to_ms()
                };
                if extra_ms > 0 {
                    log::debug!(
                        "paste timing: settle before paste = {} ms (jumped=true, remote={})",
                        extra_ms,
                        target_is_remote
                    );
                    std::thread::sleep(Duration::from_millis(extra_ms));
                }
            }
        }
    }

    // TOCTOU close (T-103): `begin_delivery` verified activation/focus, then
    // settled 60ms before returning — a focus change in that gap (a popup
    // stealing focus, the user clicking elsewhere), or during the post-jump
    // settle just above, must not receive a blind paste. Re-check immediately
    // before the paste keystroke and route a mismatch to the EXACT same
    // fail-closed park path as a verification failure. One cheap syscall — no
    // measurable latency on a normal delivery.
    #[cfg(windows)]
    {
        let focus_lost_before_paste = anchor_guard
            .as_ref()
            .map(|guard| !crate::anchor::await_guard_ready(guard, remote_jump))
            .unwrap_or(false);
        if focus_lost_before_paste {
            let reason = "focus changed before the paste keystroke (TOCTOU re-check)".to_string();
            info!(
                "Anchored delivery aborted: {} — parking text on clipboard",
                reason
            );
            park_anchor_failure(reason);
            // Finding 1 (second adversarial re-verify): this early-abort path
            // used to `return Ok(())` here directly, silently dropping the
            // owned `DeliveryGuard` WITHOUT ever running the
            // `finish_delivery` epilogue near the bottom of this function —
            // so the flow's `return_focus` policy never ran on this path,
            // even though every other paste outcome (success, a failure that
            // reaches the end of the function) does run it. `.take()`
            // consumes the guard here so the unconditional `finish_delivery`
            // call at the bottom (which only fires if `anchor_guard` is
            // still `Some`) can never double-run it.
            if let Some(guard) = anchor_guard.take() {
                let return_focus = if submit_override.is_some() {
                    settings.return_focus_submit
                } else {
                    settings.return_focus_output
                };
                crate::anchor::finish_delivery(&app_handle, guard, false, return_focus);
            }
            return Ok(());
        }
    }

    // Cross-platform handle to the active guard (if any) for the PER-KEYSTROKE
    // re-checks below (T-103, finding 1) — `None` on non-Windows (the Jumper
    // doesn't exist there) and for every non-anchored paste, so those are
    // completely unaffected. `guard_still_foreground` is itself a no-op
    // `true` on non-Windows, so no `#[cfg]` is needed at the call sites.
    #[cfg(windows)]
    let anchor_guard_ref: Option<&crate::anchor::DeliveryGuard> = anchor_guard.as_ref();
    #[cfg(not(windows))]
    let anchor_guard_ref: Option<&crate::anchor::DeliveryGuard> = None;

    let (do_submit, submit_key) = match forced_submit {
        // Submit shortcut: always submit (unless paste itself is disabled).
        Some((_, key)) => (paste_method != PasteMethod::None, key),
        // Manual paste (e.g. "Paste Last Transcription"): NEVER submit — it is a
        // plain re-paste and must not press Enter even if the global
        // auto-submit setting is on.
        None if manual_override.is_some() => (false, settings.auto_submit_key),
        // Normal path: honor the global auto-submit setting.
        None => (
            should_send_auto_submit(settings.auto_submit, paste_method),
            settings.auto_submit_key,
        ),
    };

    // When an anchored auto-submit had to JUMP the foreground to its target,
    // the target (especially an RDP/Citrix session) may still be committing the
    // pasted text when the fixed 50 ms Enter fires. Add the user-configurable
    // `jumper_submit_delay` on top of the base — ONLY on a real jump
    // (`foreground_switched`), so the already-focused case keeps its current
    // snappiness. 0 for non-anchored / non-jump / non-Windows deliveries.
    let jump_submit_extra_ms: u64 = {
        #[cfg(windows)]
        {
            pre_submit_extra_ms(
                jumped,
                target_is_remote,
                settings.jumper_submit_delay.to_ms(),
                settings.jumper_submit_delay_remote.to_ms(),
            )
        }
        #[cfg(not(windows))]
        {
            0
        }
    };

    // Perform the paste + submit, capturing the outcome instead of returning
    // early — the anchored-delivery epilogue below must run on EVERY path or a
    // failure mid-paste strands focus at the anchor and leaves it armed.
    let delivered: Result<(), String> = (|| {
        match paste_method {
            PasteMethod::None => {
                info!("PasteMethod::None selected - skipping paste action");
            }
            PasteMethod::Direct => {
                // Direct is the ONLY delivery path that never touches the
                // clipboard — that is the whole reason to pick it, and the UI
                // says so. It used to fall back to a Ctrl+V clipboard paste on
                // Windows/macOS "to avoid character-by-character flicker",
                // which silently broke that promise: anyone choosing Direct for
                // privacy got a clipboard paste. `input::paste_text_direct`
                // batches ordinary text into whole-run injections (one
                // `SendInput` per run on Windows), so there is no per-character
                // flicker to avoid.
                //
                // On Linux, native typing tools (wtype, dotool, …) are tried
                // first and fall back to the same enigo path.
                // T-103 (finding 1) parity: the clipboard path re-verifies the
                // anchor immediately before its keystroke. Routing Direct away
                // from `paste_via_clipboard` removed that check, so an anchored
                // Direct delivery could type blind into whatever had stolen
                // focus. Re-check here before the first keystroke.
                //
                // Chunking turned that residual gap into a real one: typing is
                // no longer a single burst, so a multi-batch transcript spends
                // hundreds of milliseconds injecting and focus can move
                // mid-flight. `still_valid` re-checks the anchor between
                // batches and before each Return/Tab, and aborts with the same
                // `ANCHOR_FOCUS_LOST` sentinel the pre-flight check uses so the
                // remaining text is parked instead of typed into a bystander.
                // Partial text already delivered cannot be recalled — that is
                // inherent to typing, and is why the check is per batch rather
                // than only up front.
                if let Some(guard) = anchor_guard_ref {
                    if !crate::anchor::await_guard_ready(guard, remote_jump) {
                        return Err(ANCHOR_FOCUS_LOST.to_string());
                    }
                }
                // An UNANCHORED delivery has no guard to re-verify. That was
                // harmless when it was one instant chord; it is not now that
                // this path types for hundreds of milliseconds — and it is
                // precisely the path this whole feature exists for (dictating
                // into an RDP window that already has focus). Pin the window
                // that was in front when typing began and require it to stay
                // there. `None` means "could not tell", which imposes no
                // constraint rather than blocking delivery.
                //
                // `remote_jump` is deliberately NOT passed here. Its bounded
                // 250 ms poll exists to absorb a freshly-activated window still
                // settling its inner focus, which the pre-flight check above
                // already waited out. Re-arming that budget on EVERY batch let
                // a long transcript accumulate seconds of polling (25 batches x
                // 250 ms), so once injection has begun the checks are strict
                // and immediate.
                let still_valid = still_valid_for(anchor_guard_ref, unanchored_target);
                // Chunking paces the injection so a remote session can drain
                // its input queue; one burst is instant locally but drops
                // characters over RDP. Normalised at the read boundary so a
                // hand-edited settings file cannot stall a delivery.
                let (chunk_chars, chunk_delay_ms) = crate::settings::typing_chunk_params(
                    &settings,
                    text.chars().count(),
                    text.chars()
                        .filter(|c| matches!(c, '\n' | '\r' | '\t'))
                        .count(),
                );
                #[cfg(target_os = "linux")]
                {
                    paste_direct(
                        &mut enigo,
                        &text,
                        chunk_chars,
                        chunk_delay_ms,
                        Some(&still_valid),
                        settings.typing_tool,
                    )?;
                }
                #[cfg(not(target_os = "linux"))]
                {
                    input::paste_text_direct(
                        &mut enigo,
                        &text,
                        chunk_chars,
                        chunk_delay_ms,
                        Some(&still_valid),
                    )?;
                }
            }
            PasteMethod::CtrlV | PasteMethod::CtrlShiftV | PasteMethod::ShiftInsert => {
                paste_via_clipboard(
                    &mut enigo,
                    &text,
                    &app_handle,
                    &paste_method,
                    paste_delay_ms,
                    restore_after_ms,
                    anchor_guard_ref,
                    unanchored_target,
                    remote_jump,
                )?
            }
            PasteMethod::ExternalScript => {
                let script_path = settings
                    .external_script_path
                    .as_ref()
                    .filter(|p| !p.is_empty())
                    .ok_or("External script path is not configured")?;
                // NOTE: an external script's own actions are opaque to Handy
                // (T-103, finding 1) — there is no "keystroke" here for a
                // per-action re-check to guard, and the Jumper is
                // Windows-only besides, so `anchor_guard_ref` is always
                // `None` on any path that reaches this arm anyway.
                paste_via_external_script(&text, script_path)?;
            }
        }

        if do_submit {
            #[cfg(windows)]
            log::debug!(
                "submit timing: jumped={}, remote={}, pre_submit_extra_ms={}",
                jumped,
                target_is_remote,
                jump_submit_extra_ms,
            );
            // Base ~50 ms settle, the target-aware pacing, and a typing tail.
            let submit_settle_ms = pre_submit_total_ms(
                50,
                jump_submit_extra_ms,
                paste_method,
                settings.typing_chunk_delay_ms as u64,
            );
            std::thread::sleep(Duration::from_millis(submit_settle_ms));
            // T-103 (finding 1): re-verify immediately before the auto-submit
            // keystroke too — the settle above is exactly the kind of gap a
            // focus-stealing popup can land in after the paste itself already
            // succeeded.
            if let Some(guard) = anchor_guard_ref {
                if !crate::anchor::await_guard_ready(guard, remote_jump) {
                    return Err(ANCHOR_SUBMIT_ABORTED.to_string());
                }
            } else if !unanchored_target_intact(unanchored_target) {
                // An UNANCHORED delivery had no guard, so this key used to be
                // sent unconditionally. Alt+Tab during the settle above then
                // put Enter into whatever took focus -- which can send a
                // message, submit a form or confirm a dialog in an application
                // the user never dictated into. The text itself already landed
                // in the original window; only the submit key is aborted.
                return Err(ANCHOR_SUBMIT_ABORTED.to_string());
            }
            send_return_key(&mut enigo, submit_key)?;
            // Post-Enter focus-return grace: keep the jumped-to target
            // foreground briefly so it actually PROCESSES the just-injected
            // Enter before the epilogue's `finish_delivery` returns focus to
            // `prev_foreground`. Only when this delivery switched the foreground
            // AND the flow will return focus — otherwise nothing is about to
            // yank focus, so no grace is needed. Held under the same Enigo lock
            // (intended: serializes against other synthesized input; runs on
            // the pipeline thread, never the UI thread, and never blocks the
            // user's own physical input).
            #[cfg(windows)]
            if let Some(guard) = anchor_guard_ref {
                let will_return_focus = if submit_override.is_some() {
                    settings.return_focus_submit
                } else {
                    settings.return_focus_output
                };
                if guard.foreground_switched() && will_return_focus {
                    std::thread::sleep(Duration::from_millis(POST_SUBMIT_RETURN_FOCUS_GRACE_MS));
                }
            }
        }
        Ok(())
    })();

    // T-103 (finding 1): a per-keystroke abort inside the closure above
    // (paste_via_clipboard's internal check, or the submit-key check just
    // above) is routed through the IDENTICAL fail-closed park path as every
    // other anchor-focus mismatch — never the generic paste-failure toast a
    // plain `Err` would otherwise trigger in the caller.
    #[cfg(windows)]
    if let Err(e) = &delivered {
        if e == ANCHOR_FOCUS_LOST || e == ANCHOR_FOCUS_LOST_PARTIAL || e == ANCHOR_SUBMIT_ABORTED {
            let submit_only = e == ANCHOR_SUBMIT_ABORTED;
            let partial = e == ANCHOR_FOCUS_LOST_PARTIAL;
            let anchored = anchor_guard.is_some();
            // Be honest about BOTH dimensions. Reporting a partial typed
            // delivery as if nothing landed invites the user to re-paste the
            // whole transcript and duplicate the prefix already in the window;
            // and calling an unanchored abort an "anchored delivery" names a
            // feature the user may not even be using.
            let reason = if submit_only {
                // The text IS in the target. Saying otherwise sends the user to
                // Paste Last and gets the transcript inserted twice.
                "your text was delivered, but focus moved before the submit key so it was not sent"
                    .to_string()
            } else {
                match (anchored, partial) {
                    (_, true) => "focus changed while typing — part of the transcript may already have been inserted, so check the target before re-inserting"
                    .to_string(),
                    (true, false) => {
                        "focus changed mid-paste (per-keystroke re-check)".to_string()
                    }
                    (false, false) => {
                        "focus changed before the text was delivered (per-keystroke re-check)"
                            .to_string()
                    }
                }
            };
            info!(
                "Delivery aborted (anchored={anchored}, partial={partial}, submit_only={submit_only}): {reason}"
            );
            park_anchor_failure_detail(reason, anchored, partial);
            // Finding 1 (second adversarial re-verify): same fix as the
            // TOCTOU-close abort above — run `finish_delivery` (delivered_ok
            // = false, the flow's `return_focus`) before returning, instead
            // of silently dropping the guard. `anchor_guard_ref`'s borrow of
            // `anchor_guard` ended when the closure above returned, so
            // `.take()` here is fine.
            if let Some(guard) = anchor_guard.take() {
                let return_focus = if submit_override.is_some() {
                    settings.return_focus_submit
                } else {
                    settings.return_focus_output
                };
                crate::anchor::finish_delivery(&app_handle, guard, false, return_focus);
            }
            return Ok(());
        }
    }

    // Track-last-output: capture where the text just landed into the configured
    // slot, BEFORE any focus-return so the slot points at the paste target. The
    // switch + target slot are PER FLOW and independent — the dictate flow and
    // the Transcribe & Submit flow (submit_override present) each have their
    // own, mirroring return_focus_output/submit. Unlike the deferred on-finish
    // Set/Clear action (T-102), this capture is decided and executed in the
    // SAME instant the paste finishes — there's no earlier snapshot that
    // could go stale, so the generation-CAS guard doesn't apply here; a
    // plain unconditional, authoritative commit is correct.
    //
    // T-104: for an ANCHORED delivery (`anchor_guard` is `Some`), source the
    // capture from the guard's already-verified hwnd/control
    // (`anchor::track_from_guard`) instead of a fresh foreground-window query
    // (the old `set_slot` path here) — a submit keystroke sent just above
    // (Enter closing a dialog, navigating a composer) can have already moved
    // the foreground window away from the real delivery target by this point,
    // and `set_slot`'s `GetForegroundWindow()` would silently track wherever
    // Enter left focus instead. Non-anchored (plain) pastes are unaffected —
    // they keep the pre-existing `set_slot` foreground-query capture, since
    // there is no known delivery target to fall back on for them.
    #[cfg(windows)]
    {
        let is_submit_flow = submit_override.is_some();
        let (track_enabled, track_slot) = if is_submit_flow {
            (
                settings.jumper_track_submit_enabled,
                settings.jumper_track_submit_slot,
            )
        } else {
            (
                settings.jumper_track_output_enabled,
                settings.jumper_track_output_slot,
            )
        };
        if flow_paste && delivered.is_ok() && paste_method != PasteMethod::None && track_enabled {
            let slot = (track_slot as usize).min(crate::anchor::SLOT_COUNT - 1);
            // T-302: cursor save is PER-SLOT, not per-flow — resolve the flag
            // for the flow's TARGET slot (the one we're about to capture into)
            // via the single source of truth `anchor::slot_save_cursor`,
            // replacing the removed per-flow output/submit toggles. Both
            // branches thread this SAME per-slot-resolved `save_cursor`:
            // anchored deliveries via `set_slot_from_guard`, the non-anchored
            // fallback via `set_slot_with_cursor_policy`. `slot` is already
            // clamped to the valid range, so it is exactly the target slot
            // index whose flag governs this capture (and `slot_save_cursor` is
            // itself bounds-safe regardless).
            let save_cursor =
                crate::anchor::slot_save_cursor(&settings.jumper_save_cursor_slots, slot);
            let result = match anchor_guard.as_ref() {
                Some(guard) => {
                    crate::anchor::track_from_guard(&app_handle, guard, slot, save_cursor)
                }
                None => crate::anchor::set_slot_with_cursor_policy(&app_handle, slot, save_cursor),
            };
            if let Err(e) = result {
                log::debug!("track-last-output capture skipped: {}", e);
            }
        }
    }

    // Anchored delivery epilogue — strictly AFTER the submit key so Enter
    // lands in the anchored app, and on failure paths too (never strand focus
    // at the anchor). Anchors are always kept; whether focus returns to the
    // auto-captured start location is the finishing FLOW's setting (submit
    // override present = Transcribe & Submit).
    #[cfg(windows)]
    if let Some(guard) = anchor_guard {
        let return_focus = if submit_override.is_some() {
            settings.return_focus_submit
        } else {
            settings.return_focus_output
        };
        crate::anchor::finish_delivery(&app_handle, guard, delivered.is_ok(), return_focus);
    }

    // NOTE: the deferred "on finish" Set/Clear is NOT consumed here — the
    // take's own pipeline took ownership of it at stop time (actions.rs) and
    // runs it after this paste returns, so a delayed paste can never execute
    // another take's action.

    // Honour `CopyToClipboard` even when delivery FAILED.
    //
    // This used to sit after `delivered?`, so an enigo/typing failure or a
    // failing external script skipped it entirely and the user got neither the
    // delivery nor the clipboard copy their setting promises. The ordinary
    // transcription flow has an outer rescue, but a manual Paste Last only logs
    // the error -- the transcript simply vanished. The delivery error still
    // wins as the returned error; this only guarantees the clipboard
    // postcondition runs first.
    let park_failed =
        clipboard_handling == ClipboardHandling::CopyToClipboard && !park_text(&app_handle, &text);

    delivered?;

    if park_failed {
        return Err("Failed to copy to clipboard".to_string());
    }

    // After pasting, optionally copy to clipboard based on settings.
    //
    // Finding 3 (second adversarial re-verify): this write used to go
    // straight to `app_handle.clipboard().write_text` — no
    // `CLIPBOARD_WRITE_LOCK`, no `RESTORE_GEN` bump. An OLDER paste's still-
    // pending delayed restore (armed before this write, e.g. a prior take
    // with a different clipboard handling) could fire AFTER this write and
    // clobber it, since nothing about this write superseded that restore's
    // armed generation. Routing it through `park_text` — the same
    // bump-then-write-under-`CLIPBOARD_WRITE_LOCK` primitive
    // `park_anchor_failure`/`paste_via_clipboard`'s own initial write already
    // use — supersedes any such pending restore.
    // (Performed above, before `delivered?`, so a failed delivery cannot skip
    // it. The comment is kept here because this is where the write logically
    // belongs in the sequence.)

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_submit_requires_setting_enabled() {
        assert!(!should_send_auto_submit(false, PasteMethod::CtrlV));
        assert!(!should_send_auto_submit(false, PasteMethod::Direct));
    }

    #[test]
    fn auto_submit_skips_none_paste_method() {
        assert!(!should_send_auto_submit(true, PasteMethod::None));
    }

    #[test]
    fn auto_submit_runs_for_active_paste_methods() {
        assert!(should_send_auto_submit(true, PasteMethod::CtrlV));
        assert!(should_send_auto_submit(true, PasteMethod::Direct));
        assert!(should_send_auto_submit(true, PasteMethod::CtrlShiftV));
        assert!(should_send_auto_submit(true, PasteMethod::ShiftInsert));
    }

    fn dummy_override(key: AutoSubmitKey) -> SubmitOverride {
        SubmitOverride {
            submit: Some((PasteMethod::CtrlV, key)),
            clipboard: Some(ClipboardHandling::CopyToClipboard),
            restore_extra_ms: 0,
        }
    }

    /// T-116, mirrors `anchor::delivery_intent_is_take_scoped_one_shot`:
    /// `take_submit_override()` is a strict one-shot over the `SUBMIT_OVERRIDE`
    /// mailbox — a second take must never observe a prior take's
    /// already-consumed arm, and a value captured BEFORE a later `set_
    /// submit_override` call stays `None` (it's a plain owned `Option`, never
    /// a live view of the global). Combined into one test — `SUBMIT_OVERRIDE`
    /// is a shared global and cargo runs tests in parallel by default, so a
    /// second test touching the same static could otherwise interleave (same
    /// rationale as the `anchor.rs` cross-platform tests).
    #[test]
    fn submit_override_is_take_scoped_one_shot() {
        clear_submit_override();
        assert_eq!(take_submit_override(), None);

        set_submit_override(dummy_override(AutoSubmitKey::Enter));
        let captured = take_submit_override();
        assert_eq!(captured, Some(dummy_override(AutoSubmitKey::Enter)));

        // Consumed: a second (later) take's capture must see None, never the
        // prior take's override.
        assert_eq!(take_submit_override(), None);

        // A value captured EARLIER is unaffected by an arm that lands AFTER
        // it was taken — it never re-reads the global (mirrors the
        // DeliveryIntent ownership-by-value proof).
        let earlier_take_override = take_submit_override();
        set_submit_override(dummy_override(AutoSubmitKey::CtrlEnter));
        let later_take_override = take_submit_override();
        assert_eq!(
            later_take_override,
            Some(dummy_override(AutoSubmitKey::CtrlEnter))
        );
        assert_eq!(earlier_take_override, None);

        // A new take's start()-time clear (`clear_submit_override`, mirroring
        // `anchor::clear_delivery_request`) must never reach back into a
        // value an earlier take already captured by value.
        set_submit_override(dummy_override(AutoSubmitKey::Enter));
        let already_stopped_takes_override = take_submit_override();
        clear_submit_override(); // the NEXT take starting, clearing the mailbox
        assert_eq!(
            already_stopped_takes_override,
            Some(dummy_override(AutoSubmitKey::Enter)),
            "a later clear must not strip an already-captured take's override"
        );
    }
}
