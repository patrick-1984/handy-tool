import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import type { TranscriptionMode } from "@/bindings";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";

interface TranscriptionModePttSettingProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const TranscriptionModePttSetting: React.FC<
  TranscriptionModePttSettingProps
> = ({ descriptionMode = "inline", grouped = false }) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();

  const options = [
    {
      value: "live" as TranscriptionMode,
      label: t("settings.advanced.transcriptionModePtt.options.live"),
    },
    {
      value: "post_recording" as TranscriptionMode,
      label: t("settings.advanced.transcriptionModePtt.options.postRecording"),
    },
  ];

  const currentValue = getSetting("transcription_mode_ptt") ?? "live";

  return (
    <SettingContainer
      title={t("settings.advanced.transcriptionModePtt.title")}
      description={t("settings.advanced.transcriptionModePtt.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    >
      <Dropdown
        options={options}
        selectedValue={currentValue}
        onSelect={(value) =>
          updateSetting("transcription_mode_ptt", value as TranscriptionMode)
        }
        disabled={false}
      />
    </SettingContainer>
  );
};
