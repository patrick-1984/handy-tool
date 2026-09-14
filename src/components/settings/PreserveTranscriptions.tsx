import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface PreserveTranscriptionsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * When on (the default), retention deletes only the audio of an expired recording
 * and keeps its transcription row forever, stamped with `audio_purged_at`.
 *
 * Note this changes what the history limit means: it then caps how many recordings
 * keep their AUDIO, not how many transcriptions are kept. The description says so.
 */
export const PreserveTranscriptions: React.FC<PreserveTranscriptionsProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled = (getSetting("preserve_transcriptions") as boolean) ?? true;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(v) => updateSetting("preserve_transcriptions", v)}
        isUpdating={isUpdating("preserve_transcriptions")}
        label={t("settings.advanced.preserveTranscriptions.label")}
        description={t("settings.advanced.preserveTranscriptions.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  });
