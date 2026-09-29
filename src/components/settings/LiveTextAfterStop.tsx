import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface LiveTextAfterStopProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * A take without the live text box shows the box when it stops, with the
 * transcript typed in as it comes in - just to watch; delivery is unchanged.
 */
export const LiveTextAfterStop: React.FC<LiveTextAfterStopProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    return (
      <ToggleSwitch
        checked={getSetting("live_text_after_stop") ?? false}
        onChange={(enabled) => updateSetting("live_text_after_stop", enabled)}
        isUpdating={isUpdating("live_text_after_stop")}
        label={t("settings.general.liveTextAfterStop.label")}
        description={t("settings.general.liveTextAfterStop.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);
