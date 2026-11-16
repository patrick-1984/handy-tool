import { useEffect } from "react";
import { SECTIONS_CONFIG, type SidebarSection } from "@/components/Sidebar";
import { useNavStore } from "@/stores/navStore";
import { isShortcutCaptureActive } from "@/lib/shortcutCapture";
import type { NavTarget } from "@/lib/navHistory";
import type { AppSettings } from "@/bindings";

/** Mouse side buttons: 3 = Back, 4 = Forward (MouseEvent.button). */
const BACK_BUTTON = 3;
const FORWARD_BUTTON = 4;

const isEditable = (target: EventTarget | null): boolean => {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
};

/**
 * Back/Forward through the pages visited in this session: the mouse side
 * buttons and Alt+←/Alt+→. The WebView's own history is never used (the app
 * has none), so its default action is always blocked.
 */
export const useBackForward = (
  settings: AppSettings | null | undefined,
  enabled: boolean,
) => {
  useEffect(() => {
    if (!enabled) return;
    // A dialog is open: going to another page would pull the page from under it.
    const dialogOpen = () =>
      document.querySelector('[aria-modal="true"]') !== null;
    const canShow = (target: NavTarget) => {
      const config = SECTIONS_CONFIG[target.section as SidebarSection];
      return !!config && (!settings || config.enabled(settings));
    };
    const go = (direction: "back" | "forward") => {
      const store = useNavStore.getState();
      if (direction === "back") store.goBack(canShow);
      else store.goForward(canShow);
    };

    const isSideButton = (e: MouseEvent) =>
      e.button === BACK_BUTTON || e.button === FORWARD_BUTTON;
    const block = (e: MouseEvent) => {
      if (isSideButton(e)) e.preventDefault();
    };
    const onMouseUp = (e: MouseEvent) => {
      if (!isSideButton(e)) return;
      e.preventDefault();
      if (dialogOpen()) return;
      go(e.button === BACK_BUTTON ? "back" : "forward");
    };
    const onKeyDown = (e: KeyboardEvent) => {
      // Alt+←/→, and on macOS also Cmd+[ / Cmd+] as in its browsers.
      const alt =
        e.altKey &&
        !e.ctrlKey &&
        !e.metaKey &&
        !e.shiftKey &&
        (e.key === "ArrowLeft" || e.key === "ArrowRight");
      const cmd =
        e.metaKey &&
        !e.ctrlKey &&
        !e.altKey &&
        !e.shiftKey &&
        (e.key === "[" || e.key === "]");
      if (!alt && !cmd) return;
      // Option+←/→ moves by word in a text field (macOS), and a shortcut
      // field may be recording this very chord.
      if (isEditable(e.target) || isShortcutCaptureActive() || dialogOpen())
        return;
      e.preventDefault();
      go(e.key === "ArrowLeft" || e.key === "[" ? "back" : "forward");
    };

    window.addEventListener("mousedown", block, true);
    window.addEventListener("auxclick", block, true);
    window.addEventListener("mouseup", onMouseUp, true);
    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      window.removeEventListener("mousedown", block, true);
      window.removeEventListener("auxclick", block, true);
      window.removeEventListener("mouseup", onMouseUp, true);
      window.removeEventListener("keydown", onKeyDown, true);
    };
  }, [settings, enabled]);
};
