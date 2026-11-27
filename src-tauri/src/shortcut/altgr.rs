//! AltGr is not Alt: in a shortcut, Alt means the LEFT Alt key.
//!
//! On Windows AltGr (the right Alt key of Polish, German, French, Czech...
//! layouts, where it types ą ę ś @ { ) is reported as left Ctrl + right Alt.
//! A global `alt+r` or `ctrl+alt+r` shortcut must not fire for it: that would
//! swallow the character the user was typing (and, the other way round, a
//! right Alt that types nothing on a US layout must not count as Alt either).
//!
//! The decisions live here as pure functions, so they are tested on every
//! platform. The Windows calls are a thin shell around them (`async_key_down`).
//! Other platforms behave exactly as before: nothing here is consulted there.
//!
//! - Tauri backend (`RegisterHotKey`): the OS cannot tell left Alt from right
//!   Alt, so the event is checked when it arrives (`GuardState`).
//! - handy-keys backend: it sees the sides, so a chord's Alt is narrowed to
//!   left Alt when it is registered (`alt_means_left_only`). A chord that names
//!   the right side (`alt_right+...`) is kept as the user wrote it.

use std::collections::HashSet;

/// Is the right Alt key held without the left one: AltGr (or a plain right Alt)?
/// With both Alt keys down the left one is really there, so the chord is genuine.
pub(crate) fn altgr_held(left_alt_down: bool, right_alt_down: bool) -> bool {
    right_alt_down && !left_alt_down
}

/// Must a hotkey event be ignored because the "Alt" (and Ctrl) it saw is AltGr?
/// Only chords that use Alt and/or Ctrl are affected; Windows reports AltGr as
/// exactly that pair.
pub(crate) fn ignore_for_altgr(
    uses_alt: bool,
    uses_ctrl: bool,
    left_alt_down: bool,
    right_alt_down: bool,
) -> bool {
    (uses_alt || uses_ctrl) && altgr_held(left_alt_down, right_alt_down)
}

/// Which presses were ignored, so their releases are ignored too: a release
/// without a delivered press would stop a push-to-talk take that never started.
#[derive(Default)]
pub(crate) struct GuardState {
    ignored: HashSet<String>,
}

impl GuardState {
    /// Should this event reach the shortcut handler? `ignore_press` is
    /// `ignore_for_altgr(..)` and only matters for a press; a release follows
    /// its press (delivered or ignored), whatever keys are down by then.
    pub(crate) fn deliver(
        &mut self,
        binding_id: &str,
        is_pressed: bool,
        ignore_press: bool,
    ) -> bool {
        if is_pressed {
            if ignore_press {
                self.ignored.insert(binding_id.to_string());
                false
            } else {
                self.ignored.remove(binding_id);
                true
            }
        } else {
            !self.ignored.remove(binding_id)
        }
    }
}

/// A chord's Alt narrowed to the left Alt key. A hotkey that asks for Alt
/// without a side matches either side in handy-keys; this keeps only the left
/// bit. A side the user chose explicitly (`alt_right`) is left alone.
pub(crate) fn alt_means_left_only(modifiers: handy_keys::Modifiers) -> handy_keys::Modifiers {
    use handy_keys::Modifiers;
    if modifiers.contains(Modifiers::OPT) {
        modifiers & !Modifiers::OPT_RIGHT
    } else {
        modifiers
    }
}

/// Is this key held right now (high bit of GetAsyncKeyState)? Windows only.
#[cfg(windows)]
pub(crate) fn async_key_down(vk: u16) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    (unsafe { GetAsyncKeyState(i32::from(vk)) } as u16 & 0x8000) != 0
}

