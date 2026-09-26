import React, { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../../hooks/useSettings";
import { useNavStore } from "../../../stores/navStore";
import { STICKY_TABS, TabBar } from "../../ui/TabBar";
import {
  SECTIONS_CONFIG,
  type SectionPlacement,
  type SidebarSection,
} from "../../Sidebar";

const ROWS: { placement: SectionPlacement; labelKey: string }[] = [
  { placement: "more-settings", labelKey: "sidebar.groups.settings" },
  { placement: "more-tools", labelKey: "sidebar.groups.tools" },
];

/**
 * The sidebar's More entry: every page that is not in the sidebar, as two rows
 * of tabs (Settings and Tools) above the open page. The rows stay in view while
 * the page scrolls.
 */
export const MorePage: React.FC<{ section: SidebarSection }> = ({
  section,
}) => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const navigateTo = useNavStore((state) => state.navigateTo);
  const setLastMoreSection = useNavStore((state) => state.setLastMoreSection);

  useEffect(() => {
    setLastMoreSection(section);
  }, [section, setLastMoreSection]);

  const ActiveComponent = SECTIONS_CONFIG[section].component;

  return (
    <div className="w-full space-y-4">
      <div className={`flex flex-col gap-1 ${STICKY_TABS}`}>
        {ROWS.map((row) => (
          <div key={row.placement} className="flex items-start gap-1">
            <span className="w-20 shrink-0 pt-2.5 text-[11px] font-semibold uppercase tracking-wider text-text/70">
              {t(row.labelKey)}
            </span>
            <TabBar
              tabs={Object.entries(SECTIONS_CONFIG)
                .filter(
                  ([_, config]) =>
                    config.placement === row.placement &&
                    config.enabled(settings),
                )
                .map(([id, config]) => ({
                  id: id as SidebarSection,
                  label: t(config.labelKey),
                  icon: config.icon,
                }))}
              active={section}
              onSelect={navigateTo}
            />
          </div>
        ))}
      </div>
      <ActiveComponent />
    </div>
  );
};
