//! Shortcut Keeper (Windows): keeps the shortcuts you choose on this PC while a
//! Remote Desktop session has the keyboard.
//!
//! Remote Desktop Connection (mstsc) and the Windows App (msrdc) install their
//! own low-level keyboard hook while their session window has focus (full
//! screen, or "Apply Windows key combinations: On the remote computer") and
//! swallow every key, so Handy's RegisterHotKey shortcuts never fire. Windows
//! calls the newest low-level hook first, so the only user-mode remedy is to
//! install ours after theirs, as accessibility tools and AutoHotkey do. That
//! is all this module does:
//!
//! - Off by default. Nothing is hooked until the user switches it on, and then
//!   only while a session's input window has keyboard focus.
//! - Every key passes through except a fresh, physical press of a checked
//!   shortcut, plus that key's repeats and release. Nothing is stored, logged
//!   or sent anywhere; the logs name binding ids only.
//! - Injected input (macros, remappers, Handy's own paste) passes through and
//!   is never acted on.
//! - Best effort: the client (or Windows, when the PC is busy) can put its
//!   hook in front of ours later. While a session has the keyboard, the 1 s
//!   tick notices when you type but our hook hears nothing and puts it back in
//!   front (at most every 10 s, never while a key is held or when idle).
//!
//! A kept shortcut runs once on its press and its release is swallowed, except
//! Paste last transcription, which runs once on its (swallowed) release.
//! Push-to-talk and Undo last word act while the key is held and cannot be kept.

// The state machine and the candidate table are driven by the Windows watcher;
// elsewhere only the classification is used.
#![cfg_attr(not(windows), allow(dead_code))]

use serde::Serialize;
use specta::Type;
use std::collections::HashMap;
use tauri::{AppHandle, Manager};

use crate::settings::{self, AppSettings, KeyboardImplementation};

// ---------------------------------------------------------------------------
// Chords
// ---------------------------------------------------------------------------

const MOD_CTRL: u8 = 1;
const MOD_ALT: u8 = 2;
const MOD_SHIFT: u8 = 4;
const MOD_WIN: u8 = 8;

const VK_LSHIFT: u8 = 0xA0;
const VK_RSHIFT: u8 = 0xA1;
const VK_LCONTROL: u8 = 0xA2;
const VK_RCONTROL: u8 = 0xA3;
const VK_LMENU: u8 = 0xA4;
const VK_RMENU: u8 = 0xA5;
const VK_LWIN: u8 = 0x5B;
const VK_RWIN: u8 = 0x5C;
const MODIFIER_VKS: [u8; 8] = [
    VK_LSHIFT,
    VK_RSHIFT,
    VK_LCONTROL,
    VK_RCONTROL,
    VK_LMENU,
    VK_RMENU,
    VK_LWIN,
    VK_RWIN,
];

const VK_SPACE: u8 = 0x20;
const VK_HOME: u8 = 0x24;
const VK_END: u8 = 0x23;
const VK_DELETE: u8 = 0x2E;
const VK_L: u8 = 0x4C;

/// Modifiers plus one key, as the hook sees it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Chord {
    mods: u8,
    vk: u8,
}

enum Parsed {
    Chord(Chord),
    ModifierOnly,
    UnknownKey,
}

/// The virtual key for one of the key names the shortcut recorder produces.
/// Deliberately small: every entry maps to exactly one key, with no aliases.
fn key_vk(name: &str) -> Option<u8> {
    let bytes = name.as_bytes();
    if bytes.len() == 1 && bytes[0].is_ascii_lowercase() {
        return Some(bytes[0] - b'a' + 0x41);
    }
    if bytes.len() == 1 && bytes[0].is_ascii_digit() {
        return Some(bytes[0] - b'0' + 0x30);
    }
    if let Some(n) = name.strip_prefix('f')
        && !n.starts_with('0')
        && let Ok(n) = n.parse::<u8>()
    {
        return (1..=24).contains(&n).then_some(0x6F + n);
    }
    Some(match name {
        "space" => VK_SPACE,
        "enter" => 0x0D,
        "tab" => 0x09,
        "escape" => 0x1B,
        "backspace" => 0x08,
        "insert" => 0x2D,
        "delete" => VK_DELETE,
        "home" => VK_HOME,
        "end" => VK_END,
        "page up" => 0x21,
        "page down" => 0x22,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        _ => return None,
    })
}

fn parse_chord(raw: &str) -> Parsed {
    let mut mods = 0u8;
    let mut key = None;
    for part in raw.split('+') {
        let part = part.trim().to_lowercase();
        let part = part
            .strip_suffix("_left")
            .or_else(|| part.strip_suffix("_right"))
            .unwrap_or(&part);
        match part {
            "" => {}
            "ctrl" | "control" => mods |= MOD_CTRL,
            "alt" | "option" => mods |= MOD_ALT,
            "shift" => mods |= MOD_SHIFT,
            "super" | "win" | "windows" | "meta" | "command" | "cmd" => mods |= MOD_WIN,
            other => {
                if key.is_some() {
                    return Parsed::UnknownKey;
                }
                match key_vk(other) {
                    Some(vk) => key = Some(vk),
                    None => return Parsed::UnknownKey,
                }
            }
        }
    }
    match key {
        Some(vk) => Parsed::Chord(Chord { mods, vk }),
        None => Parsed::ModifierOnly,
    }
}

// ---------------------------------------------------------------------------
// Which shortcuts can be kept
// ---------------------------------------------------------------------------

#[derive(Serialize, Type, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KeepCategory {
    Ok,
    /// Works, with something to know (shown with a warning).
    Warn,
    /// Windows never lets an app have it.
    Reserved,
    /// Not in this version.
    Unsupported,
}

/// Whether a shortcut can be kept on this PC, and why not (`code` names the
/// reason for the UI; empty when it simply works).
#[derive(Serialize, Type, Clone, Debug, PartialEq, Eq)]
pub struct KeepSupport {
    pub category: KeepCategory,
    pub code: String,
}

fn keep(category: KeepCategory, code: &str) -> KeepSupport {
    KeepSupport {
        category,
        code: code.to_string(),
    }
}

/// Shortcuts that can be kept: those whose release does nothing, and Paste
/// last transcription, which acts on the release alone (it pastes once the
/// keys are up). Push-to-talk and Undo last word act while the key is held.
fn keepable(id: &str) -> bool {
    matches!(
        id,
        "transcribe"
            | "transcribe_and_submit"
            | "transcribe_with_post_process"
            | "cancel"
            | "pause"
            | "toggle_live_text_box"
            | "type_text"
            | "paste_last"
    ) || crate::shortcut::is_jumper_binding(id)
}

/// Kept shortcuts that run on the key release instead of the press.
pub(crate) fn acts_on_release(id: &str) -> bool {
    id == "paste_last"
}

pub(crate) fn support(id: &str, chord: &str) -> KeepSupport {
    use KeepCategory::*;
    if !keepable(id) {
        // Push-to-talk and Undo last word act while the key is held.
        return keep(Unsupported, "needs_hold");
    }
    if crate::shortcut::is_unbound(chord) {
        return keep(Unsupported, "unbound");
    }
    let c = match parse_chord(chord) {
        Parsed::Chord(c) => c,
        Parsed::ModifierOnly => return keep(Unsupported, "modifier_only"),
        Parsed::UnknownKey => return keep(Unsupported, "unknown_key"),
    };
    let ctrl_alt = MOD_CTRL | MOD_ALT;
    if c.mods & ctrl_alt == ctrl_alt && c.vk == VK_DELETE {
        return keep(Reserved, "ctrl_alt_del");
    }
    if c.mods & MOD_WIN != 0 && c.vk == VK_L {
        return keep(Reserved, "win_l");
    }
    // The leaked Windows key would open Start in the remote; a leaked Alt
    // (without Ctrl) its menu bar.
    if c.mods & MOD_WIN != 0 {
        return keep(Unsupported, "win_key");
    }
    if c.mods & MOD_ALT != 0 && c.mods & MOD_CTRL == 0 {
        return keep(Unsupported, "alt_only");
    }
    if c.mods & ctrl_alt == ctrl_alt && (c.vk == VK_HOME || c.vk == VK_END) {
        return keep(Warn, "rdp_combo");
    }
    if c.mods & MOD_SHIFT != 0 && c.mods & MOD_CTRL != 0 {
        return keep(Warn, "layout_switch");
    }
    if c == (Chord {
        mods: MOD_CTRL,
        vk: VK_SPACE,
    }) {
        return keep(Warn, "ctrl_space");
    }
    keep(Ok, "")
}

