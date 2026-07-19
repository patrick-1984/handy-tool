import React from "react";
import { useTranslation } from "react-i18next";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { LanguageSelector } from "../LanguageSelector";
import { TranslateToEnglish } from "../TranslateToEnglish";
import { useModelStore } from "../../../stores/modelStore";
import type { ModelInfo } from "@/bindings";

// Engines that accept a forced input language. Other multilingual engines
// (e.g. Parakeet V3) detect the language automatically and cannot be forced,
// so we surface an informational note instead of a picker.
const LANGUAGE_FORCING_ENGINES = [
  "Whisper",
  "SenseVoice",
  "FlmWhisper",
  "ApiWhisper",
];

export const ModelSettingsCard: React.FC = () => {
  const { t } = useTranslation();
  const { currentModel, models } = useModelStore();

  const currentModelInfo = models.find((m: ModelInfo) => m.id === currentModel);

  const supportsLanguageSelection = currentModelInfo
    ? LANGUAGE_FORCING_ENGINES.includes(currentModelInfo.engine_type)
    : false;
  const isMultilingual =
    (currentModelInfo?.supported_languages.length ?? 0) > 1;
  // Multilingual model whose engine can't be pinned to a language → auto-detect.
  const autoDetectsOnly = isMultilingual && !supportsLanguageSelection;
  const supportsTranslation = currentModelInfo?.supports_translation ?? false;
  const hasAnySettings =
    supportsLanguageSelection || autoDetectsOnly || supportsTranslation;

  // Don't render anything if no model is selected or no settings available
  if (!currentModel || !currentModelInfo || !hasAnySettings) {
    return null;
  }

  return (
    <SettingsGroup
      title={t("settings.modelSettings.title", {
        model: currentModelInfo.name,
      })}
    >
      {supportsLanguageSelection && (
        <LanguageSelector
          descriptionMode="tooltip"
          grouped={true}
          supportedLanguages={currentModelInfo.supported_languages}
        />
      )}
      {autoDetectsOnly && (
        <SettingContainer
          title={t("settings.general.language.title")}
          description={t(
            "settings.modelSettings.autoDetectLanguage.description",
          )}
          descriptionMode="tooltip"
          grouped={true}
        >
          <span className="px-2 py-1 text-sm font-semibold text-mid-gray">
            {t("settings.modelSettings.autoDetectLanguage.value")}
          </span>
        </SettingContainer>
      )}
      {supportsTranslation && (
        <TranslateToEnglish descriptionMode="tooltip" grouped={true} />
      )}
    </SettingsGroup>
  );
};
