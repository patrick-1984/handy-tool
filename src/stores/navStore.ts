import { create } from "zustand";
import type { SidebarSection } from "@/components/Sidebar";

export type HistoryTab = "recordings" | "statistics" | "settings";
export type NotesTab = "settings" | "saved" | "manual";
/** The feature setups that run on the Setups page. */
export type FeatureSetup = "appearance" | "postProcessing" | "jumper";

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
  /** The Notes page's tab; null until one is picked (the page then chooses). */
  notesTab: NotesTab | null;
  setNotesTab: (tab: NotesTab) => void;
  /** Open Notes on a given tab. */
  openNotes: (tab: NotesTab) => void;
  /** A History entry to scroll to and highlight ("Go to transcript"). */
  focusHistoryId: number | null;
  /** Open History › Recordings at that entry. */
  goToHistoryEntry: (id: number) => void;
  clearFocusHistoryId: () => void;
  /** A setup to start when the Setups page opens (from a button elsewhere). */
  pendingSetup: FeatureSetup | null;
  /** Open Setups and start that setup there. */
  startSetup: (setup: FeatureSetup) => void;
  clearPendingSetup: () => void;
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

export const useNavStore = create<NavStore>()((set) => ({
  currentSection: "general",
  lastMoreSection: "providers",
  lastToolsSection: "keyboardTyper",
  historyTab: "recordings",
  setCurrentSection: (currentSection) => {
    rememberPage(currentSection);
    set(
      currentSection === "history"
        ? { currentSection, historyTab: "recordings" }
        : { currentSection },
    );
  },
  setLastMoreSection: (lastMoreSection) => set({ lastMoreSection }),
  setLastToolsSection: (lastToolsSection) => set({ lastToolsSection }),
  setHistoryTab: (historyTab) => set({ historyTab }),
  navigateTo: (currentSection, historyTab) => {
    rememberPage(currentSection);
    set(historyTab ? { currentSection, historyTab } : { currentSection });
  },
  notesTab: null,
  setNotesTab: (notesTab) => set({ notesTab }),
  openNotes: (notesTab) => {
    rememberPage("notes");
    set({ currentSection: "notes", notesTab });
  },
  focusHistoryId: null,
  goToHistoryEntry: (focusHistoryId) => {
    rememberPage("history");
    set({
      currentSection: "history",
      historyTab: "recordings",
      focusHistoryId,
    });
  },
  clearFocusHistoryId: () => set({ focusHistoryId: null }),
  pendingSetup: null,
  startSetup: (pendingSetup) => {
    rememberPage("setups");
    set({ currentSection: "setups", pendingSetup });
  },
  clearPendingSetup: () => set({ pendingSetup: null }),
}));