/// The chords to keep: checked, supported, and actually registered (binding id
/// and chord as registered, so a failed or duplicate registration never counts).
fn candidates(registered: &[(String, String)], local: &[String]) -> Vec<(Chord, String, String)> {
    registered
        .iter()
        .filter(|(id, _)| local.contains(id))
        .filter(|(id, chord)| {
            matches!(
                support(id, chord).category,
                KeepCategory::Ok | KeepCategory::Warn
            )
        })
        .filter_map(|(id, chord)| match parse_chord(chord) {
            Parsed::Chord(c) => Some((c, id.clone(), chord.clone())),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The keyboard state machine (pure; driven by the hook)
// ---------------------------------------------------------------------------

/// A set of virtual keys.
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct VkSet([u64; 4]);

impl VkSet {
    fn get(&self, vk: u8) -> bool {
        self.0[(vk >> 6) as usize] & (1u64 << (vk & 63)) != 0
    }
    fn set(&mut self, vk: u8) {
        self.0[(vk >> 6) as usize] |= 1u64 << (vk & 63);
    }
    fn clear(&mut self, vk: u8) {
        self.0[(vk >> 6) as usize] &= !(1u64 << (vk & 63));
    }
    fn is_empty(&self) -> bool {
        self.0 == [0; 4]
    }
    fn union(&mut self, other: &VkSet) {
        for (a, b) in self.0.iter_mut().zip(other.0) {
            *a |= b;
        }
    }
}

fn is_modifier(vk: u8) -> bool {
    MODIFIER_VKS.contains(&vk)
}

/// Keys that always pass: IME and Unicode packets, and keys no chord uses.
fn always_passes(vk: u8) -> bool {
    matches!(
        vk,
        0x03 /* CANCEL */ | 0x13 /* PAUSE */ | 0xE5 /* PROCESSKEY */ | 0xE7 /* PACKET */ | 0xFF
    )
}

/// Generic modifier codes (rare in the low-level hook) count as the left one.
fn normalize_vk(vk: u32) -> Option<u8> {
    match vk {
        0x10 => Some(VK_LSHIFT),
        0x11 => Some(VK_LCONTROL),
        0x12 => Some(VK_LMENU),
        0..=0xFF => Some(vk as u8),
        _ => None,
    }
}

/// What the hook does with a key event.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Pass,
    Swallow,
}

/// Physical keyboard state as seen by the hook since it was (re)installed.
///
/// - `phys_down`: keys whose physical press this hook saw and whose release it
///   has not. A press of a key already down is an auto-repeat, never fresh.
/// - `quarantine`: keys that were (or may have been) down while the hook could
///   not see them. Negative evidence only: such a key is never kept until the
///   hook sees it physically released, and a quarantined modifier blocks
///   every chord, so a pre-held Shift or Win can never turn one chord into
///   another.
/// - `owned`: keys whose press was kept; their repeats and release are
///   swallowed too, even after focus left the session.
#[derive(Default)]
pub(crate) struct KeyState {
    owned: VkSet,
    phys_down: VkSet,
    quarantine: VkSet,
    acquiring: bool,
}

impl KeyState {
    /// A new session window has the keyboard: start over.
    pub(crate) fn begin_context(&mut self) {
        self.phys_down = VkSet::default();
        self.quarantine = VkSet::default();
        self.acquiring = false;
    }

    /// The hook was (re)installed. Anything down now (`sample`) or seen down
    /// before stays out until its physical release; modifier state starts up.
    pub(crate) fn begin_attempt(&mut self, sample: &VkSet) {
        let seen = self.phys_down;
        self.quarantine.union(&seen);
        self.quarantine.union(sample);
        self.phys_down = VkSet::default();
        self.acquiring = true;
    }

    /// Keys that just became shortcuts to keep while the hook is in: any of
    /// them down now may have been pressed before, so it stays out until its
    /// physical release, as at an attempt.
    pub(crate) fn quarantine_more(&mut self, sample: &VkSet) {
        self.quarantine.union(sample);
    }

    pub(crate) fn stop_acquiring(&mut self) {
        self.acquiring = false;
    }

    pub(crate) fn owns_any(&self) -> bool {
        !self.owned.is_empty()
    }

    /// Recovery when an owned key's release never arrived.
    pub(crate) fn release_all_owned(&mut self) {
        self.owned = VkSet::default();
    }

    /// One key event. `claim` is asked to take a matching chord (it checks the
    /// focus and hands the press on); only if it agrees is the key kept.
    pub(crate) fn event(
        &mut self,
        raw_vk: u32,
        down: bool,
        injected: bool,
        claim: impl FnOnce(Chord) -> bool,
    ) -> Verdict {
        if injected {
            return Verdict::Pass;
        }
        let Some(vk) = normalize_vk(raw_vk) else {
            return Verdict::Pass;
        };
        // State first, so no decision below can skip it.
        let was_down = self.phys_down.get(vk);
        if down {
            self.phys_down.set(vk);
        } else {
            self.phys_down.clear(vk);
            self.quarantine.clear(vk);
        }

        if self.owned.get(vk) {
            if !down {
                self.owned.clear(vk);
            }
            return Verdict::Swallow;
        }
        if !down
            || was_down
            || !self.acquiring
            || is_modifier(vk)
            || always_passes(vk)
            || self.quarantine.get(vk)
            || MODIFIER_VKS.iter().any(|m| self.quarantine.get(*m))
        {
            return Verdict::Pass;
        }
        let held = |v: u8| self.phys_down.get(v);
        // Win makes any chord a different one; right Alt is AltGr, whose
        // characters must keep reaching the remote.
        if held(VK_LWIN) || held(VK_RWIN) || held(VK_RMENU) {
            return Verdict::Pass;
        }
        let mut mods = 0;
        if held(VK_LCONTROL) || held(VK_RCONTROL) {
            mods |= MOD_CTRL;
        }
        if held(VK_LMENU) {
            mods |= MOD_ALT;
        }
        if held(VK_LSHIFT) || held(VK_RSHIFT) {
            mods |= MOD_SHIFT;
        }
        if claim(Chord { mods, vk }) {
            self.owned.set(vk);
            return Verdict::Swallow;
        }
        Verdict::Pass
    }
}

/// The hook is no longer first in line: the system saw input (keyboard or
/// mouse, `last_input`) at least 1 s after the hook last heard a key
/// (`last_heard`), within the last 3 s, and the hook has heard nothing for
/// 3 s. All three are Windows tick counts in ms (they wrap).
pub(crate) fn hook_seems_lost(now: u32, last_heard: u32, last_input: u32) -> bool {
    now.wrapping_sub(last_heard) >= 3_000
        && (last_input.wrapping_sub(last_heard) as i32) >= 1_000
        && now.wrapping_sub(last_input) <= 3_000
}

// ---------------------------------------------------------------------------
// Status and commands
// ---------------------------------------------------------------------------

#[derive(Serialize, Type, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteKeysState {
    /// The switch is off.
    Off,
    /// Not on this system or keyboard backend.
    Unavailable,
    /// The other keyboard backend ran this session.
    NeedsRestart,
    /// On; no Remote Desktop session has the keyboard.
    Waiting,
    /// A Remote Desktop session has the keyboard and the kept shortcuts work.
    Active,
    /// The session runs as administrator; Windows keeps Handy out of it.
    HigherIntegrity,
}

#[derive(Serialize, Type, Clone, Debug, PartialEq, Eq)]
pub struct RemoteKeysStatus {
    pub enabled: bool,
    pub state: RemoteKeysState,
}

/// Why it cannot run here, if it cannot.
fn unavailable(app: &AppHandle, settings: &AppSettings) -> Option<RemoteKeysState> {
    if !cfg!(windows) || settings.keyboard_implementation != KeyboardImplementation::Tauri {
        return Some(RemoteKeysState::Unavailable);
    }
    // The other backend's hooks live until Handy exits.
    if app
        .try_state::<crate::shortcut::handy_keys::HandyKeysState>()
        .is_some()
    {
        return Some(RemoteKeysState::NeedsRestart);
    }
    None
}

pub fn status(app: &AppHandle) -> RemoteKeysStatus {
    let settings = settings::get_settings(app);
    let state = match unavailable(app, &settings) {
        Some(state) => state,
        None if !settings.remote_keys_enabled => RemoteKeysState::Off,
        None => win::thread_state(),
    };
    RemoteKeysStatus {
        enabled: settings.remote_keys_enabled,
        state,
    }
}

/// Starts Shortcut Keeper if it is switched on (once per run).
pub fn start_if_enabled(app: &AppHandle) {
    if settings::get_settings(app).remote_keys_enabled {
        win::start(app);
    }
}

/// The registered shortcuts or the settings changed: rebuild what is kept.
pub fn notify_bindings_changed() {
    win::notify();
}

/// Ties a kept press to the session window it was pressed in, so a press that
/// waited in a queue is dropped if that window lost the keyboard meanwhile.
#[derive(Clone, Copy, Debug)]
pub struct RemoteGuard {
    generation: u64,
    hwnd: isize,
}

/// True while the session the press came from still has the keyboard.
pub fn guard_valid(guard: RemoteGuard) -> bool {
    win::guard_valid(guard)
}

/// Switch Shortcut Keeper on or off.
#[tauri::command]
#[specta::specta]
pub fn change_remote_keys_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    if enabled {
        match unavailable(&app, &settings) {
            Some(RemoteKeysState::NeedsRestart) => {
                return Err("Restart Handy Tool to use Shortcut Keeper".into());
            }
            Some(_) => {
                return Err(
                    "Shortcut Keeper works on Windows with the default keyboard backend only"
                        .into(),
                );
            }
            None => {}
        }
    }
    settings.remote_keys_enabled = enabled;
    settings::write_settings(&app, settings);
    log::info!(
        "Shortcut Keeper switched {}",
        if enabled { "on" } else { "off" }
    );
    if enabled {
        win::start(&app);
    }
    win::notify();
    Ok(())
}

