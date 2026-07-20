//! Jumper (Windows-only v1): five jump slots for desktop text fields.
//!
//! Slot 0 is the HOT slot — the original "anchor": transcription flows can
//! set/clear/jump/deliver-to it via per-flow event actions. Any slot can be
//! the target of track-last-output — a per-flow, independent switch + slot
//! (`jumper_track_output_enabled`/`_slot` for dictate,
//! `jumper_track_submit_enabled`/`_slot` for Transcribe & Submit) — which
//! auto-captures where that flow last pasted. NO slot is ever auto-cleared by a
//! delivery (0.40 rework) — slots live until overwritten, cleared, or their
//! window dies. Slots 1–4 are STATIC bookmarks: set via `jump_set_slot_N`,
//! jumped via `jump_slot_N`. All slots share the same
//! capture/validation/delivery machinery and safety rails:
//!
//! - capture via `GetGUIThreadInfo` (no thread-input attachment at capture),
//!   durable identity (HWND + PID + TID + class, revalidated at delivery —
//!   bare HWNDs get recycled), password/self-window rejection — the
//!   password check is TWO layers (T-105): `ES_PASSWORD` (classic Win32
//!   `Edit` controls) plus a UI Automation `IsPassword` property query
//!   (`uia_is_password`) for non-Win32 password fields (Electron/browser/
//!   WinUI) invisible to the style bit — resolved via
//!   `IUIAutomation::GetFocusedElement()`, NOT `ElementFromHandle(control)`
//!   (a v0.42.0 adversarial-review finding: `ElementFromHandle` returns the
//!   renderer HOST element in Chrome/Edge/Electron, never the focused
//!   descendant that actually carries `IsPassword`, so it silently missed
//!   exactly the browser/Electron password fields this check exists for;
//!   `GetFocusedElement()` is process/desktop-wide and correctly resolves to
//!   the true focused control even when it has no HWND of its own — guarded
//!   by a same-process check against the target's PID so a validate()-time
//!   call, where the target window isn't the active one yet, never queries
//!   an unrelated element that merely happens to be focused elsewhere); a
//!   UIA query failure fails OPEN (never blocks capture on its own) but a
//!   definite positive result fails CLOSED exactly like `ES_PASSWORD`, at
//!   both capture and delivery revalidation (`validate()`) — deliberately
//!   NOT the per-keystroke `guard_still_foreground` TOCTOU recheck, which
//!   stays cheap-syscalls-only;
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
//! Track-last-output's capture point (T-104): when the paste that just
//! finished was an ANCHORED delivery, the capture is sourced from the
//! delivery guard's own already-verified hwnd/control
//! (`track_from_guard`/`win::capture_target_from_guard`), never a fresh
//! foreground-window query — a submit keystroke (Enter closing a dialog,
//! navigating a composer) can move the foreground window away from the real
//! delivery target before a plain `GetForegroundWindow()`-based capture would
//! run. Non-anchored (plain) pastes still use the foreground-query capture
//! (`set_slot`) — there is no known delivery target to fall back to for them.
//! Hardened further (v0.42.0 adversarial review, finding 2): the identity
//! `capture_target_from_guard` commits is the FULLY-VERIFIED
//! pid/tid/window_class/control_class `begin_delivery` already captured at
//! `DeliveryGuard` construction time, preserved verbatim — never re-derived
//! from the guard's raw HWNDs at capture time, which by then may have been
//! destroyed and recycled by Windows for an unrelated window. If the
//! captured control is no longer a live window, the capture is refused
//! outright (slot left unchanged) rather than "broadening" to the top-level
//! window's identity, which would silently bookmark the wrong element.
//!
//! Live slots are in-memory (window handles die with their windows); the
//! opt-in `jumper_persist` setting additionally saves each slot's IDENTITY
//! (app + window/control class) and re-resolves it against live windows at
//! startup and lazily on use — see `restore_slots`/`snapshot_slots`.
//! Same-app disambiguation (T-112): that identity alone can match MORE THAN
//! ONE live window (two Chrome windows, two Citrix sessions) — `resolve_saved`
//! collects every visible match and tries a persisted per-slot title-HASH
//! hint (`jumper_slot_hints.json`, a tiny sidecar written alongside
//! `jumper_saved_slots`; hashed rather than stored verbatim, since a window
//! title can carry a sensitive document/page name) to pick the one unique
//! candidate. When that still doesn't resolve to exactly one, the automatic
//! paths (startup restore, `begin_delivery`) refuse to guess and leave the
//! slot unresolved (stays "stale"/red) rather than silently committing to a
//! possibly-wrong window; only the explicit `jump` path falls back to the
//! topmost (Z-order) candidate, since the user is about to see where they
//! land.
//!
//! Hardened further (v0.42.0 adversarial review, finding 3): the hint
//! sidecar and `jumper_saved_slots` are two SEPARATE stores, so a hint write
//! that fails AFTER the identity already persisted (or vice versa) used to
//! risk a restart resolving against an unrelated leftover hint — exactly the
//! wrong-window bug this ticket exists to close. `persist_slot` now writes
//! the hint FIRST (rename-from-temp, so a crash mid-write can never leave a
//! half-written file) and only persists the identity once that succeeded;
//! every hint also carries a keyed fingerprint of the identity it was saved
//! with, so even a torn write across the two files (a hard crash between
//! them) is detected and ignored at resolve time rather than silently
//! mismatched (`hint_is_valid_for`). The title hash itself is keyed with a
//! per-install random key stored in the sidecar's own header (never a
//! fixed/public key, and never regenerated once present — that would orphan
//! every previously saved hint) and versioned, so an older, unkeyed sidecar
//! is treated as absent rather than misread.
//!
//! Every slot write bumps a per-slot capture generation (T-102), tracked in a
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

/// Number of jump slots (index 0 = Hot 1, 1–9 = static, 10 = Hot 2). T-305 grew
/// the static range from 4 to 9 and moved Hot 2 to the top of the range (was 5)
/// so `static N == slot index N` holds with no display/index decoupling.
pub const SLOT_COUNT: usize = 11;
/// The hot slot index (Hot 1).
pub const HOT: usize = 0;
/// The second hot slot index (Hot 2, T-303; relocated from 5 to 10 in T-305).
pub const HOT2: usize = 10;

/// T-302 per-slot cursor gating — the SINGLE source of truth. Given the
/// `jumper_save_cursor_slots` flags (index = slot; 0 = hot, 1–4 = static),
/// whether slot `slot`'s (unconditionally captured) cursor should be KEPT.
/// Bounds-checked: an out-of-range slot or a short/empty vec reads as `false`
/// (never panics, never assumes default-on). EVERY capture entry resolves the
/// gate through this against its TARGET slot index — the manual/hot `set_slot`
/// (its own `slot`), and the clipboard track-last-output / coordinator
/// on-finish paths (their tracked target slot) — replacing the removed per-flow
/// output/submit toggles.
///
/// Its only non-test callers are the Windows-only capture paths, so it is
/// dead on other targets — the cross-platform unit test still exercises it.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn slot_save_cursor(slots: &[bool], slot: usize) -> bool {
    slots.get(slot).copied().unwrap_or(false)
}

/// T-304 per-slot cursor MODE resolution — the single source of truth. Given
/// `jumper_cursor_mode_slots` (index = slot; 0 = Hot 1, 1–4 = static, 5 = Hot 2),
/// the `CursorMode` to stamp on slot `slot`'s captured cursor. Bounds-checked:
/// an out-of-range slot or a short/empty vec reads as `CursorMode::default()`
/// (AppRelative) — never panics. Mirrors `slot_save_cursor`; every capture entry
/// resolves the mode through this against its TARGET slot.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn slot_cursor_mode(
    modes: &[crate::settings::CursorMode],
    slot: usize,
) -> crate::settings::CursorMode {
    modes.get(slot).copied().unwrap_or_default()
}

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
    /// Full identity `begin_delivery` already validated for this delivery,
    /// snapshotted at `DeliveryGuard` construction time (T-104 finding 2,
    /// v0.42.0 adversarial review). `capture_target_from_guard` (the
    /// track-last-output source for an anchored delivery) MUST use these
    /// fields verbatim rather than re-deriving pid/tid/class from
    /// `target_hwnd`/`target_control` at capture time — by then those HWND
    /// values may have been destroyed and recycled by Windows for a wholly
    /// unrelated window, and re-querying them would silently adopt that
    /// window's identity instead of failing.
    #[cfg(windows)]
    target_pid: u32,
    #[cfg(windows)]
    target_tid: u32,
    #[cfg(windows)]
    target_window_class: String,
    #[cfg(windows)]
    target_control_class: String,
    #[cfg(windows)]
    target_app: String,
    #[cfg(windows)]
    slot: usize,
    /// T-301: the cursor position to restore after this delivery finishes,
    /// snapshotted from the committed `Target` at `begin_delivery` time (NOT
    /// restored there). Restored in `finish_delivery` as the very last
    /// focus/input op. `None` when no cursor was saved for this slot / flow.
    #[cfg(windows)]
    cursor: Option<crate::settings::SavedCursor>,
}

