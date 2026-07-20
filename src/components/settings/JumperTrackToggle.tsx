import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";

interface JumperTrackToggleProps {
  /** Which finishing flow: typical output or Transcribe & Submit. */
  flow: "output" | "submit";
  grouped?: boolean;
}

/**
 * Windows-only: track-last-output switch + slot picker, per flow and
 * INDEPENDENT (mirrors `JumperReturnFocusToggle`). When enabled, the chosen
 * jump slot auto-captures the field this flow last pasted into, so jumping
 * to that slot returns to that flow's last output spot. The dictate/
 * "Transcribe" flow and the "Transcribe & Submit" flow each have their own
 * switch + slot — toggling one never affects the other.
 */
export const JumperTrackToggle: React.FC<JumperTrackToggleProps> = React.memo(
  ({ flow, grouped = false }) => {
    const { t } = useTranslation();
    const osType = useOsType();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    if (osType !== "windows") {
      return null;
    }

    const enabledKey =
      flow === "output"
        ? "jumper_track_output_enabled"
        : "jumper_track_submit_enabled";
    const slotKey =
      flow === "output"
        ? "jumper_track_output_slot"
        : "jumper_track_submit_slot";

    const enabled = getSetting(enabledKey) ?? false;
    const selectedSlot = String(getSetting(slotKey) ?? 0);
    const slotOptions = [0, 1, 2, 3, 4].map((slot) => ({
      value: String(slot),
      label:
        slot === 0
          ? t("settings.jumper.slotNames.hot")
          : t("settings.jumper.slotNames.static", { index: slot }),
    }));

    return (
      <>
        <ToggleSwitch
          checked={enabled}
          onChange={(value) => updateSetting(enabledKey, value)}
          isUpdating={isUpdating(enabledKey)}
          label={t("settings.jumper.track.label")}
          description={t("settings.jumper.track.description")}
          descriptionMode="tooltip"
          grouped={grouped}
        />
        {enabled && (
          <SettingContainer
            title={t("settings.jumper.track.slot.title")}
            description={t("settings.jumper.track.slot.description")}
            descriptionMode="tooltip"
            grouped={grouped}
          >
            <Dropdown
              options={slotOptions}
              selectedValue={selectedSlot}
              onSelect={(value) => updateSetting(slotKey, Number(value))}
              disabled={isUpdating(slotKey)}
            />
          </SettingContainer>
        )}
      </>
    );
  },
);
