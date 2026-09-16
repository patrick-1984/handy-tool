import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { SettingContainer } from "../ui/SettingContainer";
import { Dropdown } from "../ui/Dropdown";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import { commands, type AudioDevice, type CaptureSource } from "@/bindings";

/** Alignment presets. The right value is hardware: a USB headset, a Bluetooth link
 *  and an HDMI monitor each buffer differently, so this cannot be guessed once. */
const DELAY_MS = [0, 50, 100, 150, 200, 300, 500, 750, 1000] as const;

/** Loopback is post-volume digital audio and a microphone is quiet and analogue, so
 *  the two legs can legitimately sit far apart with nothing readable predicting which
 *  way. Clamped to the same 0.25..4.0 the backend enforces. */
const GAIN_STEPS = [
  { value: "0.25", label: "0.25x (quietest)" },
  { value: "0.5", label: "0.5x" },
  { value: "0.75", label: "0.75x" },
  { value: "1", label: "1x (unchanged)" },
  { value: "1.5", label: "1.5x" },
  { value: "2", label: "2x" },
  { value: "3", label: "3x" },
  { value: "4", label: "4x (loudest)" },
];

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
      // get_available_output_devices() prepends a synthetic {index:"default",
      // name:"Default"} entry. Mapping it by NAME would pin the literal device
      // "Default", which resolves to nothing and makes every take fail - and this
      // component already offers its own "Follow system default" above.
      ...outputs
        .filter((d) => d.index !== "default")
        .map((d) => ({ value: d.name, label: d.name })),
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

        {usesSystemAudio && (
          <>
            <SettingContainer
              title={t("settings.advanced.systemAudioDelay.title")}
              description={t("settings.advanced.systemAudioDelay.description")}
              descriptionMode={descriptionMode}
              grouped={grouped}
            >
              <Dropdown
                options={DELAY_MS.map((ms) => ({
                  value: String(ms),
                  label: `${ms} ms`,
                }))}
                selectedValue={String(
                  (getSetting("system_audio_delay_ms") as number) ?? 100,
                )}
                onSelect={(v) =>
                  updateSetting("system_audio_delay_ms", Number(v))
                }
                disabled={isUpdating("system_audio_delay_ms")}
              />
            </SettingContainer>

            <SettingContainer
              title={t("settings.advanced.systemAudioGain.title")}
              description={t("settings.advanced.systemAudioGain.description")}
              descriptionMode={descriptionMode}
              grouped={grouped}
            >
              <Dropdown
                options={GAIN_STEPS}
                selectedValue={String(
                  (getSetting("system_audio_gain") as number) ?? 1,
                )}
                onSelect={(v) => updateSetting("system_audio_gain", Number(v))}
                disabled={isUpdating("system_audio_gain")}
              />
            </SettingContainer>
          </>
        )}
      </>
    );
  });
