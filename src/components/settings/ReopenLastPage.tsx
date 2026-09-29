import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface ReopenLastPageProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/** Open the app on the page that was open when it was closed. */
export const ReopenLastPage: React.FC<ReopenLastPageProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    return (
      <ToggleSwitch
        checked={getSetting("reopen_last_page") ?? true}
        onChange={(enabled) => updateSetting("reopen_last_page", enabled)}
        isUpdating={isUpdating("reopen_last_page")}
        label={t("settings.advanced.reopenLastPage.title")}
        description={t("settings.advanced.reopenLastPage.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);