/// Keep one shortcut on this PC in Remote Desktop, or let it go to the remote.
#[tauri::command]
#[specta::specta]
pub fn set_remote_local_binding(app: AppHandle, id: String, local: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    if local {
        let chord = settings
            .bindings
            .get(&id)
            .map(|b| b.current_binding.clone())
            .ok_or_else(|| format!("Unknown shortcut '{id}'"))?;
        if !matches!(
            support(&id, &chord).category,
            KeepCategory::Ok | KeepCategory::Warn
        ) {
            return Err(format!("'{id}' cannot be kept on this PC"));
        }
        if !settings.remote_local_bindings.contains(&id) {
            settings.remote_local_bindings.push(id);
        }
    } else {
        settings.remote_local_bindings.retain(|kept| kept != &id);
    }
    settings::write_settings(&app, settings);
    win::notify();
    Ok(())
}

/// Whether you type Chinese, Japanese or Korean (only then is Ctrl+Space's
/// input-method note shown).
#[tauri::command]
#[specta::specta]
pub fn change_remote_keys_cjk_input_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.remote_keys_cjk_input = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

/// For every shortcut: can it be kept on this PC, and if not, why.
#[tauri::command]
#[specta::specta]
pub fn get_remote_key_support(app: AppHandle) -> HashMap<String, KeepSupport> {
    settings::get_settings(&app)
        .bindings
        .values()
        .map(|b| (b.id.clone(), support(&b.id, &b.current_binding)))
        .collect()
}

#[tauri::command]
#[specta::specta]
pub fn get_remote_keys_status(app: AppHandle) -> RemoteKeysStatus {
    status(&app)
}

