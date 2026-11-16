/**
 * Whether a shortcut field is recording keys right now. Back/Forward
 * (Alt+←/→) stays out of the way so such a chord can still be bound.
 */
let active = 0;

/** Mark a recording session; call the returned function when it ends. */
export const beginShortcutCapture = (): (() => void) => {
  active += 1;
  let ended = false;
  return () => {
    if (ended) return;
    ended = true;
    active = Math.max(0, active - 1);
  };
};

export const isShortcutCaptureActive = (): boolean => active > 0;
