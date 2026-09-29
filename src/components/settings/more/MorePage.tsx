import React, { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../../hooks/useSettings";
import { useNavStore } from "../../../stores/navStore";
import { STICKY_TABS, TabBar } from "../../ui/TabBar";
import { PageTitle } from "../../ui/PageTitle";
import {
  MORE_GROUPS,
  SECTIONS_CONFIG,
  type MoreGroup,
  type SidebarSection,
} from "../../Sidebar";

/**
 * A sidebar entry that holds several pages as tabs: Advanced settings (the
 * settings pages; the More entry before 2.0.1) and More Tools (the tools, after
 * Jumper). The title and the tab row stay in view while the page scrolls.
 */
export const MorePage: React.FC<{ section: SidebarSection }> = ({
  section,
}) => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const navigateTo = useNavStore((state) => state.navigateTo);
  const setLastMoreSection = useNavStore((state) => state.setLastMoreSection);
  const setLastToolsSection = useNavStore((state) => state.setLastToolsSection);
  const group = SECTIONS_CONFIG[section].placement as MoreGroup;

  useEffect(() => {
    if (group === "more-tools") setLastToolsSection(section);
    else setLastMoreSection(section);
  }, [group, section, setLastMoreSection, setLastToolsSection]);

  const ActiveComponent = SECTIONS_CONFIG[section].component;
  const { icon, labelKey } = MORE_GROUPS[group];

  return (
    <div className="w-full space-y-6">
      <div className={`-mt-6 pt-6 flex flex-col gap-3 ${STICKY_TABS}`}>
        <PageTitle icon={icon} label={t(labelKey)} />
        <TabBar
          tabs={Object.entries(SECTIONS_CONFIG)
            .filter(
              ([_, config]) =>
                config.placement === group && config.enabled(settings),
            )
            .map(([id, config]) => ({
              id: id as SidebarSection,
              label: t(config.labelKey),
            }))}
          active={section}
          onSelect={navigateTo}
        />
      </div>
      <ActiveComponent />
    </div>
  );
};
