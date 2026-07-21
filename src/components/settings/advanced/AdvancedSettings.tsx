import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { ShowOverlay } from "../ShowOverlay";
import { AppearanceSetting } from "../AppearanceSetting";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { StartHidden } from "../StartHidden";
import { AutostartToggle } from "../AutostartToggle";
import { ShowTrayIcon } from "../ShowTrayIcon";
import { PostProcessingToggle } from "../PostProcessingToggle";
import { CrashResilientRecording } from "../CrashResilientRecording";
import { OpenRecordingsFolder } from "../OpenRecordingsFolder";
import { HistoryLimit } from "../HistoryLimit";
import { RecordingRetentionPeriodSelector } from "../RecordingRetentionPeriod";
import { ExperimentalToggle } from "../ExperimentalToggle";
import { ApiTranscriptionSettings } from "../ApiTranscriptionSettings";
import { OpenRouterTranscriptionSettings } from "../OpenRouterTranscriptionSettings";
import { TranscriptionModelOptions } from "../TranscriptionModelOptions";
import { KeyboardImplementationSelector } from "../debug/KeyboardImplementationSelector";
import { RegisteredLlmProviders } from "./RegisteredLlmProviders";
import { McpSettings } from "./McpSettings";
import { TranscriptionCostReport } from "./TranscriptionCostReport";
import { TranscribeAndSubmitSettings } from "../TranscribeAndSubmitSettings";
import { JumperTrackToggle } from "../JumperTrackToggle";
import { JumperReturnFocusToggle } from "../JumperReturnFocusToggle";
import { PasteMethodSetting } from "../PasteMethod";
import { PasteMethodPttSetting } from "../PasteMethodPtt";
import { TypingToolSetting } from "../TypingTool";
import { ClipboardHandlingSetting } from "../ClipboardHandling";
import { AutoSubmit } from "../AutoSubmit";
import { ClipboardRestoreDelaySetting } from "../ClipboardRestoreDelay";
import { AnchorActionSetting } from "../AnchorActionSetting";
import { useModelStore } from "../../../stores/modelStore";

const TABS = [
  "app",
  "transcription",
  "providers",
  "mcp",
  "history",
  "experimental",
] as const;
type TabId = (typeof TABS)[number];

export const AdvancedSettings: React.FC = () => {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<TabId>("app");
  // Show an engine's config only when that engine is the selected transcription
  // model, so the tab isn't cluttered with URL/key/model fields for engines you
  // aren't using (they read like unrelated LLM/post-processing config otherwise).
  // Use the model store's currentModel — it updates immediately on selection,
  // whereas the settings store's selected_model isn't pushed on change.
  const selectedModel = useModelStore((s) => s.currentModel);

  const renderTab = () => {
    switch (activeTab) {
      case "app":
        return (
          <SettingsGroup title={t("settings.advanced.groups.app")}>
            <AppearanceSetting descriptionMode="tooltip" grouped={true} />
            <StartHidden descriptionMode="tooltip" grouped={true} />
            <AutostartToggle descriptionMode="tooltip" grouped={true} />
            <ShowTrayIcon descriptionMode="tooltip" grouped={true} />
            <ShowOverlay descriptionMode="tooltip" grouped={true} />
            <ExperimentalToggle descriptionMode="tooltip" grouped={true} />
          </SettingsGroup>
        );
      case "transcription":
        return (
          <>
            <SettingsGroup title={t("settings.general.transcribeGroup.title")}>
              <PasteMethodSetting descriptionMode="tooltip" grouped={true} />
              <PasteMethodPttSetting descriptionMode="tooltip" grouped={true} />
              <TypingToolSetting descriptionMode="tooltip" grouped={true} />
              <ClipboardHandlingSetting
                descriptionMode="tooltip"
                grouped={true}
              />
              <AutoSubmit descriptionMode="tooltip" grouped={true} />
              <ClipboardRestoreDelaySetting
                settingKey="clipboard_restore_delay"
                descriptionMode="tooltip"
                grouped={true}
              />
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
            </SettingsGroup>
            <TranscribeAndSubmitSettings />
            <SettingsGroup title={t("settings.advanced.groups.transcription")}>
              <TranscriptionModelOptions
                descriptionMode="tooltip"
                grouped={true}
              />
              {selectedModel === "api-whisper" && (
                <ApiTranscriptionSettings
                  descriptionMode="tooltip"
                  grouped={true}
                />
              )}
              {selectedModel === "openrouter-transcription" && (
                <OpenRouterTranscriptionSettings
                  descriptionMode="tooltip"
                  grouped={true}
                />
              )}
              <TranscriptionCostReport />
            </SettingsGroup>
          </>
        );
      case "providers":
        return (
          <SettingsGroup title={t("settings.advanced.groups.llmProviders")}>
            <RegisteredLlmProviders />
          </SettingsGroup>
        );
      case "mcp":
        return (
          <SettingsGroup title={t("settings.advanced.groups.mcp")}>
            <McpSettings />
          </SettingsGroup>
        );
      case "history":
        return (
          <SettingsGroup title={t("settings.advanced.groups.history")}>
            <CrashResilientRecording descriptionMode="tooltip" grouped={true} />
            <OpenRecordingsFolder descriptionMode="tooltip" grouped={true} />
            <HistoryLimit descriptionMode="tooltip" grouped={true} />
            <RecordingRetentionPeriodSelector
              descriptionMode="tooltip"
              grouped={true}
            />
          </SettingsGroup>
        );
      case "experimental":
        return (
          <SettingsGroup title={t("settings.advanced.groups.experimental")}>
            <PostProcessingToggle descriptionMode="tooltip" grouped={true} />
            <KeyboardImplementationSelector
              descriptionMode="tooltip"
              grouped={true}
            />
          </SettingsGroup>
        );
    }
  };

  return (
    <div className="w-full space-y-4">
      <div className="flex gap-1 overflow-x-auto border-b border-mid-gray/20 pb-px">
        {TABS.map((tab) => (
          <button
            key={tab}
            onClick={() => setActiveTab(tab)}
            className={`px-3 py-1.5 text-sm font-medium whitespace-nowrap rounded-t-md border-b-2 transition-colors cursor-pointer ${
              activeTab === tab
                ? "border-logo-primary text-text"
                : "border-transparent text-text/50 hover:text-text/80"
            }`}
          >
            {t(`settings.advanced.tabs.${tab}`)}
          </button>
        ))}
      </div>
      {renderTab()}
    </div>
  );
};
