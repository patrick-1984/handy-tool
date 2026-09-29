import { create } from "zustand";

/**
 * Help mode: the ? button at the top right of every page. While it is on,
 * pointing at a setting shows its description, and nothing can be changed:
 * any click (or Enter/Space on a focused control) only leaves the mode, and so
 * do the ? button and Esc. Scrolling (wheel, keys, the scrollbar) still works.
 */
interface HelpModeStore {
  on: boolean;
  setOn: (on: boolean) => void;
}

export const useHelpMode = create<HelpModeStore>((set) => ({
  on: false,
  setOn: (on) => set({ on }),
}));

// The help cursor everywhere while it is on (see App.css).
useHelpMode.subscribe((state) =>
  document.documentElement.classList.toggle("help-mode", state.on),
);

const leave = () => useHelpMode.getState().setOn(false);

/**
 * A press on a scroll container's own scrollbar (dragging it scrolls): outside
 * its client area but inside its box. Measured from its edges (offsetX can be
 * relative to another element); x < 0 is a scrollbar on the left (Arabic).
 */
const onScrollbar = (e: PointerEvent): boolean => {
  const el = e.target;
  if (!(el instanceof HTMLElement)) return false;
  const scrolls =
    el.scrollHeight > el.clientHeight || el.scrollWidth > el.clientWidth;
  const box = el.getBoundingClientRect();
  const x = e.clientX - box.left - el.clientLeft;
  const y = e.clientY - box.top - el.clientTop;
  return scrolls && (x < 0 || x >= el.clientWidth || y >= el.clientHeight);
};

// The press that leaves help mode must not reach the control under it: the
// mouse events that follow it are swallowed too. Listeners on the window in
// the capture phase run before any control's own, and they read the state as
// it is at that moment (React may not have re-rendered yet).
let swallowUntil = 0;
const swallowFollowUp = (e: Event) => {
  if (performance.now() >= swallowUntil) return;
  e.preventDefault();
  e.stopPropagation();
  if (e.type === "click") swallowUntil = 0;
};

window.addEventListener(
  "pointerdown",
  (e) => {
    if (!useHelpMode.getState().on || onScrollbar(e)) return;
    e.preventDefault();
    e.stopPropagation();
    swallowUntil = performance.now() + 1000;
    leave();
  },
  true,
);
for (const type of [
  "mousedown",
  "mouseup",
  "click",
  "dblclick",
  "contextmenu",
]) {
  window.addEventListener(type, swallowFollowUp, true);
}

window.addEventListener(
  "keydown",
  (e) => {
    if (!useHelpMode.getState().on) return;
    // Space on the page itself scrolls; on a focused control it would press it.
    const onControl =
      document.activeElement !== null &&
      document.activeElement !== document.body;
    if (
      e.key === "Escape" ||
      e.key === "Enter" ||
      (e.key === " " && onControl)
    ) {
      e.preventDefault();
      e.stopPropagation();
      leave();
    }
  },
  true,
);
