use enigo::{Enigo, Key, Keyboard, Settings};
use std::sync::Mutex;

/// Wrapper for Enigo to store in Tauri's managed state.
/// Enigo is wrapped in a Mutex since it requires mutable access.
pub struct EnigoState(pub Mutex<Enigo>);

impl EnigoState {
    pub fn new() -> Result<Self, String> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| format!("Failed to initialize Enigo: {}", e))?;
        Ok(Self(Mutex::new(enigo)))
    }
}

/// Wait until all keyboard modifier keys (Ctrl / Alt / Shift / Win) are
/// PHYSICALLY released, so a synthesized keystroke isn't polluted by the
/// modifiers of the global shortcut that triggered it. A shortcut fires on
/// key-PRESS while its modifiers are still held, so injecting a paste chord
/// immediately would send e.g. `Ctrl+Alt+V` instead of `Ctrl+V` (or
/// `Ctrl+Alt+Shift+Insert` instead of `Shift+Insert`) and the target ignores
/// it. Polls the real key state every ~10 ms and requires a short continuously-
/// clear grace; bounded by `timeout_ms` (best-effort — returns after the
/// timeout even if a key is somehow still held, rather than never pasting).
/// Call this OFF the UI/event-loop thread (it sleeps). On non-Windows there is
/// no cheap physical-state API wired here, so it uses a fixed grace like the
/// Keyboard Typer.
pub fn wait_for_modifiers_released(timeout_ms: u64) {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
        };
        // VK_CONTROL/VK_MENU/VK_SHIFT are the generic (either-side) modifiers;
        // add both Win keys. High bit of GetAsyncKeyState = key currently down.
        let vks: [i32; 5] = [
            VK_CONTROL.0 as i32,
            VK_MENU.0 as i32,
            VK_SHIFT.0 as i32,
            VK_LWIN.0 as i32,
            VK_RWIN.0 as i32,
        ];
        let any_down = || {
            vks.iter()
                .any(|&vk| (unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000) != 0)
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        let mut clear_since: Option<std::time::Instant> = None;
        loop {
            let now = std::time::Instant::now();
            if any_down() {
                clear_since = None;
            } else {
                match clear_since {
                    Some(t) if now.duration_since(t) >= std::time::Duration::from_millis(10) => {
                        break;
                    }
                    Some(_) => {}
                    None => clear_since = Some(now),
                }
            }
            if now >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    #[cfg(not(windows))]
    {
        std::thread::sleep(std::time::Duration::from_millis(timeout_ms.min(300)));
    }
}

/// Synthetically release any still-held keyboard modifiers so a paste fired the
/// instant the shortcut key is released isn't polluted by a modifier the user
/// is still holding (e.g. they let go of `O` but keep Ctrl+Alt down). The
/// shortcut's own NON-modifier key is already up by the time this runs (it's
/// what fired the Released event), and modifier keys do NOT auto-repeat, so
/// after this key-UP the system treats them as released until the physical
/// release (a harmless extra up). Releases BOTH left and right of each modifier
/// explicitly (a held right Alt/AltGr or right Ctrl/Shift must be cleared too —
/// the generic VKs map to left scan codes and would miss the right side), with
/// the extended-key flag for right Ctrl/right Alt. Best-effort; no-op on
/// non-Windows (that path uses a fixed grace instead).
#[cfg(windows)]
pub fn force_release_modifiers() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
        SendInput, VIRTUAL_KEY, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_RCONTROL, VK_RMENU,
        VK_RSHIFT, VK_RWIN,
    };
    // (vk, is_extended) — right Ctrl and right Alt are extended keys.
    let keys: [(VIRTUAL_KEY, bool); 8] = [
        (VK_LCONTROL, false),
        (VK_RCONTROL, true),
        (VK_LMENU, false),
        (VK_RMENU, true),
        (VK_LSHIFT, false),
        (VK_RSHIFT, false),
        (VK_LWIN, false),
        (VK_RWIN, false),
    ];
    let inputs: Vec<INPUT> = keys
        .iter()
        .map(|&(vk, ext)| {
            let mut flags = KEYEVENTF_KEYUP;
            if ext {
                flags |= KEYEVENTF_EXTENDEDKEY;
            }
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk,
                        wScan: 0,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            }
        })
        .collect();
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        log::warn!(
            "force_release_modifiers: SendInput inserted {}/{} events",
            sent,
            inputs.len()
        );
    }
}

