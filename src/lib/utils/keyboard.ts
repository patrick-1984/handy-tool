/**
 * Keyboard utility functions for handling keyboard events
 */

export type OSType = "macos" | "windows" | "linux" | "unknown";

/**
 * Extract a consistent key name from a KeyboardEvent
 * This function provides cross-platform keyboard event handling
 * and returns key names appropriate for the target operating system
 */
export const getKeyName = (
  e: KeyboardEvent,
  osType: OSType = "unknown",
): string => {
  // Handle special cases first
  if (e.code) {
    const code = e.code;

    // Handle function keys (F1-F24)
    if (code.match(/^F\d+$/)) {
      return code.toLowerCase(); // F1, F2, ..., F14, F15, etc.
    }

    // Handle regular letter keys (KeyA -> a)
    if (code.match(/^Key[A-Z]$/)) {
      return code.replace("Key", "").toLowerCase();
    }

    // Handle digit keys (Digit0 -> 0)
    if (code.match(/^Digit\d$/)) {
      return code.replace("Digit", "");
    }

    // Handle numpad digit keys (Numpad0 -> numpad 0)
    if (code.match(/^Numpad\d$/)) {
      return code.replace("Numpad", "numpad ").toLowerCase();
    }

    // Handle modifier keys - OS-specific naming
    const getModifierName = (baseModifier: string): string => {
      switch (baseModifier) {
        case "shift":
          return "shift";
        case "ctrl":
          return osType === "macos" ? "ctrl" : "ctrl";
        case "alt":
          return osType === "macos" ? "option" : "alt";
        case "meta":
          // Windows key on Windows/Linux, Command key on Mac
          if (osType === "macos") return "command";
          return "super";
        default:
          return baseModifier;
      }
    };

    const modifierMap: Record<string, string> = {
      ShiftLeft: getModifierName("shift"),
      ShiftRight: getModifierName("shift"),
      ControlLeft: getModifierName("ctrl"),
      ControlRight: getModifierName("ctrl"),
      AltLeft: getModifierName("alt"),
      AltRight: getModifierName("alt"),
      MetaLeft: getModifierName("meta"),
      MetaRight: getModifierName("meta"),
      OSLeft: getModifierName("meta"),
      OSRight: getModifierName("meta"),
      CapsLock: "caps lock",
      Tab: "tab",
      Enter: "enter",
      Space: "space",
      Backspace: "backspace",
      Delete: "delete",
      Escape: "esc",
      ArrowUp: "up",
      ArrowDown: "down",
      ArrowLeft: "left",
      ArrowRight: "right",
      Home: "home",
      End: "end",
      PageUp: "page up",
      PageDown: "page down",
      Insert: "insert",
      PrintScreen: "print screen",
      ScrollLock: "scroll lock",
      Pause: "pause",
      ContextMenu: "menu",
      NumpadMultiply: "numpad *",
      NumpadAdd: "numpad +",
      NumpadSubtract: "numpad -",
      NumpadDecimal: "numpad .",
      NumpadDivide: "numpad /",
      NumLock: "num lock",
    };

    if (modifierMap[code]) {
      return modifierMap[code];
    }

    // Handle punctuation and special characters
    const punctuationMap: Record<string, string> = {
      Semicolon: ";",
      Equal: "=",
      Comma: ",",
      Minus: "-",
      Period: ".",
      Slash: "/",
      Backquote: "`",
      BracketLeft: "[",
      Backslash: "\\",
      BracketRight: "]",
      Quote: "'",
    };

    if (punctuationMap[code]) {
      return punctuationMap[code];
    }

    // For any other codes, try to convert to a reasonable format
    return code.toLowerCase().replace(/([a-z])([A-Z])/g, "$1 $2");
  }

  // Fallback to e.key if e.code is not available
  if (e.key) {
    const key = e.key;

    // Handle special key names with OS-specific formatting
    const keyMap: Record<string, string> = {
      Control: osType === "macos" ? "ctrl" : "ctrl",
      Alt: osType === "macos" ? "option" : "alt",
      Shift: "shift",
      Meta:
        osType === "macos" ? "command" : osType === "windows" ? "win" : "super",
      OS:
        osType === "macos" ? "command" : osType === "windows" ? "win" : "super",
      CapsLock: "caps lock",
      ArrowUp: "up",
      ArrowDown: "down",
      ArrowLeft: "left",
      ArrowRight: "right",
      Escape: "esc",
      " ": "space",
    };

    if (keyMap[key]) {
      return keyMap[key];
    }

    return key.toLowerCase();
  }

  // Last resort fallback
  return `unknown-${e.keyCode || e.which || 0}`;
};

