import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import type { ClipboardRestoreDelay } from "@/bindings";

interface ClipboardRestoreDelayProps {
  /** Which setting this dropdown edits: the global one or the
   * Transcribe & Submit one. */
  settingKey: "clipboard_restore_delay" | "submit_clipboard_restore_delay";
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * Extra wait before the original clipboard is restored after a paste. Remote
 * sessions (Citrix/RDP) fetch clipboard data on demand AFTER the paste
 * keystroke arrives — restoring too early hands them the old clipboard.
 */
export const ClipboardRestoreDelaySetting: React.FC<ClipboardRestoreDelayProps> =
  React.memo(({ settingKey, descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const selected = (getSetting(settingKey) || "none") as ClipboardRestoreDelay;
    const options = (
      ["none", "ms250", "ms500", "ms1000", "ms2500", "ms5000"] as const
    ).map((value) => ({
      value,
      label: t(`settings.advanced.clipboardRestoreDelay.options.${value}`),
    }));

    return (
      <SettingContainer
        title={t("settings.advanced.clipboardRestoreDelay.title")}
        description={t("settings.advanced.clipboardRestoreDelay.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <Dropdown
          options={options}
          selectedValue={selected}
          onSelect={(value) =>
            updateSetting(settingKey, value as ClipboardRestoreDelay)
          }
          disabled={isUpdating(settingKey)}
        />
      </SettingContainer>
    );
  });
