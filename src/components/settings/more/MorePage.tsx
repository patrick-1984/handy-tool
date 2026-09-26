import React, { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../../hooks/useSettings";
import { useNavStore } from "../../../stores/navStore";
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
 * of tabs (Settings and Tools) above the open page.
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
      <div className="flex flex-col gap-1 border-b border-mid-gray/20 pb-px">
        {ROWS.map((row) => (
          <div key={row.placement} className="flex items-center gap-1">
            <span className="w-20 shrink-0 text-[10px] font-semibold uppercase tracking-wider text-text/40">
              {t(row.labelKey)}
            </span>
            <div className="flex gap-1 overflow-x-auto">
              {Object.entries(SECTIONS_CONFIG)
                .filter(
                  ([_, config]) =>
                    config.placement === row.placement &&
                    config.enabled(settings),
                )
                .map(([id, config]) => {
                  const Icon = config.icon;
                  return (
                    <button
                      key={id}
                      onClick={() => navigateTo(id as SidebarSection)}
                      className={`flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium whitespace-nowrap rounded-t-md border-b-2 transition-colors cursor-pointer ${
                        section === id
                          ? "border-logo-primary text-text"
                          : "border-transparent text-text/50 hover:text-text/80"
                      }`}
                    >
                      <Icon width={14} height={14} className="shrink-0" />
                      {t(config.labelKey)}
                    </button>
                  );
                })}
            </div>
          </div>
        ))}
      </div>
      <ActiveComponent />
    </div>
  );
};
