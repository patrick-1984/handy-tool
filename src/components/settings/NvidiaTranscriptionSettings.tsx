import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import { SettingContainer } from "../ui/SettingContainer";
import { TEXT_FIELD } from "../ui/controlClasses";

export const NvidiaTranscriptionSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const savedKey = getSetting("nvidia_transcription_key") ?? "";
  const savedFunction = getSetting("nvidia_transcription_function_id") ?? "";
  const [apiKey, setApiKey] = useState(savedKey);
  const [functionId, setFunctionId] = useState(savedFunction);
  useEffect(() => setApiKey(savedKey), [savedKey]);
  useEffect(() => setFunctionId(savedFunction), [savedFunction]);

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
      </SettingContainer>
    </>
  );
};