#[cfg(not(windows))]
pub fn force_release_modifiers() {}

/// Wait until the shortcut's PRIMARY (non-modifier) trigger key is physically
/// up (bounded, best-effort). "Paste Last" fires on the Released event and must
/// not inject while that key is still held — its typematic repeats would leak
/// into the target. The Tauri backend only emits Released after the primary key
/// is already up (so this returns immediately), but the HandyKeys backend can
/// emit Released on a modifier change with the key still down — this makes
/// Paste Last correct on both.
///
/// `primary_vk` is the known VK of the trigger key when we could map it;
/// `None` means we couldn't identify it (an exotic key name), and we
/// conservatively wait until NO non-modifier key at all is down — so the guard
/// holds for ANY primary key without a per-key lookup table. Call OFF the UI
/// thread. No-op on non-Windows.
#[cfg(windows)]
pub fn wait_for_key_released(primary_vk: Option<i32>, timeout_ms: u64) {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    let is_down = |vk: i32| (unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000) != 0;
    // Modifier VKs (generic + left/right + Win) — excluded from the "any
    // non-modifier key down" scan; modifiers are handled separately by
    // force_release_modifiers + wait_for_modifiers_released.
    let is_modifier = |vk: i32| {
        matches!(
            vk,
            0x10 | 0x11 | 0x12 | 0xA0 | 0xA1 | 0xA2 | 0xA3 | 0xA4 | 0xA5 | 0x5B | 0x5C
        )
    };
    let still_held = || match primary_vk {
        Some(vk) => is_down(vk),
        None => (0x08..=0xFEu32).any(|vk| {
            let vk = vk as i32;
            !is_modifier(vk) && is_down(vk)
        }),
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let mut clear_since: Option<std::time::Instant> = None;
    loop {
        let now = std::time::Instant::now();
        if still_held() {
            clear_since = None;
        } else {
            match clear_since {
                Some(t) if now.duration_since(t) >= std::time::Duration::from_millis(10) => break,
                Some(_) => {}
                None => clear_since = Some(now),
            }
        }
        if now >= deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[cfg(not(windows))]
pub fn wait_for_key_released(_primary_vk: Option<i32>, _timeout_ms: u64) {}

/// Sends a Ctrl+V or Cmd+V paste command using platform-specific virtual key codes.
/// This ensures the paste works regardless of keyboard layout (e.g., Russian, AZERTY, DVORAK).
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
pub fn send_paste_ctrl_v(enigo: &mut Enigo) -> Result<(), String> {
    // Platform-specific key definitions
    #[cfg(target_os = "macos")]
    let (modifier_key, v_key_code) = (Key::Meta, Key::Other(9));
    #[cfg(target_os = "windows")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Other(0x56)); // VK_V
    #[cfg(target_os = "linux")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Unicode('v'));

    // Press modifier + V
    enigo
        .key(modifier_key, enigo::Direction::Press)
        .map_err(|e| format!("Failed to press modifier key: {}", e))?;
    enigo
        .key(v_key_code, enigo::Direction::Click)
        .map_err(|e| format!("Failed to click V key: {}", e))?;

    std::thread::sleep(std::time::Duration::from_millis(100));

    enigo
        .key(modifier_key, enigo::Direction::Release)
        .map_err(|e| format!("Failed to release modifier key: {}", e))?;

    Ok(())
}

/// Sends a Ctrl+Shift+V paste command.
/// This is commonly used in terminal applications on Linux to paste without formatting.
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
pub fn send_paste_ctrl_shift_v(enigo: &mut Enigo) -> Result<(), String> {
    // Platform-specific key definitions
    #[cfg(target_os = "macos")]
    let (modifier_key, v_key_code) = (Key::Meta, Key::Other(9)); // Cmd+Shift+V on macOS
    #[cfg(target_os = "windows")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Other(0x56)); // VK_V
    #[cfg(target_os = "linux")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Unicode('v'));

    // Press Ctrl/Cmd + Shift + V
    enigo
        .key(modifier_key, enigo::Direction::Press)
        .map_err(|e| format!("Failed to press modifier key: {}", e))?;
    enigo
        .key(Key::Shift, enigo::Direction::Press)
        .map_err(|e| format!("Failed to press Shift key: {}", e))?;
    enigo
        .key(v_key_code, enigo::Direction::Click)
        .map_err(|e| format!("Failed to click V key: {}", e))?;

    std::thread::sleep(std::time::Duration::from_millis(100));

    enigo
        .key(Key::Shift, enigo::Direction::Release)
        .map_err(|e| format!("Failed to release Shift key: {}", e))?;
    enigo
        .key(modifier_key, enigo::Direction::Release)
        .map_err(|e| format!("Failed to release modifier key: {}", e))?;

    Ok(())
}

/// Sends a Shift+Insert paste command (Windows and Linux only).
/// This is more universal for terminal applications and legacy software.
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
pub fn send_paste_shift_insert(enigo: &mut Enigo) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let insert_key_code = Key::Other(0x2D); // VK_INSERT
    #[cfg(not(target_os = "windows"))]
    let insert_key_code = Key::Other(0x76); // XK_Insert (keycode 118 / 0x76, also used as fallback)

    // Press Shift + Insert
    enigo
        .key(Key::Shift, enigo::Direction::Press)
        .map_err(|e| format!("Failed to press Shift key: {}", e))?;
    enigo
        .key(insert_key_code, enigo::Direction::Click)
        .map_err(|e| format!("Failed to click Insert key: {}", e))?;

    std::thread::sleep(std::time::Duration::from_millis(100));

    enigo
        .key(Key::Shift, enigo::Direction::Release)
        .map_err(|e| format!("Failed to release Shift key: {}", e))?;

    Ok(())
}

/// Re-verify, immediately before each injected batch or key, that delivery may
/// still proceed. Chunking made this mandatory: the caller's single pre-flight
/// check no longer covers the whole delivery, because ordinary single-line text
/// is now split into batches with sleeps between them, and focus can move in
/// any of those gaps. Returning `false` aborts fail-closed.
pub type DeliveryStillValid<'a> = &'a dyn Fn() -> bool;

/// Error string raised when `still_valid` refuses BEFORE any text was injected.
/// `clipboard.rs` aliases this as `ANCHOR_FOCUS_LOST` and routes it through its
/// fail-closed park path rather than the generic paste-failure toast.
pub const DELIVERY_ABORTED: &str = "__anchor_focus_lost_mid_paste__";

/// As [`DELIVERY_ABORTED`], but at least one batch had ALREADY been typed into
/// the target when the refusal happened.
///
/// Typing cannot be atomic the way a single paste chord is, so this outcome is
/// unavoidable -- but it must not be reported as if nothing was delivered. The
/// recovery advice differs: re-pasting the full transcript would duplicate the
/// prefix that already landed.
pub const DELIVERY_ABORTED_PARTIAL: &str = "__anchor_focus_lost_mid_paste_partial__";

/// Type text directly as simulated keystrokes — the ONLY delivery path that
/// never reads or writes the clipboard.
///
/// Ordinary text is sent in BATCHED runs via `enigo.text()`, which on Windows
/// compiles to a single `SendInput` carrying the whole run (and uses
/// `KEYEVENTF_UNICODE`, so the active keyboard layout is bypassed entirely —
/// accented characters on European layouts are unaffected). This is not the
/// character-by-character loop in `typing.rs`, which is deliberately slow for
/// password prompts.
///
/// Line breaks and tabs must NOT be handed to `enigo.text()`: it queues a
/// Return/Tab click AND then also emits the raw `\n`/`\t`, which double-breaks
/// multi-line transcripts. They are sent as explicit key clicks instead, and a
/// `\r` is dropped because the following `\n` already covers the break (the
/// same rule `typing.rs` applies).
///
/// CHUNKING: one `SendInput` carrying a whole transcript is instant locally but
/// is exactly the "instant injection" that drops characters in remote desktop
/// sessions and some VM consoles. `chunk_chars` splits the run into batches with
/// `chunk_delay_ms` between them, giving the far end time to drain its input
/// queue. At the defaults (40 chars / 15 ms) a 500-character transcript takes
/// roughly 200 ms — far quicker than the 15 ms-per-character Keyboard Typer,
/// and paced enough for RDP. `chunk_delay_ms == 0` restores the single-burst
/// behaviour; `chunk_chars == 0` means "never split".
///
/// `still_valid` is consulted before every injected batch and before every
/// Return/Tab. It aborts with the caller's fail-closed sentinel so a transcript
/// cannot keep typing into a window that stole focus mid-delivery.
pub fn paste_text_direct(
    enigo: &mut Enigo,
    text: &str,
    chunk_chars: usize,
    chunk_delay_ms: u64,
    still_valid: Option<DeliveryStillValid<'_>>,
) -> Result<(), String> {
    let mut run = String::new();
    // Only pause BETWEEN batches, never before the first or after the last.
    let mut sent_a_batch = false;

    // `already_typed` distinguishes "nothing was delivered" from "the target
    // already has part of the transcript", which the caller needs in order to
    // give honest recovery advice.
    fn check(
        still_valid: Option<DeliveryStillValid<'_>>,
        already_typed: bool,
    ) -> Result<(), String> {
        match still_valid {
            Some(f) if !f() => Err(if already_typed {
                DELIVERY_ABORTED_PARTIAL.to_string()
            } else {
                DELIVERY_ABORTED.to_string()
            }),
            _ => Ok(()),
        }
    }

    fn send_batch(
        enigo: &mut Enigo,
        batch: &str,
        sent_a_batch: &mut bool,
        chunk_delay_ms: u64,
        still_valid: Option<DeliveryStillValid<'_>>,
    ) -> Result<(), String> {
        if batch.is_empty() {
            return Ok(());
        }
        if *sent_a_batch && chunk_delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(chunk_delay_ms));
        }
        // AFTER the sleep and immediately BEFORE injecting: the sleep is
        // exactly the window in which focus can move.
        check(still_valid, *sent_a_batch)?;
        enigo
            .text(batch)
            .map_err(|e| format!("Failed to send text directly: {}", e))?;
        *sent_a_batch = true;
        Ok(())
    }

    // Flush the accumulated ordinary-text run, split into chunk_chars batches.
    // Splitting is by CHARACTER, never by byte, so a multi-byte character can
    // never be torn in half. Surrogate pairs are formed inside enigo from a
    // whole `char`, so a chunk boundary cannot split one either.
    fn flush(
        enigo: &mut Enigo,
        run: &mut String,
        chunk_chars: usize,
        chunk_delay_ms: u64,
        sent_a_batch: &mut bool,
        still_valid: Option<DeliveryStillValid<'_>>,
    ) -> Result<(), String> {
        if run.is_empty() {
            return Ok(());
        }
        if chunk_chars == 0 {
            let r = send_batch(enigo, run, sent_a_batch, chunk_delay_ms, still_valid);
            run.clear();
            return r;
        }
        // `chunk_chars` is bounded by settings::typing_chunk_params before it
        // reaches here, so this allocation cannot be made pathological by a
        // hand-edited settings file.
        let mut batch = String::with_capacity(chunk_chars);
        let mut count = 0usize;
        for ch in run.chars() {
            batch.push(ch);
            count += 1;
            if count >= chunk_chars {
                send_batch(enigo, &batch, sent_a_batch, chunk_delay_ms, still_valid)?;
                batch.clear();
                count = 0;
            }
        }
        let r = send_batch(enigo, &batch, sent_a_batch, chunk_delay_ms, still_valid);
        run.clear();
        r
    }

    // Peekable so a lone CR can be told apart from a CRLF pair. Dropping every
    // '\r' unconditionally silently swallowed classic-Mac / stray-CR line
    // breaks, turning two lines into one.
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            // CRLF: consume the '\n' here so the pair yields exactly one break.
            // A LONE '\r' is a real line break and must still produce one.
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                flush(
                    enigo,
                    &mut run,
                    chunk_chars,
                    chunk_delay_ms,
                    &mut sent_a_batch,
                    still_valid,
                )?;
                check(still_valid, sent_a_batch)?;
                enigo
                    .key(Key::Return, enigo::Direction::Click)
                    .map_err(|e| format!("Failed to send Return: {}", e))?;
            }
            // enigo returns Err on a NUL; it cannot be typed. Drop it rather
            // than failing the whole delivery.
            '\0' => {}
            '\n' => {
                flush(
                    enigo,
                    &mut run,
                    chunk_chars,
                    chunk_delay_ms,
                    &mut sent_a_batch,
                    still_valid,
                )?;
                check(still_valid, sent_a_batch)?;
                enigo
                    .key(Key::Return, enigo::Direction::Click)
                    .map_err(|e| format!("Failed to send Return: {}", e))?;
            }
            '\t' => {
                flush(
                    enigo,
                    &mut run,
                    chunk_chars,
                    chunk_delay_ms,
                    &mut sent_a_batch,
                    still_valid,
                )?;
                check(still_valid, sent_a_batch)?;
                enigo
                    .key(Key::Tab, enigo::Direction::Click)
                    .map_err(|e| format!("Failed to send Tab: {}", e))?;
            }
            _ => run.push(ch),
        }
    }

    flush(
        enigo,
        &mut run,
        chunk_chars,
        chunk_delay_ms,
        &mut sent_a_batch,
        still_valid,
    )
}