// ---------------------------------------------------------------------------
// Windows: the watcher thread and the hook
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod win {
    use super::{
        Chord, KeyState, MODIFIER_VKS, RemoteGuard, RemoteKeysState, Verdict, VkSet,
        hook_seems_lost,
    };
    use log::{debug, info, warn};
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
    use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::{Duration, Instant};
    use tauri::{AppHandle, Emitter};
    use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Security::{
        GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL,
        TOKEN_QUERY, TokenIntegrityLevel,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentThreadId, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, GetLastInputInfo, LASTINPUTINFO,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, EVENT_OBJECT_FOCUS, EVENT_SYSTEM_FOREGROUND, GA_ROOT, GUITHREADINFO,
        GetAncestor, GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetMessageW,
        GetWindowThreadProcessId, HHOOK, KBDLLHOOKSTRUCT, KillTimer, LLKHF_INJECTED, LLKHF_UP, MSG,
        PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SetTimer, SetWindowsHookExW,
        UnhookWindowsHookEx, WH_KEYBOARD_LL, WINEVENT_OUTOFCONTEXT, WM_APP, WM_TIMER,
    };

    /// Remote Desktop clients (process name) whose session input window has
    /// this class. Both run the same Remote Desktop control.
    const CLIENTS: [&str; 2] = ["mstsc", "msrdc"];
    const INPUT_CLASS: &str = "IHWindowClass";

    /// The client installs its hook when its window gets the keyboard; ours
    /// goes in after it, retried in case the client was slow.
    const ATTEMPT_DELAYS_MS: [u32; 3] = [50, 200, 750];
    /// A kept key whose release never came (hook bypassed) is let go after this.
    const OWNED_TIMEOUT: Duration = Duration::from_secs(5);

    const FOCUS_EVENTS: [u32; 2] = [EVENT_SYSTEM_FOREGROUND, EVENT_OBJECT_FOCUS];

    const WM_REFRESH: u32 = WM_APP + 1;
    const WM_DRAINED: u32 = WM_APP + 2;

    static STARTED: OnceLock<()> = OnceLock::new();
    static THREAD_ID: AtomicU32 = AtomicU32::new(0);
    /// One refresh queued at a time: re-registering every shortcut (resume
    /// after editing one) asks dozens of times in a row.
    static REFRESH_QUEUED: AtomicBool = AtomicBool::new(false);
    /// Bumped whenever the session that may receive kept presses changes.
    static CONTEXT_GEN: AtomicU64 = AtomicU64::new(1);
    static STATE: Mutex<RemoteKeysState> = Mutex::new(RemoteKeysState::Waiting);
    /// Counts the hook's calls; the tick compares it to notice a lost hook.
    static HOOK_CALLS: AtomicU32 = AtomicU32::new(0);
    /// No more often than this does the tick put the hook back in front.
    const HEAL_EVERY: Duration = Duration::from_secs(10);
    const HEAL_EVERY_UNHEARD: Duration = Duration::from_secs(60);

    struct Candidate {
        chord: Chord,
        id: Arc<str>,
        hotkey: Arc<str>,
        /// Paste last transcription: dispatched when its key is released.
        on_release: bool,
    }

    struct Press {
        generation: u64,
        hwnd: isize,
        id: Arc<str>,
        hotkey: Arc<str>,
        /// The key's release (for a shortcut that acts on it), not its press.
        release: bool,
    }

    /// Lives on the watcher thread only: the hook and the WinEvent callbacks
    /// run on that thread, inside its GetMessageW.
    struct Keeper {
        app: AppHandle,
        tx: SyncSender<Press>,
        keys: KeyState,
        table: Vec<Candidate>,
        win_events: Vec<HWINEVENTHOOK>,
        hook: Option<HHOOK>,
        armed: Option<isize>,
        attempts_done: usize,
        /// A re-hook attempt was skipped because a kept key was held; the
        /// attempts run again once it is released.
        attempts_blocked: bool,
        attempt_timer: usize,
        tick_timer: usize,
        last_owned: Instant,
        reported: Option<RemoteKeysState>,
        /// A kept key whose shortcut runs on its release: (key, the press
        /// that will go out when it comes up).
        release_pending: Option<(u8, Press)>,
        /// HOOK_CALLS at the last tick, and the tick count when it last moved.
        seen_calls: u32,
        last_heard: u32,
        last_heal: Option<Instant>,
        /// HOOK_CALLS at the last heal: if no key was heard since, the next
        /// heal waits longer (mouse-only use looks like a lost hook).
        calls_at_heal: u32,
    }

    thread_local! {
        static KEEPER: RefCell<Option<Keeper>> = const { RefCell::new(None) };
    }

    /// Runs `f` on the keeper. Nothing in here pumps messages, so the hook and
    /// event callbacks never re-enter; if one ever did, it is skipped.
    fn with_keeper<R>(f: impl FnOnce(&mut Keeper) -> R) -> Option<R> {
        KEEPER.with(|cell| {
            let mut guard = cell.try_borrow_mut().ok()?;
            guard.as_mut().map(f)
        })
    }

    pub fn start(app: &AppHandle) {
        if STARTED.set(()).is_err() {
            return;
        }
        let (tx, rx) = sync_channel::<Press>(32);
        let dispatch_app = app.clone();
        if let Err(e) = std::thread::Builder::new()
            .name("shortcut-keeper-dispatch".into())
            .spawn(move || dispatcher(dispatch_app, rx))
        {
            warn!("Shortcut Keeper: could not start: {e}");
            return;
        }
        let app = app.clone();
        if let Err(e) = std::thread::Builder::new()
            .name("shortcut-keeper".into())
            .spawn(move || run(app, tx))
        {
            warn!("Shortcut Keeper: could not start: {e}");
        }
    }

    pub fn notify() {
        let thread = THREAD_ID.load(Ordering::SeqCst);
        if thread == 0 || REFRESH_QUEUED.swap(true, Ordering::SeqCst) {
            return;
        }
        if unsafe { PostThreadMessageW(thread, WM_REFRESH, WPARAM(0), LPARAM(0)) }.is_err() {
            REFRESH_QUEUED.store(false, Ordering::SeqCst);
            debug!("Shortcut Keeper: refresh not delivered");
        }
    }

    pub fn thread_state() -> RemoteKeysState {
        *STATE.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn guard_valid(guard: RemoteGuard) -> bool {
        CONTEXT_GEN.load(Ordering::SeqCst) == guard.generation && focus_hwnd() == Some(guard.hwnd)
    }

    fn run(app: AppHandle, tx: SyncSender<Press>) {
        let mut msg = MSG::default();
        // Create this thread's message queue before anyone can post to it.
        unsafe {
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
        }
        KEEPER.with(|cell| {
            *cell.borrow_mut() = Some(Keeper {
                app,
                tx,
                keys: KeyState::default(),
                table: Vec::new(),
                win_events: Vec::new(),
                hook: None,
                armed: None,
                attempts_done: 0,
                attempts_blocked: false,
                attempt_timer: 0,
                tick_timer: 0,
                last_owned: Instant::now(),
                reported: None,
                release_pending: None,
                seen_calls: 0,
                last_heard: 0,
                last_heal: None,
                calls_at_heal: 0,
            })
        });
        THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
        with_keeper(refresh);
        loop {
            let got = unsafe { GetMessageW(&mut msg, None, 0, 0) };
            if got.0 == 0 || got.0 == -1 {
                break;
            }
            match msg.message {
                WM_TIMER => {
                    let id = msg.wParam.0;
                    with_keeper(|k| on_timer(k, id));
                }
                WM_REFRESH => {
                    // Cleared first: a change made during this refresh queues
                    // another one.
                    REFRESH_QUEUED.store(false, Ordering::SeqCst);
                    with_keeper(refresh);
                }
                WM_DRAINED => {
                    with_keeper(drained);
                }
                _ => {}
            }
        }
        warn!("Shortcut Keeper: stopped");
        THREAD_ID.store(0, Ordering::SeqCst);
        with_keeper(|k| {
            unhook(k);
            remove_win_events(k);
        });
    }

    /// Settings or registered shortcuts changed.
    fn refresh(k: &mut Keeper) {
        let settings = crate::settings::get_settings(&k.app);
        let on = settings.remote_keys_enabled && super::unavailable(&k.app, &settings).is_none();
        let table: Vec<Candidate> = if on {
            let registered = crate::shortcut::registered_tauri_bindings();
            super::candidates(&registered, &settings.remote_local_bindings)
                .into_iter()
                .map(|(chord, id, hotkey)| Candidate {
                    chord,
                    on_release: super::acts_on_release(&id),
                    id: id.into(),
                    hotkey: hotkey.into(),
                })
                .collect()
        } else {
            Vec::new()
        };
        // A press already queued for a shortcut that is no longer kept (or now
        // has other keys) must not run.
        let removed = k.table.iter().any(|old| {
            !table
                .iter()
                .any(|new| new.chord == old.chord && new.id == old.id)
        });
        if removed {
            CONTEXT_GEN.fetch_add(1, Ordering::SeqCst);
        }
        // A key that becomes a shortcut to keep while the hook is in (Cancel,
        // registered when a take starts) may already be held.
        if k.hook.is_some() {
            let added = table
                .iter()
                .map(|c| c.chord.vk)
                .filter(|vk| !k.table.iter().any(|old| old.chord.vk == *vk));
            let sample = sample_down(added);
            k.keys.quarantine_more(&sample);
        }
        k.table = table;
        if k.table.is_empty() {
            remove_win_events(k);
        } else if k.win_events.is_empty() {
            install_win_events(k);
        }
        evaluate(k);
        let _ = k.app.emit("remote-keys-status", super::status(&k.app));
    }

    /// Focus moved somewhere (or the settings changed): arm or disarm.
    fn evaluate(k: &mut Keeper) {
        // Without focus tracking, leaving the session would go unnoticed.
        let tracking = k.win_events.len() == FOCUS_EVENTS.len();
        let (target, state) = match (k.table.is_empty() || !tracking, session_focus()) {
            (false, Some(hwnd)) if k.armed == Some(hwnd) => (Some(hwnd), RemoteKeysState::Active),
            (false, Some(hwnd)) if integrity_allows(hwnd) => (Some(hwnd), RemoteKeysState::Active),
            (false, Some(_)) => (None, RemoteKeysState::HigherIntegrity),
            _ => (None, RemoteKeysState::Waiting),
        };
        if target != k.armed {
            leave(k);
            if let Some(hwnd) = target {
                enter(k, hwnd);
            }
        }
        if k.reported != Some(state) {
            k.reported = Some(state);
            *STATE.lock().unwrap_or_else(|p| p.into_inner()) = state;
            if state == RemoteKeysState::HigherIntegrity {
                info!("Shortcut Keeper: the Remote Desktop session runs as administrator");
            }
            let _ = k.app.emit("remote-keys-status", super::status(&k.app));
        }
    }

    fn enter(k: &mut Keeper, hwnd: isize) {
        CONTEXT_GEN.fetch_add(1, Ordering::SeqCst);
        k.armed = Some(hwnd);
        k.keys.begin_context();
        k.attempts_done = 0;
        k.attempts_blocked = false;
        k.last_heard = unsafe { GetTickCount() };
        set_timer(&mut k.attempt_timer, ATTEMPT_DELAYS_MS[0]);
        info!("Shortcut Keeper: a Remote Desktop session has the keyboard");
    }

    fn leave(k: &mut Keeper) {
        if k.armed.take().is_none() {
            return;
        }
        CONTEXT_GEN.fetch_add(1, Ordering::SeqCst);
        kill_timer(&mut k.attempt_timer);
        k.keys.stop_acquiring();
        // A kept key still held keeps being swallowed until its release, so
        // its repeats never reach the window that has the keyboard now.
        unhook_if_done(k);
        info!("Shortcut Keeper: the Remote Desktop session lost the keyboard");
    }

    fn on_timer(k: &mut Keeper, id: usize) {
        if id != 0 && id == k.attempt_timer {
            kill_timer(&mut k.attempt_timer);
            if k.armed.is_some() {
                attempt(k);
                k.attempts_done += 1;
                if let Some(delay) = ATTEMPT_DELAYS_MS.get(k.attempts_done) {
                    set_timer(&mut k.attempt_timer, *delay);
                }
            }
        } else if id != 0 && id == k.tick_timer {
            if k.keys.owns_any() && k.last_owned.elapsed() > OWNED_TIMEOUT {
                warn!("Shortcut Keeper: a kept key's release never arrived; letting it go");
                k.keys.release_all_owned();
                k.release_pending = None;
            }
            // A safety net for a focus change whose event never came.
            evaluate(k);
            drained(k);
            heal_if_lost(k);
        } else {
            // Posted before its timer was killed.
            unsafe {
                let _ = KillTimer(None, id);
            }
        }
    }

    /// (Re)install the hook so it sits in front of the client's.
    fn attempt(k: &mut Keeper) {
        if k.keys.owns_any() {
            debug!("Shortcut Keeper: re-hook skipped, a kept key is held");
            k.attempts_blocked = true;
            return;
        }
        if let Some(old) = k.hook.take()
            && let Err(e) = unsafe { UnhookWindowsHookEx(old) }
        {
            // Windows already removed it (a slow callback), or it was gone.
            debug!("Shortcut Keeper: the previous hook could not be removed: {e}");
        }
        k.last_heard = unsafe { GetTickCount() };
        let hook = unsafe {
            GetModuleHandleW(None).and_then(|module| {
                SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), Some(module.into()), 0)
            })
        };
        match hook {
            Ok(hook) => k.hook = Some(hook),
            Err(e) => {
                warn!("Shortcut Keeper: keyboard hook not installed: {e}");
                return;
            }
        }
        // Taken after the hook is in: a key pressed meanwhile is either in the
        // sample or seen by the hook, never neither.
        let sample = sample_down(k.table.iter().map(|c| c.chord.vk));
        k.keys.begin_attempt(&sample);
        if k.tick_timer == 0 {
            set_timer(&mut k.tick_timer, 1000);
            // No hook without its recovery timer.
            if k.tick_timer == 0 {
                warn!("Shortcut Keeper: no timer, keyboard hook removed");
                k.keys.release_all_owned();
                k.release_pending = None;
                unhook(k);
            }
        }
    }

    /// The last kept key was released (or let go). Outside a session the hook
    /// goes; inside one, attempts a held key blocked run now.
    fn drained(k: &mut Keeper) {
        if k.armed.is_none() {
            unhook_if_done(k);
        } else if k.attempts_blocked && !k.keys.owns_any() {
            k.attempts_blocked = false;
            k.attempts_done = 0;
            set_timer(&mut k.attempt_timer, ATTEMPT_DELAYS_MS[0]);
        }
    }

    /// Remove the hook once the session is gone and no kept key is held.
    fn unhook_if_done(k: &mut Keeper) {
        if k.armed.is_none() && !k.keys.owns_any() {
            unhook(k);
        }
    }

    fn unhook(k: &mut Keeper) {
        if let Some(hook) = k.hook.take()
            && let Err(e) = unsafe { UnhookWindowsHookEx(hook) }
        {
            debug!("Shortcut Keeper: the hook could not be removed: {e}");
        }
        kill_timer(&mut k.tick_timer);
    }

    /// While a session has the keyboard: if you are typing (or using the
    /// mouse) but the hook has heard nothing for 3 s, the client or Windows
    /// has put it out of line. Put it back in front - only when nothing is
    /// held, and at most every 10 s, so it is not churned while idle.
    fn heal_if_lost(k: &mut Keeper) {
        let calls = HOOK_CALLS.load(Ordering::Relaxed);
        let now = unsafe { GetTickCount() };
        if calls != k.seen_calls {
            k.seen_calls = calls;
            k.last_heard = now;
            return;
        }
        // Nothing heard since the last heal: it was probably the mouse, so
        // wait longer before trying again.
        let gap = if calls == k.calls_at_heal {
            HEAL_EVERY_UNHEARD
        } else {
            HEAL_EVERY
        };
        if k.armed.is_none()
            || k.hook.is_none()
            || k.attempt_timer != 0
            || k.keys.owns_any()
            || k.last_heal.is_some_and(|t| t.elapsed() < gap)
        {
            return;
        }
        let mut input = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        if !unsafe { GetLastInputInfo(&mut input) }.as_bool()
            || !hook_seems_lost(now, k.last_heard, input.dwTime)
        {
            return;
        }
        // A held key would be quarantined by the re-hook; wait for its release.
        if !sample_down(k.table.iter().map(|c| c.chord.vk)).is_empty() {
            return;
        }
        info!(
            "Shortcut Keeper: input reached Windows but not the hook for {} s; putting the hook back in front",
            now.wrapping_sub(k.last_heard) / 1000
        );
        k.last_heal = Some(Instant::now());
        k.calls_at_heal = calls;
        attempt(k);
    }

    /// Both or none: arming needs to see focus leave the session.
    fn install_win_events(k: &mut Keeper) {
        for event in FOCUS_EVENTS {
            let hook = unsafe {
                SetWinEventHook(
                    event,
                    event,
                    None,
                    Some(win_event_proc),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                )
            };
            if hook.is_invalid() {
                warn!("Shortcut Keeper: focus tracking not installed");
                remove_win_events(k);
                return;
            }
            k.win_events.push(hook);
        }
    }

    fn remove_win_events(k: &mut Keeper) {
        for hook in k.win_events.drain(..) {
            unsafe {
                let _ = UnhookWinEvent(hook);
            }
        }
    }

    fn set_timer(slot: &mut usize, ms: u32) {
        kill_timer(slot);
        *slot = unsafe { SetTimer(None, 0, ms, None) };
    }

    fn kill_timer(slot: &mut usize) {
        if *slot != 0 {
            unsafe {
                let _ = KillTimer(None, *slot);
            }
            *slot = 0;
        }
    }

    unsafe extern "system" fn win_event_proc(
        _hook: HWINEVENTHOOK,
        _event: u32,
        _hwnd: HWND,
        _object: i32,
        _child: i32,
        _thread: u32,
        _time: u32,
    ) {
        // The payload is not trusted: evaluate re-reads the current focus.
        with_keeper(evaluate);
    }

    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            HOOK_CALLS.fetch_add(1, Ordering::Relaxed);
            let event = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            if with_keeper(|k| on_key(k, event)).unwrap_or(false) {
                return LRESULT(1);
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    /// No allocation, I/O, settings or logging here: Windows removes a slow
    /// low-level hook.
    fn on_key(k: &mut Keeper, event: &KBDLLHOOKSTRUCT) -> bool {
        let injected = event.flags.contains(LLKHF_INJECTED);
        let down = !event.flags.contains(LLKHF_UP);
        let verdict = {
            let Keeper {
                keys,
                table,
                tx,
                armed,
                release_pending,
                ..
            } = &mut *k;
            let armed = *armed;
            keys.event(event.vkCode, down, injected, |chord| {
                let Some(candidate) = table.iter().find(|c| c.chord == chord) else {
                    return false;
                };
                let Some(hwnd) = armed else {
                    return false;
                };
                if focus_hwnd() != Some(hwnd) {
                    return false;
                }
                let press = Press {
                    generation: CONTEXT_GEN.load(Ordering::SeqCst),
                    hwnd,
                    id: candidate.id.clone(),
                    hotkey: candidate.hotkey.clone(),
                    release: candidate.on_release,
                };
                if candidate.on_release {
                    // Goes out when this key comes up (it is swallowed then).
                    *release_pending = Some((chord.vk, press));
                    return true;
                }
                tx.try_send(press).is_ok()
            })
        };
        if verdict != Verdict::Swallow {
            return false;
        }
        if !down
            && k.release_pending
                .as_ref()
                .is_some_and(|(vk, _)| u32::from(*vk) == event.vkCode)
            && let Some((_, press)) = k.release_pending.take()
        {
            let _ = k.tx.try_send(press);
        }
        k.last_owned = Instant::now();
        if !k.keys.owns_any() {
            let thread = THREAD_ID.load(Ordering::SeqCst);
            unsafe {
                let _ = PostThreadMessageW(thread, WM_DRAINED, WPARAM(0), LPARAM(0));
            }
        }
        true
    }

    /// Runs kept presses on the main thread, as the shortcut plugin does.
    fn dispatcher(app: AppHandle, rx: Receiver<Press>) {
        while let Ok(press) = rx.recv() {
            if press.generation != CONTEXT_GEN.load(Ordering::SeqCst) {
                debug!(
                    "Shortcut Keeper: press dropped, the session changed before it ran: {}",
                    press.id
                );
                continue;
            }
            let main_app = app.clone();
            let _ = app.run_on_main_thread(move || {
                let guard = RemoteGuard {
                    generation: press.generation,
                    hwnd: press.hwnd,
                };
                if !guard_valid(guard) {
                    debug!("Shortcut Keeper: press dropped, the session lost the keyboard");
                    return;
                }
                debug!("Shortcut Keeper: kept on this PC: {}", press.id);
                if press.release {
                    crate::shortcut::handle_kept_release(&main_app, &press.id, &press.hotkey);
                } else {
                    crate::shortcut::handle_kept_press(&main_app, &press.id, &press.hotkey, guard);
                }
            });
        }
    }

    fn focus_hwnd() -> Option<isize> {
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        unsafe { GetGUIThreadInfo(0, &mut info) }.ok()?;
        (!info.hwndFocus.0.is_null()).then_some(info.hwndFocus.0 as isize)
    }

    /// The focused control, if it is a Remote Desktop session's input window
    /// inside the foreground window of a supported client.
    fn session_focus() -> Option<isize> {
        let focus = HWND(focus_hwnd()? as *mut core::ffi::c_void);
        if class_name(focus) != INPUT_CLASS {
            return None;
        }
        unsafe {
            let foreground = GetForegroundWindow();
            if foreground.0.is_null() || GetAncestor(focus, GA_ROOT) != foreground {
                return None;
            }
            let mut pid = 0u32;
            if GetWindowThreadProcessId(focus, Some(&mut pid)) == 0 {
                return None;
            }
            let exe = process_stem(pid)?.to_ascii_lowercase();
            CLIENTS.contains(&exe.as_str()).then_some(focus.0 as isize)
        }
    }

    fn class_name(hwnd: HWND) -> String {
        let mut buf = [0u16; 64];
        let n = unsafe { GetClassNameW(hwnd, &mut buf) };
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }

    fn process_stem(pid: u32) -> Option<String> {
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
            std::path::Path::new(&full)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
        }
    }

    /// A process's mandatory integrity level (its RID), or None.
    fn integrity_of(process: HANDLE) -> Option<u32> {
        unsafe {
            let mut token = HANDLE::default();
            OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
            // u64 storage: TOKEN_MANDATORY_LABEL holds a pointer.
            let mut buf = [0u64; 16];
            let mut len = 0u32;
            let got = GetTokenInformation(
                token,
                TokenIntegrityLevel,
                Some(buf.as_mut_ptr().cast()),
                std::mem::size_of_val(&buf) as u32,
                &mut len,
            );
            let _ = CloseHandle(token);
            got.ok()?;
            let label = &*(buf.as_ptr() as *const TOKEN_MANDATORY_LABEL);
            let sid = label.Label.Sid;
            let count = *GetSidSubAuthorityCount(sid);
            if count == 0 {
                return None;
            }
            Some(*GetSidSubAuthority(sid, u32::from(count) - 1))
        }
    }

    /// Windows does not call our hook while a higher-integrity window has the
    /// keyboard; do not pretend otherwise. Any failure means "do not arm".
    fn integrity_allows(hwnd: isize) -> bool {
        static OWN: OnceLock<Option<u32>> = OnceLock::new();
        let Some(own) = *OWN.get_or_init(|| integrity_of(unsafe { GetCurrentProcess() })) else {
            return false;
        };
        let mut pid = 0u32;
        unsafe {
            if GetWindowThreadProcessId(HWND(hwnd as *mut core::ffi::c_void), Some(&mut pid)) == 0 {
                return false;
            }
            let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                return false;
            };
            let level = integrity_of(process);
            let _ = CloseHandle(process);
            level.is_some_and(|level| level <= own)
        }
    }

    /// One read of the keys that matter: the given keys and the modifiers
    /// (high bit = down now).
    fn sample_down(keys: impl Iterator<Item = u8>) -> VkSet {
        let mut down = VkSet::default();
        for vk in keys.chain(MODIFIER_VKS) {
            if unsafe { GetAsyncKeyState(i32::from(vk)) } as u16 & 0x8000 != 0 {
                down.set(vk);
            }
        }
        down
    }
}

