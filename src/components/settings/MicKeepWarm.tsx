import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";

interface MicKeepWarmProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

const MINUTES = [0, 1, 5, 15];

/**
 * How long the microphone stays open after a take (on-demand mode), so the next
 * take starts without an idle device's wake-up delay. Hidden with Always-On
 * Microphone, which keeps it open anyway.
 */
export const MicKeepWarm: React.FC<MicKeepWarmProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  if (getSetting("always_on_microphone")) return null;

  const options = MINUTES.map((minutes) => ({
    value: String(minutes),
    label:
      minutes === 0
        ? t("settings.sound.micKeepWarm.off")
        : t("settings.sound.micKeepWarm.minutes", { minutes }),
  }));

  return (
    <SettingContainer
      title={t("settings.sound.micKeepWarm.title")}
      description={t("settings.sound.micKeepWarm.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    >
      <Dropdown
        options={options}
        selectedValue={String(getSetting("mic_keep_warm_minutes") ?? 0)}
        onSelect={(value) =>
          updateSetting("mic_keep_warm_minutes", Number(value))
        }
        disabled={isUpdating("mic_keep_warm_minutes")}
      />
    </SettingContainer>
  );
};
