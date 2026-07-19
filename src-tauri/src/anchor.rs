//! Jumper (Windows-only v1): five jump slots for desktop text fields.
//!
//! Slot 0 is the HOT slot — the original "anchor": transcription flows can
//! set/clear/jump/deliver-to it via per-flow event actions. Any slot can be
//! the target of track-last-output (`jumper_track_enabled`/`_slot`), which
//! auto-captures where a flow last pasted. NO slot is ever auto-cleared by a
//! delivery (0.40 rework) — slots live until overwritten, cleared, or their
//! window dies. Slots 1–4 are STATIC bookmarks: set via `jump_set_slot_N`,
//! jumped via `jump_slot_N`. All slots share the same
//! capture/validation/delivery machinery and safety rails:
//!
//! - capture via `GetGUIThreadInfo` (no thread-input attachment at capture),
//!   durable identity (HWND + PID + TID + class, revalidated at delivery —
//!   bare HWNDs get recycled), password/self-window rejection;
//! - delivery NEVER pastes blind: activation verified via
//!   `GetForegroundWindow`, control focus verified via `GetGUIThreadInfo`
//!   (both the window AND the specific control — `DeliveryGuard` tracks the
//!   captured control's hwnd, not just the window), re-verified via
//!   `guard_still_foreground` immediately before EVERY synthesized keystroke
//!   a delivery dispatches (T-103 — the 60ms settle in `begin_delivery`, the
//!   clipboard-paste delay, and the auto-submit delay each leave a gap a
//!   focus-stealing popup or a refocus to a sibling control (e.g. a password
//!   field) could exploit), Enter fires before focus-return, focus restored
//!   only if the user didn't intervene; a failed delivery (at any of those
//!   checkpoints) parks the text on the clipboard and keeps the slot
//!   (cleared only when the window is destroyed);
//! - delivery is strictly opt-in per paste via a take-scoped `DeliveryIntent`
//!   (T-101): the coordinator's finishing press arms a process-global
//!   one-shot (`request_delivery`), but the take that will actually paste
//!   consumes it into an owned `DeliveryIntent` synchronously at stop() time
//!   (mirroring `POST_TAKE_ACTION`'s take-ownership) — a pathologically
//!   delayed main-thread paste can then never observe a NEWER take's intent.
//!
//! Live slots are in-memory (window handles die with their windows); the
//! opt-in `jumper_persist` setting additionally saves each slot's IDENTITY
//! (app + window/control class) and re-resolves it against live windows at
//! startup and lazily on use — see `restore_slots`/`snapshot_slots`. Every
//! slot write bumps a per-slot capture generation (T-102), tracked in a
//! SEPARATE array from the targets themselves so clearing a slot to `None`
//! never erases its generation history — a None→Set→Clear cycle a stale
//! writer didn't observe is still visible as a generation change even though
//! the slot reads back empty either way (the ABA hole a same-struct
//! generation field had). Three kinds of writers plan their capture well
//! BEFORE they actually commit it — persisted-slot re-resolution (an
//! `EnumWindows` scan), the on-finish Set/Clear action (armed at the
//! finishing press, committed only after the whole take's paste completes,
//! seconds or minutes later for a long recording), and stale-target cleanup
//! (`jump`/`begin_delivery` discovering a dead window mid-validate) — all
//! three snapshot the target slot's generation up front and commit via a
//! compare-and-swap: if a fresher write (a manual Set/Clear, or another
//! automatic write) landed in the meantime, the stale one is silently
//! skipped instead of clobbering it — that snapshot must come from the SAME
//! lock acquisition that observed whatever condition justified the delayed
//! write in the first place (e.g. "this slot is empty"), never a later,
//! separately re-read one (adversarial-review finding 5), or a write that
//! landed in between the two reads can be silently absorbed. The in-memory
//! CAS and the settings PERSIST are ordered consistently (mutate under the
//! `SLOTS` lock first, persist after releasing it — never the reverse, and
//! never with the lock held across settings I/O); the persist step itself
//! (finding 6) doesn't just check-then-write the CALLER's snapshot — two
//! such checks can still interleave their own settings read-modify-write
//! cycles — it serializes ALL persistence through a dedicated
//! `PERSIST_LOCK` and RE-READS the slot's CURRENT state right before the
//! settings write, so whatever lands in `jumper_saved_slots` is always
//! last-writer-consistent. Track-last-output does NOT need any of this: it
//! decides and captures in the same instant the paste finishes, so there is
//! no earlier snapshot that could go stale.

use serde::Serialize;
use specta::Type;
use tauri::AppHandle;

/// Number of jump slots (index 0 = hot).
pub const SLOT_COUNT: usize = 5;
/// The hot slot index.
pub const HOT: usize = 0;

/// What the UI shows for an occupied slot (never the window title — titles are
/// volatile and can carry sensitive document names). `stale` = the slot only
/// exists as a persisted identity whose window hasn't been found yet (shown
/// red in the UI; re-resolved when its app reappears).
#[derive(Clone, Serialize, Type)]
pub struct AnchorStatus {
    pub app: String,
    pub control_class: String,
    pub stale: bool,
}

/// Outcome of preparing an anchored delivery inside the paste pipeline.
pub enum BeginDelivery {
    /// No delivery requested — proceed with a normal paste at current focus.
    NoAnchor,
    /// Target activated and focused; paste now, then call `finish_delivery`.
    Ready(DeliveryGuard),
    /// A delivery was requested but could not be verified. Do NOT paste blind.
    Failed { reason: String },
}

pub struct DeliveryGuard {
    #[cfg(windows)]
    prev_foreground: isize,
    #[cfg(windows)]
    target_hwnd: isize,
    /// The captured CONTROL's hwnd (finding 2, adversarial review): the
    /// original guard only tracked the top-level window, so focus moving to
    /// a DIFFERENT control inside the same window — including a password
    /// field — was invisible to `guard_still_foreground`. Re-verified there
    /// via `GetGUIThreadInfo` on every TOCTOU check.
    #[cfg(windows)]
    target_control: isize,
    #[cfg(windows)]
    slot: usize,
}

#[cfg(windows)]
mod win {
    use super::{AnchorStatus, BeginDelivery, DeliveryGuard, HOT, SLOT_COUNT};
    use log::{debug, info, warn};
    use once_cell::sync::Lazy;
    use std::sync::Mutex;
    use tauri::{AppHandle, Emitter};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows::Win32::System::Threading::{
        AttachThreadInput, GetCurrentThreadId, OpenProcess, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, EnumWindows, FindWindowExW, GA_ROOT, GUITHREADINFO, GWL_STYLE,
        GetAncestor, GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetWindowLongW,
        GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, SW_RESTORE,
        SetForegroundWindow, ShowWindow, SwitchToThisWindow,
    };
    use windows::core::BOOL;

    /// Edit-control style: password box. Defined locally to avoid feature churn.
    const ES_PASSWORD: i32 = 0x0020;

    #[derive(Clone)]
    struct Target {
        hwnd: isize,
        control: isize,
        pid: u32,
        tid: u32,
        window_class: String,
        control_class: String,
        app: String,
    }

    /// Slot storage: the live targets AND their capture generations, guarded
    /// by ONE mutex so a read/compare/write is always atomic. Generations
    /// live in a SEPARATE array from `targets` — NOT inside `Option<Target>`
    /// (T-102 ABA fix, adversarial-review finding 4): the original design
    /// stored `generation` INSIDE `Target`, so clearing a slot to `None`
    /// erased its generation along with it. A deferred writer that snapshot
    /// `expected` while a slot was empty, then observed an UNRELATED
    /// None→Set→Clear cycle happen entirely without it, would see the slot
    /// read back as `None` again — the SAME state it snapshotted — and
    /// wrongly conclude nothing had changed, resurrecting a stale capture.
    /// Bumping the generation on EVERY write (Set, Clear, even clearing an
    /// already-empty slot) regardless of occupancy closes that hole.
    struct SlotState {
        targets: [Option<Target>; SLOT_COUNT],
        generations: [u64; SLOT_COUNT],
    }