#[cfg(not(windows))]
mod win {
    use super::{RemoteGuard, RemoteKeysState};
    use tauri::AppHandle;

    pub fn start(_app: &AppHandle) {}
    pub fn notify() {}
    pub fn thread_state() -> RemoteKeysState {
        RemoteKeysState::Unavailable
    }
    pub fn guard_valid(_guard: RemoteGuard) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTRL: u32 = VK_LCONTROL as u32;
    const RCTRL: u32 = VK_RCONTROL as u32;
    const LALT: u32 = VK_LMENU as u32;
    const RALT: u32 = VK_RMENU as u32;
    const SHIFT: u32 = VK_LSHIFT as u32;
    const LWIN: u32 = VK_LWIN as u32;
    const SPACE: u32 = VK_SPACE as u32;
    const KEY_O: u32 = 0x4F;

    fn chord(raw: &str) -> Chord {
        match parse_chord(raw) {
            Parsed::Chord(c) => c,
            _ => panic!("not a chord: {raw}"),
        }
    }

    /// A state machine with the hook just installed and nothing held.
    fn armed() -> KeyState {
        let mut keys = KeyState::default();
        keys.begin_context();
        keys.begin_attempt(&VkSet::default());
        keys
    }

    /// Physical event; claims when the chord is one of `table`.
    fn press(keys: &mut KeyState, vk: u32, down: bool, table: &[Chord]) -> Verdict {
        keys.event(vk, down, false, |c| table.contains(&c))
    }

