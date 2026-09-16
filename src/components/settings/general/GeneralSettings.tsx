import React from "react";
import { useTranslation } from "react-i18next";
import { MicrophoneSelector } from "../MicrophoneSelector";
import { CaptureSourceSettings } from "../CaptureSourceSettings";
import { ShortcutInput } from "../ShortcutInput";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { OutputDeviceSelector } from "../OutputDeviceSelector";
import { AudioFeedback } from "../AudioFeedback";
import { useSettings } from "../../../hooks/useSettings";
import { VolumeSlider } from "../VolumeSlider";
import { MuteWhileRecording } from "../MuteWhileRecording";
import { ModelSettingsCard } from "./ModelSettingsCard";
import { TranscriptionModeSetting } from "../TranscriptionModeSetting";
import { TranscriptionModePttSetting } from "../TranscriptionModePttSetting";
import { GpuDeviceSelector } from "../GpuDeviceSelector";
import { CustomWords } from "../CustomWords";
import { AppendTrailingSpace } from "../AppendTrailingSpace";
import { CancelBehaviorSetting } from "../CancelBehaviorSetting";
import { UpdateSettings } from "./UpdateSettings";
import { ShortcutRegistrationFailures } from "../ShortcutRegistrationFailures";

export const GeneralSettings: React.FC = () => {
  const { t } = useTranslation();
  const { audioFeedbackEnabled } = useSettings();
  return (
    <div className="w-full space-y-6">
      <SettingsGroup title={t("settings.general.title")}>
        <ShortcutInput shortcutId="transcribe" grouped={true} />
        <ShortcutInput shortcutId="transcribe_ptt" grouped={true} />
        {/* Both of these used to be reachable only from a feature-specific
            group — Transcribe & Submit from the Advanced tab, Paste Last from
            further down this page. Every trigger shortcut now lives here; the
            options behind them stay in Advanced › Transcription. */}
        <ShortcutInput shortcutId="transcribe_and_submit" grouped={true} />
        <ShortcutInput shortcutId="paste_last" grouped={true} />
        <CancelBehaviorSetting descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>
      <ShortcutRegistrationFailures />
      <ModelSettingsCard />
      <SettingsGroup title={t("settings.advanced.groups.transcription")}>
        <TranscriptionModeSetting descriptionMode="tooltip" grouped={true} />
        <TranscriptionModePttSetting descriptionMode="tooltip" grouped={true} />
        <GpuDeviceSelector descriptionMode="tooltip" grouped={true} />
        <CustomWords descriptionMode="tooltip" grouped />
        <AppendTrailingSpace descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>
      <SettingsGroup title={t("settings.sound.title")}>
        <CaptureSourceSettings descriptionMode="tooltip" grouped={true} />
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
      <SettingsGroup title={t("settings.general.updates.title")}>
        <UpdateSettings />
      </SettingsGroup>
    </div>
  );
};
