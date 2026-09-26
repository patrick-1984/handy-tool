import React from "react";
import {
  AppWindow,
  TextCursorInput,
  Cloud,
  Route,
  BrainCircuit,
  Terminal,
  Sparkles,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { ShowOverlay } from "../ShowOverlay";
import { AppearanceSetting } from "../AppearanceSetting";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { StartHidden } from "../StartHidden";
import { AutostartToggle } from "../AutostartToggle";
import { ShowTrayIcon } from "../ShowTrayIcon";
import { PostProcessingToggle } from "../PostProcessingToggle";
import { PostProcessingSettings } from "../post-processing/PostProcessingSettings";
import { ApiTranscriptionSettings } from "../ApiTranscriptionSettings";
import { OpenRouterTranscriptionSettings } from "../OpenRouterTranscriptionSettings";
import { KeyboardImplementationSelector } from "../debug/KeyboardImplementationSelector";
import { RegisteredLlmProviders } from "./RegisteredLlmProviders";
import { McpSettings } from "./McpSettings";
import { TranscriptionCostReport } from "./TranscriptionCostReport";
import { TranscribeAndSubmitSettings } from "../TranscribeAndSubmitSettings";
import { PasteLastSettings } from "../PasteLastSettings";
import { AffixSettings } from "../AffixSettings";
import { JumperDelaySetting } from "../JumperDelaySetting";
import { JumperTrackToggle } from "../JumperTrackToggle";
import { JumperReturnFocusToggle } from "../JumperReturnFocusToggle";
import { PasteMethodSetting } from "../PasteMethod";
import { PasteMethodPttSetting } from "../PasteMethodPtt";
import { TypingToolSetting } from "../TypingTool";
import { ClipboardHandlingSetting } from "../ClipboardHandling";
import { AutoSubmit } from "../AutoSubmit";
import { ClipboardRestoreDelaySetting } from "../ClipboardRestoreDelay";
import { ClipboardRestoreDelayRemoteSetting } from "../ClipboardRestoreDelayRemote";
import { AnchorActionSetting } from "../AnchorActionSetting";
import { useSettings } from "../../../hooks/useSettings";

// The former Advanced page's tabs, now tabs of their own on the More page. The
// History tab moved to the History page; Translate to English lives on General.

/** More › Settings › App */
export const AppSection: React.FC = () => {
  const { t } = useTranslation();
  return (
    <div className="w-full space-y-4">
      <SettingsGroup icon={AppWindow} title={t("settings.advanced.groups.app")}>
        <AppearanceSetting descriptionMode="tooltip" grouped={true} />
        <StartHidden descriptionMode="tooltip" grouped={true} />
        <AutostartToggle descriptionMode="tooltip" grouped={true} />
        <ShowTrayIcon descriptionMode="tooltip" grouped={true} />
        <ShowOverlay descriptionMode="tooltip" grouped={true} />
        <KeyboardImplementationSelector
          descriptionMode="tooltip"
          grouped={true}
        />
      </SettingsGroup>
    </div>
  );
};

/** More › Settings › Output: how dictated text is delivered, per flow. */
export const OutputSection: React.FC = () => {
  const { t } = useTranslation();
  return (
    <div className="w-full space-y-4">
      <SettingsGroup
        icon={TextCursorInput}
        title={t("settings.general.transcribeGroup.title")}
      >
        <PasteMethodSetting descriptionMode="tooltip" grouped={true} />
        <PasteMethodPttSetting descriptionMode="tooltip" grouped={true} />
        <TypingToolSetting descriptionMode="tooltip" grouped={true} />
        <ClipboardHandlingSetting descriptionMode="tooltip" grouped={true} />
        <AutoSubmit descriptionMode="tooltip" grouped={true} />
        <ClipboardRestoreDelaySetting
          settingKey="clipboard_restore_delay"
          descriptionMode="tooltip"
          grouped={true}
        />
        <ClipboardRestoreDelayRemoteSetting
          descriptionMode="tooltip"
          grouped={true}
        />
        <JumperDelaySetting kind="paste" grouped={true} />
        <AnchorActionSetting
          settingKey="anchor_action_output_idle"
          moment="idle"
          grouped={true}
        />
        <AnchorActionSetting
          settingKey="anchor_action_output_stop"
          moment="stop"
          grouped={true}
        />
        <JumperTrackToggle flow="output" grouped={true} />
        <JumperReturnFocusToggle flow="output" grouped={true} />
        <AffixSettings flow="output" grouped={true} />
      </SettingsGroup>
      <TranscribeAndSubmitSettings />
      <PasteLastSettings />
    </div>
  );
};

/** More › Settings › Providers */
export const ProvidersSection: React.FC = () => {
  const { t } = useTranslation();
  return (
    <div className="w-full space-y-4">
      <SettingsGroup
        icon={Cloud}
        title={t("settings.advanced.apiTranscription.cardTitle")}
      >
        <ApiTranscriptionSettings descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>
      <SettingsGroup
        icon={Route}
        title={t("settings.advanced.openRouterTranscription.cardTitle")}
      >
        <OpenRouterTranscriptionSettings
          descriptionMode="tooltip"
          grouped={true}
        />
        <TranscriptionCostReport />
      </SettingsGroup>
      <SettingsGroup
        icon={BrainCircuit}
        title={t("settings.advanced.groups.llmProviders")}
      >
        <RegisteredLlmProviders />
      </SettingsGroup>
    </div>
  );
};

/** More › Settings › MCP & CLI */
export const McpSection: React.FC = () => {
  const { t } = useTranslation();
  return (
    <div className="w-full space-y-4">
      <SettingsGroup icon={Terminal} title={t("settings.advanced.groups.mcp")}>
        <McpSettings />
      </SettingsGroup>
    </div>
  );
};

/**
 * More › Settings › Post-processing: the switch, and — once it is on — the
 * provider, prompt and hotkey that used to be a separate Post Process page.
 */
export const PostProcessingSection: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  return (
    <div className="w-full space-y-4">
      <SettingsGroup
        icon={Sparkles}
        title={t("settings.advanced.groups.postProcessing")}
      >
        <PostProcessingToggle descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>
      {/* Indented under the switch: these belong to it. */}
      {getSetting("post_process_enabled") && (
        <div className="ms-6 ps-4 border-s-2 border-logo-primary/50">
          <PostProcessingSettings />
        </div>
      )}
    </div>
  );
};
