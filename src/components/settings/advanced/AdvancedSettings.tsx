import React, { useEffect, useState } from "react";
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
import { ApiTranscriptionSettings } from "../ApiTranscriptionSettings";
import { OpenRouterTranscriptionSettings } from "../OpenRouterTranscriptionSettings";
import { TranscriptionModelOptions } from "../TranscriptionModelOptions";
import { KeyboardImplementationSelector } from "../debug/KeyboardImplementationSelector";
import { RegisteredLlmProviders } from "./RegisteredLlmProviders";
import { McpSettings } from "./McpSettings";
import { TranscriptionCostReport } from "./TranscriptionCostReport";
import { TranscribeAndSubmitSettings } from "../TranscribeAndSubmitSettings";
import { JumperDelaySetting } from "../JumperDelaySetting";
import { JumperTrackToggle } from "../JumperTrackToggle";
import { JumperReturnFocusToggle } from "../JumperReturnFocusToggle";
import { PasteMethodSetting } from "../PasteMethod";
import { PasteMethodPttSetting } from "../PasteMethodPtt";
import { TypingToolSetting } from "../TypingTool";
import { ClipboardHandlingSetting } from "../ClipboardHandling";
import { AutoSubmit } from "../AutoSubmit";
import { ClipboardRestoreDelaySetting } from "../ClipboardRestoreDelay";
import { AnchorActionSetting } from "../AnchorActionSetting";
import { useNavStore } from "@/stores/navStore";

const TABS = [
  "app",
  "transcription",
  "providers",
  "mcp",
  "history",
  "postProcessing",
] as const;
export type TabId = (typeof TABS)[number];

export const AdvancedSettings: React.FC = () => {
  const { t } = useTranslation();
  const pendingAdvancedTab = useNavStore((state) => state.pendingAdvancedTab);
  const consumePendingAdvancedTab = useNavStore(
    (state) => state.consumePendingAdvancedTab,
  );
  const [activeTab, setActiveTab] = useState<TabId>(
    () => pendingAdvancedTab ?? "app",
  );

  useEffect(() => {
    const pendingTab = consumePendingAdvancedTab();
    if (pendingTab !== null) {
      setActiveTab(pendingTab);
    }
  }, [pendingAdvancedTab, consumePendingAdvancedTab]);

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
            <KeyboardImplementationSelector
              descriptionMode="tooltip"
              grouped={true}
            />
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
            </SettingsGroup>
            <TranscribeAndSubmitSettings />
            <SettingsGroup title={t("settings.advanced.groups.transcription")}>
              <TranscriptionModelOptions
                descriptionMode="tooltip"
                grouped={true}
              />
              <TranscriptionCostReport />
            </SettingsGroup>
          </>
        );
      case "providers":
        return (
          <>
            <SettingsGroup
              title={t("settings.advanced.apiTranscription.cardTitle")}
            >
              <ApiTranscriptionSettings
                descriptionMode="tooltip"
                grouped={true}
              />
            </SettingsGroup>
            <SettingsGroup
              title={t("settings.advanced.openRouterTranscription.cardTitle")}
            >
              <OpenRouterTranscriptionSettings
                descriptionMode="tooltip"
                grouped={true}
              />
            </SettingsGroup>
            <SettingsGroup title={t("settings.advanced.groups.llmProviders")}>
              <RegisteredLlmProviders />
            </SettingsGroup>
          </>
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
      case "postProcessing":
        return (
          <SettingsGroup title={t("settings.advanced.groups.postProcessing")}>
            <PostProcessingToggle descriptionMode="tooltip" grouped={true} />
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
