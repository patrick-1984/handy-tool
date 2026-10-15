import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface RecordingTextFileProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * When on (the default), each take writes its transcript to `handy-{ts}.txt`
 * beside its recording, from the text the take already has - the recording is
 * not transcribed a second time (older versions watched the recordings folder).
 */
export const RecordingTextFile: React.FC<RecordingTextFileProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled = (getSetting("recording_text_file") as boolean) ?? true;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(v) => updateSetting("recording_text_file", v)}
        isUpdating={isUpdating("recording_text_file")}
        label={t("settings.advanced.recordingTextFile.label")}
        description={t("settings.advanced.recordingTextFile.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);
