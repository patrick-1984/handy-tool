import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { SidebarLayout, SidebarShortcut } from "@/bindings";
import { useSettingsStore } from "@/stores/settingsStore";
import { SECTIONS_CONFIG, type SidebarSection } from "@/components/Sidebar";
import { togglePin } from "@/lib/sidebarLayout";

/** The stored layout, read fresh (each write builds on the newest one). */
const latestLayout = (): SidebarLayout =>
  useSettingsStore.getState().settings?.sidebar_layout ?? {};

/** Changes the layout from its newest state and saves it (in order). */
export const updateLayout = (
  change: (layout: SidebarLayout) => SidebarLayout,
) =>
  useSettingsStore
    .getState()
    .updateSetting("sidebar_layout", change(latestLayout()));

/** The name a pinned page shows: the user's, or the page's own. */
export const usePinLabel = () => {
  const { t } = useTranslation();
  return (s: SidebarShortcut): string => {
    if (s.name) return s.name;
    const config = SECTIONS_CONFIG[s.section as SidebarSection];
    const page = config ? t(config.labelKey) : s.section;
    return s.history_tab
      ? `${page} › ${t(`settings.history.tabs.${s.history_tab}`)}`
      : page;
  };
};

/** Pin or unpin a page; says so when the limit is reached. */
export const useTogglePin = () => {
  const { t } = useTranslation();
  return (section: string, historyTab?: string | null) => {
    const next = togglePin(latestLayout(), section, historyTab);
    if (!next) {
      toast.error(t("sidebar.pinned.limit"));
      return;
    }
    void updateLayout(() => next);
  };
};
