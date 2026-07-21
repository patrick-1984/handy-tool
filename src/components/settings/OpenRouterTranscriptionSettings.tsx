import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import { SettingContainer } from "../ui/SettingContainer";
import { Dropdown } from "@/components/ui";
import type {
  OpenRouterTranscriptionRoute,
  TranscriptionAudioFormat,
} from "@/bindings";

interface Props {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

const INPUT_CLASS =
  "w-full rounded-md border border-zinc-700 bg-zinc-800 px-3 py-1.5 text-sm text-zinc-100 placeholder-zinc-500 focus:border-blue-500 focus:outline-none";

/**
 * Config for the "OpenRouter Transcription" engine (T-308: dedicated base URL +
 * API key, independent of the LLM providers registry). OpenRouter uses JSON +
 * base64 audio (not OpenAI's multipart upload), so it has its own engine.
 */
export const OpenRouterTranscriptionSettings: React.FC<Props> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting } = useSettings();

    const savedUrl =
      (getSetting("openrouter_transcription_url") as string) ?? "";
    const savedKey =
      (getSetting("openrouter_transcription_key") as string) ?? "";
    const savedModel =
      (getSetting("openrouter_transcription_model") as string) ?? "";
    const route =
      (getSetting("openrouter_transcription_route") as string) ?? "stt";
    const audioFormat =
      (getSetting("openrouter_transcription_audio_format") as string) ?? "opus";

    const [url, setUrl] = useState(savedUrl);
    const [apiKey, setApiKey] = useState(savedKey);
    const [model, setModel] = useState(savedModel);
    useEffect(() => setUrl(savedUrl), [savedUrl]);
    useEffect(() => setApiKey(savedKey), [savedKey]);
    useEffect(() => setModel(savedModel), [savedModel]);

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.openRouterTranscription.url.title")}
          description={t(
            "settings.advanced.openRouterTranscription.url.description",
          )}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <input
            type="text"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onBlur={() => {
              if (url !== savedUrl)
                updateSetting("openrouter_transcription_url", url);
            }}
            placeholder="https://openrouter.ai/api/v1"
            className={INPUT_CLASS}
          />
        </SettingContainer>

        <SettingContainer
          title={t("settings.advanced.openRouterTranscription.apiKey.title")}
          description={t(
            "settings.advanced.openRouterTranscription.apiKey.description",
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
                updateSetting("openrouter_transcription_key", apiKey);
            }}
            placeholder="sk-or-..."
            className={INPUT_CLASS}
          />
        </SettingContainer>

        <SettingContainer
          title={t("settings.advanced.openRouterTranscription.model.title")}
          description={t(
            "settings.advanced.openRouterTranscription.model.description",
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
                updateSetting("openrouter_transcription_model", model);
            }}
            placeholder="openai/whisper-large-v3"
            className={INPUT_CLASS}
          />
        </SettingContainer>

        <SettingContainer
          title={t("settings.advanced.openRouterTranscription.route.title")}
          description={t(
            "settings.advanced.openRouterTranscription.route.description",
          )}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <Dropdown
            selectedValue={route}
            options={[
              {
                value: "stt",
                label: t("settings.advanced.openRouterTranscription.route.stt"),
              },
              {
                value: "chat",
                label: t(
                  "settings.advanced.openRouterTranscription.route.chat",
                ),
              },
            ]}
            onSelect={(value) =>
              updateSetting(
                "openrouter_transcription_route",
                (value ?? "stt") as OpenRouterTranscriptionRoute,
              )
            }
            className="min-w-[260px]"
          />
        </SettingContainer>

        <SettingContainer
          title={t("settings.advanced.openRouterTranscription.format.title")}
          description={t(
            "settings.advanced.openRouterTranscription.format.description",
          )}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <Dropdown
            selectedValue={audioFormat}
            options={[
              {
                value: "opus",
                label: t(
                  "settings.advanced.openRouterTranscription.format.opus",
                ),
              },
              {
                value: "wav",
                label: t(
                  "settings.advanced.openRouterTranscription.format.wav",
                ),
              },
            ]}
            onSelect={(value) =>
              updateSetting(
                "openrouter_transcription_audio_format",
                (value ?? "opus") as TranscriptionAudioFormat,
              )
            }
            className="min-w-[260px]"
          />
        </SettingContainer>
      </>
    );
  },
);

OpenRouterTranscriptionSettings.displayName = "OpenRouterTranscriptionSettings";
