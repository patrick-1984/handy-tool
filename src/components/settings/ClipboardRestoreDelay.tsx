import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import { buildDelayOptions } from "../../lib/utils/delayOptions";
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
    const { t, i18n } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const selected = (getSetting(settingKey) ||
      "none") as ClipboardRestoreDelay;
    // `selected` is passed through so a legacy ms2500/ms5000 store still shows
    // its real value instead of collapsing to the "Select an option…" placeholder.
    const options = buildDelayOptions(t, i18n.language, selected);

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
