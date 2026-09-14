import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

type Flow = "output" | "submit";

interface AffixSettingsProps {
  /** Which delivery flow these affixes belong to: plain Transcribe, or
   *  Transcribe & Submit. The two are configured independently, mirroring how
   *  the repo already splits paste_method / submit_paste_method. */
  flow: Flow;
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * Custom text wrapped around a delivered transcription: a prefix, a suffix, or
 * both, each with its own "add a newline" toggle.
 *
 * Applied to what is DELIVERED only — the history row keeps the plain transcript,
 * so Paste Last cannot re-apply affixes to text that already carries them.
 */
export const AffixSettings: React.FC<AffixSettingsProps> = React.memo(
  ({ flow, descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const prefixTextKey = `${flow}_prefix_text` as const;
    const suffixTextKey = `${flow}_suffix_text` as const;

    const savedPrefix = (getSetting(prefixTextKey) as string) ?? "";
    const savedSuffix = (getSetting(suffixTextKey) as string) ?? "";

    // Local state + onBlur: a global re-render on every keystroke steals focus.
    const [prefix, setPrefix] = useState(savedPrefix);
    const [suffix, setSuffix] = useState(savedSuffix);
    useEffect(() => setPrefix(savedPrefix), [savedPrefix]);
    useEffect(() => setSuffix(savedSuffix), [savedSuffix]);

    const prefixEnabled =
      (getSetting(`${flow}_prefix_enabled`) as boolean) ?? false;
    const suffixEnabled =
      (getSetting(`${flow}_suffix_enabled`) as boolean) ?? false;

    const inputClass =
      "w-full rounded-md border border-zinc-700 bg-zinc-800 px-3 py-1.5 text-sm text-zinc-100 placeholder-zinc-500 focus:border-blue-500 focus:outline-none";

    return (
      <>
        <ToggleSwitch
          checked={prefixEnabled}
          onChange={(v) => updateSetting(`${flow}_prefix_enabled`, v)}
          isUpdating={isUpdating(`${flow}_prefix_enabled`)}
          label={t("settings.advanced.affix.prefix.enable.title")}
          description={t("settings.advanced.affix.prefix.enable.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        />
        {prefixEnabled && (
          <>
            <SettingContainer
              title={t("settings.advanced.affix.prefix.placeholder")}
              description={t(
                "settings.advanced.affix.prefix.enable.description",
              )}
              descriptionMode={descriptionMode}
              grouped={grouped}
            >
              <input
                type="text"
                value={prefix}
                onChange={(e) => setPrefix(e.target.value)}
                onBlur={() => {
                  if (prefix !== savedPrefix)
                    updateSetting(prefixTextKey, prefix);
                }}
                placeholder={t("settings.advanced.affix.prefix.placeholder")}
                className={inputClass}
              />
            </SettingContainer>
            <ToggleSwitch
              checked={
                (getSetting(`${flow}_prefix_newline`) as boolean) ?? true
              }
              onChange={(v) => updateSetting(`${flow}_prefix_newline`, v)}
              isUpdating={isUpdating(`${flow}_prefix_newline`)}
              label={t("settings.advanced.affix.prefix.newline.title")}
              description={t(
                "settings.advanced.affix.prefix.newline.description",
              )}
              descriptionMode={descriptionMode}
              grouped={grouped}
            />
          </>
        )}

        <ToggleSwitch
          checked={suffixEnabled}
          onChange={(v) => updateSetting(`${flow}_suffix_enabled`, v)}
          isUpdating={isUpdating(`${flow}_suffix_enabled`)}
          label={t("settings.advanced.affix.suffix.enable.title")}
          description={t("settings.advanced.affix.suffix.enable.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        />
        {suffixEnabled && (
          <>
            <SettingContainer
              title={t("settings.advanced.affix.suffix.placeholder")}
              description={t(
                "settings.advanced.affix.suffix.enable.description",
              )}
              descriptionMode={descriptionMode}
              grouped={grouped}
            >
              <input
                type="text"
                value={suffix}
                onChange={(e) => setSuffix(e.target.value)}
                onBlur={() => {
                  if (suffix !== savedSuffix)
                    updateSetting(suffixTextKey, suffix);
                }}
                placeholder={t("settings.advanced.affix.suffix.placeholder")}
                className={inputClass}
              />
            </SettingContainer>
            <ToggleSwitch
              checked={
                (getSetting(`${flow}_suffix_newline`) as boolean) ?? true
              }
              onChange={(v) => updateSetting(`${flow}_suffix_newline`, v)}
              isUpdating={isUpdating(`${flow}_suffix_newline`)}
              label={t("settings.advanced.affix.suffix.newline.title")}
              description={t(
                "settings.advanced.affix.suffix.newline.description",
              )}
              descriptionMode={descriptionMode}
              grouped={grouped}
            />
          </>
        )}
      </>
    );
  },
);