#[cfg(windows)]
mod win {
    use super::{AnchorStatus, BeginDelivery, DeliveryGuard, HOT, SLOT_COUNT};
    use crate::settings::{CursorMode, SavedCursor};
    use log::{debug, info, warn};
    use once_cell::sync::Lazy;
    use std::sync::Mutex;
    use tauri::{AppHandle, Emitter, Manager};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, POINT, RECT, RPC_E_CHANGED_MODE};
    // T-301: nearest-monitor clamp for cursor restore (Win32_Graphics_Gdi).
    // `ClientToScreen` also lives here in windows-rs.
    use windows::Win32::Graphics::Gdi::{
        ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    };
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED, CoCreateInstance,
        CoInitializeEx, CoUninitialize,
    };
    use windows::Win32::System::Threading::{
        AttachThreadInput, GetCurrentThreadId, OpenProcess, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, IUIAutomationElement};
    // T-301: Per-Monitor-V2 DPI-awareness prerequisite check (Win32_UI_HiDpi).
    // The EXACT-context equality check (Codex correction #3, revised) needs
    // `AreDpiAwarenessContextsEqual` + the `DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2`
    // sentinel — both live under the already-enabled `Win32_UI_HiDpi` feature,
    // so no extra Cargo feature is required.
    use windows::Win32::UI::HiDpi::{
        AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        GetThreadDpiAwarenessContext,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, EnumWindows, FindWindowExW, GA_ROOT, GUITHREADINFO, GWL_STYLE,
        GetAncestor, GetClassNameW, GetClientRect, GetCursorPos, GetForegroundWindow,
        GetGUIThreadInfo, GetWindowLongW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
        IsWindow, IsWindowVisible, SW_RESTORE, SetCursorPos, SetForegroundWindow, ShowWindow,
        SwitchToThisWindow,
    };
    use windows::core::{BOOL, HRESULT};

    /// Edit-control style: password box. Defined locally to avoid feature churn.
    const ES_PASSWORD: i32 = 0x0020;

    /// RAII guard for a possibly-newly-initialized COM apartment on the
    /// CALLING thread (T-105). Per the Win32 contract, `CoInitializeEx` must
    /// be paired with exactly one `CoUninitialize` for every SUCCESSFUL call
    /// — `S_OK` (genuinely newly initialized) AND `S_FALSE` (already
    /// initialized on this thread in the SAME mode) both count as
    /// "successful" and both need the matching uninitialize. The ONE case
    /// that must NOT be paired is `RPC_E_CHANGED_MODE`: the thread was
    /// already initialized in a DIFFERENT concurrency mode by someone else
    /// (e.g. the WebView2/Tauri UI thread's own STA init) — our call did
    /// nothing in that case, so there is nothing for us to undo, and calling
    /// `CoUninitialize` anyway would decrement a refcount we never
    /// incremented.
    struct ComApartmentGuard {
        owns: bool,
    }

    impl Drop for ComApartmentGuard {
        fn drop(&mut self) {
            if self.owns {
                unsafe { CoUninitialize() };
            }
        }
    }

    /// Outcome of a single `CoInitializeEx` attempt, decided PURELY from its
    /// returned `HRESULT` — extracted from `ensure_com_initialized` so this
    /// decision (which HRESULTs must/must-not be paired with a
    /// `CoUninitialize`) is unit-testable without a real COM call (T-105).
    #[derive(Debug, PartialEq, Eq)]
    enum ComInitOutcome {
        /// A genuinely new init, OR a repeat init that returned `S_FALSE`
        /// (already initialized on this thread in the SAME mode) — the Win32
        /// contract requires BOTH to be paired with exactly one
        /// `CoUninitialize`.
        Owned,
        /// `RPC_E_CHANGED_MODE`: already initialized in a DIFFERENT mode by
        /// someone else. COM is usable, but this call did nothing — never
        /// uninitialize.
        AlreadyInitializedElsewhere,
        /// Failed for any other reason — COM is not usable via this attempt.
        Failed,
    }

    fn classify_com_init(hr: HRESULT) -> ComInitOutcome {
        if hr.is_ok() {
            ComInitOutcome::Owned
        } else if hr == RPC_E_CHANGED_MODE {
            ComInitOutcome::AlreadyInitializedElsewhere
        } else {
            ComInitOutcome::Failed
        }
    }

    /// Ensure COM is usable on the calling thread for a UIA query, returning
    /// a guard that undoes ONLY what this call itself set up. Tries
    /// apartment-threaded first (the common case: most callers here run on a
    /// Tauri/WebView2-hosted thread that already has STA COM initialized, so
    /// this typically returns `S_FALSE` and costs nothing extra), falling
    /// back to multithreaded if that particular attempt fails for a reason
    /// OTHER than "already initialized in a different mode". Returns `None`
    /// if COM could not be made available at all on this thread — callers
    /// treat that as "UIA unavailable" (see `uia_is_password`'s fail-open
    /// contract), never as a capture-blocking error by itself.
    fn ensure_com_initialized() -> Option<ComApartmentGuard> {
        unsafe {
            match classify_com_init(CoInitializeEx(None, COINIT_APARTMENTTHREADED)) {
                ComInitOutcome::Owned => return Some(ComApartmentGuard { owns: true }),
                ComInitOutcome::AlreadyInitializedElsewhere => {
                    return Some(ComApartmentGuard { owns: false });
                }
                ComInitOutcome::Failed => {}
            }
            match classify_com_init(CoInitializeEx(None, COINIT_MULTITHREADED)) {
                ComInitOutcome::Owned => Some(ComApartmentGuard { owns: true }),
                ComInitOutcome::AlreadyInitializedElsewhere => {
                    Some(ComApartmentGuard { owns: false })
                }
                ComInitOutcome::Failed => {
                    debug!("UIA: COM initialization failed — IsPassword query unavailable");
                    None
                }
            }
        }
    }

    /// UIA `IsPassword` check (T-105, revised after a v0.42.0 adversarial
    /// review finding): catches non-Win32 password fields (Electron/browser/
    /// WinUI, e.g. a Chrome/Edge `<input type="password">`) invisible to the
    /// `ES_PASSWORD` style check above, which only exists on classic Win32
    /// `Edit` controls.
    ///
    /// `pid` is the owning process of the control being checked (the
    /// caller's already-known target/control pid — capture and `validate()`
    /// both have it on hand). The ORIGINAL implementation queried
    /// `IUIAutomation::ElementFromHandle(control)` — which, for a Chromium/
    /// Electron host HWND, returns the renderer HOST element, not the
    /// specific focused input descendant that actually carries
    /// `IsPassword`; a browser password field was therefore invisible to
    /// this check even though it looked like the right API call. The fix
    /// queries `GetFocusedElement()` instead: UIA's own process/desktop-wide
    /// focus tracking, which correctly resolves to the true focused
    /// descendant (a web/Electron control has no HWND of its own, so
    /// `ElementFromHandle` can never reach it, but `GetFocusedElement` does).
    /// Since that query is desktop-wide rather than scoped to a specific
    /// HWND, its result is verified to belong to `pid` before being trusted
    /// — at capture time `pid` IS whatever currently has focus (the capture
    /// target), so this always matches; at delivery revalidation
    /// (`validate()`, which runs BEFORE the target is re-activated) the
    /// currently focused element may well belong to a different app
    /// entirely, and a pid mismatch there correctly means "can't determine
    /// for THIS target" rather than misreading an unrelated window's focus.
    ///
    /// Returns `None` on ANY failure — COM unavailable, `CoCreateInstance`,
    /// `GetFocusedElement`, a pid mismatch, or the property query itself
    /// erroring. Callers MUST treat `None` as "could not determine" and fail
    /// OPEN (never capture-blocking on its own): a UIA failure must never be
    /// read as "is a password field", and the `ES_PASSWORD` check remains
    /// the hard floor regardless of whether UIA is available on this
    /// machine. Only a definite `Some(true)` refuses capture/delivery.
    /// Deliberately NOT called from the per-keystroke `guard_still_foreground`
    /// TOCTOU re-check (T-103) — that path is cheap-syscalls-only and a COM
    /// automation round-trip (element lookup + property fetch) does not belong
    /// there. It runs at capture, at delivery revalidation (`validate()`), and
    /// once after activation in `begin_delivery`. NOTE (T-105 LATER-13): a
    /// cross-process UIA round-trip is synchronous and has NO timeout, and one
    /// of its call sites (an on-start anchor `Set`) sits on the
    /// recording-START path ahead of mic-open — so it CAN add latency there if
    /// the target's UIA provider is slow/hung. Moving that specific call off
    /// the start path onto a bounded MTA worker thread is the deferred
    /// LATER-13 follow-up.
    fn uia_is_password(pid: u32) -> Option<bool> {
        unsafe {
            let _com = ensure_com_initialized()?;
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            let element: IUIAutomationElement = automation.GetFocusedElement().ok()?;
            let focused_pid = element.CurrentProcessId().ok()?;
            if focused_pid < 0 || focused_pid as u32 != pid {
                return None;
            }
            let is_password = element.CurrentIsPassword().ok()?;
            Some(is_password.as_bool())
        }
    }

    #[derive(Clone)]
    struct Target {
        hwnd: isize,
        control: isize,
        pid: u32,
        tid: u32,
        window_class: String,
        control_class: String,
        app: String,
        /// T-301: cursor position captured with this target (or `None`). Rides
        /// inside the committed `Target` and mirrors to/from
        /// `SavedJumpSlot.cursor` in the persist path. Captured
        /// UNCONDITIONALLY at capture time; a save toggle nulls it at commit.
        /// T-302: the gating is now PER-SLOT (`jumper_save_cursor_slots[slot]`)
        /// rather than per-flow. Every capture entry resolves that flag for its
        /// TARGET slot and passes it in as `save_cursor`: the manual/hot
        /// `set_slot` resolves `slots[slot]` itself; the flow-driven track
        /// captures (`set_slot_if_unchanged` / `set_slot_from_guard` /
        /// `set_slot_with_cursor_policy`) receive it from the
        /// coordinator/clipboard caller, which resolved `slots[target_slot]`.
        cursor: Option<SavedCursor>,
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
            targets: [
                None, None, None, None, None, None, None, None, None, None, None,
            ],
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

    /// T-112/LATER-10: on-disk sidecar format version. Bumped whenever the
    /// shape or hashing scheme changes, so an incompatible file (an older
    /// keyed format, or a stray file of some other shape) is treated as
    /// ABSENT rather than misread. The very first (pre-finding-3) format was
    /// a bare `Vec<Option<u64>>` JSON array with no `version`/`key` at all —
    /// a completely different shape, so it already fails `serde_json`
    /// deserialization into `HintsFile` outright; the explicit version check
    /// covers any future shape-compatible-but-semantically-different bump.
    const HINTS_FORMAT_VERSION: u32 = 2;

    /// Per-slot disambiguation hint (T-112), persisted in
    /// `jumper_slot_hints.json`. Both hashes are KEYED (LATER-10) with the
    /// sidecar's own per-install `HintsFile::key` — never a fixed/public
    /// seed — so a leaked hash alone can't be dictionary-matched against
    /// common window titles using a precomputed table shared across
    /// installs.
    #[derive(serde::Serialize, serde::Deserialize, Clone, Copy)]
    struct SlotHint {
        /// Keyed hash of (app, window_class, control_class) as they were at
        /// the moment this hint was saved. NOT sensitive on its own (those
        /// strings already live in cleartext in `jumper_saved_slots`) — its
        /// purpose is purely to let `resolve_saved` detect a hint that no
        /// longer corresponds to the identity CURRENTLY saved for this slot
        /// (finding 3: a torn/partial persist between the two files) and
        /// ignore it rather than risk disambiguating against a stale
        /// window's title.
        identity_fp: u64,
        /// Keyed hash of the window's title at save time.
        title_hash: u64,
    }

    /// The sidecar file's full shape (T-112/LATER-10, finding 3).
    #[derive(serde::Serialize, serde::Deserialize)]
    struct HintsFile {
        version: u32,
        /// Per-install random key (LATER-10), generated ONCE and persisted
        /// here — regenerating it on every load would make every
        /// previously saved hint permanently unmatchable, defeating the
        /// whole disambiguation feature. Only created fresh when no valid
        /// file exists yet (`Default::default`).
        key: [u8; 16],
        /// Index = slot.
        slots: Vec<Option<SlotHint>>,
    }

    impl Default for HintsFile {
        fn default() -> Self {
            HintsFile {
                version: HINTS_FORMAT_VERSION,
                key: *uuid::Uuid::new_v4().as_bytes(),
                slots: vec![None; SLOT_COUNT],
            }
        }
    }

    /// Pure parse + version gate, extracted so the "old/incompatible format
    /// is ignored, never misread" decision is unit-testable without real
    /// file I/O (LATER-10).
    fn parse_hints_file(s: &str) -> Option<HintsFile> {
        serde_json::from_str::<HintsFile>(s)
            .ok()
            .filter(|f| f.version == HINTS_FORMAT_VERSION)
    }

    /// Keyed hash (LATER-10): the key is never fixed/public, so a hash alone
    /// (without this install's sidecar) can't be dictionary-matched against
    /// common strings using a table computed once and shared across
    /// installs.
    fn keyed_hash(key: &[u8; 16], data: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        data.hash(&mut hasher);
        hasher.finish()
    }

    /// Keyed fingerprint of a slot's (app, window_class, control_class)
    /// identity (finding 3) — used to detect a hint that no longer matches
    /// what's currently saved for its slot.
    fn identity_fingerprint(
        key: &[u8; 16],
        app: &str,
        window_class: &str,
        control_class: &str,
    ) -> u64 {
        keyed_hash(
            key,
            &format!("{app}\u{0}{window_class}\u{0}{control_class}"),
        )
    }

    /// T-112: a KEYED hash (LATER-10; never the raw text) of a window's
    /// current title, used only as a same-app/same-class DISAMBIGUATION
    /// signal — see the module doc. Hashing rather than persisting the
    /// title verbatim means a document/page name never lands on disk even
    /// though its identity is still comparable across a restart. `None` for
    /// an untitled/inaccessible window (never treated as a match against a
    /// saved hint — see `resolve_saved`).
    fn title_hash(key: &[u8; 16], hwnd: HWND) -> Option<u64> {
        let mut buf = [0u16; 256];
        let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
        if n <= 0 {
            return None;
        }
        let title = String::from_utf16_lossy(&buf[..n as usize]);
        if title.is_empty() {
            return None;
        }
        Some(keyed_hash(key, &title))
    }

    /// Pure decision (finding 3, v0.42.0 adversarial re-verify): a persisted
    /// hint is usable ONLY if its `identity_fp` matches the identity
    /// CURRENTLY being resolved — extracted so the "torn/partial persist"
    /// detection is unit-testable without real file I/O or a live window.
    fn hint_is_valid_for(hint: &SlotHint, current_identity_fp: u64) -> bool {
        hint.identity_fp == current_identity_fp
    }

    /// T-112: per-slot title-hash hints for same-app/same-class
    /// disambiguation (see `resolve_saved`'s module doc). Deliberately kept
    /// OUTSIDE `AppSettings`/`jumper_saved_slots` — a tiny sidecar JSON file
    /// of its own — so this hardening needs no settings-schema change; it is
    /// purely a resolver-side refinement of the SAME saved identity.
    fn hints_path(app: &AppHandle) -> Option<std::path::PathBuf> {
        // Portable-aware (T-114): keep the hints sidecar beside the rest of
        // the app's data so a portable launch doesn't strand it in %APPDATA%.
        crate::portable::resolve_app_data_dir(app)
            .ok()
            .map(|d| d.join("jumper_slot_hints.json"))
    }

    /// Load the sidecar file, defaulting to a fresh (new random key, all
    /// slots empty) `HintsFile` if it's missing, unreadable, or an
    /// old/incompatible format (`parse_hints_file`'s version gate — LATER-10).
    fn load_hints_file(app: &AppHandle) -> HintsFile {
        let mut file = hints_path(app)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| parse_hints_file(&s))
            .unwrap_or_default();
        // T-305: a pre-expansion sidecar (exactly the old SLOT_COUNT of 6, with
        // Hot 2 at index 5) must move its Hot 2 hint to the new HOT2 index as it
        // grows to SLOT_COUNT. A plain resize would strand the hint at index 5
        // (now Static 5) and leave the real Hot 2 unhinted, so a migrated Hot 2
        // with two matching windows couldn't auto-disambiguate. Mirrors the
        // settings-side jumper_v4 migration. Other lengths (Hot 2 never existed,
        // or already grown) just resize. Deterministic, so re-running before the
        // next persist rewrites the file yields the same result.
        const PRE_T305_LEN: usize = 6;
        const OLD_HOT2: usize = 5;
        if file.slots.len() == PRE_T305_LEN && SLOT_COUNT > PRE_T305_LEN {
            file.slots.resize(SLOT_COUNT, None);
            file.slots[super::HOT2] = file.slots[OLD_HOT2].take();
        } else if file.slots.len() != SLOT_COUNT {
            file.slots.resize(SLOT_COUNT, None);
        }
        file
    }

    /// Serialize and write `file` to `path` via write-then-rename (finding
    /// 3): a crash mid-write can never leave a half-written
    /// `jumper_slot_hints.json` in place of a previously-good one — the
    /// rename only lands once the full temp-file write has already
    /// succeeded. Returns whether the write succeeded, so `persist_slot` can
    /// make identity persistence fail-atomic with it.
    fn write_hints_file(path: &std::path::Path, file: &HintsFile) -> bool {
        let Ok(s) = serde_json::to_string(file) else {
            debug!("Jumper: failed to serialize jump slot hints");
            return false;
        };
        let tmp = path.with_extension("json.tmp");
        if let Err(e) = std::fs::write(&tmp, &s) {
            debug!("Failed to write jump slot hints temp file: {}", e);
            return false;
        }
        if let Err(e) = std::fs::rename(&tmp, path) {
            debug!("Failed to finalize jump slot hints file: {}", e);
            return false;
        }
        true
    }

    /// Write (or clear, when `target` is `None`) `slot`'s disambiguation
    /// hint, returning whether the write succeeded. Called from
    /// `persist_slot`, which already runs under `PERSIST_LOCK` (finding 6) —
    /// piggybacking this write onto that SAME critical section avoids adding
    /// a second, independently-racing persistence path for what is really
    /// one logical "persist this slot's identity" operation.
    ///
    /// Finding 3 (v0.42.0 adversarial review): `persist_slot` treats a
    /// `false` return here as fatal to a SET (the identity write is aborted
    /// too — see its doc) so a hint that fails to land can never be paired
    /// with a newer identity than the one it actually describes.
    fn save_slot_hint(app: &AppHandle, slot: usize, target: Option<&Target>) -> bool {
        let Some(path) = hints_path(app) else {
            debug!(
                "Jumper: no app-data dir available — cannot persist slot hint for slot {}",
                slot
            );
            return false;
        };
        let mut file = load_hints_file(app);
        if slot >= file.slots.len() {
            file.slots.resize(SLOT_COUNT, None);
        }
        let key = file.key;
        let new_hint = target.and_then(|t| {
            let th = title_hash(&key, HWND(t.hwnd as _))?;
            Some(SlotHint {
                identity_fp: identity_fingerprint(&key, &t.app, &t.window_class, &t.control_class),
                title_hash: th,
            })
        });
        file.slots[slot] = new_hint;
        write_hints_file(&path, &file)
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
    ///
    /// Finding 3 (v0.42.0 adversarial review): identity (`jumper_saved_slots`
    /// in settings) and the disambiguation hint (`jumper_slot_hints.json`)
    /// are two SEPARATE stores. The old ordering wrote identity FIRST, hint
    /// SECOND — if the hint write for a NEW occupant of this slot failed
    /// after the identity had already landed, a restart could find the OLD
    /// hint (from whatever window previously occupied this slot) still on
    /// disk and mis-resolve against it: exactly the wrong-window bug T-112
    /// exists to prevent. Fixed by making a SET fail-atomic: the hint is
    /// written FIRST (verified via `save_slot_hint`'s return), and the
    /// identity is only persisted once that succeeded; on failure the WHOLE
    /// persist is aborted (identity left at its previous on-disk value) —
    /// the in-memory `SLOTS` state, already committed by the caller before
    /// `persist_slot` runs, is unaffected, so the live slot keeps working
    /// regardless.
    ///
    /// A CLEAR (`target` is `None`) is exempt from that abort: with no saved
    /// identity, `resolve_saved` early-returns before ever reading the hint
    /// (see its `let Some(Some(saved)) = ... else { return false }`), so a
    /// hint write failure during a clear can only leave harmless orphan
    /// bytes, never a mis-resolution risk — aborting the identity clear
    /// would be actively worse, since it would let the just-cleared anchor
    /// silently reappear on the next restart.
    fn persist_slot(app: &AppHandle, slot: usize, target: Option<&Target>) {
        if !crate::settings::get_settings(app).jumper_persist {
            return;
        }

        if target.is_some() {
            if !save_slot_hint(app, slot, target) {
                warn!(
                    "Jumper: aborting identity persist for slot {} — failed to write its disambiguation hint first (T-112 fail-atomic ordering)",
                    slot
                );
                return;
            }
        } else if !save_slot_hint(app, slot, None) {
            debug!(
                "Jumper: slot {} hint clear failed (non-fatal — no identity means it's never read)",
                slot
            );
        }

        let saved = target.map(|t| crate::settings::SavedJumpSlot {
            app: t.app.clone(),
            window_class: t.window_class.clone(),
            control_class: t.control_class.clone(),
            // T-301: mirror the saved cursor alongside the identity.
            cursor: t.cursor,
        });
        // Partial RMW under SETTINGS_MUTATION_LOCK, touching ONLY this slot —
        // NOT the whole settings struct (finding 11): a bare whole-struct
        // write from a stale snapshot could resurrect `jumper_persist=true`
        // and cleared identities if a `change_jumper_persist_setting` disable
        // landed in the gap. Re-checking `jumper_persist` inside the locked
        // closure means a disable that committed first wins — we skip the
        // identity write instead of resurrecting it.
        crate::settings::update_settings(app, |settings| {
            if !settings.jumper_persist {
                return;
            }
            if settings.jumper_saved_slots.len() < SLOT_COUNT {
                settings.jumper_saved_slots.resize(SLOT_COUNT, None);
            }
            settings.jumper_saved_slots[slot] = saved;
        });
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
        /// T-112: EVERY visible window matching (exe, window_class), in
        /// `EnumWindows`' own Z-order (topmost first) — previously this
        /// stopped at the FIRST match, so two windows of the same app (two
        /// Chrome windows, two Citrix sessions) were silently
        /// indistinguishable; `resolve_saved` needs all of them to attempt
        /// hint-based disambiguation before picking one.
        candidates: Vec<isize>,
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
            if let Some(p) = process_name(pid) {
                if p.eq_ignore_ascii_case(&ctx.app) {
                    ctx.candidates.push(hwnd.0 as isize);
                }
            }
            // Never stop early (T-112) — a same-app window further back in
            // Z-order than the first match must still be collected so
            // disambiguation has every candidate to work with.
            true.into()
        }
    }

    /// Try to turn one saved identity back into a live target: the same
    /// executable + window class. `expected` is the slot's generation at the
    /// moment the CALLER observed it empty (finding 5) — it must be captured
    /// in the SAME lock acquisition as that emptiness check (see
    /// `empty_slot_generation`), never re-read in here: re-reading it here,
    /// AFTER the caller's separate check, would let a manual Set landing in
    /// that gap become this function's own "expected" baseline — and the
    /// stale restore below would then incorrectly win the race against it
    /// once the slow scan finishes.
    ///
    /// T-112 (same-app multi-window disambiguation): when the identity match
    /// is AMBIGUOUS (more than one visible window shares it — two Chrome
    /// windows, two Citrix sessions), a persisted title-hash hint is tried
    /// first (verified via `hint_is_valid_for` against a fingerprint of
    /// THIS `saved` identity — finding 3: a hint that doesn't match is
    /// treated exactly like no hint at all, since it can only mean a
    /// torn/partial persist rather than a hint for the window actually
    /// saved here); if it uniquely picks ONE candidate, that one is used.
    /// Otherwise, `allow_ambiguous_fallback` decides:
    /// `false` (the automatic/lazy paths — startup restore, `begin_delivery`)
    /// refuses to guess and leaves the slot unresolved (stays "stale"/red in
    /// the UI rather than silently committing to a possibly-wrong window);
    /// `true` (the explicit `jump` path only) falls back to the topmost
    /// (Z-order-first) candidate — the user is about to SEE where they land,
    /// so a visible, reversible guess is acceptable there in a way it is not
    /// for a silent automatic delivery.
    fn resolve_saved(
        app: &AppHandle,
        slot: usize,
        expected: u64,
        allow_ambiguous_fallback: bool,
    ) -> bool {
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
                candidates: Vec::new(),
            };
            let _ = EnumWindows(
                Some(find_window_cb),
                LPARAM(&mut ctx as *mut FindCtx as isize),
            );
            if ctx.candidates.is_empty() {
                return false;
            }
            let chosen = if ctx.candidates.len() == 1 {
                ctx.candidates[0]
            } else {
                let hints = load_hints_file(app);
                let current_fp = identity_fingerprint(
                    &hints.key,
                    &saved.app,
                    &saved.window_class,
                    &saved.control_class,
                );
                let raw_hint = hints.slots.get(slot).copied().flatten();
                let saved_hint = raw_hint
                    .filter(|h| hint_is_valid_for(h, current_fp))
                    .map(|h| h.title_hash);
                if raw_hint.is_some() && saved_hint.is_none() {
                    debug!(
                        "Jump slot {} hint ignored — identity fingerprint mismatch (stale/partial persist)",
                        slot
                    );
                }
                let hint_matches: Vec<isize> = match saved_hint {
                    Some(hint) => ctx
                        .candidates
                        .iter()
                        .copied()
                        .filter(|&h| title_hash(&hints.key, HWND(h as _)) == Some(hint))
                        .collect(),
                    None => Vec::new(),
                };
                match hint_matches.len() {
                    1 => hint_matches[0],
                    _ if allow_ambiguous_fallback => {
                        debug!(
                            "Jump slot {} ambiguous ({} same-app/class windows, {}) — falling back to the topmost (explicit jump)",
                            slot,
                            ctx.candidates.len(),
                            if saved_hint.is_some() {
                                "hint present but not a unique match"
                            } else {
                                "no title hint saved"
                            }
                        );
                        ctx.candidates[0]
                    }
                    _ => {
                        debug!(
                            "Jump slot {} ambiguous ({} same-app/class windows, {}) — leaving unresolved (stale)",
                            slot,
                            ctx.candidates.len(),
                            if saved_hint.is_some() {
                                "hint present but not a unique match"
                            } else {
                                "no title hint saved"
                            }
                        );
                        return false;
                    }
                }
            };
            let hwnd = HWND(chosen as _);
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
                // T-301: restore the persisted cursor back into the live target.
                cursor: saved.cursor,
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
    ///
    /// Finding 3 (v0.42.0 SECOND adversarial review) — fail-atomic ordering,
    /// matching `persist_slot`: the ORIGINAL version wrote identities into
    /// `jumper_saved_slots` FIRST, then wrote the hints sidecar SECOND,
    /// best-effort. If an OLD (pre-existing) `jumper_slot_hints.json`
    /// happened to still be on disk (e.g. persistence was toggled off/on
    /// before, or the hint write below failed) and this call's hint write
    /// then also failed, a restart would resolve the freshly-snapshotted
    /// identities against whatever STALE hints were left over — able to
    /// mis-disambiguate two same-app/class windows exactly like the
    /// original bug T-112 exists to close, just reached via this alternate
    /// "persistence just turned ON" path instead of `persist_slot`'s
    /// per-slot one. Fixed the same way `persist_slot` was: write/replace
    /// the hints sidecar FIRST (`write_hints_file`'s existing
    /// temp-then-rename), and only persist the slot identities into
    /// settings if that succeeded — an aborted snapshot leaves
    /// `jumper_saved_slots` at whatever it was before (typically empty,
    /// freshly toggled on) rather than risking a mismatched pairing; the
    /// in-memory `SLOTS` state is unaffected either way, so live anchors
    /// keep working regardless.
    ///
    /// Finding 11 (same review pass): the settings mutation now goes
    /// through `settings::update_settings` (the `SETTINGS_MUTATION_LOCK`
    /// path) instead of a bare `get_settings`/`write_settings` pair — the
    /// bare pattern could interleave with an UNRELATED concurrent
    /// `change_jumper_persist_setting` DISABLE's own settings write and
    /// silently resurrect a just-cleared `jumper_saved_slots` (last-
    /// writer-wins on the whole struct). The mutate closure also
    /// re-checks `jumper_persist` is still `true` at the moment it runs —
    /// belt-and-suspenders alongside `with_persist_toggle_lock` (which
    /// should make a concurrent disable land entirely before or after this
    /// whole call, never interleaved) so this function stays self-
    /// consistent even if some future caller ever invokes it outside that
    /// lock. See `with_persist_toggle_lock`'s doc for the full lock order.
    pub fn snapshot_all(app: &AppHandle) {
        let _persist_guard = PERSIST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let live = SLOTS.lock().map(|s| s.targets.clone()).unwrap_or_default();

        let Some(path) = hints_path(app) else {
            warn!(
                "Jumper: no app-data dir available — aborting snapshot_all (cannot persist hints first)"
            );
            return;
        };
        let mut file = load_hints_file(app);
        let key = file.key;
        let new_hints: Vec<Option<SlotHint>> = (0..SLOT_COUNT)
            .map(|i| {
                live.get(i).and_then(|t| t.as_ref()).and_then(|t| {
                    let th = title_hash(&key, HWND(t.hwnd as _))?;
                    Some(SlotHint {
                        identity_fp: identity_fingerprint(
                            &key,
                            &t.app,
                            &t.window_class,
                            &t.control_class,
                        ),
                        title_hash: th,
                    })
                })
            })
            .collect();
        file.slots = new_hints;
        if !snapshot_may_persist_identities(write_hints_file(&path, &file)) {
            warn!(
                "Jumper: aborting snapshot_all — failed to write the disambiguation hints sidecar first (T-112/finding-3 fail-atomic ordering)"
            );
            return;
        }

        crate::settings::update_settings(app, |settings| {
            if !snapshot_should_write_identities(settings.jumper_persist) {
                debug!(
                    "Jumper: snapshot_all no-op — persistence was disabled before this snapshot's settings write could land"
                );
                return;
            }
            settings.jumper_saved_slots = (0..SLOT_COUNT)
                .map(|i| {
                    live.get(i)
                        .and_then(|t| t.as_ref())
                        .map(|t| crate::settings::SavedJumpSlot {
                            app: t.app.clone(),
                            window_class: t.window_class.clone(),
                            control_class: t.control_class.clone(),
                            // T-301: mirror the saved cursor alongside the identity.
                            cursor: t.cursor,
                        })
                })
                .collect();
        });
    }

    /// Pure decision (finding 3, v0.42.0 SECOND adversarial review): whether
    /// `snapshot_all` may proceed to persist slot identities into settings,
    /// given whether its FIRST step — writing the disambiguation-hints
    /// sidecar — succeeded. Mirrors `persist_slot`'s existing fail-atomic
    /// SET ordering (hint first, abort the identity write entirely on
    /// failure) rather than the old identities-first/hints-best-effort
    /// order. Extracted so the ordering decision itself is unit-testable
    /// without real file I/O.
    fn snapshot_may_persist_identities(hints_write_succeeded: bool) -> bool {
        hints_write_succeeded
    }

    /// Pure decision (finding 11, same review pass): whether `snapshot_all`'s
    /// settings mutation should actually write `jumper_saved_slots`, given
    /// the CURRENT `jumper_persist` flag value read inside the SAME
    /// `update_settings` critical section that performs the write. Guards
    /// against a concurrent disable landing between this function's hint-file
    /// write and its settings-mutate closure running — `with_persist_toggle_
    /// lock` should make that window unreachable in practice (see its doc),
    /// but this keeps `snapshot_all` self-consistent even for a hypothetical
    /// future caller outside that lock, rather than relying on lock
    /// discipline alone.
    fn snapshot_should_write_identities(persist_currently_enabled: bool) -> bool {
        persist_currently_enabled
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
                // T-112: startup restore never guesses an ambiguous match —
                // no user is watching to catch a wrong pick.
                if resolve_saved(app, slot, expected, false) {
                    restored += 1;
                }
            }
        }
        if restored > 0 {
            info!("Jumper: restored {restored} persisted slot(s)");
            emit_changed(app);
        }
    }

    /// LATER-11: delete the sidecar hints file entirely. Meant to be called
    /// from the persistence-disable path (`jumper_persist` turned off) —
    /// once `jumper_saved_slots` is wiped there, `resolve_saved` early-
    /// returns before ever reading the hint file again, so
    /// `jumper_slot_hints.json` becomes permanently orphaned: nothing reads
    /// it, and nothing deletes it either, so it silently lingers on disk
    /// (holding keyed-but-still-somebody's-window-title hashes) forever. A
    /// missing file is not an error — most of the time persistence was
    /// never turned on at all.
    pub fn delete_persisted_hints(app: &AppHandle) {
        let Some(path) = hints_path(app) else {
            return;
        };
        match std::fs::remove_file(&path) {
            Ok(()) => debug!("Jumper: deleted orphaned jump slot hints file"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => debug!("Failed to delete jump slot hints file: {}", e),
        }
    }

    /// Turn persistence OFF as ONE operation under `PERSIST_LOCK` (finding 11,
    /// v0.42.0 THIRD adversarial review): flip `jumper_persist` false, clear
    /// the saved identities, and delete the hints sidecar — all while holding
    /// `PERSIST_LOCK`. `persist_slot` runs under the SAME lock (via
    /// `persist_current_slot`/`snapshot_all`), so it can NEVER interleave: an
    /// in-flight persist either fully completes before this runs (its identity
    /// is then cleared here) or starts after (it re-reads `jumper_persist` ==
    /// false under the lock and writes nothing — no hint recreated after the
    /// delete, no identity a stale resolver could pick up). `update_settings`
    /// (SETTINGS_MUTATION_LOCK) and `delete_persisted_hints` (a bare file
    /// remove) take no lock that conflicts, preserving the documented order
    /// PERSIST_TOGGLE_LOCK → PERSIST_LOCK → SETTINGS_MUTATION_LOCK.
    pub fn disable_persistence(app: &AppHandle) {
        let _persist_guard = PERSIST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        crate::settings::update_settings(app, |settings| {
            settings.jumper_persist = false;
            settings.jumper_saved_slots = vec![None; SLOT_COUNT];
        });
        delete_persisted_hints(app);
    }

    // ------------------------------------------------------------------
    // T-301: mouse-cursor save & restore (Windows-only, best-effort).
    //
    // Every cursor op here is STRICTLY best-effort: it must never fail a
    // jump/paste, never panic, never retry. Any Win32 failure (locked/secure/
    // headless desktop, RDP/Citrix remap, monitor hotplug, a destroyed/recycled
    // HWND, GetCursorPos/SetCursorPos erroring, ClipCursor silently adjusting
    // the point) degrades to "did nothing" / "used the absolute fallback".
    // All coordinates are PHYSICAL virtual-screen pixels, coherent only under
    // verified Per-Monitor-V2 DPI awareness (checked before every restore).
    // ------------------------------------------------------------------

    /// Warn at most once when cursor restore is skipped for lack of
    /// per-monitor DPI awareness (a process-wide, unchanging condition — no
    /// point logging it per jump).
    static DPI_AWARENESS_WARNED: std::sync::Once = std::sync::Once::new();

    /// T-301 PREREQUISITE (Codex correction #3, revised): cursor restore is
    /// only safe when this thread is Per-Monitor-V2 DPI aware — all the math is
    /// in physical pixels, which mean something different under system/unaware/
    /// PMv1 contexts. tao attempts PMv2 at runtime but can fall back, so verify
    /// at runtime rather than trusting a manifest (there is none — the runtime
    /// check IS the gate).
    ///
    /// This is an EXACT-context equality check, NOT the earlier
    /// `GetAwarenessFromDpiAwarenessContext(...) == DPI_AWARENESS_PER_MONITOR_AWARE`
    /// coarse test: that awareness ENUM collapses PMv1 and PMv2 into the SAME
    /// `PER_MONITOR_AWARE` value, so it wrongly accepted PMv1 (whose coordinate
    /// semantics differ from PMv2). `AreDpiAwarenessContextsEqual` against the
    /// `PER_MONITOR_AWARE_V2` sentinel confirms V2 specifically. If it is not
    /// exactly V2, callers SKIP restore entirely (never corrupt coords).
    fn per_monitor_dpi_aware() -> bool {
        unsafe {
            AreDpiAwarenessContextsEqual(
                GetThreadDpiAwarenessContext(),
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            )
            .as_bool()
        }
    }

    /// Pure divide-by-zero-guarded client-area normalization: a screen point
    /// `(px, py)`, the client rect's screen-space top-left `(ox, oy)`, and the
    /// client size `(w, h)` → fraction (0..1) of the client area. `None` on a
    /// non-positive dimension. Extracted so the norm math is unit-testable
    /// without a live window.
    ///
    /// The app-relative deltas (`px - ox`, `py - oy`) are computed in `i64`
    /// (Codex correction #4): both operands are physical virtual-screen pixels
    /// that can legitimately be far negative (monitors left of / above the
    /// primary), so the raw i32 subtraction could overflow — panicking in a
    /// debug build, wrapping in release. Widening before the subtraction makes
    /// it total; the ratio is a plain `f64` divide afterward.
    fn normalize_in_client(
        px: i32,
        py: i32,
        ox: i32,
        oy: i32,
        w: i32,
        h: i32,
    ) -> Option<(f64, f64)> {
        if w <= 0 || h <= 0 {
            return None;
        }
        let dx = px as i64 - ox as i64;
        let dy = py as i64 - oy as i64;
        Some((dx as f64 / w as f64, dy as f64 / h as f64))
    }

    /// Pure inverse of `normalize_in_client` for ONE axis, with checked/widened
    /// arithmetic (Codex correction #6): client `origin` (screen px) + `norm` ×
    /// `size` (client px), computed in `i64` and saturated back to `i32` so a
    /// wild persisted norm or a huge client rect can never overflow. `f64→i64`
    /// `as` casts saturate in Rust, so this is total.
    fn norm_to_screen(origin: i32, size: i32, norm: f64) -> i32 {
        let offset = (norm * size as f64).round() as i64;
        (origin as i64 + offset).clamp(i32::MIN as i64, i32::MAX as i64) as i32
    }

    /// One client-rect dimension (`right - left` or `bottom - top`), widened to
    /// `i64` before the subtraction (Codex correction #4) so two far-apart i32
    /// edge coordinates can never overflow, then validated strictly positive
    /// and saturated back to `i32`. `None` on a degenerate/non-positive axis
    /// (caller falls back to the absolute cursor pixel). Extracted so the
    /// widening is unit-testable without a live window.
    fn client_dim(hi: i32, lo: i32) -> Option<i32> {
        let d = hi as i64 - lo as i64;
        if d <= 0 {
            return None;
        }
        Some(d.min(i32::MAX as i64) as i32)
    }

    /// Pure decision (Codex correction #3): should AppRelative restore attempt
    /// window-relative placement at all? Only when the mode is AppRelative AND
    /// both normalized coords were captured. Otherwise (ScreenAbsolute, or
    /// AppRelative whose client rect was unavailable at capture → `None` norms)
    /// restore falls back to the absolute pixel. Testable without a window.
    fn wants_app_relative(cursor: &SavedCursor) -> Option<(f64, f64)> {
        if cursor.mode == CursorMode::AppRelative {
            if let (Some(nx), Some(ny)) = (cursor.norm_x, cursor.norm_y) {
                return Some((nx, ny));
            }
        }
        None
    }

    /// Pure 1-D clamp into a monitor rect edge (Codex correction #1): the
    /// monitor rect's `right`/`bottom` are ONE PAST the last valid pixel
    /// (exclusive), so the inclusive max is `hi - 1`. A degenerate rect
    /// (`hi <= lo`) clamps to `lo`. Testable without a monitor.
    fn clamp_into_rect_1d(v: i32, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        v.clamp(lo, hi - 1)
    }

    /// Capture the current cursor position as a `SavedCursor` (T-301),
    /// UNCONDITIONALLY — the per-flow save toggle is honored at commit, not
    /// here, so the hot slot and manual sets always carry it. `hwnd` is the
    /// anchor window, used only for the AppRelative client-normalized coords;
    /// `abs_*` is the physical virtual-screen pixel and doubles as the
    /// AppRelative fallback. The per-slot `CursorMode` is chosen by the CALLER
    /// (via `slot_cursor_mode` against the TARGET slot) and passed in as `mode`;
    /// capture stays the single choke point that STAMPS it onto the
    /// `SavedCursor`. `None` only if `GetCursorPos` fails outright.
    fn capture_cursor(_app: &AppHandle, hwnd: HWND, mode: CursorMode) -> Option<SavedCursor> {
        let mut pt = POINT::default();
        unsafe {
            if GetCursorPos(&mut pt).is_err() {
                return None;
            }
        }
        let (norm_x, norm_y) = match client_norm(hwnd, pt) {
            Some((nx, ny)) => (Some(nx), Some(ny)),
            None => (None, None),
        };
        Some(SavedCursor {
            abs_x: pt.x,
            abs_y: pt.y,
            norm_x,
            norm_y,
            mode,
        })
    }

    /// Client-area-normalized cursor coords via `GetClientRect` +
    /// `ClientToScreen`. `None` (→ absolute fallback) on any failure or a
    /// degenerate client rect.
    fn client_norm(hwnd: HWND, pt: POINT) -> Option<(f64, f64)> {
        let mut rc = RECT::default();
        let mut origin = POINT { x: 0, y: 0 };
        unsafe {
            if GetClientRect(hwnd, &mut rc).is_err() {
                return None;
            }
            if !ClientToScreen(hwnd, &mut origin).as_bool() {
                return None;
            }
        }
        // Codex correction #4: compute the client dimensions in i64 (via
        // `client_dim`), validate positive, and saturate back to i32 before
        // the norm math — a raw `rc.right - rc.left` could overflow i32.
        let w = client_dim(rc.right, rc.left)?;
        let h = client_dim(rc.bottom, rc.top)?;
        normalize_in_client(pt.x, pt.y, origin.x, origin.y, w, h)
    }

    /// AppRelative restore point: `norm` fraction of the TARGET window's LIVE
    /// client rect, in screen coords. `None` (→ caller uses abs) if the window
    /// is gone or the client rect is unavailable/degenerate.
    fn app_relative_point(target_hwnd: isize, nx: f64, ny: f64) -> Option<(i32, i32)> {
        let hwnd = HWND(target_hwnd as _);
        let mut rc = RECT::default();
        let mut origin = POINT { x: 0, y: 0 };
        unsafe {
            if !IsWindow(Some(hwnd)).as_bool() {
                return None;
            }
            if GetClientRect(hwnd, &mut rc).is_err() {
                return None;
            }
            if !ClientToScreen(hwnd, &mut origin).as_bool() {
                return None;
            }
        }
        // Codex correction #4: widen the client-dimension subtraction to i64
        // (via `client_dim`) — it validates positivity and saturates back to
        // i32, so a huge/degenerate live client rect can neither overflow nor
        // slip past the positive-dimension check.
        let w = client_dim(rc.right, rc.left)?;
        let h = client_dim(rc.bottom, rc.top)?;
        Some((
            norm_to_screen(origin.x, w, nx),
            norm_to_screen(origin.y, h, ny),
        ))
    }

    /// Resolve the desired restore point in physical virtual-screen pixels:
    /// AppRelative (window-relative) when available, else the captured
    /// absolute pixel (also the ScreenAbsolute path).
    fn resolve_restore_point(cursor: &SavedCursor, target_hwnd: isize) -> (i32, i32) {
        if let Some((nx, ny)) = wants_app_relative(cursor) {
            if let Some(pt) = app_relative_point(target_hwnd, nx, ny) {
                return pt;
            }
        }
        (cursor.abs_x, cursor.abs_y)
    }

    /// Clamp a point to the nearest CURRENTLY-present monitor's rect (Codex
    /// correction #1): the virtual-desktop bounding box has dead pixels between
    /// staggered/L-shaped/separated monitors, so we clamp to an ACTUAL monitor,
    /// never `SM_*VIRTUALSCREEN`. `MONITOR_DEFAULTTONEAREST` returns the
    /// monitor the point is in (clamp is then a no-op) or, for a point in dead
    /// space / off all monitors (e.g. the saved monitor was unplugged), the
    /// closest one — into whose `rcMonitor` we clamp. Best-effort: on API
    /// failure the point is returned unchanged (SetCursorPos clamps anyway).
    fn clamp_to_nearest_monitor(pt: (i32, i32)) -> (i32, i32) {
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        unsafe {
            let mon = MonitorFromPoint(POINT { x: pt.0, y: pt.1 }, MONITOR_DEFAULTTONEAREST);
            if !GetMonitorInfoW(mon, &mut mi).as_bool() {
                return pt;
            }
        }
        (
            clamp_into_rect_1d(pt.0, mi.rcMonitor.left, mi.rcMonitor.right),
            clamp_into_rect_1d(pt.1, mi.rcMonitor.top, mi.rcMonitor.bottom),
        )
    }

    /// Best-effort cursor restore (T-301). Verifies the PMv2 prerequisite,
    /// resolves the point (AppRelative w/ absolute fallback, or ScreenAbsolute),
    /// clamps it onto the nearest present monitor, and `SetCursorPos`. NEVER
    /// fails a jump/paste, never panics — every failure is swallowed.
    fn restore_cursor(cursor: &SavedCursor, target_hwnd: isize) {
        if !per_monitor_dpi_aware() {
            DPI_AWARENESS_WARNED.call_once(|| {
                warn!(
                    "Jumper: cursor restore skipped — thread is not Per-Monitor-V2 DPI aware (physical-pixel coords would be incoherent)"
                );
            });
            return;
        }
        let point = resolve_restore_point(cursor, target_hwnd);
        let (x, y) = clamp_to_nearest_monitor(point);
        unsafe {
            let _ = SetCursorPos(x, y);
        }
    }

    /// Capture the current foreground window/control as a `Target`, applying
    /// every capture-time refusal (no foreground window, Handy's own window,
    /// password field). `Target` no longer carries a generation (that lives
    /// in `SlotState`, separately) — callers assign one at commit time via
    /// `next_generation()`, kept as close as possible to the moment the
    /// target actually lands in `SLOTS`.
    fn capture_current_target(app: &AppHandle, slot: usize) -> Result<Target, String> {
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
            // T-105: the ES_PASSWORD check above is the hard floor — it only
            // exists for classic Win32 `Edit` controls, so it never catches
            // Electron/browser/WinUI password inputs. Query UIA's
            // `IsPassword` as an additional check: a definite `true` refuses
            // capture; any query failure (COM unavailable, UIA error, pid
            // mismatch) is treated as "not a password" (fail open on the
            // QUERY only) so a machine where UIA misbehaves never loses the
            // ability to anchor at all. `pid` is this capture's own process
            // — at capture time the currently UIA-focused element always
            // belongs to it, so `uia_is_password` reliably resolves the real
            // focused descendant instead of the classic-control host.
            if uia_is_password(pid).unwrap_or(false) {
                return Err("refusing to anchor a password field".into());
            }
            let app_name = process_name(pid).unwrap_or_else(|| "unknown".into());
            // T-301: capture the cursor UNCONDITIONALLY (relative to the
            // top-level window); flow gating nulls it at commit for the
            // flow-driven paths, never for the manual/hot `set_slot`.
            let mode = super::slot_cursor_mode(
                &crate::settings::get_settings(app).jumper_cursor_mode_slots,
                slot,
            );
            let cursor = capture_cursor(app, hwnd, mode);
            Ok(Target {
                hwnd: hwnd.0 as isize,
                control: control.0 as isize,
                pid,
                tid,
                window_class: class_name(hwnd),
                control_class,
                app: app_name,
                cursor,
            })
        }
    }

    /// Manual/unconditional capture: always wins regardless of what was
    /// there before (a direct user action — Set Anchor, the hot-slot Set
    /// binding, the static `jump_set_slot_N` bindings — is authoritative and
    /// never deferred, so there's nothing to compare-and-swap against). Since
    /// T-302 the cursor save is PER-SLOT: this manual/hot capture keeps the
    /// cursor IFF `jumper_save_cursor_slots[slot]` is on (each slot, hot or
    /// static, has its own toggle on the Jumper page — the old per-flow
    /// output/submit toggles are gone). This wrapper resolves that per-slot
    /// flag and defers to `set_slot_with_cursor_policy`. (Flow-driven track
    /// captures resolve the SAME per-slot flag for their TARGET slot in the
    /// coordinator/clipboard caller and pass it in — see
    /// `set_slot_if_unchanged` / `set_slot_from_guard` /
    /// `set_slot_with_cursor_policy`, which take an explicit `save_cursor`.)
    pub fn set_slot(app: &AppHandle, slot: usize) -> Result<AnchorStatus, String> {
        let save_cursor = super::slot_save_cursor(
            &crate::settings::get_settings(app).jumper_save_cursor_slots,
            slot,
        );
        set_slot_with_cursor_policy(app, slot, save_cursor)
    }

    /// Manual/unconditional capture with an explicit cursor policy (T-301):
    /// identical to `set_slot`'s authoritative "always wins" commit, but the
    /// caller decides whether the (unconditionally captured) cursor is kept.
    /// Used by the non-anchored track-last-output fallback in clipboard.rs,
    /// which gates the cursor on the DRIVING flow's toggle rather than always
    /// the output toggle.
    pub fn set_slot_with_cursor_policy(
        app: &AppHandle,
        slot: usize,
        save_cursor: bool,
    ) -> Result<AnchorStatus, String> {
        let mut target = capture_current_target(app, slot)?;
        if !save_cursor {
            target.cursor = None;
        }
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

    /// T-104: build a `Target` from an ACTIVE delivery's already-verified
    /// hwnd/control instead of a fresh `GetForegroundWindow()` query.
    /// `guard`'s target was activated and focus-verified in `begin_delivery`,
    /// then re-verified via `guard_still_foreground` immediately before the
    /// paste keystroke and before any submit keystroke — so it IS where the
    /// text landed, regardless of what window a submit key (Enter closing a
    /// dialog, navigating a composer) may have left in the foreground
    /// AFTERWARD. The pre-T-104 track-last-output capture ran a plain
    /// `capture_current_target` (fresh `GetForegroundWindow()`) AFTER the
    /// whole paste+submit sequence, so Enter could have already moved focus
    /// away from the real delivery target by the time it ran.
    ///
    /// Finding 2 (v0.42.0 adversarial review): this used to re-derive
    /// pid/tid/window_class/control_class from the raw HWNDs at THIS call —
    /// seconds after `begin_delivery` verified them — and, worse, when the
    /// control HWND was no longer a valid window, it substituted the
    /// top-level window's HWND as the "control" and rebuilt identity from
    /// THAT. Both are unsafe: a destroyed HWND value can be recycled by
    /// Windows for an entirely unrelated new window, so re-querying at
    /// capture time risks silently adopting that new window's identity, and
    /// "broadening to root" bookmarks the wrong element (or a replacement)
    /// instead of where the text actually landed. Fixed: the identity is
    /// taken from `guard`'s own already-verified fields VERBATIM (snapshotted
    /// at `begin_delivery` time, see the `DeliveryGuard` doc) — never
    /// re-derived from the HWNDs here. The HWNDs are used ONLY for a
    /// liveness (`IsWindow`) check; if the control is no longer a live
    /// window, this fails outright (the caller must leave the slot
    /// unchanged) rather than falling back to anything.
    fn capture_target_from_guard(guard: &DeliveryGuard) -> Result<Target, String> {
        unsafe {
            let hwnd = HWND(guard.target_hwnd as _);
            if !IsWindow(Some(hwnd)).as_bool() {
                return Err(
                    "delivery target window no longer exists — skipping track capture".into(),
                );
            }
            let control = HWND(guard.target_control as _);
            if !IsWindow(Some(control)).as_bool() {
                debug!(
                    "Track-from-guard capture for slot {} skipped — delivery target control no longer exists (not broadening to the window root)",
                    guard.slot
                );
                return Err(
                    "delivery target control no longer exists — skipping track capture".into(),
                );
            }
            // T-104 finding 2 (v0.42.0 SECOND adversarial review):
            // `IsWindow` only proves the HWND VALUE currently refers to SOME
            // live window — it says nothing about whether it's still the
            // SAME window this guard was constructed for. Between
            // `begin_delivery`'s snapshot (seconds ago, before a full
            // paste+submit sequence) and this call, the target could have
            // been destroyed and Windows could have recycled its HWND value
            // for an entirely unrelated new window — one that also happens
            // to be alive right now, so `IsWindow` alone would pass. Re-read
            // the LIVE identity of both HWNDs and compare against the
            // snapshot taken at `begin_delivery` time (the pure comparison
            // lives in `guard_identity_still_matches` so it's unit-testable
            // without a real HWND); only a match proves the handle still IS
            // that window/control — never adopt the snapshot's identity
            // without this check passing.
            let mut live_window_pid = 0u32;
            let live_window_tid = GetWindowThreadProcessId(hwnd, Some(&mut live_window_pid));
            let mut live_control_pid = 0u32;
            let live_control_tid = GetWindowThreadProcessId(control, Some(&mut live_control_pid));
            if live_window_tid == 0 || live_control_tid == 0 {
                debug!(
                    "Track-from-guard capture for slot {} skipped — could not re-read the live window/control's owning thread",
                    guard.slot
                );
                return Err(
                    "delivery target identity could not be reverified — skipping track capture"
                        .into(),
                );
            }
            let live_window_class = class_name(hwnd);
            let live_control_class = class_name(control);
            if !guard_identity_still_matches(
                live_window_pid,
                live_window_tid,
                &live_window_class,
                live_control_pid,
                &live_control_class,
                guard.target_pid,
                guard.target_tid,
                &guard.target_window_class,
                &guard.target_control_class,
            ) {
                debug!(
                    "Track-from-guard capture for slot {} skipped — live window/control identity no longer matches the delivery snapshot (handle likely recycled)",
                    guard.slot
                );
                return Err("delivery target identity changed — skipping track capture".into());
            }
            Ok(Target {
                hwnd: guard.target_hwnd,
                control: guard.target_control,
                pid: guard.target_pid,
                tid: guard.target_tid,
                window_class: guard.target_window_class.clone(),
                control_class: guard.target_control_class.clone(),
                app: guard.target_app.clone(),
                // T-301: filled in by `set_slot_from_guard` (needs `app` for
                // the settings mode + flow gating) — captured fresh there.
                cursor: None,
            })
        }
    }

    /// Pure decision behind `capture_target_from_guard`'s recycled-handle
    /// guard (T-104 finding 2): does a freshly re-read window/control
    /// identity still match what `begin_delivery` snapshotted into the
    /// `DeliveryGuard`? Mirrors `validate()`'s existing convention —
    /// same-process AND same-thread for the top-level window (a replaced
    /// window is a different process or gets a fresh thread), same process
    /// (deliberately NOT same thread — Citrix/Chromium/XAML input sites
    /// legitimately host a control on a different thread than its top-level
    /// window) plus same class for the control. Extracted so this comparison
    /// is unit-testable without a live HWND/COM environment.
    fn guard_identity_still_matches(
        live_window_pid: u32,
        live_window_tid: u32,
        live_window_class: &str,
        live_control_pid: u32,
        live_control_class: &str,
        expected_pid: u32,
        expected_tid: u32,
        expected_window_class: &str,
        expected_control_class: &str,
    ) -> bool {
        live_window_pid == expected_pid
            && live_window_tid == expected_tid
            && live_window_class == expected_window_class
            && live_control_pid == expected_pid
            && live_control_class == expected_control_class
    }

    /// Track-last-output capture for an ANCHORED delivery (T-104): same
    /// authoritative "always wins" commit as `set_slot` (a track-last-output
    /// decision is made and executed in the SAME instant the paste finishes,
    /// so the generation-CAS guard the deferred writers use doesn't apply —
    /// see the module doc) — but sourced via `capture_target_from_guard`
    /// instead of re-querying the foreground window.
    pub fn set_slot_from_guard(
        app: &AppHandle,
        guard: &DeliveryGuard,
        slot: usize,
        save_cursor: bool,
    ) -> Result<AnchorStatus, String> {
        if slot >= SLOT_COUNT {
            return Err(format!("invalid jump slot {slot}"));
        }
        let mut target = capture_target_from_guard(guard)?;
        // T-301 (Codex correction #5): flow-driven track capture — grab the
        // cursor (relative to the delivery target window) UNCONDITIONALLY,
        // then null it unless the DRIVING flow opted in. `save_cursor` is
        // computed by clipboard.rs from the correct flow's toggle (submit flow
        // → submit toggle, dictate/output flow → output toggle), so the two
        // flows gate independently rather than sharing an "either" test.
        let mode = super::slot_cursor_mode(
            &crate::settings::get_settings(app).jumper_cursor_mode_slots,
            slot,
        );
        target.cursor = capture_cursor(app, HWND(guard.target_hwnd as _), mode);
        if !save_cursor {
            target.cursor = None;
        }
        info!(
            "Jump slot {} tracked from delivery: {} (class '{}', pid {}, tid {})",
            slot, target.app, target.control_class, target.pid, target.tid
        );
        match SLOTS.lock() {
            Ok(mut state) => {
                let g = next_generation();
                state.generations[slot] = g;
                state.targets[slot] = Some(target.clone());
            }
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
    pub fn set_slot_if_unchanged(
        app: &AppHandle,
        slot: usize,
        expected: u64,
        save_cursor: bool,
    ) -> bool {
        let mut target = match capture_current_target(app, slot) {
            Ok(t) => t,
            Err(e) => {
                debug!("Automatic capture for slot {} skipped: {}", slot, e);
                return false;
            }
        };
        // T-301 (Codex correction #5): flow-driven capture — drop the
        // (unconditionally captured) cursor unless the DRIVING flow opted in.
        // `save_cursor` is decided by the caller from the correct flow's
        // toggle. The manual/hot `set_slot` gates on the output toggle instead.
        if !save_cursor {
            target.cursor = None;
        }
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
            // T-105: same UIA `IsPassword` re-check at delivery revalidation
            // time as capture — a non-Win32 password field (Electron/
            // browser/WinUI) invisible to ES_PASSWORD above can just as
            // easily arm AFTER capture (e.g. navigating a saved Jump slot to
            // a page that turned out to be a login form) as it can before.
            // Same fail-open-on-query-failure contract as `capture_current_
            // target` — never capture/delivery-blocking on its own; only a
            // definite `true` refuses. Not window-gone, so `false` here.
            // `t.pid` scopes the (desktop-wide) `GetFocusedElement()` query
            // to this specific target: `validate()` runs BEFORE the target
            // is re-activated, so whatever currently holds OS focus may well
            // belong to an entirely different app — a pid mismatch there
            // correctly yields "can't determine", not a false verdict about
            // this control.
            if uia_is_password(t.pid).unwrap_or(false) {
                return Err(("target field became a password field".into(), false));
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
        //
        // T-112: `jump` is the ONE caller allowed to fall back on an
        // ambiguous match (`true`) — the user is about to see where they
        // land, so a visible, reversible guess is acceptable here in a way
        // it is not for an automatic/silent path.
        if let Some(expected) = empty_slot_generation(slot) {
            if resolve_saved(app, slot, expected, true) {
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
        // T-301: restore the saved cursor AFTER activation + focus, using the
        // validated target's own cursor. Best-effort — a failure never fails
        // the jump.
        if let Some(cursor) = target.cursor.as_ref() {
            restore_cursor(cursor, target.hwnd);
        }
        debug!("Jumped to slot {}: {}", slot, target.app);
        Ok(())
    }

    pub fn begin_delivery(app: &AppHandle, slot: usize) -> BeginDelivery {
        // Lazy restore of a persisted identity before giving up. Finding 5:
        // same atomic empty-check + generation-snapshot as `jump`. T-112:
        // unlike `jump`, delivery never guesses an ambiguous match (`false`)
        // — pasting into a silently-wrong window is exactly the failure mode
        // this hardening exists to prevent, so an ambiguous slot stays
        // unresolved here (delivery then fails closed with "not set", same
        // as any other unresolved slot) rather than picking one.
        if let Some(expected) = empty_slot_generation(slot) {
            if resolve_saved(app, slot, expected, false) {
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
        // T-105 finding 1 (v0.42.0 SECOND adversarial review): `validate()`
        // above runs BEFORE activation, while the target may not hold OS
        // focus at all — if some OTHER window currently has it,
        // `uia_is_password`'s pid-scoped `GetFocusedElement()` query can
        // never reach the target (a pid mismatch yields `None`, which fails
        // OPEN as "not a password" by design), so a saved slot that became a
        // password field between capture and delivery could slip through
        // silently. NOW that activation is verified (`GetForegroundWindow()
        // == target.hwnd`, just proven by `activate_verified`) AND the
        // control itself is focused (`focus_control_verified` just
        // succeeded), this is the FIRST point in the flow where
        // `GetFocusedElement()` is guaranteed to resolve to the target's own
        // focused element with a matching pid — the query can actually SEE
        // this specific control. A definite positive here means the field is
        // a password field right now, at the exact moment we're about to
        // paste into it — abort so the text parks on the clipboard (the
        // caller's `BeginDelivery::Failed` contract) instead of landing in a
        // password box. `ES_PASSWORD`/`validate()`'s checks remain as
        // defense in depth; this closes the gap they could not reach.
        if uia_is_password(target.pid).unwrap_or(false) {
            warn!(
                "Jump slot {} delivery aborted post-activation — UIA reports the focused control is a password field",
                slot
            );
            unsafe {
                if prev != 0 && IsWindow(Some(HWND(prev as _))).as_bool() {
                    let _ = SetForegroundWindow(HWND(prev as _));
                }
            }
            return BeginDelivery::Failed {
                reason: "target field is a password field".into(),
            };
        }
        // Small settle so the target app processes the focus change before
        // the paste keystroke arrives.
        std::thread::sleep(std::time::Duration::from_millis(60));
        BeginDelivery::Ready(DeliveryGuard {
            prev_foreground: prev,
            target_hwnd: target.hwnd,
            target_control: target.control,
            // T-104 finding 2: snapshot the FULL verified identity here,
            // while `target` is still the just-`validate()`d struct — this
            // is what `capture_target_from_guard` must use verbatim later,
            // never a fresh re-query of the (by-then possibly recycled)
            // HWNDs above.
            target_pid: target.pid,
            target_tid: target.tid,
            target_window_class: target.window_class.clone(),
            target_control_class: target.control_class.clone(),
            target_app: target.app.clone(),
            slot,
            // T-301: snapshot the saved cursor into the guard — do NOT restore
            // here; `finish_delivery` restores it as the very last op.
            cursor: target.cursor,
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
        // T-301: cursor restore is the VERY LAST focus/input op of the whole
        // delivery — strictly after the anchor-delivered emit AND the
        // return-focus block above — so it can never perturb the TOCTOU
        // keystroke path. Best-effort; a failure never affects the delivery.
        if let Some(cursor) = guard.cursor.as_ref() {
            restore_cursor(cursor, guard.target_hwnd);
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
                cursor: None,
            }
        }

        fn empty_state() -> SlotState {
            SlotState {
                targets: [
                    None, None, None, None, None, None, None, None, None, None, None,
                ],
                generations: [0; SLOT_COUNT],
            }
        }

        #[test]
        fn cas_commit_succeeds_when_generation_matches_expected() {
            let mut state = empty_state();
            // Derive `expected` from the allocator instead of a literal:
            // `next_generation()` is a process-global monotonic counter shared
            // across parallel tests, so a hardcoded expected (e.g. 5) is flaky —
            // the fresh generation cas_commit allocates could coincidentally
            // equal it when the counter happens to sit there.
            let expected = next_generation();
            state.targets[HOT] = Some(dummy_target());
            state.generations[HOT] = expected;
            let result = cas_commit(&mut state, HOT, expected, dummy_target());
            assert!(result.is_some());
            // Committing always allocates a FRESH generation, strictly GREATER
            // than any previously issued one (monotonic) — so a subsequent stale
            // writer holding the old snapshot can never match it.
            assert!(state.generations[HOT] > expected);
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

        /// T-105: `classify_com_init`'s pure decision logic, without any real
        /// COM call — a success HRESULT (S_OK, value 0) must be `Owned`
        /// (needs a matching `CoUninitialize`); `RPC_E_CHANGED_MODE` must be
        /// `AlreadyInitializedElsewhere` (must NOT be uninitialized by us);
        /// and any other failure HRESULT must be `Failed`.
        #[test]
        fn classify_com_init_decides_ownership_from_hresult() {
            assert_eq!(classify_com_init(HRESULT(0)), ComInitOutcome::Owned);
            // S_FALSE (already initialized on this thread, SAME mode) is
            // still a success HRESULT (>= 0) and must ALSO be Owned — the
            // Win32 contract requires pairing it with CoUninitialize too.
            assert_eq!(classify_com_init(HRESULT(1)), ComInitOutcome::Owned);
            assert_eq!(
                classify_com_init(RPC_E_CHANGED_MODE),
                ComInitOutcome::AlreadyInitializedElsewhere
            );
            // E_FAIL: an arbitrary failure HRESULT unrelated to apartment
            // mode — must be `Failed`, never mistaken for either success
            // case.
            assert_eq!(
                classify_com_init(HRESULT(0x80004005_u32 as i32)),
                ComInitOutcome::Failed
            );
        }

        /// LATER-10: the hash is KEYED — the same title under two different
        /// keys must not collide, or an attacker holding a hash but not this
        /// install's key gains nothing over the old unkeyed `DefaultHasher`
        /// scheme.
        #[test]
        fn keyed_hash_differs_across_keys_for_the_same_data() {
            let a = keyed_hash(&[0u8; 16], "My Document.docx - Notepad");
            let b = keyed_hash(&[1u8; 16], "My Document.docx - Notepad");
            assert_ne!(a, b);
        }

        /// The hint feature depends on the SAME key reproducing the SAME
        /// hash across restarts (the key is persisted, not regenerated per
        /// run) — otherwise every previously saved hint would immediately
        /// stop matching.
        #[test]
        fn keyed_hash_is_deterministic_for_the_same_key_and_data() {
            let key = [7u8; 16];
            assert_eq!(
                keyed_hash(&key, "same title"),
                keyed_hash(&key, "same title")
            );
            assert_ne!(keyed_hash(&key, "title a"), keyed_hash(&key, "title b"));
        }

        /// T-112 finding 3: `hint_is_valid_for` is the pure decision behind
        /// "ignore a hint that doesn't correspond to the identity currently
        /// being resolved" (a torn/partial persist between the settings
        /// file and the hints sidecar).
        #[test]
        fn hint_is_valid_for_requires_matching_identity_fingerprint() {
            let hint = SlotHint {
                identity_fp: 42,
                title_hash: 99,
            };
            assert!(hint_is_valid_for(&hint, 42));
            assert!(!hint_is_valid_for(&hint, 43));
        }

        /// LATER-10: the OLD (pre-finding-3) sidecar format was a bare
        /// `Vec<Option<u64>>` JSON array — a totally different shape from
        /// `HintsFile`, so it must fail to parse rather than being
        /// misinterpreted (e.g. a stray leading integer read as `version`).
        #[test]
        fn parse_hints_file_rejects_pre_keyed_bare_array_format() {
            assert!(parse_hints_file("[null,null,null,null,null]").is_none());
        }

        /// LATER-10: an otherwise well-shaped file with the WRONG version
        /// must also be ignored (never misread) — a future format bump
        /// checks this exact gate.
        #[test]
        fn parse_hints_file_rejects_mismatched_version() {
            let json = r#"{"version":1,"key":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"slots":[null,null,null,null,null]}"#;
            assert!(parse_hints_file(json).is_none());
        }

        #[test]
        fn parse_hints_file_accepts_current_version_and_round_trips_the_key() {
            let json = format!(
                r#"{{"version":{},"key":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16],"slots":[null,null,null,null,null]}}"#,
                HINTS_FORMAT_VERSION
            );
            let file = parse_hints_file(&json).expect("current-version file should parse");
            assert_eq!(
                file.key,
                [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
            );
            assert_eq!(file.slots.len(), 5);
        }

        /// T-104 finding 2 (v0.42.0 SECOND adversarial review):
        /// `guard_identity_still_matches` is the pure recycled-handle
        /// comparison behind `capture_target_from_guard` — a perfect match
        /// on every field is the only thing that may adopt the snapshotted
        /// identity.
        #[test]
        fn guard_identity_still_matches_requires_every_field_to_match() {
            assert!(guard_identity_still_matches(
                100,
                200,
                "Chrome_WidgetWin_1",
                100,
                "Edit",
                100,
                200,
                "Chrome_WidgetWin_1",
                "Edit",
            ));
        }

        /// A live pid mismatch on the top-level window is exactly the
        /// "handle recycled for an unrelated new window" case this fix
        /// closes — must be rejected even though `IsWindow` would have
        /// reported the handle as alive.
        #[test]
        fn guard_identity_still_matches_rejects_window_pid_mismatch() {
            assert!(!guard_identity_still_matches(
                999,
                200,
                "Chrome_WidgetWin_1",
                100,
                "Edit",
                100,
                200,
                "Chrome_WidgetWin_1",
                "Edit",
            ));
        }

        #[test]
        fn guard_identity_still_matches_rejects_window_tid_mismatch() {
            assert!(!guard_identity_still_matches(
                100,
                999,
                "Chrome_WidgetWin_1",
                100,
                "Edit",
                100,
                200,
                "Chrome_WidgetWin_1",
                "Edit",
            ));
        }

        #[test]
        fn guard_identity_still_matches_rejects_window_class_mismatch() {
            assert!(!guard_identity_still_matches(
                100,
                200,
                "Notepad",
                100,
                "Edit",
                100,
                200,
                "Chrome_WidgetWin_1",
                "Edit",
            ));
        }

        /// The control's pid must match the snapshot too, even though its
        /// THREAD deliberately is not compared (see the function doc — a
        /// legitimately multi-threaded host like Citrix/Chromium/XAML can
        /// run a control's thread separately from its window's).
        #[test]
        fn guard_identity_still_matches_rejects_control_pid_mismatch() {
            assert!(!guard_identity_still_matches(
                100,
                200,
                "Chrome_WidgetWin_1",
                999,
                "Edit",
                100,
                200,
                "Chrome_WidgetWin_1",
                "Edit",
            ));
        }

        #[test]
        fn guard_identity_still_matches_rejects_control_class_mismatch() {
            assert!(!guard_identity_still_matches(
                100,
                200,
                "Chrome_WidgetWin_1",
                100,
                "Static",
                100,
                200,
                "Chrome_WidgetWin_1",
                "Edit",
            ));
        }

        /// T-112 finding 3 (v0.42.0 SECOND adversarial review):
        /// `snapshot_all` must not persist identities into settings unless
        /// its hints-sidecar write (which now runs FIRST) actually
        /// succeeded — the fail-atomic ordering mirroring `persist_slot`.
        #[test]
        fn snapshot_may_persist_identities_requires_successful_hint_write() {
            assert!(snapshot_may_persist_identities(true));
            assert!(!snapshot_may_persist_identities(false));
        }

        /// Finding 11 (same review pass): `snapshot_all`'s settings mutation
        /// must be a no-op if `jumper_persist` is no longer enabled at the
        /// moment it runs — the defense-in-depth check alongside
        /// `with_persist_toggle_lock` that keeps a concurrent disable from
        /// being silently undone.
        #[test]
        fn snapshot_should_write_identities_follows_current_persist_flag() {
            assert!(snapshot_should_write_identities(true));
            assert!(!snapshot_should_write_identities(false));
        }

        // ------------------------------------------------------------------
        // T-301: cursor save/restore pure-logic tests.
        // ------------------------------------------------------------------

        /// A `SavedCursor` (incl. `None` norms) must survive a serde
        /// round-trip unchanged — persisted inside `SavedJumpSlot.cursor`.
        #[test]
        fn saved_cursor_serde_round_trips_including_none_norms() {
            let with_norms = SavedCursor {
                abs_x: -1920,
                abs_y: 37,
                norm_x: Some(0.25),
                norm_y: Some(0.75),
                mode: CursorMode::AppRelative,
            };
            let json = serde_json::to_string(&with_norms).unwrap();
            let back: SavedCursor = serde_json::from_str(&json).unwrap();
            assert_eq!(back, with_norms);

            let no_norms = SavedCursor {
                abs_x: 10,
                abs_y: 20,
                norm_x: None,
                norm_y: None,
                mode: CursorMode::ScreenAbsolute,
            };
            let json = serde_json::to_string(&no_norms).unwrap();
            let back: SavedCursor = serde_json::from_str(&json).unwrap();
            assert_eq!(back, no_norms);
            assert!(back.norm_x.is_none() && back.norm_y.is_none());
        }

        /// The `CursorMode` variants must cross the command boundary as the
        /// exact strings the frontend Dropdown + manual parse rely on.
        #[test]
        fn cursor_mode_serializes_as_pinned_variant_names() {
            assert_eq!(
                serde_json::to_string(&CursorMode::AppRelative).unwrap(),
                "\"AppRelative\""
            );
            assert_eq!(
                serde_json::to_string(&CursorMode::ScreenAbsolute).unwrap(),
                "\"ScreenAbsolute\""
            );
        }

        /// Client-normalization math: a point at the client origin is 0.0, at
        /// the far corner is ~1.0, at the center is 0.5; a non-positive
        /// dimension guards divide-by-zero with `None`.
        #[test]
        fn normalize_in_client_maps_points_to_fractions() {
            // origin (100, 200), size 800x600.
            assert_eq!(
                normalize_in_client(100, 200, 100, 200, 800, 600),
                Some((0.0, 0.0))
            );
            assert_eq!(
                normalize_in_client(500, 500, 100, 200, 800, 600),
                Some((0.5, 0.5))
            );
            assert_eq!(
                normalize_in_client(900, 800, 100, 200, 800, 600),
                Some((1.0, 1.0))
            );
            // Degenerate client rect → None (abs fallback).
            assert_eq!(normalize_in_client(0, 0, 0, 0, 0, 600), None);
            assert_eq!(normalize_in_client(0, 0, 0, 0, 800, -1), None);
        }

        /// `norm_to_screen` is the inverse of `normalize_in_client` on one
        /// axis and round-trips the center/corner points.
        #[test]
        fn norm_to_screen_inverts_normalization() {
            assert_eq!(norm_to_screen(100, 800, 0.0), 100);
            assert_eq!(norm_to_screen(100, 800, 0.5), 500);
            assert_eq!(norm_to_screen(100, 800, 1.0), 900);
            // Negative origin (secondary monitor left of primary) is fine.
            assert_eq!(norm_to_screen(-1920, 1920, 0.5), -960);
        }

        /// Checked/widened arithmetic (Codex correction #6): a pathological
        /// persisted norm or size can never overflow — the result saturates
        /// into `i32` instead of panicking/wrapping.
        #[test]
        fn norm_to_screen_saturates_instead_of_overflowing() {
            assert_eq!(norm_to_screen(i32::MAX, i32::MAX, 1000.0), i32::MAX);
            assert_eq!(norm_to_screen(i32::MIN, i32::MAX, -1000.0), i32::MIN);
        }

        /// Codex correction #4: the app-relative delta is computed in i64, so
        /// far-apart physical pixels (a point on a monitor far left of the
        /// origin) never overflow the i32 subtraction — in debug this would
        /// otherwise panic. Values chosen so `px - ox` exceeds i32 range.
        #[test]
        fn normalize_in_client_delta_does_not_overflow_i32() {
            // ox near i32::MIN, px near i32::MAX → true delta ~2^32, unrepresentable in i32.
            let (nx, _ny) = normalize_in_client(i32::MAX, 0, i32::MIN, 0, i32::MAX, 600).unwrap();
            // ~ (i32::MAX - i32::MIN) / i32::MAX ≈ 2.0, computed without overflow.
            assert!((nx - 2.0).abs() < 0.001, "nx was {nx}");
        }

        /// Codex correction #4: `client_dim` widens `hi - lo` to i64 (no
        /// overflow), rejects non-positive/degenerate axes, and saturates the
        /// result back into i32.
        #[test]
        fn client_dim_widens_validates_and_saturates() {
            assert_eq!(client_dim(800, 100), Some(700));
            // Degenerate / inverted → None.
            assert_eq!(client_dim(100, 100), None);
            assert_eq!(client_dim(100, 200), None);
            // hi - lo would overflow i32 (positive); widened then saturated to i32::MAX.
            assert_eq!(client_dim(i32::MAX, i32::MIN), Some(i32::MAX));
            // Large-but-in-range difference is preserved exactly.
            assert_eq!(client_dim(i32::MAX, 0), Some(i32::MAX));
        }

        /// Nearest-monitor clamp (Codex correction #1): a point inside the
        /// rect is unchanged; a point past the exclusive right/bottom edge
        /// clamps to `right-1`/`bottom-1`; a point before the top-left clamps
        /// to `left`/`top`; a degenerate rect clamps to its origin.
        #[test]
        fn clamp_into_rect_1d_respects_exclusive_far_edge() {
            // Synthetic secondary monitor at [-1920, 0) width 1920.
            assert_eq!(clamp_into_rect_1d(-1000, -1920, 0), -1000); // inside
            assert_eq!(clamp_into_rect_1d(-1920, -1920, 0), -1920); // left edge
            assert_eq!(clamp_into_rect_1d(50, -1920, 0), -1); // past exclusive right
            assert_eq!(clamp_into_rect_1d(-5000, -1920, 0), -1920); // far left
            // Degenerate rect → origin.
            assert_eq!(clamp_into_rect_1d(500, 100, 100), 100);
            assert_eq!(clamp_into_rect_1d(500, 100, 50), 100);
        }

        /// AppRelative WITH captured norms attempts window-relative placement.
        #[test]
        fn wants_app_relative_uses_norms_when_present_and_app_relative() {
            let c = SavedCursor {
                abs_x: 5,
                abs_y: 6,
                norm_x: Some(0.3),
                norm_y: Some(0.4),
                mode: CursorMode::AppRelative,
            };
            assert_eq!(wants_app_relative(&c), Some((0.3, 0.4)));
        }

        /// AppRelative with MISSING norms falls back to absolute (Codex
        /// correction #3 fallback) — the client rect was unavailable at
        /// capture, so there is no window-relative point to restore to.
        #[test]
        fn wants_app_relative_none_when_norms_missing_falls_back_to_abs() {
            let c = SavedCursor {
                abs_x: 5,
                abs_y: 6,
                norm_x: None,
                norm_y: None,
                mode: CursorMode::AppRelative,
            };
            assert_eq!(wants_app_relative(&c), None);
            // Half-captured norms are treated as absent too.
            let half = SavedCursor {
                norm_x: Some(0.5),
                norm_y: None,
                ..c
            };
            assert_eq!(wants_app_relative(&half), None);
        }

        /// ScreenAbsolute never uses norms even when they were captured.
        #[test]
        fn wants_app_relative_none_for_screen_absolute_mode() {
            let c = SavedCursor {
                abs_x: 5,
                abs_y: 6,
                norm_x: Some(0.3),
                norm_y: Some(0.4),
                mode: CursorMode::ScreenAbsolute,
            };
            assert_eq!(wants_app_relative(&c), None);
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
static POST_TAKE_ACTION: std::sync::Mutex<Option<(PostTakeAction, usize, Option<u64>, bool)>> =
    std::sync::Mutex::new(None);

/// `save_cursor` is the DRIVING flow's cursor policy, resolved at the
/// finishing press by the coordinator (submit flow → submit toggle,
/// dictate/output flow → output toggle) and carried verbatim into the
/// deferred on-finish Set so `run_post_take_action` never has to recompute it
/// (it has no flow context at run time). Ignored for the Clear variant.
pub fn arm_post_take_action(action: PostTakeAction, slot: usize, save_cursor: bool) {
    if slot < SLOT_COUNT {
        let expected = slot_generation(slot);
        if let Ok(mut guard) = POST_TAKE_ACTION.lock() {
            *guard = Some((action, slot, expected, save_cursor));
        }
    }
}

pub fn take_post_take_action() -> Option<(PostTakeAction, usize, Option<u64>, bool)> {
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
pub fn run_post_take_action(
    app: &AppHandle,
    action: Option<(PostTakeAction, usize, Option<u64>, bool)>,
) {
    match action {
        Some((PostTakeAction::Set, slot, expected, save_cursor)) => {
            // T-301: the deferred on-finish Set carries the DRIVING flow's own
            // cursor policy, resolved and threaded through `arm_post_take_action`
            // at the finishing press (submit flow → submit toggle, dictate/
            // output flow → output toggle) — NOT recomputed from the output
            // toggle here, which ignored the submit flow's toggle (the last
            // per-flow cursor-gating gap).
            if !set_slot_if_unchanged(app, slot, expected, save_cursor) {
                log::debug!(
                    "on-finish set for slot {} skipped — a newer capture won the race",
                    slot
                );
            }
        }
        Some((PostTakeAction::Clear, slot, expected, _save_cursor)) => {
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
pub fn set_slot_if_unchanged(
    app: &AppHandle,
    slot: usize,
    expected: Option<u64>,
    save_cursor: bool,
) -> bool {
    #[cfg(windows)]
    {
        win::set_slot_if_unchanged(app, slot, expected.unwrap_or(0), save_cursor)
    }
    #[cfg(not(windows))]
    {
        let _ = (app, slot, expected, save_cursor);
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

/// Explicit-policy manual capture (T-301/T-302): like `set_slot` but the
/// caller decides whether the cursor is kept, instead of `set_slot` resolving
/// the per-slot flag itself. Used by the non-anchored track-last-output
/// fallback in clipboard.rs, which resolves `jumper_save_cursor_slots[slot]`
/// for the TARGET slot (T-302 per-slot gating) and passes it in, matching the
/// anchored `track_from_guard` path. `set_slot` resolves the same per-slot flag
/// itself for the manual/hot callers.
pub fn set_slot_with_cursor_policy(
    app: &AppHandle,
    slot: usize,
    save_cursor: bool,
) -> Result<AnchorStatus, String> {
    #[cfg(windows)]
    {
        win::set_slot_with_cursor_policy(app, slot, save_cursor)
    }
    #[cfg(not(windows))]
    {
        let _ = (app, slot, save_cursor);
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

/// Track-last-output capture for an ANCHORED delivery (T-104): sources the
/// target from the delivery guard's already-verified hwnd/control instead of
/// a fresh foreground-window query — see `win::capture_target_from_guard`.
/// Callers use this instead of `set_slot` whenever a `DeliveryGuard` is
/// active for the paste being tracked.
#[cfg(windows)]
pub fn track_from_guard(
    app: &AppHandle,
    guard: &DeliveryGuard,
    slot: usize,
    save_cursor: bool,
) -> Result<AnchorStatus, String> {
    win::set_slot_from_guard(app, guard, slot, save_cursor)
}
#[cfg(not(windows))]
pub fn track_from_guard(
    app: &AppHandle,
    guard: &DeliveryGuard,
    slot: usize,
    save_cursor: bool,
) -> Result<AnchorStatus, String> {
    let _ = (app, guard, slot, save_cursor);
    Err("The Jumper is Windows-only in this version".into())
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

/// Serializes the ENTIRE persistence enable/disable toggle
/// (`shortcut::change_jumper_persist_setting`) as one atomic sequence
/// (finding 11, v0.42.0 SECOND adversarial review). Each individual step is
/// already race-free on its own — the `jumper_persist` flag flip goes
/// through `settings::update_settings`, and `snapshot_all`'s own settings
/// write does too (finding 3/11) — but the TWO-OR-THREE steps of one
/// enable (flip flag → snapshot identities + hints) or one disable (flip
/// flag + clear identities → delete the hints sidecar) are not atomic AS A
/// SEQUENCE. Without this lock, an enable and a disable running
/// concurrently could interleave their steps — e.g. enable flips the flag
/// true, then a disable fully runs (flag false, identities cleared, hints
/// file deleted), and only THEN does the enable's now-stale `snapshot_all`
/// call finally run, repopulating `jumper_saved_slots` and recreating the
/// hints file as if the disable had never happened. Holding this lock
/// across the WHOLE toggle body turns each enable/disable into one
/// indivisible operation relative to the other.
///
/// Lock order (must be preserved everywhere to avoid deadlock):
/// `PERSIST_TOGGLE_LOCK` (this lock — outermost, held for an entire toggle
/// call) → `PERSIST_LOCK` (`anchor::win`, held across a single
/// persist/snapshot operation's hint-file I/O plus its settings write) →
/// `SETTINGS_MUTATION_LOCK` (`settings.rs`, held only for the duration of
/// one `update_settings` read-modify-write) → the in-memory `SLOTS` mutex
/// (held only to clone/mutate live targets, NEVER across settings I/O —
/// pre-existing invariant, unchanged). Every caller in this codebase
/// acquires them in this same order (never the reverse: nothing holds
/// `PERSIST_LOCK` or `SETTINGS_MUTATION_LOCK` while trying to acquire
/// `PERSIST_TOGGLE_LOCK`), so no cycle is possible.
pub fn with_persist_toggle_lock<R>(f: impl FnOnce() -> R) -> R {
    static PERSIST_TOGGLE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = PERSIST_TOGGLE_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    f()
}

/// LATER-11: delete the orphaned `jumper_slot_hints.json` sidecar (no-op if
/// it doesn't exist). Meant to be called from the persistence-DISABLE path
/// alongside wiping `jumper_saved_slots` — see
/// `shortcut::change_jumper_persist_setting`'s `if !enabled` branch (that
/// file is outside this module's ownership; wiring the call in is tracked in
/// the T-112 ticket notes for its owner).
pub fn delete_persisted_hints(app: &AppHandle) {
    #[cfg(windows)]
    {
        win::delete_persisted_hints(app);
    }
    #[cfg(not(windows))]
    {
        let _ = app;
    }
}

/// Turn jump-slot persistence OFF atomically under `PERSIST_LOCK` (finding 11).
/// On non-Windows the Jumper is inert, so this just clears the settings flag
/// and identities (no sidecar exists to delete).
pub fn disable_persistence(app: &AppHandle) {
    #[cfg(windows)]
    {
        win::disable_persistence(app);
    }
    #[cfg(not(windows))]
    {
        crate::settings::update_settings(app, |settings| {
            settings.jumper_persist = false;
            settings.jumper_saved_slots = vec![None; SLOT_COUNT];
        });
    }
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

        arm_post_take_action(PostTakeAction::Clear, HOT, false);
        let action = take_post_take_action();
        match action {
            Some((PostTakeAction::Clear, slot, _expected, _save_cursor)) => assert_eq!(slot, HOT),
            _ => panic!("expected an armed Clear action for HOT"),
        }
        // One-shot: taking it again yields nothing.
        assert!(take_post_take_action().is_none());

        // Out-of-range slot is refused at arm time, never silently stored.
        arm_post_take_action(PostTakeAction::Set, SLOT_COUNT + 3, false);
        assert!(take_post_take_action().is_none());
    }

    /// T-302: `set_slot` (and every other capture entry) gates the cursor on
    /// the PER-SLOT flag for the TARGET slot — `jumper_save_cursor_slots[slot]`
    /// — not the removed per-flow output/submit toggles. `slot_save_cursor` is
    /// the single source of truth that gate resolves through; it reads each
    /// slot independently and is bounds-safe (out-of-range / short / empty vec
    /// → `false`, never panics, never assumes default-on).
    #[test]
    fn slot_save_cursor_gates_per_target_slot() {
        // hot (0) on, static 2 on, the rest off.
        let slots = [true, false, true, false, false];
        assert!(slot_save_cursor(&slots, HOT));
        assert!(!slot_save_cursor(&slots, 1));
        assert!(slot_save_cursor(&slots, 2));
        assert!(!slot_save_cursor(&slots, 3));
        assert!(!slot_save_cursor(&slots, 4));

        // Each slot is independent — a neighbor being on never leaks in.
        let only_static_1 = [false, true, false, false, false];
        assert!(!slot_save_cursor(&only_static_1, HOT));
        assert!(slot_save_cursor(&only_static_1, 1));

        // Bounds-safe: out-of-range, short, and empty all read false.
        assert!(!slot_save_cursor(&slots, SLOT_COUNT));
        assert!(!slot_save_cursor(&slots, 99));
        assert!(!slot_save_cursor(&[true, true], 3));
        assert!(!slot_save_cursor(&[], HOT));
    }
}
