import type { TFunction } from "i18next";
import navMap from "../../scripts/nav-map.json";
import type { ShortcutBinding } from "@/bindings";
import {
  TAKE_ONLY_SHORTCUTS,
  takeShortcutsAvailable,
  type OSType,
} from "@/lib/utils/keyboard";
import type { HistoryTab } from "@/stores/navStore";

/**
 * One searchable control. `title` is the translated title the control renders
 * with, which is also how the page finds it to highlight it.
 */
export interface SearchEntry {
  section: string;
  /** For History: the tab the control is on. */
  historyTab?: HistoryTab;
  title: string;
  /** Where it lives, e.g. "Transcription › Paste last transcription". */
  where: string;
  haystack: string;
}

interface NavMapEntry {
  pageKey: string;
  tabKey: string | null;
  group: string | null;
  groupKey: string | null;
  control: string;
  titleKey: string | null;
  descKey: string | null;
  type: string;
  options: string[];
  gating: string;
}

// Mirrors `is_jumper_binding` in src-tauri/src/shortcut/mod.rs.
const isJumperBinding = (id: string) =>
  id.startsWith("anchor_") ||
  id.startsWith("jump_slot_") ||
  id.startsWith("jump_set_slot_");

const plain = (text: string) => text.replace(/\*\*/g, "");

/**
 * The settings index: every control listed in scripts/nav-map.json (the same map
 * the docs checker uses, so keep it current when a control is added) plus every
 * shortcut, which point to the Shortcuts page. Only pages that are currently
 * shown count, so hidden pages (Debug) and other platforms' controls never
 * appear, nor post-processing's own controls while it is switched off.
 */
export const buildSearchIndex = (
  t: TFunction,
  /** Pages currently shown; `group` is the i18n key of the tabbed entry
   * (Advanced settings, More Tools) a page is in, null for sidebar pages. */
  sections: { id: string; labelKey: string; group: string | null }[],
  osType: OSType,
  bindings: Partial<Record<string, ShortcutBinding>>,
  postProcessEnabled: boolean,
  /** Take-only shortcuts that are switched on (Pause, Undo word). */
  enabledTakeShortcuts: string[],
): SearchEntry[] => {
  const sectionByLabelKey = new Map(sections.map((s) => [s.labelKey, s]));
  const entries: SearchEntry[] = [];

  for (const e of navMap as NavMapEntry[]) {
    // Shortcuts are indexed below, pointing at the Shortcuts page.
    if (e.type === "shortcut recorder") continue;
    const section = sectionByLabelKey.get(e.pageKey);
    if (!section) continue;
    if (e.gating === "windows" && osType !== "windows") continue;
    if (e.gating === "linux" && osType !== "linux") continue;
    if (e.gating === "not-linux" && osType === "linux") continue;
    if (e.gating === "post-processing" && !postProcessEnabled) continue;

    const title = e.titleKey ? t(e.titleKey, e.control) : e.control;
    const tab = e.tabKey ? t(e.tabKey) : null;
    const group = e.groupKey ? t(e.groupKey, e.group ?? "") : e.group;
    const description = e.descKey ? plain(t(e.descKey)) : "";
    const page = t(e.pageKey);
    // A group titled like its page adds nothing to the path.
    const where = [
      section.group ? t(section.group) : null,
      page,
      tab,
      group !== page ? group : null,
    ]
      .filter(Boolean)
      .join(" › ");
    entries.push({
      section: section.id,
      historyTab:
        e.pageKey === "sidebar.history" && e.tabKey
          ? (e.tabKey.split(".").pop() as HistoryTab)
          : undefined,
      title,
      where,
      haystack: [title, where, description, ...e.options]
        .join(" ")
        .toLowerCase(),
    });
  }

  const shortcutsPage = t("sidebar.shortcuts");
  for (const [id, binding] of Object.entries(bindings)) {
    if (!binding) continue;
    if (isJumperBinding(id) && osType !== "windows") continue;
    if (TAKE_ONLY_SHORTCUTS.includes(id) && !takeShortcutsAvailable(osType))
      continue;
    if (id === "transcribe_with_post_process" && !postProcessEnabled) continue;
    if (
      (id === "pause" || id === "undo_word") &&
      !enabledTakeShortcuts.includes(id)
    )
      continue;
    const title = t(
      `settings.general.shortcut.bindings.${id}.name`,
      binding.name,
    );
    const description = plain(
      t(
        `settings.general.shortcut.bindings.${id}.description`,
        binding.description,
      ),
    );
    entries.push({
      section: "shortcuts",
      title,
      where: shortcutsPage,
      haystack: [title, description, binding.current_binding, shortcutsPage]
        .join(" ")
        .toLowerCase(),
    });
  }

  return entries;
};

/**
 * Entries containing every word of `query`, best first: title starts with the
 * query, title contains it, every word in the title, then any match.
 */
export const searchSettings = (
  entries: SearchEntry[],
  query: string,
  limit = 20,
): SearchEntry[] => {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const words = q.split(/\s+/);
  const score = (e: SearchEntry) => {
    const title = e.title.toLowerCase();
    if (title.startsWith(q)) return 0;
    if (title.includes(q)) return 1;
    if (words.every((w) => title.includes(w))) return 2;
    return 3;
  };
  return entries
    .filter((e) => words.every((w) => e.haystack.includes(w)))
    .map((e) => ({ e, s: score(e) }))
    .sort((a, b) => a.s - b.s || a.e.title.localeCompare(b.e.title))
    .slice(0, limit)
    .map(({ e }) => e);
};

/**
 * Scroll to the control titled `title` on the page just opened and outline it
 * briefly. The page renders after navigation, so it retries for about a second;
 * if it never shows up (a setting only shown while its parent is on), the
 * `fallback` title (that parent) is outlined instead.
 */
export const highlightSetting = (
  title: string,
  attempt = 0,
  fallback?: string,
) => {
  const el = document.querySelector(
    `[data-setting-title="${CSS.escape(title)}"]`,
  );
  if (!el) {
    if (attempt < 10) {
      setTimeout(() => highlightSetting(title, attempt + 1, fallback), 100);
    } else if (fallback) {
      highlightSetting(fallback);
    }
    return;
  }
  el.scrollIntoView({ block: "center", behavior: "smooth" });
  el.classList.add("search-highlight");
  setTimeout(() => el.classList.remove("search-highlight"), 2000);
};