/**
 * Capitalize a key name for display (e.g. "space" -> "Space", "f1" -> "F1")
 */
const capitalizeKey = (key: string): string => {
  // fn key: keep lowercase
  if (key === "fn") return "fn";
  // Function keys: f1 -> F1
  if (/^f\d+$/.test(key)) return key.toUpperCase();
  // Single char: a -> A
  if (key.length === 1) return key.toUpperCase();
  // Multi-word: capitalize first letter of each word
  return key.replace(/\b\w/g, (c) => c.toUpperCase());
};

/**
 * Format a single key part for display.
 * Handles _left/_right suffixes and capitalizes names.
 * e.g. "shift_left" -> "Left Shift", "option" -> "Option", "space" -> "Space"
 */
const formatKeyPart = (part: string): string => {
  const trimmed = part.trim();
  if (!trimmed) return "";

  if (trimmed.endsWith("_left")) {
    const name = trimmed.slice(0, -5);
    return `Left ${capitalizeKey(name)}`;
  }
  if (trimmed.endsWith("_right")) {
    const name = trimmed.slice(0, -6);
    return `Right ${capitalizeKey(name)}`;
  }

  return capitalizeKey(trimmed);
};

/**
 * Get display-friendly key combination string for the current OS
 * Formats raw hotkey strings like "option_left+shift+space" into
 * human-readable form like "Left Option + Shift + Space"
 */
export const formatKeyCombination = (
  combination: string,
  _osType: OSType,
): string => {
  if (!combination) return "";
  return combination.split("+").map(formatKeyPart).join(" + ");
};

/**
 * Normalize modifier keys to handle left/right variants
 */
export const normalizeKey = (key: string): string => {
  // Handle left/right variants of modifier keys
  if (key.startsWith("left ") || key.startsWith("right ")) {
    const parts = key.split(" ");
    if (parts.length === 2) {
      // Return just the modifier name without left/right prefix
      return parts[1];
    }
  }
  return key;
};

/**
 * True when `binding` is a chord that AltGr can type a character with.
 *
 * Mirrors `is_altgr_risky_chord` in `src-tauri/src/settings.rs` — keep the two
 * in step. Windows reports AltGr as Ctrl+Alt, so a global `ctrl+alt+<letter>`
 * hotkey fires instead of typing the character European layouts put there (the
 * Polish Programmers layout maps AltGr + a c e l n o s x z to ą ć ę ł ń ó ś ź ż).
 * Space counts too, because AltGr is routinely still held down when the space
 * after an accented word is pressed.
 *
 * Digits are deliberately not flagged: no common European layout maps
 * AltGr+<digit> to a character, and the Jumper's slot chords rely on that.
 */
export const isAltGrRiskyChord = (binding: string): boolean => {
  // Side-qualified modifiers ("ctrl_left", "alt_right") are what the handy-keys
  // backend stores, and AltGr IS the right Alt — stripping the side is the case
  // that matters most, not an edge case.
  const parts = binding
    .split("+")
    .map((part) =>
      part
        .trim()
        .toLowerCase()
        .replace(/_(?:left|right)$/, ""),
    )
    .filter((part) => part.length > 0);

  const hasCtrl = parts.some((p) => p === "ctrl" || p === "control");
  const hasAlt = parts.some((p) => p === "alt" || p === "option");
  if (!hasCtrl || !hasAlt) return false;

  return parts.some((p) => p === "space" || /^[a-z]$/.test(p));
};
