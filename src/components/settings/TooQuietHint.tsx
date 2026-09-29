import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { SubSettings } from "../ui/SettingsGroup";
import { useSettings } from "../../hooks/useSettings";

interface TooQuietHintProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * "Too quiet — speak up" when speech is too quiet to be kept: in a small box
 * under the pill, or in the pill itself.
 */
export const TooQuietHint: React.FC<TooQuietHintProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled = getSetting("too_quiet_hint") ?? true;

    return (
      <>
        <ToggleSwitch
          lead
          checked={enabled}
          onChange={(value) => updateSetting("too_quiet_hint", value)}
          isUpdating={isUpdating("too_quiet_hint")}
          label={t("settings.sound.tooQuiet.title")}
          description={t("settings.sound.tooQuiet.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        />
        {enabled && (
          <SubSettings>
            <ToggleSwitch
              checked={getSetting("too_quiet_hint_box") ?? true}
              onChange={(value) => updateSetting("too_quiet_hint_box", value)}
              isUpdating={isUpdating("too_quiet_hint_box")}
              label={t("settings.sound.tooQuiet.box.title")}
              description={t("settings.sound.tooQuiet.box.description")}
              descriptionMode={descriptionMode}
              grouped={grouped}
            />
          </SubSettings>
        )}
      </>
    );
  },
);