    fn injected(keys: &mut KeyState, vk: u32, down: bool) -> Verdict {
        keys.event(vk, down, true, |_| {
            panic!("injected input must never claim")
        })
    }

    #[test]
    fn a_hook_is_lost_only_when_input_arrived_that_it_did_not_hear() {
        // Heard a key at t=10 s; typing went on at 14 s; now 15 s: lost.
        assert!(hook_seems_lost(15_000, 10_000, 14_000));
        // Idle: no input since the last key heard. Not lost.
        assert!(!hook_seems_lost(60_000, 10_000, 10_000));
        // Heard recently (under 3 s). Not lost.
        assert!(!hook_seems_lost(12_000, 10_000, 11_500));
        // Input only right after the last key (under 1 s later). Not lost.
        assert!(!hook_seems_lost(15_000, 10_000, 10_500));
        // The input is old (over 3 s ago). Not lost (yet).
        assert!(!hook_seems_lost(20_000, 10_000, 15_000));
        // Tick counts wrap after ~49.7 days.
        assert!(hook_seems_lost(3_500, u32::MAX - 1_000, 2_800));
    }

    #[test]
    fn every_key_in_the_table_parses_to_one_virtual_key() {
        let cases: Vec<(String, u8)> = ('a'..='z')
            .map(|c| (c.to_string(), c.to_ascii_uppercase() as u8))
            .chain(('0'..='9').map(|c| (c.to_string(), c as u8)))
            .chain((1..=24u8).map(|n| (format!("f{n}"), 0x6F + n)))
            .chain(
                [
                    ("space", 0x20),
                    ("enter", 0x0D),
                    ("tab", 0x09),
                    ("escape", 0x1B),
                    ("backspace", 0x08),
                    ("insert", 0x2D),
                    ("delete", 0x2E),
                    ("home", 0x24),
                    ("end", 0x23),
                    ("page up", 0x21),
                    ("page down", 0x22),
                    ("left", 0x25),
                    ("up", 0x26),
                    ("right", 0x27),
                    ("down", 0x28),
                ]
                .map(|(k, v)| (k.to_string(), v)),
            )
            .collect();
        let mut seen = std::collections::HashSet::new();
        for (name, vk) in cases {
            assert_eq!(key_vk(&name), Some(vk), "{name}");
            assert!(seen.insert(vk), "two names for vk {vk:#x}");
        }
        for unknown in [
            "f0",
            "f25",
            "f01",
            "numpad 1",
            "print screen",
            "menu",
            "-",
            "esc",
        ] {
            assert_eq!(key_vk(unknown), None, "{unknown}");
        }
    }