    static SLOTS: Lazy<Mutex<SlotState>> = Lazy::new(|| {
        Mutex::new(SlotState {
            targets: [None, None, None, None, None],
            generations: [0; SLOT_COUNT],
        })
    });

    /// Monotonic allocator for slot generations. Starts at 1 so `0` is a safe
    /// "never written" sentinel for an untouched slot.
    static NEXT_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

    fn next_generation() -> u64 {
        NEXT_GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// Pure compare-and-swap over the in-memory slot state: commit `target`
    /// into `slot` only if its CURRENT generation still matches `expected` —
    /// the snapshot the caller took before starting its (possibly slow)
    /// capture work. The comparison is blind to whether the slot is
    /// occupied; only the counter matters (see the `SlotState` doc / T-102
    /// ABA fix). Returns the freshly allocated generation on success (`None`
    /// on a stale race) so callers can generation-guard a subsequent PERSIST
    /// step (finding 6) without holding this lock across settings I/O.
    /// Extracted from the SLOTS-mutex-holding callers so it's unit-testable
    /// without a live window or AppHandle.
    fn cas_commit(
        state: &mut SlotState,
        slot: usize,
        expected: u64,
        target: Target,
    ) -> Option<u64> {
        if state.generations[slot] != expected {
            return None;
        }
        let new_gen = next_generation();
        state.generations[slot] = new_gen;
        state.targets[slot] = Some(target);
        Some(new_gen)
    }

    /// Serializes ALL slot persistence (finding 6, adversarial re-verify).
    /// The earlier design (`persist_if_current`, generation-guarded) checked
    /// "is my write still current" under the `SLOTS` lock, then released it
    /// and did settings I/O with NO lock held across that check-then-write —
    /// two persist calls (even for the SAME slot) could still interleave
    /// their own `get_settings`/`write_settings` RMW cycles: writer A checks
    /// "still current" (true), then before A's own `get_settings`/
    /// `write_settings` runs, writer B (a fresher write that raced in)
    /// completes its ENTIRE settings RMW; A's now-stale `get_settings` (read
    /// before B's write) + `write_settings` then overwrites B's change with
    /// A's older snapshot. A dedicated mutex held across the ENTIRE
    /// read-mutate-write of the settings store (never across `SLOTS`, never
    /// across the slot mutation itself — only across this persist step)
    /// closes that gap by serializing persist calls against EACH OTHER, not
    /// just checking staleness against a snapshot.
    static PERSIST_LOCK: Mutex<()> = Mutex::new(());

    /// Persist a slot's mutation into the saved identities. Rather than
    /// writing the CALLER's own (possibly already-stale-by-the-time-we-get-
    /// the-lock) snapshot, this ALWAYS re-reads the slot's CURRENT target
    /// fresh from `SLOTS` — a quick, separate lock acquisition, released
    /// before any settings I/O — right before the settings write, under
    /// `PERSIST_LOCK`. So even if a newer mutation raced ahead of this call,
    /// what actually lands in settings is whatever is current AT THE TIME OF
    /// THE WRITE (last-writer-consistent), never a stale snapshot clobbering
    /// a fresher one. Idempotent: multiple callers persisting the same
    /// current state is harmless — whichever runs last under `PERSIST_LOCK`
    /// writes the same thing.
    fn persist_current_slot(app: &AppHandle, slot: usize) {
        let _persist_guard = PERSIST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let current = SLOTS
            .lock()
            .map(|state| state.targets.get(slot).cloned().flatten())
            .unwrap_or(None);
        persist_slot(app, slot, current.as_ref());
    }

    /// Fetch a slot's target AND its generation in ONE lock acquisition, so
    /// the two can't drift apart (used by stale-target cleanup — finding 5 —
    /// which needs the exact generation the target was read under).
    fn get_slot_with_generation(slot: usize) -> Option<(Target, u64)> {
        let state = SLOTS.lock().ok()?;
        let target = state.targets.get(slot)?.clone()?;
        let generation = *state.generations.get(slot)?;
        Some((target, generation))
    }

    /// Pure helper: `slot`'s generation IF it is currently empty, else
    /// `None`. Extracted so the "observed empty + snapshot generation" logic
    /// is unit-testable without a live `SLOTS` mutex (finding 5).
    fn empty_generation(state: &SlotState, slot: usize) -> Option<u64> {
        if state.targets.get(slot)?.is_some() {
            return None;
        }
        state.generations.get(slot).copied()
    }

    /// Fetch "is this slot empty, and if so what's its generation" in ONE
    /// lock acquisition (finding 5, adversarial re-verify): `jump`/
    /// `begin_delivery`/`restore_persisted_slots` used to call
    /// `get_slot(slot).is_none()` and then, separately, let `resolve_saved`
    /// take its OWN (later) generation snapshot via a second, independent
    /// lock acquisition. A manual Set landing in the gap between those two
    /// separate locks would be captured as `resolve_saved`'s "expected"
    /// baseline — so when `resolve_saved`'s slow `EnumWindows` scan finished
    /// and its CAS commit ran against that (now-current) generation, the
    /// stale restored-from-settings identity would incorrectly win the race
    /// and clobber the fresher manual capture. Capturing "empty" and "its
    /// generation" together, in the SAME lock acquisition the caller uses to
    /// decide whether to call `resolve_saved` at all, and threading that
    /// exact snapshot through to `resolve_saved`'s CAS closes the gap.
    fn empty_slot_generation(slot: usize) -> Option<u64> {
        let state = SLOTS.lock().ok()?;
        empty_generation(&state, slot)
    }

    fn emit_changed(app: &AppHandle) {
        let _ = app.emit("anchor-changed", statuses(app));
    }

    fn class_name(hwnd: HWND) -> String {
        let mut buf = [0u16; 128];
        let n = unsafe { GetClassNameW(hwnd, &mut buf) };
        if n > 0 {
            String::from_utf16_lossy(&buf[..n as usize])
        } else {
            String::new()
        }
    }

    /// Executable stem for the anchored process ("notepad", "chrome", …).
    fn process_name(pid: u32) -> Option<String> {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 512];
            let mut len = buf.len() as u32;
            let res = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(buf.as_mut_ptr()),
                &mut len,
            );
            let _ = CloseHandle(handle);
            res.ok()?;
            let full = String::from_utf16_lossy(&buf[..len as usize]);
            let stem = std::path::Path::new(&full)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())?;
            Some(stem)
        }
    }

    /// Live slots, with persisted-but-unresolved identities surfaced as
    /// `stale` entries so the UI can show them red.
    pub fn statuses(app: &AppHandle) -> Vec<Option<AnchorStatus>> {
        let live: Vec<Option<AnchorStatus>> = SLOTS
            .lock()
            .map(|state| {
                state
                    .targets
                    .iter()
                    .map(|t| {
                        t.as_ref().map(|t| AnchorStatus {
                            app: t.app.clone(),
                            control_class: t.control_class.clone(),
                            stale: false,
                        })
                    })
                    .collect()
            })
            .unwrap_or_else(|_| vec![None; SLOT_COUNT]);

        let settings = crate::settings::get_settings(app);
        if !settings.jumper_persist {
            return live;
        }
        live.into_iter()
            .enumerate()
            .map(|(i, entry)| {
                entry.or_else(|| {
                    settings
                        .jumper_saved_slots
                        .get(i)
                        .and_then(|s| s.as_ref())
                        .map(|s| AnchorStatus {
                            app: s.app.clone(),
                            control_class: s.control_class.clone(),
                            stale: true,
                        })
                })
            })
            .collect()
    }

    /// Mirror a slot mutation into the persisted identities (when enabled).
    fn persist_slot(app: &AppHandle, slot: usize, target: Option<&Target>) {
        let mut settings = crate::settings::get_settings(app);
        if !settings.jumper_persist {
            return;
        }
        if settings.jumper_saved_slots.len() < SLOT_COUNT {
            settings.jumper_saved_slots.resize(SLOT_COUNT, None);
        }
        settings.jumper_saved_slots[slot] = target.map(|t| crate::settings::SavedJumpSlot {
            app: t.app.clone(),
            window_class: t.window_class.clone(),
            control_class: t.control_class.clone(),
        });
        crate::settings::write_settings(app, settings);
    }

    pub fn clear(app: &AppHandle, slot: usize) {
        if slot >= SLOT_COUNT {
            return;
        }
        // T-102/finding 6: mutate SLOTS under the lock first, persist AFTER
        // releasing it — unifies the ordering with every other writer below.
        // Bumps the generation even though the slot ends up empty (finding 4
        // ABA fix). The persist step (finding 6) re-reads whatever is CURRENT
        // under `PERSIST_LOCK` rather than trusting this call's own snapshot —
        // see `persist_current_slot`.
        match SLOTS.lock() {
            Ok(mut state) => {
                let g = next_generation();
                state.targets[slot] = None;
                state.generations[slot] = g;
            }
            Err(_) => return,
        };
        emit_changed(app);
        persist_current_slot(app, slot);
    }

    // ------------------------------------------------------------------
    // Persistence: re-resolve saved identities against live windows.
    // ------------------------------------------------------------------

    struct FindCtx {
        app: String,
        window_class: String,
        found: isize,
    }

    unsafe extern "system" fn find_window_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = unsafe { &mut *(lparam.0 as *mut FindCtx) };
        unsafe {
            if !IsWindowVisible(hwnd).as_bool() {
                return true.into();
            }
            if class_name(hwnd) != ctx.window_class {
                return true.into();
            }
            let mut pid = 0u32;
            let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
            match process_name(pid) {
                Some(p) if p.eq_ignore_ascii_case(&ctx.app) => {
                    ctx.found = hwnd.0 as isize;
                    false.into() // stop enumeration
                }
                _ => true.into(),
            }
        }
    }

    /// Try to turn one saved identity back into a live target: first visible
    /// window with the same executable + window class; the control is the
    /// first direct child of the saved class (window itself otherwise —
    /// delivery accepts window-level focus). `expected` is the slot's
    /// generation at the moment the CALLER observed it empty (finding 5) —
    /// it must be captured in the SAME lock acquisition as that emptiness
    /// check (see `empty_slot_generation`), never re-read in here: re-reading
    /// it here, AFTER the caller's separate check, would let a manual Set
    /// landing in that gap become this function's own "expected" baseline —
    /// and the stale restore below would then incorrectly win the race
    /// against it once the slow scan finishes.
    fn resolve_saved(app: &AppHandle, slot: usize, expected: u64) -> bool {
        let settings = crate::settings::get_settings(app);
        if !settings.jumper_persist {
            return false;
        }
        let Some(Some(saved)) = settings.jumper_saved_slots.get(slot).map(|s| s.as_ref()) else {
            return false;
        };
        unsafe {
            let mut ctx = FindCtx {
                app: saved.app.clone(),
                window_class: saved.window_class.clone(),
                found: 0,
            };
            let _ = EnumWindows(
                Some(find_window_cb),
                LPARAM(&mut ctx as *mut FindCtx as isize),
            );
            if ctx.found == 0 {
                return false;
            }
            let hwnd = HWND(ctx.found as _);
            let mut pid = 0u32;
            let tid = GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if tid == 0 {
                return false;
            }
            // Best-effort control: first direct child with the saved class.
            let control = if saved.control_class == saved.window_class {
                hwnd
            } else {
                let wide: Vec<u16> = saved
                    .control_class
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                FindWindowExW(
                    Some(hwnd),
                    None,
                    windows::core::PCWSTR(wide.as_ptr()),
                    windows::core::PCWSTR::null(),
                )
                .unwrap_or(hwnd)
            };
            let control_class = class_name(control);
            let target = Target {
                hwnd: hwnd.0 as isize,
                control: control.0 as isize,
                pid,
                tid,
                window_class: saved.window_class.clone(),
                control_class,
                app: saved.app.clone(),
            };
            info!(
                "Jump slot {} restored: {} ({})",
                slot, target.app, target.window_class
            );
            // The EnumWindows search above takes real time — commit only if
            // the slot's generation still matches what we snapshotted BEFORE
            // the scan, so a manual Set (or even a manual Clear) that landed
            // while we were searching isn't clobbered by this stale restore.
            // Live re-population never persists here — the identity already
            // lives in settings; only the in-memory SLOTS commit matters.
            match SLOTS.lock() {
                Ok(mut state) => {
                    if cas_commit(&mut state, slot, expected, target).is_none() {
                        debug!(
                            "Persisted-slot restore for slot {} skipped — a fresher capture won the race",
                            slot
                        );
                        return false;
                    }
                }
                Err(_) => return false,
            }
            true
        }
    }

    /// Snapshot every live slot into the persisted identities (used when the
    /// user turns persistence ON so existing anchors survive the restart).
    /// Finding 6: same principle as `persist_current_slot` — held across
    /// `PERSIST_LOCK` for the whole read-mutate-write so this can't race a
    /// concurrent single-slot persist's own settings RMW; `SLOTS` is only
    /// ever locked briefly to clone the current targets, never across the
    /// settings I/O.
    pub fn snapshot_all(app: &AppHandle) {
        let _persist_guard = PERSIST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let mut settings = crate::settings::get_settings(app);
        let live = SLOTS.lock().map(|s| s.targets.clone()).unwrap_or_default();
        settings.jumper_saved_slots = (0..SLOT_COUNT)
            .map(|i| {
                live.get(i)
                    .and_then(|t| t.as_ref())
                    .map(|t| crate::settings::SavedJumpSlot {
                        app: t.app.clone(),
                        window_class: t.window_class.clone(),
                        control_class: t.control_class.clone(),
                    })
            })
            .collect();
        crate::settings::write_settings(app, settings);
    }

    /// Attempt to restore every persisted slot at startup. Unresolved slots
    /// stay saved (red in the UI) and retry lazily on jump/delivery.
    pub fn restore_persisted_slots(app: &AppHandle) {
        let settings = crate::settings::get_settings(app);
        if !settings.jumper_persist {
            return;
        }
        let mut restored = 0;
        for slot in 0..SLOT_COUNT {
            if let Some(expected) = empty_slot_generation(slot) {
                if resolve_saved(app, slot, expected) {
                    restored += 1;
                }
            }
        }
        if restored > 0 {
            info!("Jumper: restored {restored} persisted slot(s)");
            emit_changed(app);
        }
    }

    /// Capture the current foreground window/control as a `Target`, applying
    /// every capture-time refusal (no foreground window, Handy's own window,
    /// password field). `Target` no longer carries a generation (that lives
    /// in `SlotState`, separately) — callers assign one at commit time via
    /// `next_generation()`, kept as close as possible to the moment the
    /// target actually lands in `SLOTS`.
    fn capture_current_target(_app: &AppHandle, slot: usize) -> Result<Target, String> {
        if slot >= SLOT_COUNT {
            return Err(format!("invalid jump slot {slot}"));
        }
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return Err("no foreground window".into());
            }
            let mut pid = 0u32;
            let tid = GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if tid == 0 {
                return Err("could not identify the foreground window".into());
            }
            if pid == std::process::id() {
                return Err("cannot anchor Handy Tool's own windows".into());
            }
            let mut gti = GUITHREADINFO {
                cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            let got = GetGUIThreadInfo(tid, &mut gti).is_ok();
            let control = if got && !gti.hwndFocus.0.is_null() {
                gti.hwndFocus
            } else {
                hwnd
            };
            let control_class = class_name(control);
            if control_class.eq_ignore_ascii_case("Edit") {
                let style = GetWindowLongW(control, GWL_STYLE);
                if style & ES_PASSWORD != 0 {
                    return Err("refusing to anchor a password field".into());
                }
            }
            let app_name = process_name(pid).unwrap_or_else(|| "unknown".into());
            Ok(Target {
                hwnd: hwnd.0 as isize,
                control: control.0 as isize,
                pid,
                tid,
                window_class: class_name(hwnd),
                control_class,
                app: app_name,
            })
        }
    }

    /// Manual/unconditional capture: always wins regardless of what was
    /// there before (a direct user action — Set Anchor, the hot-slot Set
    /// binding — is authoritative and never deferred, so there's nothing to
    /// compare-and-swap against).
    pub fn set_slot(app: &AppHandle, slot: usize) -> Result<AnchorStatus, String> {
        let target = capture_current_target(app, slot)?;
        info!(
            "Jump slot {} set: {} (class '{}', pid {}, tid {})",
            slot, target.app, target.control_class, target.pid, target.tid
        );
        // T-102/finding 6: mutate SLOTS under the lock FIRST, persist AFTER
        // releasing it (previously persisted BEFORE the SLOTS write —
        // opposite order from the automatic writers below, so an
        // interleaving with one of those could leave `jumper_saved_slots`
        // stale). A direct user action is still authoritative — it always
        // wins the SLOTS write unconditionally — and the persist step
        // re-reads whatever is CURRENT under `PERSIST_LOCK` rather than
        // trusting this call's own snapshot, so two rapid manual writes (or a
        // manual write racing an automatic one) can't have their settings I/O
        // land out of order either — see `persist_current_slot`.
        match SLOTS.lock() {
            Ok(mut state) => {
                let g = next_generation();
                state.generations[slot] = g;
                state.targets[slot] = Some(target.clone());
            }
            // Never report success without storing — callers use the Ok
            // to e.g. suppress the hot slot's one-shot clear.
            Err(_) => return Err("jump slot storage is unavailable".into()),
        };
        emit_changed(app);
        persist_current_slot(app, slot);
        Ok(AnchorStatus {
            app: target.app,
            control_class: target.control_class,
            stale: false,
        })
    }

    /// Snapshot a slot's current capture generation. Every valid slot always
    /// HAS a generation (`0` = never written, regardless of whether the slot
    /// is currently occupied — see the `SlotState` doc / T-102 ABA fix, so
    /// this is never `None` for an in-range slot). A caller planning a
    /// DELAYED automatic write (the deferred on-finish Set/Clear action,
    /// persisted-slot re-resolution, stale-target cleanup) takes this
    /// snapshot up front and commits later via [`set_slot_if_unchanged`]/
    /// [`clear_if_unchanged`] — see the module doc and T-102.
    pub fn current_generation(slot: usize) -> u64 {
        SLOTS
            .lock()
            .map(|state| state.generations.get(slot).copied().unwrap_or(0))
            .unwrap_or(0)
    }

    /// Automatic/deferred capture: commits ONLY if the slot's generation is
    /// still exactly `expected` — i.e. nothing (manual Set, another
    /// automatic write, a Clear) touched it since the caller's snapshot.
    /// Returns `false` (no-op) on a stale race, never overwriting a fresher
    /// state. The capture itself still runs unconditionally (foreground
    /// window can change harmlessly between snapshot and here); only the
    /// final commit is guarded, and it happens under one `SLOTS` lock
    /// acquisition so the compare-and-swap is atomic.
    pub fn set_slot_if_unchanged(app: &AppHandle, slot: usize, expected: u64) -> bool {
        let target = match capture_current_target(app, slot) {
            Ok(t) => t,
            Err(e) => {
                debug!("Automatic capture for slot {} skipped: {}", slot, e);
                return false;
            }
        };
        // T-102/finding 6: mutate under the lock, THEN persist after
        // releasing it — unified ordering with every other writer, guarded
        // against a newer mutation racing the persist (`persist_current_slot`
        // re-reads whatever is CURRENT rather than trusting this call's own
        // snapshot).
        match SLOTS.lock() {
            Ok(mut state) => match cas_commit(&mut state, slot, expected, target.clone()) {
                Some(_) => {}
                None => {
                    debug!(
                        "Automatic capture for slot {} skipped — a newer capture won the race",
                        slot
                    );
                    return false;
                }
            },
            Err(_) => return false,
        };
        emit_changed(app);
        persist_current_slot(app, slot);
        true
    }

    /// Automatic/deferred clear: clears ONLY if the slot's generation is
    /// still exactly `expected`, mirroring [`set_slot_if_unchanged`] — a
    /// deferred on-finish Clear must not wipe out a fresher manual capture
    /// that landed while the take was still in flight. Also the
    /// generation-guarded cleanup path used by `jump`/`begin_delivery` when a
    /// stale (window-gone) target is discovered (finding 5).
    pub fn clear_if_unchanged(app: &AppHandle, slot: usize, expected: u64) -> bool {
        if slot >= SLOT_COUNT {
            return false;
        }
        match SLOTS.lock() {
            Ok(mut state) => {
                if state.generations[slot] != expected {
                    debug!(
                        "Automatic clear for slot {} skipped — a newer capture won the race",
                        slot
                    );
                    return false;
                }
                let g = next_generation();
                state.targets[slot] = None;
                state.generations[slot] = g;
            }
            Err(_) => return false,
        };
        persist_current_slot(app, slot);
        emit_changed(app);
        true
    }

    /// Revalidate the stored identity — HWND values get recycled by Windows, so
    /// the handle alone is not proof it's still the same window (or control).
    fn validate(t: &Target) -> Result<(), (String, bool)> {
        unsafe {
            let hwnd = HWND(t.hwnd as _);
            if !IsWindow(Some(hwnd)).as_bool() {
                return Err(("target window no longer exists".into(), true));
            }
            let mut pid = 0u32;
            let tid = GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid != t.pid || tid != t.tid {
                return Err(("target window was replaced by another".into(), true));
            }
            // The control HWND is even more recyclable than the top-level
            // window (child controls are torn down freely). Require the same
            // owning PROCESS and the same class; a recycled handle with a
            // different class must not receive keystrokes. Deliberately NOT
            // the same thread: multi-threaded UIs (Citrix CtxICADisp, Windows
            // Terminal's XAML input site, Chromium) legitimately host input
            // controls on a different thread than their top-level window.
            let control = HWND(t.control as _);
            if !IsWindow(Some(control)).as_bool() {
                return Err(("target field no longer exists".into(), false));
            }
            let mut cpid = 0u32;
            let ctid = GetWindowThreadProcessId(control, Some(&mut cpid));
            if cpid != t.pid || ctid == 0 {
                return Err(("target field was replaced by another".into(), false));
            }
            if class_name(control) != t.control_class {
                return Err(("target field changed type".into(), false));
            }
            // Re-check the password style — a live Edit control can gain
            // ES_PASSWORD after capture (e.g. a login form re-arming).
            if t.control_class.eq_ignore_ascii_case("Edit") {
                let style = GetWindowLongW(control, GWL_STYLE);
                if style & ES_PASSWORD != 0 {
                    return Err(("target field became a password field".into(), false));
                }
            }
            Ok(())
        }
    }

    /// Activate the target window and verify it actually became foreground —
    /// Windows may refuse `SetForegroundWindow` and flash the taskbar instead.
    /// Escalation ladder for stubborn targets (Citrix/RDP session windows,
    /// fullscreen apps): plain SFW → attach to the current foreground thread's
    /// input queue and retry → `SwitchToThisWindow` (the taskbar's own path).
    fn activate_verified(hwnd: HWND) -> Result<(), String> {
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }

            let verify = |attempts: u32| -> bool {
                for _ in 0..attempts {
                    if GetForegroundWindow() == hwnd {
                        return true;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                false
            };

            // 1) Plain request.
            let _ = SetForegroundWindow(hwnd);
            if verify(8) {
                return Ok(());
            }

            // 2) Foreground-lock workaround: temporarily join the current
            // foreground thread's input queue, which grants us its right to
            // change the foreground window.
            let fg = GetForegroundWindow();
            if !fg.0.is_null() && fg != hwnd {
                let fg_tid = GetWindowThreadProcessId(fg, None);
                let me = GetCurrentThreadId();
                let attached = fg_tid != 0 && AttachThreadInput(me, fg_tid, true).as_bool();
                let _ = BringWindowToTop(hwnd);
                let _ = SetForegroundWindow(hwnd);
                if attached {
                    let _ = AttachThreadInput(me, fg_tid, false);
                }
                if verify(8) {
                    return Ok(());
                }
            }

            // 3) Last resort: the ALT+TAB switcher's own code path.
            SwitchToThisWindow(hwnd, true);
            if verify(12) {
                return Ok(());
            }

            Err("Windows refused to bring the target window to the foreground".into())
        }
    }

    /// Focus the target control and verify via GetGUIThreadInfo. Attaches to
    /// the CONTROL's own thread — it can differ from the top-level window's
    /// thread (Citrix CtxICADisp, Windows Terminal's XAML input site).
    ///
    /// Acceptance ladder: exact focus on the stored control; else focus on
    /// any control INSIDE the target window; else the target window simply
    /// being foreground (remote-desktop canvases manage their own inner focus
    /// and never report Win32 focus the way native apps do — the wrong-WINDOW
    /// case stays impossible because activation was verified first).
    fn focus_control_verified(root: HWND, control: HWND) -> Result<(), String> {
        unsafe {
            let ctid = GetWindowThreadProcessId(control, None);
            if ctid == 0 {
                return Err("target control's thread is gone".into());
            }
            let me = GetCurrentThreadId();
            let attached = AttachThreadInput(me, ctid, true).as_bool();
            let _ = SetFocus(Some(control));
            if attached {
                let _ = AttachThreadInput(me, ctid, false);
            }
            let mut gti = GUITHREADINFO {
                cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            if GetGUIThreadInfo(ctid, &mut gti).is_ok() {
                if gti.hwndFocus == control {
                    return Ok(());
                }
                if !gti.hwndFocus.0.is_null() && GetAncestor(gti.hwndFocus, GA_ROOT) == root {
                    debug!("focus landed on a sibling control inside the target window; accepting");
                    return Ok(());
                }
            }
            if GetForegroundWindow() == root {
                debug!("accepting window-level focus (remote canvas / opaque focus)");
                return Ok(());
            }
            Err("target control did not accept focus".into())
        }
    }

    pub fn jump(app: &AppHandle, slot: usize) -> Result<(), String> {
        // Lazy restore: a persisted slot whose app appeared after startup
        // resolves on first use ("recovers when the proper app is started").
        // Finding 5: the emptiness check and the generation snapshot handed
        // to `resolve_saved` come from the SAME lock acquisition — see
        // `empty_slot_generation`.
        if let Some(expected) = empty_slot_generation(slot) {
            if resolve_saved(app, slot, expected) {
                emit_changed(app);
            }
        }
        // Fetch target + generation atomically (finding 5): the stale-target
        // cleanup below must guard against a manual recapture landing during
        // `validate()`, so it needs the EXACT generation `target` was read
        // under.
        let (target, expected) =
            get_slot_with_generation(slot).ok_or_else(|| format!("jump slot {slot} is not set"))?;
        match validate(&target) {
            Ok(()) => {}
            Err((reason, window_gone)) => {
                if window_gone && !clear_if_unchanged(app, slot, expected) {
                    debug!(
                        "Stale-target cleanup for slot {} skipped — a newer capture won the race",
                        slot
                    );
                }
                return Err(reason);
            }
        }
        activate_verified(HWND(target.hwnd as _))?;
        // Best-effort focus — jump is navigation, not delivery, so a focus
        // miss is not fatal.
        let _ = focus_control_verified(HWND(target.hwnd as _), HWND(target.control as _));
        debug!("Jumped to slot {}: {}", slot, target.app);
        Ok(())
    }

    pub fn begin_delivery(app: &AppHandle, slot: usize) -> BeginDelivery {
        // Lazy restore of a persisted identity before giving up. Finding 5:
        // same atomic empty-check + generation-snapshot as `jump`.
        if let Some(expected) = empty_slot_generation(slot) {
            if resolve_saved(app, slot, expected) {
                emit_changed(app);
            }
        }
        // Same atomic target+generation fetch as `jump` (finding 5).
        let (target, expected) = match get_slot_with_generation(slot) {
            Some(t) => t,
            None => {
                return BeginDelivery::Failed {
                    reason: format!("jump slot {slot} is not set"),
                };
            }
        };
        if let Err((reason, window_gone)) = validate(&target) {
            if window_gone && !clear_if_unchanged(app, slot, expected) {
                debug!(
                    "Stale-target cleanup for slot {} skipped — a newer capture won the race",
                    slot
                );
            }
            return BeginDelivery::Failed { reason };
        }
        // The return-focus target is a FOCUS-ONLY restore (alt-tab semantics):
        // no keystrokes or text ever go to it, so the delivery-capture
        // refusals (password controls, Handy's own windows) deliberately do
        // not apply. Its own guards live in finish_delivery: only restored if
        // the window is still alive and the user didn't switch away.
        let prev = unsafe { GetForegroundWindow().0 as isize };
        if let Err(reason) = activate_verified(HWND(target.hwnd as _)) {
            return BeginDelivery::Failed { reason };
        }
        if let Err(reason) =
            focus_control_verified(HWND(target.hwnd as _), HWND(target.control as _))
        {
            // Don't strand the user in the target app after a failed delivery —
            // give the foreground back to where they were.
            unsafe {
                if prev != 0 && IsWindow(Some(HWND(prev as _))).as_bool() {
                    let _ = SetForegroundWindow(HWND(prev as _));
                }
            }
            return BeginDelivery::Failed { reason };
        }
        // Small settle so the target app processes the focus change before
        // the paste keystroke arrives.
        std::thread::sleep(std::time::Duration::from_millis(60));
        BeginDelivery::Ready(DeliveryGuard {
            prev_foreground: prev,
            target_hwnd: target.hwnd,
            target_control: target.control,
            slot,
        })
    }

    /// TOCTOU close (T-103), extended for finding 2 (adversarial review): the
    /// original check only verified the TOP-LEVEL window stayed foreground —
    /// focus could still move to a DIFFERENT control inside that same
    /// window (including a password field) between `begin_delivery` and the
    /// paste keystroke, and this check would never notice. Now re-verifies,
    /// via `GetGUIThreadInfo` on the control's own thread, that focus is
    /// STILL the captured control — and re-checks `ES_PASSWORD` on it as
    /// defense in depth (a field can flip to password style after capture,
    /// e.g. a login form re-arming). Call this immediately before EVERY
    /// synthesized keystroke while a delivery is active; on a mismatch the
    /// caller must abort and park instead of pasting. Cheap syscalls only —
    /// no measurable latency added to a normal delivery.
    pub fn guard_still_foreground(guard: &DeliveryGuard) -> bool {
        unsafe {
            if GetForegroundWindow().0 as isize != guard.target_hwnd {
                return false;
            }
            let control = HWND(guard.target_control as _);
            if !IsWindow(Some(control)).as_bool() {
                return false;
            }
            // Finding 2 (second adversarial re-verify): fail CLOSED, not open.
            // The previous version treated "can't verify" (control's thread
            // gone, or `GetGUIThreadInfo` itself erroring) as "still
            // foreground" — the opposite of every other check in this
            // pipeline, all of which fail closed → park. A verification
            // failure must never be read as a pass.
            let ctid = GetWindowThreadProcessId(control, None);
            if ctid == 0 {
                return false;
            }
            let mut gti = GUITHREADINFO {
                cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            if GetGUIThreadInfo(ctid, &mut gti).is_err() {
                return false;
            }
            // The ONE deliberate exception: accept a null `hwndFocus` when
            // `GetGUIThreadInfo` SUCCEEDED (we have real data, not an error)
            // and the top-level window is still foreground. This is safe
            // specifically because it only runs after we've already
            // confirmed (above) both that the top-level window is still
            // foreground AND that the captured control HWND is still alive —
            // a genuine focus-stealing popup would have changed the
            // foreground window (caught by the first check) and a refocus
            // onto a sibling/password control would report a non-null
            // `hwndFocus` that mismatches `control` (caught by `focus_ok`
            // below). The only thing this fallback actually covers is
            // remote-desktop/virtualized canvases (Citrix, RDP) that manage
            // their own inner focus and never surface Win32 focus for it —
            // the same rationale as `focus_control_verified`'s ladder.
            let focus_ok = gti.hwndFocus == control
                || (gti.hwndFocus.0.is_null()
                    && GetForegroundWindow() == HWND(guard.target_hwnd as _));
            if !focus_ok {
                return false;
            }
            // Defense in depth: re-check ES_PASSWORD even though the control
            // handle/class still matches — a live Edit control can gain the
            // style after capture.
            if class_name(control).eq_ignore_ascii_case("Edit") {
                let style = GetWindowLongW(control, GWL_STYLE);
                if style & ES_PASSWORD != 0 {
                    return false;
                }
            }
            true
        }
    }

    /// Delivery epilogue. Anchors are ALWAYS kept after a delivery (0.40
    /// rework — a set anchor stays set until the user changes it or its
    /// window dies; the old keep/one-shot options are gone). `return_focus`
    /// is the finishing FLOW's setting, decided by the caller; the location
    /// returned to is `guard.prev_foreground`, auto-captured at
    /// `begin_delivery` — the invisible start-location slot.
    pub fn finish_delivery(
        app: &AppHandle,
        guard: DeliveryGuard,
        delivered_ok: bool,
        return_focus: bool,
    ) {
        if delivered_ok {
            let _ = app.emit("anchor-delivered", guard.slot as u32);
        }
        if return_focus {
            unsafe {
                let current = GetForegroundWindow().0 as isize;
                // Restore only if the target is still foreground (the user
                // didn't switch away mid-delivery) and the previous window is
                // still alive — never yank focus from where the user went.
                if current == guard.target_hwnd
                    && guard.prev_foreground != 0
                    && guard.prev_foreground != guard.target_hwnd
                    && IsWindow(Some(HWND(guard.prev_foreground as _))).as_bool()
                {
                    let _ = SetForegroundWindow(HWND(guard.prev_foreground as _));
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// A dummy target — no OS calls, so these tests exercise
        /// `cas_commit`'s pure compare-and-swap logic without a live window
        /// or AppHandle (T-102). Generation lives OUTSIDE `Target` now (see
        /// the `SlotState` doc / finding 4), so this carries no generation.
        fn dummy_target() -> Target {
            Target {
                hwnd: 1,
                control: 1,
                pid: 1,
                tid: 1,
                window_class: "TestWindow".into(),
                control_class: "TestControl".into(),
                app: "test".into(),
            }
        }

        fn empty_state() -> SlotState {
            SlotState {
                targets: [None, None, None, None, None],
                generations: [0; SLOT_COUNT],
            }
        }

        #[test]
        fn cas_commit_succeeds_when_generation_matches_expected() {
            let mut state = empty_state();
            state.targets[HOT] = Some(dummy_target());
            state.generations[HOT] = 5;
            let result = cas_commit(&mut state, HOT, 5, dummy_target());
            assert!(result.is_some());
            // Committing always allocates a FRESH generation — never reuses
            // the expected one, so a subsequent stale writer can't match it.
            assert_ne!(state.generations[HOT], 5);
            assert_eq!(state.generations[HOT], result.unwrap());
        }

        #[test]
        fn cas_commit_rejects_stale_expected_generation() {
            let mut state = empty_state();
            // A manual Set landed (generation 9) after the automatic
            // writer's snapshot (generation 5, taken earlier).
            state.targets[HOT] = Some(dummy_target());
            state.generations[HOT] = 9;
            assert!(cas_commit(&mut state, HOT, 5, dummy_target()).is_none());
            // The fresher manual capture must survive untouched.
            assert_eq!(state.generations[HOT], 9);
        }

        #[test]
        fn cas_commit_rejects_when_slot_was_cleared_since_snapshot() {
            let mut state = empty_state();
            // Snapshot was taken while occupied at generation 3; a manual
            // Clear happened since. The clear ITSELF bumps the generation
            // (T-102 fix — see the ABA test below), so it no longer reads
            // back as 3.
            state.generations[HOT] = 4;
            assert!(cas_commit(&mut state, HOT, 3, dummy_target()).is_none());
            assert!(state.targets[HOT].is_none());
        }

        #[test]
        fn cas_commit_succeeds_populating_a_still_empty_slot() {
            let mut state = empty_state();
            assert!(cas_commit(&mut state, HOT, 0, dummy_target()).is_some());
            assert!(state.targets[HOT].is_some());
        }

        #[test]
        fn cas_commit_generations_are_monotonic_across_commits() {
            let mut state = empty_state();
            let first_gen = cas_commit(&mut state, HOT, 0, dummy_target()).unwrap();
            let second_gen = cas_commit(&mut state, HOT, first_gen, dummy_target()).unwrap();
            assert!(second_gen > first_gen);
        }

        /// T-102 ABA regression (adversarial-review finding 4). Before this
        /// fix, the generation lived INSIDE `Option<Target>`, so clearing a
        /// slot to `None` erased it — a deferred writer that snapshotted
        /// `expected` while a slot was empty, then observed an ENTIRE
        /// None→Set→Clear cycle happen without it, would read the slot back
        /// as `None` again (the SAME state it snapshotted) and wrongly
        /// conclude nothing had changed, resurrecting a stale capture. With
        /// generations in their own array bumped on every write (including
        /// Clear), the cycle is visible: the stale snapshot can never match
        /// again.
        #[test]
        fn aba_cycle_through_set_and_clear_invalidates_a_stale_none_snapshot() {
            let mut state = empty_state();
            // The deferred writer's snapshot: slot HOT is empty, generation 0.
            let stale_expected = state.generations[HOT];
            assert_eq!(stale_expected, 0);

            // Meanwhile, entirely without the deferred writer's knowledge:
            // a manual Set...
            let set_gen = cas_commit(&mut state, HOT, 0, dummy_target()).unwrap();
            assert!(state.targets[HOT].is_some());

            // ...then a manual Clear. This bumps the generation even though
            // the slot goes back to logically empty — the crux of the fix.
            let clear_gen = next_generation();
            state.generations[HOT] = clear_gen;
            state.targets[HOT] = None;
            assert_ne!(clear_gen, stale_expected);
            assert_ne!(clear_gen, set_gen);

            // The slot is empty again — the SAME observable state as the
            // writer's original snapshot — but the generation has moved on.
            // The stale writer's commit must be rejected, never resurrecting
            // the slot.
            assert!(cas_commit(&mut state, HOT, stale_expected, dummy_target()).is_none());
            assert!(state.targets[HOT].is_none());
        }

        #[test]
        fn empty_generation_returns_none_when_slot_occupied() {
            let mut state = empty_state();
            state.targets[HOT] = Some(dummy_target());
            state.generations[HOT] = 7;
            assert_eq!(empty_generation(&state, HOT), None);
        }

        #[test]
        fn empty_generation_snapshots_the_generation_of_a_truly_empty_slot() {
            let mut state = empty_state();
            state.generations[HOT] = 3; // e.g. previously occupied then cleared
            assert_eq!(empty_generation(&state, HOT), Some(3));
        }

        /// Finding 5 regression (adversarial re-verify): `resolve_saved`'s CAS
        /// must be handed the generation the CALLER captured atomically with
        /// its own "is this slot empty" check — never a LATER, separately
        /// re-read snapshot. This models the exact race: the caller observes
        /// the slot empty (generation 0), but before `resolve_saved`'s slow
        /// `EnumWindows` scan even starts, a manual Set lands. If
        /// `resolve_saved` had taken its OWN snapshot at that later point, it
        /// would capture the manual Set's fresh generation as "expected" —
        /// and its CAS commit (using the stale settings-derived target) would
        /// then incorrectly SUCCEED once the scan finishes, clobbering the
        /// manual Set. Using the ORIGINAL (pre-Set) snapshot instead, exactly
        /// as `empty_slot_generation` captures it, correctly rejects the CAS.
        #[test]
        fn stale_expected_from_before_a_manual_set_is_rejected_by_cas_commit() {
            let mut state = empty_state();
            // The caller observes the slot empty and snapshots its generation
            // atomically (what `empty_slot_generation` would return here).
            let expected = empty_generation(&state, HOT).unwrap();
            assert_eq!(expected, 0);

            // A manual Set lands in the gap before the slow scan completes.
            cas_commit(&mut state, HOT, 0, dummy_target()).unwrap();
            assert!(state.targets[HOT].is_some());

            // `resolve_saved`'s CAS, using the ORIGINAL snapshot (not a fresh
            // re-read), must be rejected — never overwriting the manual Set.
            assert!(cas_commit(&mut state, HOT, expected, dummy_target()).is_none());
            assert!(state.targets[HOT].is_some());
        }
    }
}

// ---------------------------------------------------------------------------
// Cross-platform surface
// ---------------------------------------------------------------------------

/// One-shot opt-in for anchored delivery: the slot index whose target the next
/// paste should deliver into, or -1 for none. Pastes NEVER involve the Jumper
/// unless the pressed flow's configured action armed this for the current take.
///
/// This is the ARMING side only. Consuming it into a take-scoped
/// [`DeliveryIntent`] (see `take_delivery_intent`) happens once per take, at
/// stop()/take time — never lazily at paste time, which used to leave a
/// window for a pathologically delayed paste to observe a NEWER take's
/// request (T-101).
static DELIVERY_SLOT: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(-1);

pub fn request_delivery(slot: usize) {
    if slot < SLOT_COUNT {
        DELIVERY_SLOT.store(slot as isize, std::sync::atomic::Ordering::SeqCst);
    }
}

pub fn clear_delivery_request() {
    DELIVERY_SLOT.store(-1, std::sync::atomic::Ordering::SeqCst);
}

/// A take's delivery intent: the slot (if any) its single paste should
/// deliver into. Captured ONCE per take via `take_delivery_intent()` — while
/// the coordinator thread still serializes everything, so no other take can
/// be starting concurrently — and threaded BY VALUE through the take's async
/// pipeline into `clipboard::paste`/`paste_inner`, mirroring how
/// `POST_TAKE_ACTION` is taken at stop() time. `paste_plain` (MCP/CLI) is
/// constructed with `DeliveryIntent::NONE` and never touches the global.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeliveryIntent(Option<usize>);

impl DeliveryIntent {
    /// No anchored delivery for this take's paste.
    pub const NONE: DeliveryIntent = DeliveryIntent(None);
}

/// Consume the process-global delivery request into an owned, take-scoped
/// intent. Call ONCE per take, synchronously, at the same point
/// `take_post_take_action()` is called (stop(), before the async task
/// spawns) — after this, the global is armed only by a LATER take, which can
/// never affect an intent already captured by value here.
pub fn take_delivery_intent() -> DeliveryIntent {
    let slot = DELIVERY_SLOT.swap(-1, std::sync::atomic::Ordering::SeqCst);
    DeliveryIntent(if slot >= 0 { Some(slot as usize) } else { None })
}

/// Deferred "on finish" side-action (Set/Clear): armed at the finishing
/// press, executed only after the take's paste fully completed (keystrokes
/// sent, focus returned) — "finished" means finished, not "stop was pressed".
/// Jump-on-finish is NOT deferred this way; it arms `DELIVERY_SLOT` because
/// delivery IS the paste itself.
#[derive(Clone, Copy)]
pub enum PostTakeAction {
    Set,
    Clear,
}

/// Armed action + target slot + the slot's capture GENERATION at arm time
/// (T-102). The action is deferred across the whole transcription pipeline —
/// seconds, potentially minutes for a long chunked recording — so a manual
/// Set/Clear of the same slot can easily land before this finally runs.
/// Comparing generations at run time lets the fresher manual action win
/// instead of being silently clobbered by the stale deferred one.
static POST_TAKE_ACTION: std::sync::Mutex<Option<(PostTakeAction, usize, Option<u64>)>> =
    std::sync::Mutex::new(None);

pub fn arm_post_take_action(action: PostTakeAction, slot: usize) {
    if slot < SLOT_COUNT {
        let expected = slot_generation(slot);
        if let Ok(mut guard) = POST_TAKE_ACTION.lock() {
            *guard = Some((action, slot, expected));
        }
    }
}

pub fn take_post_take_action() -> Option<(PostTakeAction, usize, Option<u64>)> {
    POST_TAKE_ACTION.lock().ok().and_then(|mut g| g.take())
}

pub fn clear_post_take_action() {
    if let Ok(mut guard) = POST_TAKE_ACTION.lock() {
        *guard = None;
    }
}

/// Execute a take's deferred on-finish action (after its paste completed).
/// Both variants are generation-guarded (compare-and-set against the
/// snapshot taken at arm time): if the slot was touched by anything else in
/// the meantime, this deferred write is skipped rather than clobbering it.
pub fn run_post_take_action(app: &AppHandle, action: Option<(PostTakeAction, usize, Option<u64>)>) {
    match action {
        Some((PostTakeAction::Set, slot, expected)) => {
            if !set_slot_if_unchanged(app, slot, expected) {
                log::debug!(
                    "on-finish set for slot {} skipped — a newer capture won the race",
                    slot
                );
            }
        }
        Some((PostTakeAction::Clear, slot, expected)) => {
            if !clear_if_unchanged(app, slot, expected) {
                log::debug!(
                    "on-finish clear for slot {} skipped — a newer capture won the race",
                    slot
                );
            }
        }
        None => {}
    }
}

/// Snapshot a slot's current capture generation (`None` only when the Jumper
/// is unavailable on this platform — a valid Windows slot ALWAYS has a
/// generation, `0` meaning "never written", regardless of occupancy; see
/// `SlotState` in `mod win` / the T-102 ABA fix). Used by callers planning a
/// delayed automatic write — see the module doc and T-102.
pub fn slot_generation(slot: usize) -> Option<u64> {
    #[cfg(windows)]
    {
        Some(win::current_generation(slot))
    }
    #[cfg(not(windows))]
    {
        let _ = slot;
        None
    }
}

/// Automatic/deferred capture: commits only if `slot`'s generation is still
/// exactly `expected` (see `slot_generation`). Returns `false` on a stale
/// race — the caller should treat that as a benign no-op, not an error.
/// `expected` is only ever `None` when it originated from a non-Windows
/// `slot_generation()` call (where this whole path is unreachable anyway);
/// `unwrap_or(0)` degrades to "never written" rather than panicking.
pub fn set_slot_if_unchanged(app: &AppHandle, slot: usize, expected: Option<u64>) -> bool {
    #[cfg(windows)]
    {
        win::set_slot_if_unchanged(app, slot, expected.unwrap_or(0))
    }
    #[cfg(not(windows))]
    {
        let _ = (app, slot, expected);
        false
    }
}

/// Automatic/deferred clear: clears only if `slot`'s generation is still
/// exactly `expected`. Mirrors `set_slot_if_unchanged`.
pub fn clear_if_unchanged(app: &AppHandle, slot: usize, expected: Option<u64>) -> bool {
    #[cfg(windows)]
    {
        win::clear_if_unchanged(app, slot, expected.unwrap_or(0))
    }
    #[cfg(not(windows))]
    {
        let _ = (app, slot, expected);
        false
    }
}

pub fn set_slot(app: &AppHandle, slot: usize) -> Result<AnchorStatus, String> {
    #[cfg(windows)]
    {
        win::set_slot(app, slot)
    }
    #[cfg(not(windows))]
    {
        let _ = (app, slot);
        Err("The Jumper is Windows-only in this version".into())
    }
}

/// Back-compat alias used by the hot-slot action.
pub fn set_anchor(app: &AppHandle) -> Result<AnchorStatus, String> {
    set_slot(app, HOT)
}

pub fn jump(app: &AppHandle, slot: usize) -> Result<(), String> {
    #[cfg(windows)]
    {
        win::jump(app, slot)
    }
    #[cfg(not(windows))]
    {
        let _ = (app, slot);
        Err("The Jumper is Windows-only in this version".into())
    }
}

/// Begin an anchored delivery for the given take-scoped intent (T-101). The
/// caller must have obtained `intent` via `take_delivery_intent()` at
/// stop()/take time — NOT by reading `DELIVERY_SLOT` here, which is what let
/// a delayed paste observe a newer take's request.
pub fn begin_delivery(app: &AppHandle, intent: DeliveryIntent) -> BeginDelivery {
    let Some(slot) = intent.0 else {
        return BeginDelivery::NoAnchor;
    };
    #[cfg(windows)]
    {
        win::begin_delivery(app, slot)
    }
    #[cfg(not(windows))]
    {
        let _ = (app, slot);
        BeginDelivery::NoAnchor
    }
}

/// TOCTOU re-verify (T-103): true if the delivery target is still the
/// foreground window right now. Call immediately before the paste keystroke
/// whenever a `DeliveryGuard` is present; on `false` the caller must abort
/// and park instead of pasting blind.
pub fn guard_still_foreground(guard: &DeliveryGuard) -> bool {
    #[cfg(windows)]
    {
        win::guard_still_foreground(guard)
    }
    #[cfg(not(windows))]
    {
        let _ = guard;
        true
    }
}

pub fn finish_delivery(
    app: &AppHandle,
    guard: DeliveryGuard,
    delivered_ok: bool,
    return_focus: bool,
) {
    #[cfg(windows)]
    {
        win::finish_delivery(app, guard, delivered_ok, return_focus);
    }
    #[cfg(not(windows))]
    {
        let _ = (app, guard, delivered_ok, return_focus);
    }
}

/// Clear a slot (cross-platform surface for actions/commands).
pub fn clear(app: &AppHandle, slot: usize) {
    #[cfg(windows)]
    {
        win::clear(app, slot);
    }
    #[cfg(not(windows))]
    {
        let _ = (app, slot);
    }
}

/// All slot statuses, index = slot (0 = hot), for the Jumper UI.
#[tauri::command]
#[specta::specta]
pub fn get_jump_slots(app: AppHandle) -> Vec<Option<AnchorStatus>> {
    #[cfg(windows)]
    {
        win::statuses(&app)
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        vec![None; SLOT_COUNT]
    }
}

/// Hot-slot status (legacy surface kept for the settings chip).
#[tauri::command]
#[specta::specta]
pub fn get_anchor_status(app: AppHandle) -> Option<AnchorStatus> {
    get_jump_slots(app).into_iter().next().flatten()
}

/// Restore persisted jump slots at startup (no-op unless `jumper_persist`).
pub fn restore_persisted_slots(app: &AppHandle) {
    #[cfg(windows)]
    {
        win::restore_persisted_slots(app);
    }
    #[cfg(not(windows))]
    {
        let _ = app;
    }
}

/// Snapshot live slots into the persisted identities (persistence toggle ON).
#[cfg(windows)]
pub fn snapshot_slots(app: &AppHandle) {
    win::snapshot_all(app);
}

/// Explicit clear from the UI.
#[tauri::command]
#[specta::specta]
pub fn clear_jump_slot(app: AppHandle, slot: u32) {
    clear(&app, slot as usize);
}

/// Test/jump a slot from the UI (same navigation as the jump hotkey).
#[tauri::command]
#[specta::specta]
pub fn jump_to_slot(app: AppHandle, slot: u32) -> Result<(), String> {
    jump(&app, slot as usize)
}

/// Legacy hot-slot clear (kept for existing UI wiring).
#[tauri::command]
#[specta::specta]
pub fn clear_anchor(app: AppHandle) {
    clear(&app, HOT);
}

#[cfg(test)]
mod cross_platform_tests {
    use super::*;

    /// T-101: `take_delivery_intent()` is a strict one-shot — a second take
    /// must never observe a prior take's (already-consumed) request; an
    /// out-of-range `request_delivery` call is dropped; and a value captured
    /// BEFORE a later request existed stays `NONE` (it's an owned Copy,
    /// never re-reads the global). Combined into one test — `DELIVERY_SLOT`
    /// is a shared global and cargo runs tests in parallel by default, so a
    /// second test touching the same static could otherwise interleave.
    #[test]
    fn delivery_intent_is_take_scoped_one_shot() {
        clear_delivery_request();
        assert_eq!(take_delivery_intent(), DeliveryIntent::NONE);

        request_delivery(2);
        let intent = take_delivery_intent();
        assert_eq!(intent, DeliveryIntent(Some(2)));

        // Consumed: a second (later) take's capture must see NONE, never the
        // prior take's slot.
        assert_eq!(take_delivery_intent(), DeliveryIntent::NONE);

        // Out-of-range requests are dropped by request_delivery itself.
        request_delivery(SLOT_COUNT + 1);
        assert_eq!(take_delivery_intent(), DeliveryIntent::NONE);

        // A value captured EARLIER is unaffected by a request armed AFTER it
        // was taken — it never reads the global again.
        let earlier_take_intent = take_delivery_intent();
        request_delivery(1);
        let later_take_intent = take_delivery_intent();
        assert_eq!(later_take_intent, DeliveryIntent(Some(1)));
        assert_eq!(earlier_take_intent, DeliveryIntent::NONE);
    }

    /// T-102: arming a post-take action snapshots the slot's generation at
    /// arm time and hands it back unchanged through the one-shot take; an
    /// out-of-range slot is refused at arm time. Combined into one test for
    /// the same shared-global-under-parallel-tests reason as above.
    #[test]
    fn post_take_action_roundtrip_and_bounds_check() {
        clear_post_take_action();
        assert!(take_post_take_action().is_none());

        arm_post_take_action(PostTakeAction::Clear, HOT);
        let action = take_post_take_action();
        match action {
            Some((PostTakeAction::Clear, slot, _expected)) => assert_eq!(slot, HOT),
            _ => panic!("expected an armed Clear action for HOT"),
        }
        // One-shot: taking it again yields nothing.
        assert!(take_post_take_action().is_none());

        // Out-of-range slot is refused at arm time, never silently stored.
        arm_post_take_action(PostTakeAction::Set, SLOT_COUNT + 3);
        assert!(take_post_take_action().is_none());
    }
}
