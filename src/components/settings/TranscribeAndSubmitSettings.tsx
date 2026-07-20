import React from "react";
import { useTranslation } from "react-i18next";
import { ShortcutInput } from "./ShortcutInput";
import { SettingsGroup } from "../ui/SettingsGroup";
import { SettingContainer } from "../ui/SettingContainer";
import { Dropdown } from "../ui/Dropdown";
import { buildPasteMethodOptions } from "./PasteMethod";
import { ClipboardRestoreDelaySetting } from "./ClipboardRestoreDelay";
import { AnchorActionSetting } from "./AnchorActionSetting";
import { JumperReturnFocusToggle } from "./JumperReturnFocusToggle";
import { JumperTrackToggle } from "./JumperTrackToggle";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import type {
  PasteMethod,
  AutoSubmitKey,
  SubmitIdleBehavior,
  ClipboardHandling,
} from "@/bindings";

/**
 * "Transcribe & Submit" shortcut: finishes the active recording, pastes the
 * transcription with a chosen paste method, then presses a submit key (Enter by
 * default). The shortcut itself plus its two options live together here.
 */
export const TranscribeAndSubmitSettings: React.FC = React.memo(() => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const osType = useOsType();

  const pasteMethod = (getSetting("submit_paste_method") ||
    "ctrl_v") as PasteMethod;
  const submitKey = (getSetting("submit_key") || "enter") as AutoSubmitKey;

  // Full option set — same wiring as the global Output paste method.
  const pasteMethodOptions = buildPasteMethodOptions(t, osType);

  const submitWithMetaLabel =
    osType === "macos"
      ? t("settings.advanced.autoSubmit.options.cmdEnter")
      : t("settings.advanced.autoSubmit.options.superEnter");
  const submitKeyOptions = [
    { value: "enter", label: t("settings.advanced.autoSubmit.options.enter") },
    {
      value: "ctrl_enter",
      label: t("settings.advanced.autoSubmit.options.ctrlEnter"),
    },
    { value: "cmd_enter", label: submitWithMetaLabel },
  ];

  // Any stored value other than "do_nothing" (including the legacy
  // "start_and_submit") means "start a recording".
  const idleBehavior: SubmitIdleBehavior =
    getSetting("submit_idle_behavior") === "do_nothing"
      ? "do_nothing"
      : "start_normal";
  const idleBehaviorOptions = [
    {
      value: "start_normal",
      label: t(
        "settings.general.transcribeAndSubmit.idleBehavior.options.startNormal",
      ),
    },
    {
      value: "do_nothing",
      label: t(
        "settings.general.transcribeAndSubmit.idleBehavior.options.doNothing",
      ),
    },
  ];

  const clipboardHandling = (getSetting("submit_clipboard_handling") ||
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
    <SettingsGroup title={t("settings.general.transcribeAndSubmit.title")}>
      <ShortcutInput shortcutId="transcribe_and_submit" grouped={true} />
      <SettingContainer
        title={t("settings.general.transcribeAndSubmit.pasteMethod.title")}
        description={t(
          "settings.general.transcribeAndSubmit.pasteMethod.description",
        )}
        descriptionMode="tooltip"
        grouped={true}
      >
        <Dropdown
          options={pasteMethodOptions}
          selectedValue={pasteMethod}
          onSelect={(value) =>
            updateSetting("submit_paste_method", value as PasteMethod)
          }
          disabled={isUpdating("submit_paste_method")}
        />
      </SettingContainer>
      <SettingContainer
        title={t("settings.general.transcribeAndSubmit.submitKey.title")}
        description={t(
          "settings.general.transcribeAndSubmit.submitKey.description",
        )}
        descriptionMode="tooltip"
        grouped={true}
      >
        <Dropdown
          options={submitKeyOptions}
          selectedValue={submitKey}
          onSelect={(value) =>
            updateSetting("submit_key", value as AutoSubmitKey)
          }
          disabled={isUpdating("submit_key")}
        />
      </SettingContainer>
      <SettingContainer
        title={t("settings.general.transcribeAndSubmit.idleBehavior.title")}
        description={t(
          "settings.general.transcribeAndSubmit.idleBehavior.description",
        )}
        descriptionMode="tooltip"
        grouped={true}
      >
        <Dropdown
          options={idleBehaviorOptions}
          selectedValue={idleBehavior}
          onSelect={(value) =>
            updateSetting("submit_idle_behavior", value as SubmitIdleBehavior)
          }
          disabled={isUpdating("submit_idle_behavior")}
        />
      </SettingContainer>
      <SettingContainer
        title={t("settings.general.transcribeAndSubmit.clipboard.title")}
        description={t(
          "settings.general.transcribeAndSubmit.clipboard.description",
        )}
        descriptionMode="tooltip"
        grouped={true}
      >
        <Dropdown
          options={clipboardOptions}
          selectedValue={clipboardHandling}
          onSelect={(value) =>
            updateSetting(
              "submit_clipboard_handling",
              value as ClipboardHandling,
            )
          }
          disabled={isUpdating("submit_clipboard_handling")}
        />
      </SettingContainer>
      <ClipboardRestoreDelaySetting
        settingKey="submit_clipboard_restore_delay"
        descriptionMode="tooltip"
        grouped={true}
      />
      <AnchorActionSetting
        settingKey="anchor_action_submit_idle"
        moment="idle"
        grouped={true}
      />
      <AnchorActionSetting
        settingKey="anchor_action_submit_stop"
        moment="stop"
        grouped={true}
      />
      <JumperReturnFocusToggle flow="submit" grouped={true} />
      {/* The global track-last-output switch applies to BOTH flows; render it
          here too (T-117) so it's discoverable from Transcribe & Submit, not
          only the General page. Same setting key — stays in sync. */}
      <JumperTrackToggle grouped={true} />
    </SettingsGroup>
  );
});
