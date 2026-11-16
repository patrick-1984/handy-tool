/**
 * How the sidebar is arranged (2.7.0): the user's order, what sits under
 * "Show more", and the pinned pages. Pure, so it can be tested; the stored
 * layout is checked again here (a hand-edited or newer settings file must not
 * break the sidebar).
 */
import type { SidebarLayout, SidebarShortcut } from "@/bindings";

/** The two sidebar entries that open a row of tabs (see MORE_GROUPS). */
export const GROUP_IDS = ["more-tools", "more-settings"] as const;

/** History's tabs, the only tabs outside the two groups. */
export const HISTORY_TABS = [
  "recordings",
  "favorites",
  "statistics",
  "settings",
] as const;

export interface ResolvedSidebar {
  /** Entry ids shown in the sidebar, in order. */
  shown: string[];
  /** Entry ids under "Show more", in order. */
  more: string[];
  /** Pinned pages shown at the top, and those under "Show more". */
  pinsTop: SidebarShortcut[];
  pinsMore: SidebarShortcut[];
  /** Nothing arranged: the sidebar looks exactly as before 2.7.0. */
  isDefault: boolean;
}

/**
 * The user's order with every known entry in it: unknown ids are dropped and
 * entries the order does not name (new in this version, say) keep their
 * default place - right after the entry before them by default.
 */
export const orderedEntries = (
  defaults: string[],
  saved: string[] | undefined,
): string[] => {
  const known = new Set(defaults);
  const order = (saved ?? []).filter((id) => known.has(id));
  if (order.length === 0) return [...defaults];
  const result = [...order];
  defaults.forEach((id, index) => {
    if (result.includes(id)) return;
    const before = defaults
      .slice(0, index)
      .reverse()
      .find((d) => result.includes(d));
    const at = before === undefined ? 0 : result.indexOf(before) + 1;
    result.splice(at, 0, id);
  });
  return result;
};

/** A shortcut to a page that exists (and, for History, a real tab). */
export const isValidShortcut = (
  s: SidebarShortcut,
  sectionExists: (id: string) => boolean,
): boolean =>
  sectionExists(s.section) &&
  (s.history_tab == null ||
    (s.section === "history" &&
      (HISTORY_TABS as readonly string[]).includes(s.history_tab)));

export const resolveSidebar = (
  layout: SidebarLayout | null | undefined,
  defaults: string[],
  isShown: (id: string) => boolean,
  sectionExists: (id: string) => boolean,
): ResolvedSidebar => {
  const order = orderedEntries(defaults, layout?.order).filter(isShown);
  const inMore = new Set(layout?.in_more ?? []);
  const pins = (layout?.shortcuts ?? []).filter(
    (s) => isValidShortcut(s, sectionExists) && isShown(s.section),
  );
  return {
    shown: order.filter((id) => !inMore.has(id)),
    more: order.filter((id) => inMore.has(id)),
    pinsTop: pins.filter((s) => !s.in_more),
    pinsMore: pins.filter((s) => s.in_more),
    isDefault: (layout?.order ?? []).length === 0 && inMore.size === 0,
  };
};

export const samePinTarget = (
  s: SidebarShortcut,
  section: string,
  historyTab?: string | null,
): boolean =>
  s.section === section && (s.history_tab ?? null) === (historyTab ?? null);

/** Most pins kept (the backend keeps no more either). */
export const MAX_PINS = 50;

const newId = () =>
  `pin-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;

/**
 * Pin the page, or unpin it if it is pinned. Returns null when the limit is
 * reached.
 */
export const togglePin = (
  layout: SidebarLayout | null | undefined,
  section: string,
  historyTab?: string | null,
): SidebarLayout | null => {
  const shortcuts = layout?.shortcuts ?? [];
  const pinned = shortcuts.some((s) => samePinTarget(s, section, historyTab));
  if (pinned) {
    return {
      ...layout,
      shortcuts: shortcuts.filter(
        (s) => !samePinTarget(s, section, historyTab),
      ),
    };
  }
  if (shortcuts.length >= MAX_PINS) return null;
  return {
    ...layout,
    shortcuts: [
      ...shortcuts,
      {
        id: newId(),
        section,
        history_tab: historyTab ?? null,
        name: null,
        icon: "pin",
        in_more: false,
      },
    ],
  };
};

/** Moves the item at `from` one place up (-1) or down (+1). */
export const moveBy = <T>(list: T[], from: number, by: -1 | 1): T[] => {
  const to = from + by;
  if (from < 0 || from >= list.length || to < 0 || to >= list.length)
    return list;
  const next = [...list];
  [next[from], next[to]] = [next[to], next[from]];
  return next;
};
