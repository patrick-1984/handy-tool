import React from "react";
import { useTranslation } from "react-i18next";
import { MicrophoneSelector } from "../MicrophoneSelector";
import { ShortcutInput } from "../ShortcutInput";
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
import { SettingsGroup } from "../../ui/SettingsGroup";
import { OutputDeviceSelector } from "../OutputDeviceSelector";
import { AudioFeedback } from "../AudioFeedback";
import { useSettings } from "../../../hooks/useSettings";
import { VolumeSlider } from "../VolumeSlider";
import { MuteWhileRecording } from "../MuteWhileRecording";
import { ModelSettingsCard } from "./ModelSettingsCard";

export const GeneralSettings: React.FC = () => {
  const { t } = useTranslation();
  const { audioFeedbackEnabled } = useSettings();
  return (
    <div className="w-full space-y-6">
      <SettingsGroup title={t("settings.general.title")}>
        <ShortcutInput shortcutId="transcribe" grouped={true} />
        <ShortcutInput shortcutId="transcribe_ptt" grouped={true} />
      </SettingsGroup>
      <SettingsGroup title={t("settings.general.transcribeGroup.title")}>
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
      <ModelSettingsCard />
      <SettingsGroup title={t("settings.sound.title")}>
        <MicrophoneSelector descriptionMode="tooltip" grouped={true} />
        <MuteWhileRecording descriptionMode="tooltip" grouped={true} />
        <AudioFeedback descriptionMode="tooltip" grouped={true} />
        <OutputDeviceSelector
          descriptionMode="tooltip"
          grouped={true}
          disabled={!audioFeedbackEnabled}
        />
        <VolumeSlider disabled={!audioFeedbackEnabled} />
      </SettingsGroup>
    </div>
  );
};
