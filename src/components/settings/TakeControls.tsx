import React, { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { ShortcutInput } from "./ShortcutInput";
import { useSettings } from "../../hooks/useSettings";

interface Props {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/** Pause/resume button on the recording overlay; its shortcut shows once it is on. */
export const PauseButtonSetting: React.FC<Props> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = getSetting("pause_button_enabled") ?? false;
  return (
    <>
      <ToggleSwitch
        checked={enabled}
        onChange={(value) => updateSetting("pause_button_enabled", value)}
        isUpdating={isUpdating("pause_button_enabled")}
        label={t("settings.general.pauseButton.label")}
        description={t("settings.general.pauseButton.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
      {enabled && <ShortcutInput shortcutId="pause" grouped={grouped} />}
    </>
  );
};

/** Undo-last-word shortcut for live takes; its shortcut shows once it is on. */
export const UndoWordSetting: React.FC<Props> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = getSetting("undo_word_enabled") ?? false;
  return (
    <>
      <ToggleSwitch
        checked={enabled}
        onChange={(value) => updateSetting("undo_word_enabled", value)}
        isUpdating={isUpdating("undo_word_enabled")}
        label={t("settings.general.undoWord.label")}
        description={t("settings.general.undoWord.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
      {enabled && <ShortcutInput shortcutId="undo_word" grouped={grouped} />}
    </>
  );
};

/**
 * Live text box next to the recording pill, and what it shows. The overlay's T
 * button flips the same setting, so follow its change event.
 */
export const LiveTextBoxSetting: React.FC<Props> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating, refreshSettings } =
    useSettings();
  const enabled = getSetting("live_text_box_enabled") ?? false;
  const mode = getSetting("live_text_mode") ?? "last_words";

  useEffect(() => {
    const unlisten = listen("live-text-box-changed", () => {
      void refreshSettings();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [refreshSettings]);

  return (
    <>
      <ToggleSwitch
        checked={enabled}
        onChange={(value) => updateSetting("live_text_box_enabled", value)}
        isUpdating={isUpdating("live_text_box_enabled")}
        label={t("settings.general.liveTextBox.label")}
        description={t("settings.general.liveTextBox.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
      {enabled && (
        <SettingContainer
          title={t("settings.general.liveTextMode.title")}
          description={t("settings.general.liveTextMode.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <Dropdown
            options={[
              {
                value: "last_words",
                label: t("settings.general.liveTextMode.lastWords"),
              },
              {
                value: "full_text",
                label: t("settings.general.liveTextMode.fullText"),
              },
            ]}
            selectedValue={mode}
            onSelect={(value) =>
              updateSetting(
                "live_text_mode",
                value as "last_words" | "full_text",
              )
            }
            disabled={isUpdating("live_text_mode")}
          />
        </SettingContainer>
      )}
    </>
  );
};