    #[test]
    fn chords_parse_modifiers_and_one_key() {
        assert_eq!(
            chord("ctrl+shift+f1"),
            Chord {
                mods: MOD_CTRL | MOD_SHIFT,
                vk: 0x70
            }
        );
        assert_eq!(chord("Control+Space"), chord("ctrl+space"));
        assert_eq!(chord("ctrl_left+alt+1"), chord("ctrl+alt+1"));
        assert!(matches!(parse_chord("ctrl+shift"), Parsed::ModifierOnly));
        assert!(matches!(parse_chord("ctrl+a+b"), Parsed::UnknownKey));
        assert!(matches!(parse_chord("ctrl+numpad +"), Parsed::UnknownKey));
    }

    #[test]
    fn support_names_why_a_shortcut_cannot_be_kept() {
        let s = |id: &str, chord: &str| support(id, chord);
        assert_eq!(s("transcribe", "ctrl+alt+delete").code, "ctrl_alt_del");
        assert_eq!(
            s("transcribe", "ctrl+alt+delete").category,
            KeepCategory::Reserved
        );
        assert_eq!(s("transcribe", "super+l").code, "win_l");
        assert_eq!(s("transcribe", "super+ctrl+l").code, "win_l");
        assert_eq!(s("transcribe", "alt+x").code, "alt_only");
        assert_eq!(s("transcribe", "alt+shift+x").code, "alt_only");
        assert_eq!(s("transcribe", "super+x").code, "win_key");
        assert_eq!(s("transcribe", "ctrl+alt+home").code, "rdp_combo");
        assert_eq!(s("transcribe", "ctrl+alt+end").category, KeepCategory::Warn);
        assert_eq!(s("transcribe", "ctrl+shift+x").code, "layout_switch");
        assert_eq!(s("transcribe", "ctrl+alt+shift+1").code, "layout_switch");
        assert_eq!(s("transcribe", "ctrl+space").code, "ctrl_space");
        assert_eq!(s("transcribe", "ctrl+shift").code, "modifier_only");
        assert_eq!(s("transcribe", "ctrl+numpad 1").code, "unknown_key");
        assert_eq!(s("transcribe", "").code, "unbound");
        assert_eq!(s("transcribe_ptt", "ctrl+f5").code, "needs_hold");
        assert_eq!(s("undo_word", "ctrl+backspace").code, "needs_hold");
        assert_eq!(s("paste_last", "ctrl+alt+i").category, KeepCategory::Ok);
        assert_eq!(s("transcribe", "ctrl+f5").category, KeepCategory::Ok);
        assert_eq!(s("cancel", "escape").category, KeepCategory::Ok);
        assert_eq!(s("jump_slot_3", "ctrl+alt+3").category, KeepCategory::Ok);
        assert_eq!(s("anchor_set_2", "ctrl+f9").category, KeepCategory::Ok);
    }