/// Left and right Alt as held now; `(false, false)` where there is no AltGr to
/// guard against (everything but Windows).
pub(crate) fn alt_keys_down() -> (bool, bool) {
    #[cfg(windows)]
    {
        const VK_LMENU: u16 = 0xA4;
        const VK_RMENU: u16 = 0xA5;
        (async_key_down(VK_LMENU), async_key_down(VK_RMENU))
    }
    #[cfg(not(windows))]
    {
        (false, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use handy_keys::Modifiers;

    #[test]
    fn right_alt_alone_is_altgr_but_left_alt_or_both_is_alt() {
        assert!(altgr_held(false, true));
        assert!(!altgr_held(true, false));
        assert!(!altgr_held(true, true));
        assert!(!altgr_held(false, false));
    }

    #[test]
    fn altgr_is_ignored_for_alt_and_ctrl_chords_only() {
        // alt+r, ctrl+alt+r, ctrl+r with AltGr down: ignored.
        assert!(ignore_for_altgr(true, false, false, true));
        assert!(ignore_for_altgr(true, true, false, true));
        assert!(ignore_for_altgr(false, true, false, true));
        // A chord with neither (shift+f5, f9) is not touched.
        assert!(!ignore_for_altgr(false, false, false, true));
        // Real left Alt: delivered.
        assert!(!ignore_for_altgr(true, true, true, false));
        assert!(!ignore_for_altgr(true, false, true, true));
        // No Alt held at all.
        assert!(!ignore_for_altgr(true, true, false, false));
    }

    #[test]
    fn an_ignored_press_ignores_its_release_too() {
        let mut guard = GuardState::default();
        assert!(!guard.deliver("transcribe", true, true));
        // AltGr already up when the key comes up: still ignored.
        assert!(!guard.deliver("transcribe", false, false));
        // The next real press and release go through.
        assert!(guard.deliver("transcribe", true, false));
        assert!(guard.deliver("transcribe", false, false));
    }

    #[test]
    fn a_delivered_press_always_gets_its_release() {
        let mut guard = GuardState::default();
        assert!(guard.deliver("transcribe", true, false));
        // AltGr pressed while the chord is held must not eat the release.
        assert!(guard.deliver("transcribe", false, false));
        // A release with no press seen at all is delivered, as before.
        assert!(guard.deliver("other", false, false));
    }

    #[test]
    fn ignoring_one_binding_does_not_touch_another() {
        let mut guard = GuardState::default();
        assert!(!guard.deliver("a", true, true));
        assert!(guard.deliver("b", true, false));
        assert!(guard.deliver("b", false, false));
        assert!(!guard.deliver("a", false, false));
    }

    #[test]
    fn a_plain_alt_chord_narrows_to_left_alt() {
        assert_eq!(alt_means_left_only(Modifiers::OPT), Modifiers::OPT_LEFT);
        assert_eq!(
            alt_means_left_only(Modifiers::CTRL | Modifiers::OPT | Modifiers::SHIFT),
            Modifiers::CTRL | Modifiers::OPT_LEFT | Modifiers::SHIFT
        );
    }

    #[test]
    fn explicit_sides_and_chords_without_alt_are_kept() {
        assert_eq!(
            alt_means_left_only(Modifiers::OPT_RIGHT),
            Modifiers::OPT_RIGHT
        );
        assert_eq!(
            alt_means_left_only(Modifiers::CTRL_LEFT | Modifiers::OPT_RIGHT),
            Modifiers::CTRL_LEFT | Modifiers::OPT_RIGHT
        );
        assert_eq!(
            alt_means_left_only(Modifiers::OPT_LEFT),
            Modifiers::OPT_LEFT
        );
        assert_eq!(alt_means_left_only(Modifiers::CTRL), Modifiers::CTRL);
        assert_eq!(alt_means_left_only(Modifiers::empty()), Modifiers::empty());
    }

    #[test]
    fn narrowed_alt_no_longer_matches_altgr_but_matches_left_alt() {
        let altgr_event = Modifiers::CTRL_LEFT | Modifiers::OPT_RIGHT;
        let left_alt_event = Modifiers::CTRL_LEFT | Modifiers::OPT_LEFT;
        let chord = Modifiers::CTRL | Modifiers::OPT;
        // Before: ctrl+alt matched AltGr (the bug).
        assert!(chord.matches(altgr_event));
        let narrowed = alt_means_left_only(chord);
        assert!(!narrowed.matches(altgr_event));
        assert!(narrowed.matches(left_alt_event));
        // Alt alone: a bare right Alt (a US layout) no longer counts as Alt.
        let alt = alt_means_left_only(Modifiers::OPT);
        assert!(alt.matches(Modifiers::OPT_LEFT));
        assert!(!alt.matches(Modifiers::OPT_RIGHT));
        // An explicit right-side chord still works.
        assert!(Modifiers::OPT_RIGHT.matches(Modifiers::OPT_RIGHT));
    }
}
