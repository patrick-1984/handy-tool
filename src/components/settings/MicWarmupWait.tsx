import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface MicWarmupWaitProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/** Cold starts measured per microphone before the pill waits (settings.rs). */
const MIC_WARMUP_KEPT = 5;
/** Added to the shortest measured warm-up (settings.rs). */
const MIC_WARMUP_MARGIN_MS = 100;
const MIC_WARMUP_MAX_MS = 3000;

/**
 * After a cold start, keep "Starting mic..." on the overlay until the
 * microphone has warmed up. Each microphone is measured on its own; the wait
 * starts after five measured cold starts and is the shortest of them (see
 * `AppSettings::applied_mic_warmup_ms`). Hidden with Always-On Microphone,
 * which has no cold starts during takes.
 */
export const MicWarmupWait: React.FC<MicWarmupWaitProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    if (getSetting("always_on_microphone")) return null;

    const byDevice = (getSetting("mic_warmup_by_device") ?? {}) as Record<
      string,
      number[] | undefined
    >;
    const lines = Object.entries(byDevice)
      .filter(([, measured]) => measured && measured.length > 0)
      .map(([device, measured = []]) => {
        if (measured.length < MIC_WARMUP_KEPT) {
          return t("settings.sound.micWarmup.counting", {
            device,
            count: measured.length,
            total: MIC_WARMUP_KEPT,
          });
        }
        const shortest = Math.min(...measured);
        if (shortest === 0) {
          return t("settings.sound.micWarmup.noFadeIn", { device });
        }
        const wait = Math.min(
          shortest + MIC_WARMUP_MARGIN_MS,
          MIC_WARMUP_MAX_MS,
        );
        return t("settings.sound.micWarmup.waits", {
          device,
          seconds: (wait / 1000).toFixed(1),
        });
      });
    const note = lines.length
      ? lines.join("\n")
      : t("settings.sound.micWarmup.notMeasured");

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
