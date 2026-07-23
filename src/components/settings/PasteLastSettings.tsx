import React from "react";
import { useTranslation } from "react-i18next";
import { ShortcutInput } from "./ShortcutInput";
import { SettingsGroup } from "../ui/SettingsGroup";
import { SettingContainer } from "../ui/SettingContainer";
import { Dropdown } from "../ui/Dropdown";
import { buildPasteMethodOptions } from "./PasteMethod";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import type { PasteMethod, ClipboardHandling } from "@/bindings";

/**
 * "Paste Last Transcription" shortcut: re-pastes the most recent transcription
 * from history into the focused window — a manual fallback for when the
 * automatic paste didn't land. Has its own paste method + clipboard handling
 * (independent of the global ones), so it can be tuned per target app.
 */
export const PasteLastSettings: React.FC = React.memo(() => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const osType = useOsType();

  const pasteMethod = (getSetting("paste_last_paste_method") ||
    "ctrl_v") as PasteMethod;
  const pasteMethodOptions = buildPasteMethodOptions(t, osType);

  const clipboardHandling = (getSetting("paste_last_clipboard_handling") ||
    "dont_modify") as ClipboardHandling;
  const clipboardOptions = [
    {
      value: "dont_modify",
      label: t("settings.advanced.clipboardHandling.options.dontModify"),
    },
    {
      value: "copy_to_clipboard",
      label: t("settings.advanced.clipboardHandling.options.copyToClipboard"),
    },
  ];

  return (
    <SettingsGroup title={t("settings.general.pasteLast.title")}>
      <ShortcutInput shortcutId="paste_last" grouped={true} />
      <SettingContainer
        title={t("settings.general.pasteLast.pasteMethod.title")}
        description={t("settings.general.pasteLast.pasteMethod.description")}
        descriptionMode="tooltip"
        grouped={true}
      >
        <Dropdown
          options={pasteMethodOptions}
          selectedValue={pasteMethod}
          onSelect={(value) =>
            updateSetting("paste_last_paste_method", value as PasteMethod)
          }
          disabled={isUpdating("paste_last_paste_method")}
        />
      </SettingContainer>
      <SettingContainer
        title={t("settings.general.pasteLast.clipboard.title")}
        description={t("settings.general.pasteLast.clipboard.description")}
        descriptionMode="tooltip"
        grouped={true}
      >
        <Dropdown
          options={clipboardOptions}
          selectedValue={clipboardHandling}
          onSelect={(value) =>
            updateSetting(
              "paste_last_clipboard_handling",
              value as ClipboardHandling,
            )
          }
          disabled={isUpdating("paste_last_clipboard_handling")}
        />
      </SettingContainer>
    </SettingsGroup>
  );
});
