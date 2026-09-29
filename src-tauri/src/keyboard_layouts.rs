//! Which installed keyboard layouts type something with AltGr, and on which keys.
//!
//! Windows reports AltGr as Ctrl+Alt, so a `ctrl+alt+<letter>` shortcut fires
//! instead of the character AltGr puts on that letter. Whether that happens
//! depends on the keyboard layout (Polish, German, French, Czech... have AltGr
//! characters; US English has none), not on the dictation language, so the
//! Shortcuts pages ask here and warn only for keys a layout really uses.

use serde::Serialize;
use specta::Type;

/// A keyboard language (Windows lists keyboards under a language) with a
/// keyboard on which AltGr types a character on some keys.
#[derive(Serialize, Type, Clone, Debug, PartialEq)]
pub struct AltGrLayout {
    /// The language the keyboard belongs to, as a BCP 47 tag, e.g. "pl-PL" (the
    /// UI names it). Several keyboards of one language are merged.
    pub locale: String,
    /// Lower-case letters AltGr types a character (or an accent) on, plus
    /// "space": AltGr is often still held for the space after such a character.
    pub keys: Vec<String>,
    /// The same for AltGr+Shift (a Ctrl+Alt+Shift chord).
    pub shift_keys: Vec<String>,
}

/// The installed layouts with AltGr characters; `None` where this cannot be
/// told (not Windows), so the caller falls back to warning for every Ctrl+Alt
/// letter.
#[tauri::command]
#[specta::specta]
pub fn get_altgr_layouts() -> Option<Vec<AltGrLayout>> {
    #[cfg(windows)]
    {
        Some(windows_impl::altgr_layouts())
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
mod windows_impl {
    use super::AltGrLayout;
    use windows::Win32::Globalization::LCIDToLocaleName;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardLayoutList, HKL, MAPVK_VK_TO_VSC, MapVirtualKeyExW, ToUnicodeEx, VK_CONTROL,
        VK_LCONTROL, VK_LSHIFT, VK_MENU, VK_RMENU, VK_SHIFT,
    };

    /// ToUnicodeEx flag: do not change the keyboard state (no dead key left
    /// behind for the next real key press). Windows 10 1607 and later.
    const NO_STATE_CHANGE: u32 = 0x4;

    pub fn altgr_layouts() -> Vec<AltGrLayout> {
        // SAFETY: plain Win32 calls with buffers sized as the API asks.
        let layouts = unsafe {
            let count = GetKeyboardLayoutList(None);
            let mut list = vec![HKL::default(); count.max(0) as usize];
            let got = GetKeyboardLayoutList(Some(&mut list));
            list.truncate(got.max(0) as usize);
            list
        };

        // Ctrl and Alt held, as Windows sees AltGr; and with Shift too.
        let mut state = [0u8; 256];
        for vk in [VK_CONTROL, VK_LCONTROL, VK_MENU, VK_RMENU] {
            state[vk.0 as usize] = 0x80;
        }
        let mut shift_state = state;
        for vk in [VK_SHIFT, VK_LSHIFT] {
            shift_state[vk.0 as usize] = 0x80;
        }

        let mut found: Vec<AltGrLayout> = Vec::new();
        for hkl in layouts {
            let letters = |state: &[u8; 256]| -> Vec<String> {
                (b'A'..=b'Z')
                    .filter(|&letter| types_something(hkl, letter as u32, state))
                    .map(|letter| (letter as char).to_ascii_lowercase().to_string())
                    .collect()
            };
            let mut keys = letters(&state);
            let mut shift_keys = letters(&shift_state);
            if keys.is_empty() && shift_keys.is_empty() {
                continue;
            }
            // Space: not typed with AltGr, but AltGr is often still held for the
            // space after an AltGr character - so it counts on such a keyboard.
            keys.push("space".to_string());
            shift_keys.push("space".to_string());
            let locale = locale_of(hkl);
            // Several keyboards of one language (e.g. two Polish ones): one entry.
            match found.iter_mut().find(|l| l.locale == locale) {
                Some(entry) => {
                    merge(&mut entry.keys, keys);
                    merge(&mut entry.shift_keys, shift_keys);
                }
                None => found.push(AltGrLayout {
                    locale,
                    keys,
                    shift_keys,
                }),
            }
        }
        found
    }

    fn merge(into: &mut Vec<String>, more: Vec<String>) {
        for key in more {
            if !into.contains(&key) {
                into.push(key);
            }
        }
    }

    /// Does AltGr+<vk> type a printable character (or start an accent) here?
    fn types_something(hkl: HKL, vk: u32, state: &[u8; 256]) -> bool {
        let mut buf = [0u16; 8];
        // SAFETY: as above; `state` and `buf` outlive the calls.
        let written = unsafe {
            let scan = MapVirtualKeyExW(vk, MAPVK_VK_TO_VSC, Some(hkl));
            ToUnicodeEx(vk, scan, state, &mut buf, NO_STATE_CHANGE, Some(hkl))
        };
        // Negative: a dead key (an accent waiting for the next letter).
        written < 0
            || buf[..written.max(0) as usize]
                .iter()
                .any(|&unit| unit >= 0x20 && unit != 0x7f)
    }

    /// The language the keyboard belongs to (low word of the HKL) as "pl-PL".
    /// Not the keyboard itself: a Polish keyboard added to English (United
    /// States) reports en-US, which is how Windows Settings lists it too.
    fn locale_of(hkl: HKL) -> String {
        let lang_id = (hkl.0 as usize & 0xffff) as u32;
        let mut name = [0u16; 85];
        // SAFETY: as above.
        let len = unsafe { LCIDToLocaleName(lang_id, Some(&mut name), 0) };
        if len > 1 {
            String::from_utf16_lossy(&name[..(len - 1) as usize])
        } else {
            format!("{lang_id:04x}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn altgr_layouts_lists_only_letters_and_space() {
        // Whatever layouts this machine has: the call works and the keys are
        // what the Shortcuts pages compare against.
        #[cfg(windows)]
        for layout in get_altgr_layouts().expect("Windows answers") {
            assert!(!layout.locale.is_empty());
            assert!(layout.keys.contains(&"space".to_string()));
            assert!(layout.shift_keys.contains(&"space".to_string()));
            for key in layout.keys.iter().chain(&layout.shift_keys) {
                assert!(
                    key == "space"
                        || (key.len() == 1 && key.chars().all(|c| c.is_ascii_lowercase())),
                    "unexpected key {key}"
                );
            }
        }
        #[cfg(not(windows))]
        assert!(get_altgr_layouts().is_none());
    }
}
