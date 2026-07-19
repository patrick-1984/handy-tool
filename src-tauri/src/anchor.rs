//! Jumper (Windows-only v1): five jump slots for desktop text fields.
//!
//! Slot 0 is the HOT slot — the original "anchor": transcription flows can
//! set/clear/jump/deliver-to it via per-flow event actions, it is one-shot
//! (cleared after a VERIFIED delivery unless `anchor_keep`), and it can
//! auto-track where a flow last pasted (`jumper_track_*`). Slots 1–4 are
//! STATIC bookmarks: set via `jump_set_slot_N`, jumped via `jump_slot_N`,
//! never auto-cleared by delivery — they live until overwritten, cleared, or
//! their window dies. All slots share the same capture/validation/delivery
//! machinery and safety rails:
//!
//! - capture via `GetGUIThreadInfo` (no thread-input attachment at capture),
//!   durable identity (HWND + PID + TID + class, revalidated at delivery —
//!   bare HWNDs get recycled), password/self-window rejection;
//! - delivery NEVER pastes blind: activation verified via
//!   `GetForegroundWindow`, control focus verified via `GetGUIThreadInfo`,
//!   Enter fires before focus-return, focus restored only if the user didn't
//!   intervene; a failed delivery parks the text on the clipboard and keeps
//!   the slot (cleared only when the window is destroyed);
//! - delivery is strictly opt-in per paste via a one-shot requested-slot.
//!
//! Slots are in-memory only: window handles die with their windows, so
//! persisting them across restarts would be fake precision.

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

    static SLOTS: Lazy<Mutex<[Option<Target>; SLOT_COUNT]>> =
        Lazy::new(|| Mutex::new([None, None, None, None, None]));

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
            .map(|slots| {
                slots
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
        if let Ok(mut slots) = SLOTS.lock() {
            slots[slot] = None;
        }
        persist_slot(app, slot, None);
        emit_changed(app);
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
    /// delivery accepts window-level focus).
    fn resolve_saved(app: &AppHandle, slot: usize) -> bool {
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
            if let Ok(mut slots) = SLOTS.lock() {
                slots[slot] = Some(target);
            }
            true
        }
    }

    /// Snapshot every live slot into the persisted identities (used when the
    /// user turns persistence ON so existing anchors survive the restart).
    pub fn snapshot_all(app: &AppHandle) {
        let mut settings = crate::settings::get_settings(app);
        let live = SLOTS.lock().map(|s| s.clone()).unwrap_or_default();
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
            if get_slot(slot).is_none() && resolve_saved(app, slot) {
                restored += 1;
            }
        }
        if restored > 0 {
            info!("Jumper: restored {restored} persisted slot(s)");
            emit_changed(app);
        }
    }

    pub fn set_slot(app: &AppHandle, slot: usize) -> Result<AnchorStatus, String> {
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
            let target = Target {
                hwnd: hwnd.0 as isize,
                control: control.0 as isize,
                pid,
                tid,
                window_class: class_name(hwnd),
                control_class: control_class.clone(),
                app: app_name.clone(),
            };
            info!(
                "Jump slot {} set: {} (class '{}', pid {}, tid {})",
                slot, app_name, control_class, pid, tid
            );
            persist_slot(app, slot, Some(&target));
            match SLOTS.lock() {
                Ok(mut slots) => slots[slot] = Some(target),
                // Never report success without storing — callers use the Ok
                // to e.g. suppress the hot slot's one-shot clear.
                Err(_) => return Err("jump slot storage is unavailable".into()),
            }
            emit_changed(app);
            Ok(AnchorStatus {
                app: app_name,
                control_class,
                stale: false,
            })
        }
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

    fn get_slot(slot: usize) -> Option<Target> {
        SLOTS
            .lock()
            .ok()
            .and_then(|slots| slots.get(slot).cloned().flatten())
    }

    pub fn jump(app: &AppHandle, slot: usize) -> Result<(), String> {
        // Lazy restore: a persisted slot whose app appeared after startup
        // resolves on first use ("recovers when the proper app is started").
        if get_slot(slot).is_none() && resolve_saved(app, slot) {
            emit_changed(app);
        }
        let target = get_slot(slot).ok_or_else(|| format!("jump slot {slot} is not set"))?;
        match validate(&target) {
            Ok(()) => {}
            Err((reason, window_gone)) => {
                if window_gone {
                    clear(app, slot);
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
        // Lazy restore of a persisted identity before giving up.
        if get_slot(slot).is_none() && resolve_saved(app, slot) {
            emit_changed(app);
        }
        let target = match get_slot(slot) {
            Some(t) => t,
            None => {
                return BeginDelivery::Failed {
                    reason: format!("jump slot {slot} is not set"),
                };
            }
        };
        if let Err((reason, window_gone)) = validate(&target) {
            if window_gone {
                clear(app, slot);
            }
            return BeginDelivery::Failed { reason };
        }
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
            slot,
        })
    }

    pub fn finish_delivery(
        app: &AppHandle,
        guard: DeliveryGuard,
        delivered_ok: bool,
        hot_recaptured: bool,
    ) {
        let settings = crate::settings::get_settings(app);
        // Keep/return-focus are PER-SLOT options: the hot slot uses the
        // legacy anchor_keep/anchor_return_focus pair (default one-shot),
        // static slots their jumper_slot_* entries (default durable). A
        // static slot with keep=off becomes one-shot like the hot anchor.
        let (slot_keep, slot_return_focus) = if guard.slot == HOT {
            (settings.anchor_keep, settings.anchor_return_focus)
        } else {
            let i = guard.slot - 1;
            (
                settings.jumper_slot_keep.get(i).copied().unwrap_or(true),
                settings
                    .jumper_slot_return_focus
                    .get(i)
                    .copied()
                    .unwrap_or(true),
            )
        };
        // The clear is skipped when the paste itself failed (keep the target
        // for a retry) and when track-last-output just re-captured HOT
        // (tracking wins — clearing would erase the fresher capture). A
        // one-shot clear removes the persisted identity too.
        if delivered_ok && !slot_keep && !(guard.slot == HOT && hot_recaptured) {
            if let Ok(mut slots) = SLOTS.lock() {
                slots[guard.slot] = None;
            }
            persist_slot(app, guard.slot, None);
            emit_changed(app);
        }
        if delivered_ok {
            let _ = app.emit("anchor-delivered", guard.slot as u32);
        }
        if slot_return_focus {
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
}

// ---------------------------------------------------------------------------
// Cross-platform surface
// ---------------------------------------------------------------------------

/// One-shot opt-in for anchored delivery: the slot index whose target the next
/// paste should deliver into, or -1 for none. Pastes NEVER involve the Jumper
/// unless the pressed flow's configured action armed this for the current take.
static DELIVERY_SLOT: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(-1);

pub fn request_delivery(slot: usize) {
    if slot < SLOT_COUNT {
        DELIVERY_SLOT.store(slot as isize, std::sync::atomic::Ordering::SeqCst);
    }
}

pub fn clear_delivery_request() {
    DELIVERY_SLOT.store(-1, std::sync::atomic::Ordering::SeqCst);
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

static POST_TAKE_ACTION: std::sync::Mutex<Option<(PostTakeAction, usize)>> =
    std::sync::Mutex::new(None);

pub fn arm_post_take_action(action: PostTakeAction, slot: usize) {
    if slot < SLOT_COUNT {
        if let Ok(mut guard) = POST_TAKE_ACTION.lock() {
            *guard = Some((action, slot));
        }
    }
}

pub fn take_post_take_action() -> Option<(PostTakeAction, usize)> {
    POST_TAKE_ACTION.lock().ok().and_then(|mut g| g.take())
}

pub fn clear_post_take_action() {
    if let Ok(mut guard) = POST_TAKE_ACTION.lock() {
        *guard = None;
    }
}

/// Execute a take's deferred on-finish action (after its paste completed).
pub fn run_post_take_action(app: &AppHandle, action: Option<(PostTakeAction, usize)>) {
    match action {
        Some((PostTakeAction::Set, slot)) => {
            if let Err(e) = set_slot(app, slot) {
                log::warn!("on-finish set failed: {}", e);
            }
        }
        Some((PostTakeAction::Clear, slot)) => clear(app, slot),
        None => {}
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

pub fn begin_delivery(app: &AppHandle) -> BeginDelivery {
    // Consume the one-shot request; without it, this paste ignores the Jumper.
    let slot = DELIVERY_SLOT.swap(-1, std::sync::atomic::Ordering::SeqCst);
    if slot < 0 {
        return BeginDelivery::NoAnchor;
    }
    #[cfg(windows)]
    {
        win::begin_delivery(app, slot as usize)
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        BeginDelivery::NoAnchor
    }
}

pub fn finish_delivery(
    app: &AppHandle,
    guard: DeliveryGuard,
    delivered_ok: bool,
    hot_recaptured: bool,
) {
    #[cfg(windows)]
    {
        win::finish_delivery(app, guard, delivered_ok, hot_recaptured);
    }
    #[cfg(not(windows))]
    {
        let _ = (app, guard, delivered_ok, hot_recaptured);
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
