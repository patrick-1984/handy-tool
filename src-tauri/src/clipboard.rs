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
/// a recording. Consumed (taken) by the next `paste()`.
///
/// - `submit`: when `Some`, force this paste method and always send the submit key
///   (independent of the global `auto_submit`).
/// - `clipboard`: when `Some`, override clipboard handling for this paste.
/// - `restore_extra_ms`: extra wait before restoring the original clipboard.
#[derive(Clone, Copy)]
pub struct SubmitOverride {
    pub submit: Option<(PasteMethod, AutoSubmitKey)>,
    pub clipboard: Option<ClipboardHandling>,
    pub restore_extra_ms: u64,
}

/// Monotonic paste generation. Every clipboard paste bumps it; a pending delayed
/// clipboard restore only fires if no newer paste has superseded it.
static RESTORE_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

static SUBMIT_OVERRIDE: Lazy<Mutex<Option<SubmitOverride>>> = Lazy::new(|| Mutex::new(None));

/// Arm the next paste with a submit/clipboard override.
pub fn set_submit_override(over: SubmitOverride) {
    if let Ok(mut guard) = SUBMIT_OVERRIDE.lock() {
        *guard = Some(over);
    }
}

/// Park text on the clipboard as the delivery of last resort. Bumps the paste
/// generation FIRST so a pending delayed clipboard-restore (from an earlier
/// paste) can never overwrite what was just parked. Returns whether the write
/// stuck.
pub fn park_text(app: &AppHandle, text: &str) -> bool {
    RESTORE_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    app.clipboard().write_text(text).is_ok()
}

/// Clear any armed submit override (called when a new recording starts or its
/// lifecycle ends, so a stale override can never leak into an unrelated paste).
pub fn clear_submit_override() {
    if let Ok(mut guard) = SUBMIT_OVERRIDE.lock() {
        *guard = None;
    }
}

fn take_submit_override() -> Option<SubmitOverride> {
    SUBMIT_OVERRIDE
        .lock()
        .ok()
        .and_then(|mut guard| guard.take())
}

/// Pastes text using the clipboard: saves current content, writes text, sends
/// the paste keystroke, then (when `restore_after_ms` is `Some`) restores the
/// original clipboard after that delay on a background thread.
fn paste_via_clipboard(
    enigo: &mut Enigo,
    text: &str,
    app_handle: &AppHandle,
    paste_method: &PasteMethod,
    paste_delay_ms: u64,
    restore_after_ms: Option<u64>,
) -> Result<(), String> {
    let clipboard = app_handle.clipboard();
    let clipboard_content = clipboard.read_text().unwrap_or_default();

    // Supersede any pending delayed restore from a previous paste — it must not
    // clobber the text we are about to place on the clipboard.
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

    std::thread::sleep(Duration::from_millis(paste_delay_ms));

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
    // the old content. The generation guard aborts this restore if a newer
    // paste supersedes it while we wait.
    if let Some(delay_ms) = restore_after_ms {
        let app = app_handle.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(delay_ms));
            if RESTORE_GEN.load(std::sync::atomic::Ordering::SeqCst) != generation {
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

/// Flow paste: consumes the one-shot submit override and anchored-delivery
/// request, and can auto-track the output location. Used by the transcription
/// flows only.
pub fn paste(text: String, app_handle: AppHandle, is_ptt: bool) -> Result<(), String> {
    paste_inner(text, app_handle, is_ptt, true)
}

/// Plain paste for non-flow callers (MCP/CLI `keyboard_type`): must NEVER
/// consume or observe flow one-shots — a stale submit override or delivery
/// request would redirect unrelated text into the submit pipeline or an
/// anchored window.
pub fn paste_plain(text: String, app_handle: AppHandle) -> Result<(), String> {
    paste_inner(text, app_handle, false, false)
}

fn paste_inner(
    text: String,
    app_handle: AppHandle,
    is_ptt: bool,
    flow_paste: bool,
) -> Result<(), String> {
    let settings = get_settings(&app_handle);
    let submit_override = if flow_paste {
        take_submit_override()
    } else {
        None
    };
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

    // Anchored delivery: activate + focus the captured target BEFORE any
    // keystroke, with verification — never paste blind into a surprise
    // location. On failure the text is parked on the clipboard instead
    // (superseding any pending delayed restore) and the anchor is kept for a
    // retry (or cleared if the window is gone). Non-flow pastes (MCP/CLI)
    // never touch the delivery request.
    #[cfg(windows)]
    let anchor_guard = if flow_paste && paste_method != PasteMethod::None {
        match crate::anchor::begin_delivery(&app_handle) {
            crate::anchor::BeginDelivery::NoAnchor => None,
            crate::anchor::BeginDelivery::Ready(guard) => Some(guard),
            crate::anchor::BeginDelivery::Failed { reason } => {
                info!(
                    "Anchored delivery failed: {} — parking text on clipboard",
                    reason
                );
                // Supersede any pending delayed restore BEFORE parking, so it
                // can't overwrite the parked text in the gap.
                RESTORE_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let reason = match app_handle.clipboard().write_text(&text) {
                    Ok(()) => reason,
                    Err(e) => {
                        // Don't claim the text is on the clipboard when it isn't.
                        log::warn!("Could not park transcription on clipboard: {}", e);
                        format!("{reason}; clipboard also unavailable ({e})")
                    }
                };
                let _ = app_handle.emit("anchor-delivery-failed", reason);
                return Ok(());
            }
        }
    } else {
        // Paste disabled: any delivery request or deferred on-finish action
        // armed for this take must die with it — stranded, they would hijack
        // the NEXT unrelated paste.
        if flow_paste {
            crate::anchor::clear_delivery_request();
            crate::anchor::clear_post_take_action();
        }
        None
    };

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
                )?
            }
            PasteMethod::ExternalScript => {
                let script_path = settings
                    .external_script_path
                    .as_ref()
                    .filter(|p| !p.is_empty())
                    .ok_or("External script path is not configured")?;
                paste_via_external_script(&text, script_path)?;
            }
        }

        if do_submit {
            std::thread::sleep(Duration::from_millis(50));
            send_return_key(&mut enigo, submit_key)?;
        }
        Ok(())
    })();

    // Track-last-output: ONE global switch for both flows — capture where the
    // text just landed into the configured slot, BEFORE any focus-return so
    // the slot points at the paste target.
    #[cfg(windows)]
    if flow_paste
        && delivered.is_ok()
        && paste_method != PasteMethod::None
        && settings.jumper_track_enabled
    {
        let slot = (settings.jumper_track_slot as usize).min(crate::anchor::SLOT_COUNT - 1);
        if let Err(e) = crate::anchor::set_slot(&app_handle, slot) {
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

    // After pasting, optionally copy to clipboard based on settings
    if clipboard_handling == ClipboardHandling::CopyToClipboard {
        let clipboard = app_handle.clipboard();
        clipboard
            .write_text(&text)
            .map_err(|e| format!("Failed to copy to clipboard: {}", e))?;
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
}
