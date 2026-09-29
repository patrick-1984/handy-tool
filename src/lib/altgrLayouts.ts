import { useEffect, useState } from "react";
import { commands, type AltGrLayout } from "@/bindings";

/**
 * The installed keyboards with AltGr characters (see
 * src-tauri/src/keyboard_layouts.rs), asked once and shared by every shortcut
 * row; asked again when the window regains focus, as keyboards can be added or
 * removed meanwhile. `null`: not known here (not Windows).
 */
let cached: Promise<AltGrLayout[] | null> | null = null;
window.addEventListener("focus", () => {
  cached = null;
});

const loadAltGrLayouts = (): Promise<AltGrLayout[] | null> =>
  (cached ??= commands.getAltgrLayouts().catch(() => null));

/**
 * The keyboards, kept current: `undefined` until known, `null` where they
 * cannot be told. Registered after the module's own focus listener, so a
 * focus always fetches afresh.
 */
export const useAltGrLayouts = (): AltGrLayout[] | null | undefined => {
  const [layouts, setLayouts] = useState<AltGrLayout[] | null | undefined>();
  useEffect(() => {
    let live = true;
    const load = () =>
      void loadAltGrLayouts().then((l) => live && setLayouts(l));
    load();
    window.addEventListener("focus", load);
    return () => {
      live = false;
      window.removeEventListener("focus", load);
    };
  }, []);
  return layouts;
};

/** The key a chord ends on, as the layouts list it: "o", "space". */
export const chordKey = (binding: string): string =>
  (binding.split("+").pop() ?? "")
    .trim()
    .toLowerCase()
    .replace(/_(?:left|right)$/, "");

/** Whether a chord holds Shift too (then AltGr+Shift is what it collides with). */
export const chordHasShift = (binding: string): boolean =>
  binding
    .toLowerCase()
    .split("+")
    .some((p) => p.trim().replace(/_(?:left|right)$/, "") === "shift");
