import React, { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../../hooks/useSettings";
import { useNavStore } from "../../../stores/navStore";
import { Ellipsis } from "lucide-react";
import { STICKY_TABS, TabBar } from "../../ui/TabBar";
import { PageTitle } from "../../ui/PageTitle";
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
    <div className="w-full space-y-6">
      {/* The title and both tab rows stay in view while the page scrolls. */}
      <div className={`-mt-6 pt-6 flex flex-col gap-3 ${STICKY_TABS}`}>
        <PageTitle icon={Ellipsis} label={t("sidebar.more")} />
        <div className="flex flex-col">
          {ROWS.map((row) => (
            <div key={row.placement} className="flex items-start gap-1">
              <span className="w-20 shrink-0 pt-2.5 text-xs font-semibold uppercase tracking-[0.06em] text-text-secondary">
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
                  }))}
                active={section}
                onSelect={navigateTo}
              />
            </div>
          ))}
        </div>
      </div>
      <ActiveComponent />
    </div>
  );
};
