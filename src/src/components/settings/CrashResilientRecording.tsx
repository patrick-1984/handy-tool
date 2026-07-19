import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface CrashResilientRecordingProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const CrashResilientRecording: React.FC<CrashResilientRecordingProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled = getSetting("crash_resilient_recording") ?? true;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(value) => updateSetting("crash_resilient_recording", value)}
        isUpdating={isUpdating("crash_resilient_recording")}
        label={t("settings.advanced.crashResilientRecording.label")}
        description={t("settings.advanced.crashResilientRecording.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  });
