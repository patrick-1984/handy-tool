import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { SettingContainer } from "../ui/SettingContainer";
import { Dropdown } from "../ui/Dropdown";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import { commands, type AudioDevice, type CaptureSource } from "@/bindings";

interface CaptureSourceSettingsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * Which sound source a take records: the microphone, the system audio being played
 * through a speaker endpoint, or both mixed together.
 *
 * Windows-only. cpal's WASAPI host gives loopback capture for free on any render
 * endpoint opened as an input, while its CoreAudio host has no tap and its ALSA host
 * cannot reach a PipeWire monitor — so the control is hidden elsewhere rather than
 * offered and then silently recording nothing. The backend clamps the stored value
 * too (`effective_capture_source`), so a settings file copied from Windows is
 * harmless on another platform.
 */
export const CaptureSourceSettings: React.FC<CaptureSourceSettingsProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const osType = useOsType();

    const [outputs, setOutputs] = useState<AudioDevice[]>([]);

    const source = (getSetting("capture_source") as string) ?? "microphone";
    const savedDevice =
      (getSetting("system_audio_device") as string | null) ?? null;

    useEffect(() => {
      if (osType !== "windows") return;
      commands
        .getAvailableOutputDevices()
        .then((r) => {
          if (r.status === "ok") setOutputs(r.data);
        })
        .catch(() => {
          /* the picker falls back to "system default", which always resolves */
        });
    }, [osType]);

    // Hidden, not disabled: this can never work off Windows, and a greyed-out
    // control only invites "why?". Matches how the repo handles its other
    // Windows-only settings.
    if (osType !== "windows") return null;

    const sourceOptions = [
      {
        value: "microphone",
        label: t("settings.advanced.captureSource.options.microphone"),
      },
      {
        value: "system_audio",
        label: t("settings.advanced.captureSource.options.systemAudio"),
      },
      {
        value: "microphone_and_system_audio",
        label: t("settings.advanced.captureSource.options.both"),
      },
    ];

    const usesSystemAudio = source !== "microphone";

    const deviceOptions = [
      {
        value: "default",
        label: t("settings.advanced.systemAudioDevice.followDefault"),
      },
      ...outputs.map((d) => ({ value: d.name, label: d.name })),
    ];

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.captureSource.title")}
          description={t("settings.advanced.captureSource.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <Dropdown
            options={sourceOptions}
            selectedValue={source}
            onSelect={(v) =>
              updateSetting("capture_source", v as CaptureSource)
            }
            disabled={isUpdating("capture_source")}
          />
        </SettingContainer>

        {usesSystemAudio && (
          <SettingContainer
            title={t("settings.advanced.systemAudioDevice.title")}
            description={t("settings.advanced.systemAudioDevice.description")}
            descriptionMode={descriptionMode}
            grouped={grouped}
          >
            <Dropdown
              options={deviceOptions}
              selectedValue={savedDevice ?? "default"}
              onSelect={(v) =>
                updateSetting("system_audio_device", v as string)
              }
              disabled={isUpdating("system_audio_device")}
            />
          </SettingContainer>
        )}
      </>
    );
  });
