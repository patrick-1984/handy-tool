import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { X } from "lucide-react";
import { useSettings } from "../../hooks/useSettings";
import { SettingContainer } from "../ui/SettingContainer";

interface CustomWordsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const CustomWords: React.FC<CustomWordsProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const [newWord, setNewWord] = useState("");
    const customWords = getSetting("custom_words") || [];

    const handleAddWord = () => {
      const trimmedWord = newWord.trim();
      const sanitizedWord = trimmedWord.replace(/[<>"'&]/g, "");
      if (
        sanitizedWord &&
        !sanitizedWord.includes(" ") &&
        sanitizedWord.length <= 50
      ) {
        if (customWords.includes(sanitizedWord)) {
          toast.error(
            t("settings.advanced.customWords.duplicate", {
              word: sanitizedWord,
            }),
          );
          return;
        }
        updateSetting("custom_words", [...customWords, sanitizedWord]);
        setNewWord("");
      }
    };

    const handleRemoveWord = (wordToRemove: string) => {
      updateSetting(
        "custom_words",
        customWords.filter((word) => word !== wordToRemove),
      );
    };

    const handleKeyPress = (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        handleAddWord();
      }
    };

    const busy = isUpdating("custom_words");

    // One field holding the words as chips; a new word is added with Enter.
    return (
      <SettingContainer
        title={t("settings.advanced.customWords.title")}
        description={t("settings.advanced.customWords.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
        layout="stacked"
      >
        <div className="flex flex-wrap items-center gap-1.5 p-1.5 rounded-md border border-control-border border-b-control-bottom bg-control focus-within:shadow-[inset_0_-2px_0_var(--color-accent)]">
          {customWords.map((word) => (
            <span
              key={word}
              className="inline-flex items-center gap-0.5 h-[26px] ps-2.5 pe-0.5 rounded-sm border border-border bg-surface2 text-[13px] text-text"
            >
              {word}
              <button
                type="button"
                onClick={() => handleRemoveWord(word)}
                disabled={busy}
                aria-label={t("settings.advanced.customWords.remove", { word })}
                className="inline-flex items-center justify-center w-5 h-5 rounded-[3px] text-text-secondary hover:bg-hover hover:text-text transition-colors cursor-pointer disabled:cursor-not-allowed"
              >
                <X className="w-3 h-3" aria-hidden />
              </button>
            </span>
          ))}
          <input
            type="text"
            value={newWord}
            onChange={(e) => setNewWord(e.target.value)}
            onKeyDown={handleKeyPress}
            placeholder={t("settings.advanced.customWords.placeholder")}
            aria-label={t("settings.advanced.customWords.placeholder")}
            disabled={busy}
            className="flex-1 min-w-40 h-[26px] px-1.5 bg-transparent text-[13px] text-text placeholder:text-text-secondary focus:outline-none"
          />
        </div>
      </SettingContainer>
    );
  },
);
