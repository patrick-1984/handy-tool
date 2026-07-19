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

/// Serializes every clipboard WRITE with the generation bump that guards it
/// (T-103, finding 3). Without this, the delayed restore thread could read
/// `RESTORE_GEN`, find it still matches its own generation, and then — in the
/// gap before its own `write_text` call — lose a race to a `park_text`/paste
/// that bumps the generation and writes AFTER the restore's check but BEFORE
/// its write, so the restore's write (now stale) lands last and clobbers the
/// just-parked text. Held ONLY around the bump+write pair itself — never
/// across the paste keystroke, any sleep, or other blocking work.
static CLIPBOARD_WRITE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

static SUBMIT_OVERRIDE: Lazy<Mutex<Option<SubmitOverride>>> = Lazy::new(|| Mutex::new(None));

/// Sentinel error returned internally when a per-keystroke anchor re-check
/// (T-103, finding 1) finds the delivery target no longer foreground/focused
/// mid-paste. Distinct from an ordinary paste failure so `paste_inner` routes
/// it through the SAME fail-closed park path as a `begin_delivery`/TOCTOU
/// verification failure — never the generic paste-failure toast. Never
/// surfaced to the user as-is.
const ANCHOR_FOCUS_LOST: &str = "__anchor_focus_lost_mid_paste__";

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
    let _write_lock = CLIPBOARD_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    RESTORE_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    app.clipboard().write_text(text).is_ok()
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
) -> Result<(), String> {
    let clipboard = app_handle.clipboard();
    let clipboard_content = clipboard.read_text().unwrap_or_default();

    // Supersede any pending delayed restore from a previous paste, and place
    // OUR text on the clipboard, atomically w.r.t. the restore thread below
    // (T-103, finding 3): the restore thread re-checks the generation INSIDE
    // the SAME `CLIPBOARD_WRITE_LOCK` right before its own write, so it can
    // never land between our bump and our write and clobber this paste's text.
    let generation = {
        let _write_lock = CLIPBOARD_WRITE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let generation = RESTORE_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;

        // Write text to clipboard first
        // On Wayland, prefer wl-copy for better compatibility (especially with umlauts)
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
        generation
    };

    std::thread::sleep(Duration::from_millis(paste_delay_ms));

    // T-103 (finding 1): re-verify the anchor immediately before the
    // synthesized keystroke below — a focus change since `begin_delivery`'s
    // own check (or since the last re-check) must abort rather than paste
    // blind. No-op (`anchor_guard` is `None`) for every non-anchored paste.
    if let Some(guard) = anchor_guard {
        if !crate::anchor::guard_still_foreground(guard) {
            return Err(ANCHOR_FOCUS_LOST.to_string());
        }
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

    // Restore the original clipboard content after a delay, off this thread.
    // Remote sessions (Citrix/RDP) fetch clipboard data on demand AFTER the
    // paste keystroke lands in the remote app; restoring too early hands them
    // the old content. The generation is re-checked INSIDE the SAME
    // `CLIPBOARD_WRITE_LOCK` as the write itself (T-103, finding 3) — a
    // check-then-write without the lock left a gap where a park/paste could
    // bump the generation and write AFTER this check but BEFORE this write,
    // so this (now-stale) restore would land last and clobber it.
    if let Some(delay_ms) = restore_after_ms {
        let app = app_handle.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(delay_ms));
            let _write_lock = CLIPBOARD_WRITE_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if !should_restore(
                generation,
                RESTORE_GEN.load(std::sync::atomic::Ordering::SeqCst),
            ) {
                return;
            }
            // On Wayland, prefer wl-copy for better compatibility
            #[cfg(target_os = "linux")]
            if is_wayland() && is_wl_copy_available() {
                let _ = write_clipboard_via_wl_copy(&clipboard_content);
                return;
            }
            let _ = app.clipboard().write_text(&clipboard_content);
        });
    }

    Ok(())
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
    #[cfg(target_os = "linux")] typing_tool: TypingTool,
) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        if try_direct_typing_linux(text, typing_tool)? {
            return Ok(());
        }
        info!("Falling back to enigo for direct text input");
    }

    input::paste_text_direct(enigo, text)
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
    )
}

