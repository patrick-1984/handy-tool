import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import type { CursorMode } from "../../bindings";

interface JumperSaveCursorToggleProps {
  /** Which finishing flow: typical output or Transcribe & Submit. */
  flow: "output" | "submit";
  grouped?: boolean;
}

/**
 * Windows-only: save-and-restore mouse cursor position for a delivery flow,
 * per flow and INDEPENDENT (mirrors `JumperTrackToggle`). When enabled, the
 * flow captures the mouse cursor position at delivery so a jump restores it.
 * The cursor mode (App-relative vs screen-absolute) is a single shared setting
 * (`jumper_cursor_mode`); either flow's dropdown edits the same value.
 */
export const JumperSaveCursorToggle: React.FC<JumperSaveCursorToggleProps> =
  React.memo(({ flow, grouped = false }) => {
    const { t } = useTranslation();
    const osType = useOsType();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    if (osType !== "windows") {
      return null;
    }

    const enabledKey =
      flow === "output"
        ? "jumper_save_cursor_output_enabled"
        : "jumper_save_cursor_submit_enabled";

    const enabled = getSetting(enabledKey) ?? false;
    const cursorMode = String(getSetting("jumper_cursor_mode") ?? "AppRelative");
    const modeOptions = [
      {
        value: "AppRelative",
        label: t("settings.jumper.saveCursor.mode.appRelative"),
      },
      {
        value: "ScreenAbsolute",
        label: t("settings.jumper.saveCursor.mode.screenAbsolute"),
      },
    ];

    return (
      <>
        <ToggleSwitch
          checked={enabled}
          onChange={(value) => updateSetting(enabledKey, value)}
          isUpdating={isUpdating(enabledKey)}
          label={t("settings.jumper.saveCursor.label")}
          description={t("settings.jumper.saveCursor.description")}
          descriptionMode="tooltip"
          grouped={grouped}
        />
        {enabled && (
          <SettingContainer
            title={t("settings.jumper.saveCursor.mode.title")}
            description={t("settings.jumper.saveCursor.mode.description")}
            descriptionMode="tooltip"
            grouped={grouped}
          >
            <Dropdown
              options={modeOptions}
              selectedValue={cursorMode}
              onSelect={(value) =>
                updateSetting("jumper_cursor_mode", value as CursorMode)
              }
              disabled={isUpdating("jumper_cursor_mode")}
            />
          </SettingContainer>
        )}
      </>
    );
  });
