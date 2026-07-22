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
  JumperSubmitDelay,
  JumperPasteDelay,
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

  // Windows-only (Jumper). Extra settle before Enter after a jump-to-focus
  // submit, so the freshly-activated target commits the paste before Enter.
  const jumperSubmitDelay = (getSetting("jumper_submit_delay") ||
    "ms250") as JumperSubmitDelay;
  const jumperSubmitDelayOptions = [
    {
      value: "none",
      label: t(
        "settings.general.transcribeAndSubmit.jumperSubmitDelay.options.none",
      ),
    },
    { value: "ms100", label: "100 ms" },
    { value: "ms250", label: "250 ms" },
    { value: "ms500", label: "500 ms" },
    { value: "ms1000", label: "1000 ms" },
    { value: "ms2000", label: "2000 ms" },
  ];

  const jumperPasteDelay = (getSetting("jumper_paste_delay") ||
    "ms250") as JumperPasteDelay;
  const jumperPasteDelayOptions = [
    {
      value: "none",
      label: t(
        "settings.general.transcribeAndSubmit.jumperPasteDelay.options.none",
      ),
    },
    { value: "ms100", label: "100 ms" },
    { value: "ms250", label: "250 ms" },
    { value: "ms500", label: "500 ms" },
    { value: "ms1000", label: "1000 ms" },
    { value: "ms2000", label: "2000 ms" },
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
      {osType === "windows" && (
        <SettingContainer
          title={t(
            "settings.general.transcribeAndSubmit.jumperPasteDelay.title",
          )}
          description={t(
            "settings.general.transcribeAndSubmit.jumperPasteDelay.description",
          )}
          descriptionMode="tooltip"
          grouped={true}
        >
          <Dropdown
            options={jumperPasteDelayOptions}
            selectedValue={jumperPasteDelay}
            onSelect={(value) =>
              updateSetting("jumper_paste_delay", value as JumperPasteDelay)
            }
            disabled={isUpdating("jumper_paste_delay")}
          />
        </SettingContainer>
      )}
      {osType === "windows" && (
        <SettingContainer
          title={t(
            "settings.general.transcribeAndSubmit.jumperSubmitDelay.title",
          )}
          description={t(
            "settings.general.transcribeAndSubmit.jumperSubmitDelay.description",
          )}
          descriptionMode="tooltip"
          grouped={true}
        >
          <Dropdown
            options={jumperSubmitDelayOptions}
            selectedValue={jumperSubmitDelay}
            onSelect={(value) =>
              updateSetting("jumper_submit_delay", value as JumperSubmitDelay)
            }
            disabled={isUpdating("jumper_submit_delay")}
          />
        </SettingContainer>
      )}
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
      {/* Track-last-output is per-flow and independent (T-118): this instance
          reads/writes jumper_track_submit_enabled/_slot only, and never
          affects the General page's "output" flow instance. */}
      <JumperTrackToggle flow="submit" grouped={true} />
    </SettingsGroup>
  );
});
