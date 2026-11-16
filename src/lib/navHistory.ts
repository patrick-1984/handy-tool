/**
 * In-app Back/Forward (2.7.0): the pages and History tabs visited in this
 * session, like a browser's history. Pure, so the store only swaps values.
 */

/** A place in the app: a page, and for History the tab shown. */
export interface NavTarget {
  section: string;
  historyTab?: string;
}

export interface NavHistory {
  back: NavTarget[];
  forward: NavTarget[];
}

/** How many steps back are kept. */
export const NAV_HISTORY_LIMIT = 50;

export const EMPTY_HISTORY: NavHistory = { back: [], forward: [] };

export const sameTarget = (a: NavTarget, b: NavTarget): boolean =>
  a.section === b.section && (a.historyTab ?? "") === (b.historyTab ?? "");

/**
 * The user moved from `from` to `to`: `from` goes on the back stack and the
 * forward stack is dropped, as in a browser. Staying in place records nothing.
 */
export const recordVisit = (
  history: NavHistory,
  from: NavTarget,
  to: NavTarget,
): NavHistory => {
  if (sameTarget(from, to)) return history;
  const back = [...history.back, from].slice(-NAV_HISTORY_LIMIT);
  return { back, forward: [] };
};

/**
 * One step back (or forward), skipping places that can no longer be shown
 * (a page switched off since). Returns null when there is nowhere to go.
 */
export const step = (
  history: NavHistory,
  current: NavTarget,
  direction: "back" | "forward",
  canShow: (target: NavTarget) => boolean,
): { history: NavHistory; target: NavTarget } | null => {
  const from = direction === "back" ? [...history.back] : [...history.forward];
  const to = direction === "back" ? [...history.forward] : [...history.back];
  while (from.length > 0) {
    const target = from.pop() as NavTarget;
    if (!canShow(target) || sameTarget(target, current)) continue;
    to.push(current);
    const next =
      direction === "back"
        ? { back: from, forward: to }
        : { back: to, forward: from };
    return { history: next, target };
  }
  return null;
};
