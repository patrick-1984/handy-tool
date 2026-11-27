import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import { SettingContainer } from "../ui/SettingContainer";
import { Dropdown } from "../ui/Dropdown";
import { TEXT_FIELD } from "../ui/controlClasses";
import {
  NVIDIA_FUNCTION_PRESETS,
  presetKeyForFunctionId,
} from "../../lib/nvidiaPresets";

export const NvidiaTranscriptionSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const savedKey = getSetting("nvidia_transcription_key") ?? "";
  const savedFunction = getSetting("nvidia_transcription_function_id") ?? "";
  const [apiKey, setApiKey] = useState(savedKey);
  const [functionId, setFunctionId] = useState(savedFunction);
  useEffect(() => setApiKey(savedKey), [savedKey]);
  useEffect(() => setFunctionId(savedFunction), [savedFunction]);
  // "Custom" stays chosen while the saved ID matches a preset only if picked.
  const [customPicked, setCustomPicked] = useState(false);
  const savedPreset = presetKeyForFunctionId(savedFunction);
  const selectedPreset = customPicked ? "custom" : savedPreset;
  const presetOptions = [
    ...NVIDIA_FUNCTION_PRESETS.map((p) => ({
      value: p.key,
      label: t(`settings.advanced.nvidiaTranscription.preset.${p.key}`),
    })),
    {
      value: "custom",
      label: t("settings.advanced.nvidiaTranscription.preset.custom"),
    },
  ];

  return (
    <>
      <SettingContainer
        title={t("settings.advanced.nvidiaTranscription.apiKey.title")}
        description={t(
          "settings.advanced.nvidiaTranscription.apiKey.description",
        )}
        descriptionMode="tooltip"
        grouped
      >
        <input
          type="password"
          aria-label={t("settings.advanced.nvidiaTranscription.apiKey.title")}
          autoComplete="off"
          value={apiKey}
          onChange={(event) => setApiKey(event.target.value)}
          onBlur={() => {
            if (apiKey !== savedKey)
              updateSetting("nvidia_transcription_key", apiKey);
          }}
          placeholder={t(
            "settings.advanced.nvidiaTranscription.apiKey.placeholder",
          )}
          className={`w-full ${TEXT_FIELD}`}
        />
      </SettingContainer>
      <SettingContainer
        title={t("settings.advanced.nvidiaTranscription.functionId.title")}
        description={t(
          "settings.advanced.nvidiaTranscription.functionId.description",
        )}
        descriptionMode="tooltip"
        grouped
      >
        <div className="flex w-full flex-col gap-2">
          <Dropdown
            options={presetOptions}
            selectedValue={selectedPreset}
            onSelect={(value) => {
              const preset = NVIDIA_FUNCTION_PRESETS.find(
                (p) => p.key === value,
              );
              if (preset) {
                setCustomPicked(false);
                setFunctionId(preset.id);
                if (preset.id !== savedFunction)
                  updateSetting("nvidia_transcription_function_id", preset.id);
              } else {
                setCustomPicked(true);
              }
            }}
          />
          {selectedPreset === "custom" && (
            <input
              type="text"
              aria-label={t(
                "settings.advanced.nvidiaTranscription.functionId.title",
              )}
              value={functionId}
              onChange={(event) => setFunctionId(event.target.value)}
              onBlur={() => {
                if (functionId !== savedFunction)
                  updateSetting("nvidia_transcription_function_id", functionId);
              }}
              placeholder={t(
                "settings.advanced.nvidiaTranscription.functionId.placeholder",
              )}
              className={`w-full ${TEXT_FIELD}`}
            />
          )}
        </div>
      </SettingContainer>
    </>
  );
};
