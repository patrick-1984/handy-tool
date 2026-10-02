import React from "react";
import { useTranslation } from "react-i18next";
import { LANGUAGE_METADATA } from "@/i18n/languages";
import { useSettings } from "@/hooks/useSettings";
import { Dropdown, type DropdownOption } from "../../ui/Dropdown";

/** A note language's own name ("Polski"); the code if it is unknown. */
export const noteLanguageName = (code: string): string =>
  LANGUAGE_METADATA[code]?.nativeName ?? code;

/**
 * The language new notes are written in, right where notes are made
 * (History's toolbar, Manual note): "Same as the transcript" or one of the
 * app's languages by its own name. A skill written in English then can't
 * turn a Polish transcript's note into English.
 */
export const OutputLanguagePicker: React.FC<{ disabled?: boolean }> = ({
  disabled,
}) => {
  const { t } = useTranslation();
  const { settings, updateSetting } = useSettings();

  const options: DropdownOption[] = [
    { value: "", label: t("settings.notes.language.sameAsTranscript") },
    ...Object.entries(LANGUAGE_METADATA)
      .sort(
        ([, a], [, b]) => (a.priority ?? Infinity) - (b.priority ?? Infinity),
      )
      .map(([code, meta]) => ({ value: code, label: meta.nativeName })),
  ];
  const selected = settings?.note_language ?? "";

  return (
    <div className="flex items-center gap-2 min-w-0">
      <span
        className="text-xs text-text-secondary shrink-0 cursor-help"
        title={t("settings.notes.language.description")}
      >
        {t("settings.notes.language.title")}
      </span>
      <Dropdown
        className="w-[200px]"
        options={options}
        selectedValue={
          options.some((o) => o.value === selected) ? selected : ""
        }
        onSelect={(value) => void updateSetting("note_language", value)}
        disabled={disabled}
      />
    </div>
  );
};
