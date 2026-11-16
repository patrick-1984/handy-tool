import { create } from "zustand";
import type { SidebarSection } from "@/components/Sidebar";
import {
  EMPTY_HISTORY,
  recordVisit,
  step,
  type NavHistory,
  type NavTarget,
} from "@/lib/navHistory";

export type HistoryTab = "recordings" | "favorites" | "statistics" | "settings";
/** The feature setups that run on the Setups page. */
export type FeatureSetup =
  | "appearance"
  | "postProcessing"
  | "jumper"
  | "remote"
  | "history";

interface NavStore {
  currentSection: SidebarSection;
  /** Advanced settings' tab last open, so that sidebar entry returns to it. */
  lastMoreSection: SidebarSection;
  /** More Tools' tab last open, likewise. */
  lastToolsSection: SidebarSection;
  /** The History page's tab; opening History from the sidebar shows Recordings. */
  historyTab: HistoryTab;
  setCurrentSection: (section: SidebarSection) => void;
  setLastMoreSection: (section: SidebarSection) => void;
  setLastToolsSection: (section: SidebarSection) => void;
  setHistoryTab: (tab: HistoryTab) => void;
  /** Open a page; for History, optionally on a given tab (search results). */
  navigateTo: (section: SidebarSection, historyTab?: HistoryTab) => void;
  /** A setup to start when the Setups page opens (from a button elsewhere). */
  pendingSetup: FeatureSetup | null;
  /** Open Setups and start that setup there. */
  startSetup: (setup: FeatureSetup) => void;
  clearPendingSetup: () => void;
  /** Pages and History tabs visited this session (Back/Forward). */
  navHistory: NavHistory;
  /** Back / Forward one step; `canShow` skips pages switched off since. */
  goBack: (canShow: (target: NavTarget) => boolean) => void;
  goForward: (canShow: (target: NavTarget) => boolean) => void;
}

/** Where the page open last is kept (Reopen Last Page). */
const LAST_PAGE_KEY = "handy.lastPage";

const rememberPage = (section: SidebarSection) => {
  try {
    localStorage.setItem(LAST_PAGE_KEY, section);
  } catch {
    // No storage (private mode): the app just opens on General next time.
  }
};

/** The page open when the app was last closed, if any. */
export const lastPage = (): string | null => {
  try {
    return localStorage.getItem(LAST_PAGE_KEY);
  } catch {
    return null;
  }
};

/** Where the app is now, as Back/Forward sees it. */
const targetOf = (state: {
  currentSection: SidebarSection;
  historyTab: HistoryTab;
}): NavTarget =>
  state.currentSection === "history"
    ? { section: "history", historyTab: state.historyTab }
    : { section: state.currentSection };

export const useNavStore = create<NavStore>()((set, get) => {
  /** Go to `section` (and History tab), recording the visit for Back. */
  const go = (
    section: SidebarSection,
    historyTab: HistoryTab | undefined,
    extra: Partial<NavStore> = {},
  ) => {
    rememberPage(section);
    const state = get();
    const next = {
      currentSection: section,
      historyTab: historyTab ?? state.historyTab,
    };
    set({
      ...next,
      ...extra,
      navHistory: recordVisit(state.navHistory, targetOf(state), targetOf(next)),
    });
  };

  const travel =
    (direction: "back" | "forward") =>
    (canShow: (target: NavTarget) => boolean) => {
      const state = get();
      const moved = step(state.navHistory, targetOf(state), direction, canShow);
      if (!moved) return;
      const section = moved.target.section as SidebarSection;
      rememberPage(section);
      set({
        currentSection: section,
        ...(moved.target.historyTab
          ? { historyTab: moved.target.historyTab as HistoryTab }
          : {}),
        navHistory: moved.history,
      });
    };

  return {
    currentSection: "general",
    lastMoreSection: "providers",
    lastToolsSection: "keyboardTyper",
    historyTab: "recordings",
    navHistory: EMPTY_HISTORY,
    // Opening History from the sidebar shows Recordings.
    setCurrentSection: (currentSection) =>
      go(
        currentSection,
        currentSection === "history" ? "recordings" : undefined,
      ),
    setLastMoreSection: (lastMoreSection) => set({ lastMoreSection }),
    setLastToolsSection: (lastToolsSection) => set({ lastToolsSection }),
    setHistoryTab: (historyTab) => go(get().currentSection, historyTab),
    navigateTo: (currentSection, historyTab) =>
      go(currentSection, historyTab),
    pendingSetup: null,
    startSetup: (pendingSetup) =>
      go("setups", undefined, { pendingSetup }),
    clearPendingSetup: () => set({ pendingSetup: null }),
    goBack: travel("back"),
    goForward: travel("forward"),
  };
});