/// Split a run into batches exactly as [`paste_text_direct`] does, so the
/// chunking rule is unit-testable without simulating any input.
#[cfg(test)]
pub(crate) fn chunk_run(run: &str, chunk_chars: usize) -> Vec<String> {
    if run.is_empty() {
        return Vec::new();
    }
    if chunk_chars == 0 {
        return vec![run.to_string()];
    }
    let mut out = Vec::new();
    let mut batch = String::new();
    for ch in run.chars() {
        batch.push(ch);
        if batch.chars().count() >= chunk_chars {
            out.push(std::mem::take(&mut batch));
        }
    }
    if !batch.is_empty() {
        out.push(batch);
    }
    out
}

#[cfg(test)]
mod direct_typing_tests {
    use super::chunk_run;

    #[test]
    fn empty_run_produces_no_batches() {
        assert!(chunk_run("", 40).is_empty());
    }

    #[test]
    fn run_shorter_than_a_chunk_is_one_batch() {
        assert_eq!(chunk_run("hello", 40), vec!["hello".to_string()]);
    }

    #[test]
    fn exact_multiple_does_not_emit_a_trailing_empty_batch() {
        // The classic off-by-one: 8 chars at chunk 4 must be 2 batches, not 3.
        assert_eq!(
            chunk_run("abcdefgh", 4),
            vec!["abcd".to_string(), "efgh".to_string()]
        );
    }

    #[test]
    fn remainder_becomes_a_final_short_batch() {
        assert_eq!(
            chunk_run("abcdefghi", 4),
            vec!["abcd".to_string(), "efgh".to_string(), "i".to_string()]
        );
    }

    #[test]
    fn chunking_is_by_character_so_multibyte_is_never_torn() {
        // Polish diacritics and an astral-plane emoji. Splitting by BYTE here
        // would corrupt them; splitting by char cannot.
        let s = "ąćęłń😀śźż";
        let batches = chunk_run(s, 3);
        assert_eq!(batches.concat(), s);
        assert_eq!(
            batches,
            vec!["ąćę".to_string(), "łń😀".to_string(), "śźż".to_string()]
        );
        // Every batch is independently valid UTF-8 with the expected char count.
        assert!(batches.iter().all(|b| b.chars().count() <= 3));
    }

    #[test]
    fn zero_chunk_size_means_one_unsplit_batch() {
        assert_eq!(chunk_run("abcdefgh", 0), vec!["abcdefgh".to_string()]);
    }
}
