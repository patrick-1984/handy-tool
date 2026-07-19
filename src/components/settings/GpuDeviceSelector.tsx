import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import { useModelStore } from "../../stores/modelStore";
import { useOsType } from "../../hooks/useOsType";
import { commands, type GpuDeviceOption } from "@/bindings";
import { Dropdown, type DropdownOption } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";

// Sentinel values mirroring the backend encoding in
// `AppSettings::transcribe_gpu_device` (T-212): rather than a parallel
// accelerator enum, Auto/CPU/explicit-device all live in one persisted i32.
const AUTO_VALUE = "-1";
const CPU_VALUE = "-2";

interface GpuDeviceSelectorProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const GpuDeviceSelector: React.FC<GpuDeviceSelectorProps> = ({
  descriptionMode = "inline",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const osType = useOsType();
  const currentModelId = useModelStore((s) => s.currentModel);
  const getModelInfo = useModelStore((s) => s.getModelInfo);
  const [devices, setDevices] = useState<GpuDeviceOption[]>([]);
  const [loaded, setLoaded] = useState(false);

  // Vulkan/whisper.cpp only — the fork's other engines (Parakeet, Moonshine,
  // SenseVoice) run CPU-only, and FLM/API/OpenRouter are external processes,
  // so this selector is meaningless outside the local Whisper engine.
  // Enumeration itself works on Windows and Linux (both build whisper-rs
  // with the vulkan feature); gating the UI to Windows only is a deliberate
  // scope reduction for this pass.
  const isWhisperModel =
    getModelInfo(currentModelId)?.engine_type === "Whisper";
  const shouldRender = osType === "windows" && isWhisperModel;

  const refreshDevices = async () => {
    try {
      const result = await commands.listGpuDevices();
      if (result.status === "ok") {
        setDevices(result.data);
      }
    } catch (error) {
      console.error("Failed to list GPU devices:", error);
    } finally {
      setLoaded(true);
    }
  };

  // Load once up front (not just on dropdown open) so a previously selected
  // explicit device index shows its real adapter name instead of falling
  // back to the dropdown's placeholder before the user ever opens it.
  useEffect(() => {
    if (shouldRender && !loaded) {
      refreshDevices();
    }
  }, [shouldRender, loaded]);

  if (!shouldRender) {
    return null;
  }

  const options: DropdownOption[] = [
    { value: AUTO_VALUE, label: t("settings.advanced.gpuDevice.options.auto") },
    { value: CPU_VALUE, label: t("settings.advanced.gpuDevice.options.cpu") },
    ...devices.map((d) => ({
      value: String(d.index),
      label: d.vram_total_mb
        ? t("settings.advanced.gpuDevice.options.deviceWithVram", {
            index: d.index,
            name: d.name,
            vram: Math.round(d.vram_total_mb / 1024),
          })
        : t("settings.advanced.gpuDevice.options.device", {
            index: d.index,
            name: d.name,
          }),
    })),
  ];

  const currentValue = String(getSetting("transcribe_gpu_device") ?? -1);

  return (
    <SettingContainer
      title={t("settings.advanced.gpuDevice.title")}
      description={t("settings.advanced.gpuDevice.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    >
      <Dropdown
        options={options}
        selectedValue={currentValue}
        onSelect={(value) =>
          updateSetting("transcribe_gpu_device", Number(value))
        }
        onRefresh={refreshDevices}
        disabled={false}
      />
    </SettingContainer>
  );
};
