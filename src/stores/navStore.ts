import { create } from "zustand";
import type { SidebarSection } from "@/components/Sidebar";

interface NavStore {
  currentSection: SidebarSection;
  /** The More page's tab last open, so the sidebar's More entry returns to it. */
  lastMoreSection: SidebarSection;
  setCurrentSection: (section: SidebarSection) => void;
  setLastMoreSection: (section: SidebarSection) => void;
  navigateTo: (section: SidebarSection) => void;
}

export const useNavStore = create<NavStore>()((set) => ({
  currentSection: "general",
  lastMoreSection: "app",
  setCurrentSection: (currentSection) => set({ currentSection }),
  setLastMoreSection: (lastMoreSection) => set({ lastMoreSection }),
  navigateTo: (currentSection) => set({ currentSection }),
}));
