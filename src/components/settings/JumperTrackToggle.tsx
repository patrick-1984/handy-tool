import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";

interface JumperTrackToggleProps {
  grouped?: boolean;
}

/**
 * Windows-only: ONE global track-last-output switch shared by both flows.
 * When enabled, the chosen jump slot auto-captures the field a flow last
 * pasted into, so jumping to that slot returns to the last output spot.
 */
export const JumperTrackToggle: React.FC<JumperTrackToggleProps> = React.memo(
  ({ grouped = false }) => {
    const { t } = useTranslation();
    const osType = useOsType();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    if (osType !== "windows") {
      return null;
    }

    const enabled = getSetting("jumper_track_enabled") ?? false;
    const selectedSlot = String(getSetting("jumper_track_slot") ?? 0);
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
          onChange={(value) => updateSetting("jumper_track_enabled", value)}
          isUpdating={isUpdating("jumper_track_enabled")}
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
              onSelect={(value) =>
                updateSetting("jumper_track_slot", Number(value))
              }
              disabled={isUpdating("jumper_track_slot")}
            />
          </SettingContainer>
        )}
      </>
    );
  },
);
