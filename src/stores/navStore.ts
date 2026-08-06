import { create } from "zustand";
import type { SidebarSection } from "@/components/Sidebar";
import type { TabId as AdvancedTabId } from "@/components/settings/advanced/AdvancedSettings";

interface NavStore {
  currentSection: SidebarSection;
  pendingAdvancedTab: AdvancedTabId | null;
  setCurrentSection: (section: SidebarSection) => void;
  setPendingAdvancedTab: (tab: AdvancedTabId | null) => void;
  consumePendingAdvancedTab: () => AdvancedTabId | null;
  navigateTo: (section: SidebarSection, advancedTab?: AdvancedTabId) => void;
}

export const useNavStore = create<NavStore>()((set, get) => ({
  currentSection: "general",
  pendingAdvancedTab: null,
  setCurrentSection: (currentSection) => set({ currentSection }),
  setPendingAdvancedTab: (pendingAdvancedTab) => set({ pendingAdvancedTab }),
  consumePendingAdvancedTab: () => {
    const pendingAdvancedTab = get().pendingAdvancedTab;
    if (pendingAdvancedTab !== null) {
      set({ pendingAdvancedTab: null });
    }
    return pendingAdvancedTab;
  },
  navigateTo: (currentSection, advancedTab) =>
    set({
      currentSection,
      pendingAdvancedTab:
        currentSection === "advanced" ? (advancedTab ?? null) : null,
    }),
}));
