import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";

interface JumperReturnFocusToggleProps {
  /** Which finishing flow: typical output or Transcribe & Submit. */
  flow: "output" | "submit";
  grouped?: boolean;
}

/**
 * Windows-only: return focus after an anchored delivery, per flow. The
 * location returned to is captured automatically every time a delivery
 * starts (an internal slot — no user slot selection involved).
 */
export const JumperReturnFocusToggle: React.FC<JumperReturnFocusToggleProps> =
  React.memo(({ flow, grouped = false }) => {
    const { t } = useTranslation();
    const osType = useOsType();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    if (osType !== "windows") {
      return null;
    }

    const settingKey =
      flow === "output" ? "return_focus_output" : "return_focus_submit";
    const enabled = getSetting(settingKey) ?? true;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(value) => updateSetting(settingKey, value)}
        isUpdating={isUpdating(settingKey)}
        label={t("settings.jumper.returnFocus.label")}
        description={t("settings.jumper.returnFocus.description")}
        descriptionMode="tooltip"
        grouped={grouped}
      />
    );
  });