fn paste_inner(
    text: String,
    app_handle: AppHandle,
    is_ptt: bool,
    flow_paste: bool,
    delivery_intent: crate::anchor::DeliveryIntent,
    submit_override: Option<SubmitOverride>,
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
    let clipboard_handling = submit_override
        .and_then(|o| o.clipboard)
        .unwrap_or(settings.clipboard_handling);
    let paste_method = match forced_submit {
        Some((method, _)) => method,
        None if is_ptt => settings.paste_method_ptt,
        None => settings.paste_method,
    };
    let paste_delay_ms = settings.paste_delay_ms;
    // Base 50 ms settle + user-configured extra (per-shortcut override or global).
    // CopyToClipboard leaves the transcription as the final clipboard state, so
    // there is nothing to restore.
    let restore_extra_ms = submit_override
        .map(|o| o.restore_extra_ms)
        .unwrap_or_else(|| settings.clipboard_restore_delay.to_ms());
    let restore_after_ms = if clipboard_handling == ClipboardHandling::CopyToClipboard {
        None
    } else {
        Some(50 + restore_extra_ms)
    };

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
        "Using paste method: {:?}, delay: {}ms, text: {} chars",
        paste_method,
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

    // Park `text` on the clipboard as the anchored-delivery fail-closed path:
    // supersede any pending delayed restore first (so it can't overwrite the
    // parked text), write, and emit `anchor-delivery-failed`. Shared by a
    // verification failure at `begin_delivery` and the T-103 TOCTOU re-check
    // right before the keystroke — both must behave identically.
    #[cfg(windows)]
    let park_anchor_failure = |reason: String| {
        // Bump-and-write under CLIPBOARD_WRITE_LOCK (T-103, finding 3): the
        // SAME serialization `park_text`/`paste_via_clipboard`'s restore use,
        // so a delayed restore can never land between this bump and this
        // write and clobber the parked text.
        let reason = {
            let _write_lock = CLIPBOARD_WRITE_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            RESTORE_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            match app_handle.clipboard().write_text(&text) {
                Ok(()) => reason,
                Err(e) => {
                    // Don't claim the text is on the clipboard when it isn't.
                    log::warn!("Could not park transcription on clipboard: {}", e);
                    format!("{reason}; clipboard also unavailable ({e})")
                }
            }
        };
        let _ = app_handle.emit("anchor-delivery-failed", reason);
    };

    // Anchored delivery: activate + focus the captured target BEFORE any
    // keystroke, with verification — never paste blind into a surprise
    // location. On failure the text is parked on the clipboard instead
    // (superseding any pending delayed restore) and the anchor is kept for a
    // retry (or cleared if the window is gone). Non-flow pastes (MCP/CLI)
    // never touch delivery — `delivery_intent` is `DeliveryIntent::NONE` by
    // construction for them.
    #[cfg(windows)]
    let mut anchor_guard = if flow_paste && paste_method != PasteMethod::None {
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

    // TOCTOU close (T-103): `begin_delivery` verified activation/focus, then
    // settled 60ms before returning — a focus change in that gap (a popup
    // stealing focus, the user clicking elsewhere) must not receive a blind
    // paste. Re-check immediately before the paste keystroke and route a
    // mismatch to the EXACT same fail-closed park path as a verification
    // failure. One cheap syscall — no measurable latency on a normal
    // delivery.
    #[cfg(windows)]
    {
        let focus_lost_before_paste = anchor_guard
            .as_ref()
            .map(|guard| !crate::anchor::guard_still_foreground(guard))
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
        // Normal path: honor the global auto-submit setting.
        None => (
            should_send_auto_submit(settings.auto_submit, paste_method),
            settings.auto_submit_key,
        ),
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
                // On Linux, try native direct typing tools first (wtype, dotool, etc.)
                #[cfg(target_os = "linux")]
                {
                    paste_direct(&mut enigo, &text, settings.typing_tool)?;
                }
                // On Windows/macOS, use clipboard paste to avoid character-by-character flicker
                #[cfg(not(target_os = "linux"))]
                {
                    paste_via_clipboard(
                        &mut enigo,
                        &text,
                        &app_handle,
                        &PasteMethod::CtrlV,
                        paste_delay_ms,
                        restore_after_ms,
                        anchor_guard_ref,
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
            std::thread::sleep(Duration::from_millis(50));
            // T-103 (finding 1): re-verify immediately before the auto-submit
            // keystroke too — the 50ms settle above is exactly the kind of
            // gap a focus-stealing popup can land in after the paste itself
            // already succeeded.
            if let Some(guard) = anchor_guard_ref {
                if !crate::anchor::guard_still_foreground(guard) {
                    return Err(ANCHOR_FOCUS_LOST.to_string());
                }
            }
            send_return_key(&mut enigo, submit_key)?;
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
        if e == ANCHOR_FOCUS_LOST {
            let reason = "focus changed mid-paste (per-keystroke re-check)".to_string();
            info!(
                "Anchored delivery aborted: {} — parking text on clipboard",
                reason
            );
            park_anchor_failure(reason);
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

    // Track-last-output: ONE global switch for both flows — capture where the
    // text just landed into the configured slot, BEFORE any focus-return so
    // the slot points at the paste target. Unlike the deferred on-finish
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
    if flow_paste
        && delivered.is_ok()
        && paste_method != PasteMethod::None
        && settings.jumper_track_enabled
    {
        let slot = (settings.jumper_track_slot as usize).min(crate::anchor::SLOT_COUNT - 1);
        let result = match anchor_guard.as_ref() {
            Some(guard) => crate::anchor::track_from_guard(&app_handle, guard, slot),
            None => crate::anchor::set_slot(&app_handle, slot),
        };
        if let Err(e) = result {
            log::debug!("track-last-output capture skipped: {}", e);
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

    delivered?;

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
    if clipboard_handling == ClipboardHandling::CopyToClipboard && !park_text(&app_handle, &text) {
        return Err("Failed to copy to clipboard".to_string());
    }

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