    #[test]
    fn candidates_are_the_checked_supported_registered_ones() {
        let registered = vec![
            ("transcribe".to_string(), "ctrl+space".to_string()),
            ("type_text".to_string(), "ctrl+shift+f11".to_string()),
            ("transcribe_ptt".to_string(), "ctrl+f5".to_string()),
        ];
        // jump_slot_1 is checked but its registration failed (a duplicate):
        // it is not in `registered`, so it is not kept.
        let local = vec![
            "transcribe".to_string(),
            "transcribe_ptt".to_string(),
            "jump_slot_1".to_string(),
        ];
        let kept = candidates(&registered, &local);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].1, "transcribe");
        assert_eq!(kept[0].0, chord("ctrl+space"));
    }

    #[test]
    fn a_checked_chord_is_kept_with_its_repeats_and_release() {
        let table = [chord("ctrl+space")];
        let mut keys = armed();
        assert_eq!(press(&mut keys, CTRL, true, &table), Verdict::Pass);
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Swallow);
        // Repeats are swallowed without a second claim.
        assert_eq!(
            keys.event(SPACE, true, false, |_| panic!("repeat claimed")),
            Verdict::Swallow
        );
        assert_eq!(press(&mut keys, SPACE, false, &table), Verdict::Swallow);
        assert_eq!(press(&mut keys, CTRL, false, &table), Verdict::Pass);
        assert!(!keys.owns_any());
    }

    #[test]
    fn modifiers_must_match_exactly() {
        let table = [chord("ctrl+space")];
        let mut keys = armed();
        press(&mut keys, CTRL, true, &table);
        press(&mut keys, SHIFT, true, &table);
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Pass);
        let mut keys = armed();
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Pass);
        let mut keys = armed();
        press(&mut keys, RCTRL, true, &table);
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Swallow);
    }

    #[test]
    fn ctrl_alt_matches_only_left_alt_so_altgr_reaches_the_remote() {
        let table = [chord("ctrl+alt+o")];
        let mut keys = armed();
        press(&mut keys, CTRL, true, &table);
        press(&mut keys, LALT, true, &table);
        assert_eq!(press(&mut keys, KEY_O, true, &table), Verdict::Swallow);
        // AltGr: Windows sends left Ctrl plus right Alt.
        let mut keys = armed();
        press(&mut keys, CTRL, true, &table);
        press(&mut keys, RALT, true, &table);
        assert_eq!(press(&mut keys, KEY_O, true, &table), Verdict::Pass);
        // A plain Ctrl chord is not taken while AltGr is down either.
        let table = [chord("ctrl+o")];
        let mut keys = armed();
        press(&mut keys, CTRL, true, &table);
        press(&mut keys, RALT, true, &table);
        assert_eq!(press(&mut keys, KEY_O, true, &table), Verdict::Pass);
    }

    #[test]
    fn a_held_windows_key_vetoes_every_chord() {
        let table = [chord("ctrl+space")];
        for win in [LWIN, VK_RWIN as u32] {
            let mut keys = armed();
            press(&mut keys, win, true, &table);
            press(&mut keys, CTRL, true, &table);
            assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Pass);
        }
    }

    #[test]
    fn a_modifier_held_before_the_hook_never_counts_and_blocks_until_released() {
        let table = [chord("ctrl+space")];
        let mut keys = KeyState::default();
        keys.begin_context();
        let mut sample = VkSet::default();
        sample.set(VK_LCONTROL);
        keys.begin_attempt(&sample);
        // Ctrl was down before the hook: its repeat does not make it "held".
        press(&mut keys, CTRL, true, &table);
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Pass);
        press(&mut keys, SPACE, false, &table);
        // Released physically, pressed again: now it counts.
        press(&mut keys, CTRL, false, &table);
        press(&mut keys, CTRL, true, &table);
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Swallow);
    }

    #[test]
    fn a_win_key_held_before_the_hook_vetoes_until_its_physical_release() {
        let table = [chord("ctrl+space")];
        let mut keys = KeyState::default();
        keys.begin_context();
        let mut sample = VkSet::default();
        sample.set(VK_LWIN);
        keys.begin_attempt(&sample);
        press(&mut keys, CTRL, true, &table);
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Pass);
        press(&mut keys, SPACE, false, &table);
        injected(&mut keys, LWIN, false);
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Pass);
        press(&mut keys, SPACE, false, &table);
        press(&mut keys, LWIN, false, &table);
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Swallow);
    }

    #[test]
    fn a_key_held_before_the_hook_is_not_kept_and_an_injected_release_does_not_free_it() {
        let table = [chord("ctrl+space")];
        let mut keys = KeyState::default();
        keys.begin_context();
        let mut sample = VkSet::default();
        sample.set(VK_SPACE);
        keys.begin_attempt(&sample);
        injected(&mut keys, SPACE, false);
        press(&mut keys, CTRL, true, &table);
        // The physical Space repeat: still quarantined.
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Pass);
        // A physical release frees it, even though phys_down was already clear.
        press(&mut keys, SPACE, false, &table);
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Swallow);
    }

    #[test]
    fn quarantine_survives_a_retry_and_grows() {
        let table = [chord("ctrl+space")];
        let mut keys = KeyState::default();
        keys.begin_context();
        let mut sample = VkSet::default();
        sample.set(VK_SPACE);
        keys.begin_attempt(&sample); // +50 ms: Space held
        injected(&mut keys, SPACE, false);
        keys.begin_attempt(&VkSet::default()); // +250 ms: the sample misses it
        press(&mut keys, CTRL, true, &table);
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Pass);

        // The same for a modifier held across a retry.
        let mut keys = KeyState::default();
        keys.begin_context();
        keys.begin_attempt(&VkSet::default());
        press(&mut keys, SHIFT, true, &table); // seen down, never released
        keys.begin_attempt(&VkSet::default());
        press(&mut keys, CTRL, true, &table);
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Pass);
    }

    #[test]
    fn a_key_that_becomes_a_shortcut_while_held_is_not_kept() {
        // Escape becomes a shortcut (Cancel registered at take start) while
        // its earlier press already went to the remote.
        let table = [chord("escape")];
        let mut keys = armed();
        let mut sample = VkSet::default();
        sample.set(0x1B);
        keys.quarantine_more(&sample);
        assert_eq!(press(&mut keys, 0x1B, true, &table), Verdict::Pass);
        press(&mut keys, 0x1B, false, &table);
        assert_eq!(press(&mut keys, 0x1B, true, &table), Verdict::Swallow);
    }

    #[test]
    fn a_ctrl_tap_during_the_unhook_gap_leaves_no_stale_ctrl() {
        let table = [chord("ctrl+space")];
        let mut keys = KeyState::default();
        keys.begin_context();
        keys.begin_attempt(&VkSet::default());
        press(&mut keys, CTRL, true, &table);
        // Its release fell into the re-hook gap: the retry quarantines Ctrl
        // instead of believing it is still held.
        keys.begin_attempt(&VkSet::default());
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Pass);
        press(&mut keys, SPACE, false, &table);
        // Space alone is not the chord either way; after a physical Ctrl
        // press-and-release cycle the chord works again.
        press(&mut keys, CTRL, false, &table);
        press(&mut keys, CTRL, true, &table);
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Swallow);
    }

    #[test]
    fn holding_a_key_then_adding_the_modifier_never_keeps_it() {
        let table = [chord("ctrl+space")];
        let mut keys = armed();
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Pass);
        press(&mut keys, CTRL, true, &table);
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Pass);
    }

    #[test]
    fn a_refused_claim_passes_the_key() {
        // Full queue, or the focus moved: the remote gets the key.
        let mut keys = armed();
        keys.event(CTRL, true, false, |_| false);
        assert_eq!(keys.event(SPACE, true, false, |_| false), Verdict::Pass);
        assert!(!keys.owns_any());
    }

    #[test]
    fn injected_events_never_change_state() {
        let table = [chord("ctrl+space")];
        let mut keys = armed();
        injected(&mut keys, CTRL, true);
        assert_eq!(injected(&mut keys, SPACE, true), Verdict::Pass);
        // Injected Ctrl did not count as held.
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Pass);
        press(&mut keys, SPACE, false, &table);
        // An injected release of a physically held Ctrl does not drop it.
        press(&mut keys, CTRL, true, &table);
        injected(&mut keys, CTRL, false);
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Swallow);
        // ...and an injected release cannot end a kept key.
        assert_eq!(injected(&mut keys, SPACE, false), Verdict::Pass);
        assert!(keys.owns_any());
    }

    #[test]
    fn ime_and_packet_keys_always_pass() {
        let mut keys = armed();
        press(&mut keys, CTRL, true, &[]);
        for vk in [0xE5, 0xE7, 0xFF, 0x13, 0x03] {
            assert_eq!(
                keys.event(vk, true, false, |_| panic!("claimed {vk:#x}")),
                Verdict::Pass
            );
        }
    }

    #[test]
    fn draining_swallows_only_owned_keys() {
        let table = [chord("ctrl+space")];
        let mut keys = armed();
        press(&mut keys, CTRL, true, &table);
        press(&mut keys, SPACE, true, &table);
        keys.stop_acquiring();
        // Long hold after focus left: repeats keep being swallowed.
        for _ in 0..100 {
            assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Swallow);
        }
        assert_eq!(press(&mut keys, KEY_O, true, &table), Verdict::Pass);
        assert_eq!(press(&mut keys, SPACE, false, &table), Verdict::Swallow);
        assert!(!keys.owns_any());
        // Nothing new is taken while draining.
        press(&mut keys, KEY_O, false, &table);
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Pass);
    }

    #[test]
    fn ownership_survives_a_new_context_and_recovery_lets_go() {
        let table = [chord("ctrl+space")];
        let mut keys = armed();
        press(&mut keys, CTRL, true, &table);
        press(&mut keys, SPACE, true, &table);
        keys.stop_acquiring();
        keys.begin_context();
        assert_eq!(press(&mut keys, SPACE, true, &table), Verdict::Swallow);
        keys.release_all_owned();
        assert!(!keys.owns_any());
    }
}
