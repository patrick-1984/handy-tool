import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import type { TranscriptionMode } from "@/bindings";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";

interface TranscriptionModeSettingProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const TranscriptionModeSetting: React.FC<
  TranscriptionModeSettingProps
> = ({ descriptionMode = "inline", grouped = false }) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();

  const options = [
    {
      value: "live" as TranscriptionMode,
      label: t("settings.advanced.transcriptionMode.options.live"),
    },
    {
      value: "post_recording" as TranscriptionMode,
      label: t("settings.advanced.transcriptionMode.options.postRecording"),
    },
  ];

  const currentValue = getSetting("transcription_mode") ?? "post_recording";

  return (
    <SettingContainer
      title={t("settings.advanced.transcriptionMode.title")}
      description={t("settings.advanced.transcriptionMode.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    >
      <Dropdown
        options={options}
        selectedValue={currentValue}
        onSelect={(value) =>
          updateSetting("transcription_mode", value as TranscriptionMode)
        }
        disabled={false}
      />
    </SettingContainer>
  );
};
