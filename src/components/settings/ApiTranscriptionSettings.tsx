import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import { SettingContainer } from "../ui/SettingContainer";

interface ApiTranscriptionSettingsProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const ApiTranscriptionSettings: React.FC<ApiTranscriptionSettingsProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting } = useSettings();

    const savedUrl = (getSetting("api_transcription_url") as string) ?? "";
    const savedKey = (getSetting("api_transcription_key") as string) ?? "";
    const savedModel = (getSetting("api_transcription_model") as string) ?? "";

    const [url, setUrl] = useState(savedUrl);
    const [apiKey, setApiKey] = useState(savedKey);
    const [model, setModel] = useState(savedModel);

    // Sync local state when store changes externally
    useEffect(() => setUrl(savedUrl), [savedUrl]);
    useEffect(() => setApiKey(savedKey), [savedKey]);
    useEffect(() => setModel(savedModel), [savedModel]);

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.apiTranscription.url.title")}
          description={t("settings.advanced.apiTranscription.url.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <input
            type="text"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onBlur={() => {
              if (url !== savedUrl) updateSetting("api_transcription_url", url);
            }}
            placeholder={t(
              "settings.advanced.apiTranscription.url.placeholder",
            )}
            className="w-full rounded-md border border-zinc-700 bg-zinc-800 px-3 py-1.5 text-sm text-zinc-100 placeholder-zinc-500 focus:border-blue-500 focus:outline-none"
          />
        </SettingContainer>

        <SettingContainer
          title={t("settings.advanced.apiTranscription.apiKey.title")}
          description={t(
            "settings.advanced.apiTranscription.apiKey.description",
          )}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            onBlur={() => {
              if (apiKey !== savedKey)
                updateSetting("api_transcription_key", apiKey);
            }}
            placeholder={t(
              "settings.advanced.apiTranscription.apiKey.placeholder",
            )}
            className="w-full rounded-md border border-zinc-700 bg-zinc-800 px-3 py-1.5 text-sm text-zinc-100 placeholder-zinc-500 focus:border-blue-500 focus:outline-none"
          />
        </SettingContainer>

        <SettingContainer
          title={t("settings.advanced.apiTranscription.model.title")}
          description={t(
            "settings.advanced.apiTranscription.model.description",
          )}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <input
            type="text"
            value={model}
            onChange={(e) => setModel(e.target.value)}
            onBlur={() => {
              if (model !== savedModel)
                updateSetting("api_transcription_model", model);
            }}
            placeholder={t(
              "settings.advanced.apiTranscription.model.placeholder",
            )}
            className="w-full rounded-md border border-zinc-700 bg-zinc-800 px-3 py-1.5 text-sm text-zinc-100 placeholder-zinc-500 focus:border-blue-500 focus:outline-none"
          />
        </SettingContainer>
      </>
    );
  });
