import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";

interface JumperTrackToggleProps {
  /** Which finished flow updates the hot slot: typical output or Transcribe & Submit. */
  flow: "output" | "submit";
  grouped?: boolean;
}

/**
 * Windows-only: when enabled, the hot jump slot auto-captures the field this
 * flow last pasted into, so "Jump to Anchor" returns to the last output spot.
 */
export const JumperTrackToggle: React.FC<JumperTrackToggleProps> = React.memo(
  ({ flow, grouped = false }) => {
    const { t } = useTranslation();
    const osType = useOsType();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    if (osType !== "windows") {
      return null;
    }

    const settingKey =
      flow === "output" ? "jumper_track_output" : "jumper_track_submit";
    const enabled = getSetting(settingKey) ?? false;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(value) => updateSetting(settingKey, value)}
        isUpdating={isUpdating(settingKey)}
        label={t(`settings.jumper.track.${flow}.label`)}
        description={t(`settings.jumper.track.${flow}.description`)}
        descriptionMode="tooltip"
        grouped={grouped}
      />
    );
  },
);
