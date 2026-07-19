import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import { useModelStore } from "@/stores/modelStore";

interface Props {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * Transcription switches whose availability depends on the selected model.
 * Translate-to-English is greyed out for models that don't support it
 * (ModelInfo.supports_translation).
 */
export const TranscriptionModelOptions: React.FC<Props> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const info = useModelStore((s) =>
      s.models.find((m) => m.id === s.currentModel),
    );
    const supportsTranslate = info?.supports_translation ?? false;
    const translate = (getSetting("translate_to_english") as boolean) || false;

    return (
      <ToggleSwitch
        checked={supportsTranslate && translate}
        disabled={!supportsTranslate}
        onChange={(enabled) => updateSetting("translate_to_english", enabled)}
        isUpdating={isUpdating("translate_to_english")}
        label={t("settings.advanced.translateToEnglish.label")}
        description={
          supportsTranslate
            ? t("settings.advanced.translateToEnglish.description")
            : t("settings.advanced.translateToEnglish.unsupported")
        }
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);

TranscriptionModelOptions.displayName = "TranscriptionModelOptions";
