import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import { Dropdown, SettingContainer } from "@/components/ui";
import { SearchableModelSelect } from "./SearchableModelSelect";
import {
  commands,
  type LlmProvider,
  type OpenRouterTranscriptionRoute,
  type TranscriptionAudioFormat,
} from "@/bindings";

interface Props {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

/**
 * Config for the "OpenRouter Transcription" engine. Only takes effect when that
 * model is selected in the Models page. OpenRouter uses JSON + base64 audio
 * (not OpenAI's multipart upload), so it has its own engine + this config.
 */
export const OpenRouterTranscriptionSettings: React.FC<Props> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting } = useSettings();

    const providers =
      (getSetting("llm_providers") as LlmProvider[] | undefined) ?? [];
    // Only USABLE providers: an OpenRouter slot that's enabled and has an API key
    // (a keyless slot would just 401). If none qualify, we surface a hint.
    const openRouterProviders = providers
      .map((p, idx) => ({ p, idx }))
      .filter(
        ({ p }) =>
          p.kind === "openrouter" &&
          p.enabled &&
          (p.api_key ?? "").trim() !== "",
      );

    const providerRef =
      (getSetting("openrouter_transcription_provider_ref") as string) ?? "";
    const route =
      (getSetting("openrouter_transcription_route") as string) ?? "stt";
    const audioFormat =
      (getSetting("openrouter_transcription_audio_format") as string) ?? "opus";
    const model =
      (getSetting("openrouter_transcription_model") as string) ?? "";

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.openRouterTranscription.provider.title")}
          description={t(
            "settings.advanced.openRouterTranscription.provider.description",
          )}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          {openRouterProviders.length === 0 ? (
            <p className="text-xs text-red-400 max-w-[260px]">
              {t("settings.advanced.openRouterTranscription.provider.none")}
            </p>
          ) : (
            <Dropdown
              selectedValue={providerRef || null}
              options={openRouterProviders.map(({ p, idx }) => ({
                value: p.id,
                label: `#${idx + 1} ${p.name}`,
              }))}
              onSelect={(value) =>
                updateSetting(
                  "openrouter_transcription_provider_ref",
                  value ?? "",
                )
              }
              placeholder={t(
                "settings.advanced.openRouterTranscription.provider.placeholder",
              )}
              className="min-w-[260px]"
            />
          )}
        </SettingContainer>

        <SettingContainer
          title={t("settings.advanced.openRouterTranscription.model.title")}
          description={t(
            "settings.advanced.openRouterTranscription.model.description",
          )}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <SearchableModelSelect
            value={model}
            // Pass the provider ref as the source key (not for fetching — that's
            // fetchOverride) so switching provider invalidates the cached list.
            providerId={providerRef || null}
            fetchOverride={async () => {
              const r =
                await commands.listOpenrouterTranscriptionModels(providerRef);
              return r.status === "ok" ? r.data : [];
            }}
            onCommit={(v) => updateSetting("openrouter_transcription_model", v)}
            placeholder={t(
              "settings.advanced.openRouterTranscription.model.placeholder",
            )}
            className="min-w-[260px]"
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
