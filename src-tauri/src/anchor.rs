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
/// volatile and can carry sensitive document names).
#[derive(Clone, Serialize, Type)]
pub struct AnchorStatus {
    pub app: String,
    pub control_class: String,
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
    use windows::Win32::Foundation::{CloseHandle, HWND};
    use windows::Win32::System::Threading::{
        AttachThreadInput, GetCurrentThreadId, OpenProcess, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows::Win32::UI::WindowsAndMessaging::{
        GUITHREADINFO, GWL_STYLE, GetClassNameW, GetForegroundWindow, GetGUIThreadInfo,
        GetWindowLongW, GetWindowThreadProcessId, IsIconic, IsWindow, SW_RESTORE,
        SetForegroundWindow, ShowWindow,
    };

    /// Edit-control style: password box. Defined locally to avoid feature churn.
    const ES_PASSWORD: i32 = 0x0020;

    #[derive(Clone)]
    struct Target {
        hwnd: isize,
        control: isize,
        pid: u32,
        tid: u32,
        control_class: String,
        app: String,
    }

    static SLOTS: Lazy<Mutex<[Option<Target>; SLOT_COUNT]>> =
        Lazy::new(|| Mutex::new([None, None, None, None, None]));

    fn emit_changed(app: &AppHandle) {
        let _ = app.emit("anchor-changed", statuses());
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

    pub fn statuses() -> Vec<Option<AnchorStatus>> {
        SLOTS
            .lock()
            .map(|slots| {
                slots
                    .iter()
                    .map(|t| {
                        t.as_ref().map(|t| AnchorStatus {
                            app: t.app.clone(),
                            control_class: t.control_class.clone(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_else(|_| vec![None; SLOT_COUNT])
    }

    pub fn clear(app: &AppHandle, slot: usize) {
        if slot >= SLOT_COUNT {
            return;
        }
        if let Ok(mut slots) = SLOTS.lock() {
            slots[slot] = None;
        }
        emit_changed(app);
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
                control_class: control_class.clone(),
                app: app_name.clone(),
            };
            info!(
                "Jump slot {} set: {} (class '{}', pid {}, tid {})",
                slot, app_name, control_class, pid, tid
            );
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
            // owning thread and the same class; a recycled handle with a
            // different class must not receive keystrokes. Static slots make
            // this likely enough to matter.
            let control = HWND(t.control as _);
            if !IsWindow(Some(control)).as_bool() {
                return Err(("target field no longer exists".into(), false));
            }
            let mut cpid = 0u32;
            let ctid = GetWindowThreadProcessId(control, Some(&mut cpid));
            if cpid != t.pid || ctid != t.tid {
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
    fn activate_verified(hwnd: HWND) -> Result<(), String> {
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(hwnd);
            for _ in 0..20 {
                if GetForegroundWindow() == hwnd {
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err("Windows refused to bring the target window to the foreground".into())
        }
    }

    /// Focus the target control and verify via GetGUIThreadInfo. Scoped
    /// thread-input attachment; always detached.
    fn focus_control_verified(tid: u32, control: HWND) -> Result<(), String> {
        unsafe {
            let me = GetCurrentThreadId();
            let attached = AttachThreadInput(me, tid, true).as_bool();
            let _ = SetFocus(Some(control));
            if attached {
                let _ = AttachThreadInput(me, tid, false);
            }
            let mut gti = GUITHREADINFO {
                cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            if GetGUIThreadInfo(tid, &mut gti).is_ok() && gti.hwndFocus == control {
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
        let _ = focus_control_verified(target.tid, HWND(target.control as _));
        debug!("Jumped to slot {}: {}", slot, target.app);
        Ok(())
    }

    pub fn begin_delivery(app: &AppHandle, slot: usize) -> BeginDelivery {
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
        if let Err(reason) = focus_control_verified(target.tid, HWND(target.control as _)) {
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
        // One-shot semantics apply to the HOT slot only — static slots are
        // durable bookmarks and survive deliveries. The clear is skipped when
        // the paste itself failed (keep the target for a retry) and when
        // track-last-output just re-captured HOT (tracking wins — clearing
        // would erase the fresher capture).
        if delivered_ok && !hot_recaptured && guard.slot == HOT && !settings.anchor_keep {
            if let Ok(mut slots) = SLOTS.lock() {
                slots[HOT] = None;
            }
            emit_changed(app);
        }
        if delivered_ok {
            let _ = app.emit("anchor-delivered", guard.slot as u32);
        }
        if settings.anchor_return_focus {
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
pub fn get_jump_slots() -> Vec<Option<AnchorStatus>> {
    #[cfg(windows)]
    {
        win::statuses()
    }
    #[cfg(not(windows))]
    {
        vec![None; SLOT_COUNT]
    }
}

/// Hot-slot status (legacy surface kept for the settings chip).
#[tauri::command]
#[specta::specta]
pub fn get_anchor_status() -> Option<AnchorStatus> {
    get_jump_slots().into_iter().next().flatten()
}

/// Explicit clear from the UI.
#[tauri::command]
#[specta::specta]
pub fn clear_jump_slot(app: AppHandle, slot: u32) {
    clear(&app, slot as usize);
}

/// Legacy hot-slot clear (kept for existing UI wiring).
#[tauri::command]
#[specta::specta]
pub fn clear_anchor(app: AppHandle) {
    clear(&app, HOT);
}
