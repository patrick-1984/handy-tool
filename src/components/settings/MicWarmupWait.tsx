import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface MicWarmupWaitProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * After a cold start, keep "Starting mic..." on the overlay until the
 * microphone has warmed up; the wait is the average of its measured cold
 * starts, shown in the description. Hidden with Always-On Microphone, which
 * has no cold starts during takes.
 */
export const MicWarmupWait: React.FC<MicWarmupWaitProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    if (getSetting("always_on_microphone")) return null;

    const measured = getSetting("mic_fade_in_measured_ms") ?? [];
    const average = measured.length
      ? measured.reduce((sum, ms) => sum + ms, 0) / measured.length
      : null;
    const note =
      average === null
        ? t("settings.sound.micWarmup.notMeasured")
        : t("settings.sound.micWarmup.measured", {
            seconds: (average / 1000).toFixed(1),
          });

    return (
      <ToggleSwitch
        checked={getSetting("mic_warmup_wait") ?? true}
        onChange={(enabled) => updateSetting("mic_warmup_wait", enabled)}
        isUpdating={isUpdating("mic_warmup_wait")}
        label={t("settings.sound.micWarmup.title")}
        description={`${t("settings.sound.micWarmup.description")}\n\n${note}`}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);
