import { create } from "zustand";
import type { SidebarSection } from "@/components/Sidebar";

export type HistoryTab = "recordings" | "statistics" | "settings";

interface NavStore {
  currentSection: SidebarSection;
  /** The More page's tab last open, so the sidebar's More entry returns to it. */
  lastMoreSection: SidebarSection;
  /** The History page's tab; opening History from the sidebar shows Recordings. */
  historyTab: HistoryTab;
  setCurrentSection: (section: SidebarSection) => void;
  setLastMoreSection: (section: SidebarSection) => void;
  setHistoryTab: (tab: HistoryTab) => void;
  /** Open a page; for History, optionally on a given tab (search results). */
  navigateTo: (section: SidebarSection, historyTab?: HistoryTab) => void;
}

export const useNavStore = create<NavStore>()((set) => ({
  currentSection: "general",
  lastMoreSection: "app",
  historyTab: "recordings",
  setCurrentSection: (currentSection) =>
    set(
      currentSection === "history"
        ? { currentSection, historyTab: "recordings" }
        : { currentSection },
    ),
  setLastMoreSection: (lastMoreSection) => set({ lastMoreSection }),
  setHistoryTab: (historyTab) => set({ historyTab }),
  navigateTo: (currentSection, historyTab) =>
    set(historyTab ? { currentSection, historyTab } : { currentSection }),
}));
