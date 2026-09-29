import React from "react";
import { chordHasShift, chordKey, useAltGrLayouts } from "@/lib/altgrLayouts";
import { formatKeyCombination } from "@/lib/utils/keyboard";
import { useOsType } from "@/hooks/useOsType";
import { WarningIcon } from "@/components/ui/WarningIcon";
import { useTranslation } from "react-i18next";
import { isAltGrRiskyChord } from "@/lib/utils/keyboard";
import { useSettings } from "@/hooks/useSettings";
import { useNavStore } from "@/stores/navStore";
import { highlightSetting } from "@/lib/settingsSearch";

/**
 * Warns that a chord is one AltGr can type a character with.
 *
 * Windows reports AltGr as Ctrl+Alt, so `ctrl+alt+o` steals ó from anyone on a
 * Polish (Programmers) layout — the shortcut fires and the character never
 * arrives. The application's own defaults are kept clear of these chords
 * (`no_default_binding_collides_with_altgr`), but a user can still pick one by
 * hand, and until 1.4.0 one of the shipped defaults was `ctrl+alt+space`.
 *
 * Whether a chord really collides depends on the keyboard (Polish, German,
 * French, Czech... have AltGr characters; US English has none), so on Windows
 * it warns only when an installed keyboard types something with AltGr (or
 * AltGr+Shift, for a chord with Shift) on that key, and names the language the
 * keyboard belongs to. Space counts on any such keyboard: AltGr is often still
 * held for the space after an AltGr character. Elsewhere every Ctrl+Alt letter
 * is flagged.
 *
 * Anyone who never types with AltGr can switch the warning off: its tooltip
 * leads to the switch at the end of the Shortcuts page.
 */
export const AltGrWarning: React.FC<{ binding: string }> = ({ binding }) => {
  const { t, i18n } = useTranslation();
  const { getSetting } = useSettings();
  const osType = useOsType();
  const navigateTo = useNavStore((state) => state.navigateTo);
  // undefined until known; null = cannot tell (not Windows).
  const layouts = useAltGrLayouts();
  if (!isAltGrRiskyChord(binding)) return null;
  if (getSetting("altgr_warning_enabled") === false) return null;
  if (layouts === undefined) return null;
  const key = chordKey(binding);
  const shift = chordHasShift(binding);
  const hits =
    layouts?.filter((l) => (shift ? l.shift_keys : l.keys).includes(key)) ??
    null;
  // No installed layout types anything with AltGr on this key: no clash.
  if (hits && hits.length === 0) return null;
  const names = hits?.map((l) => {
    try {
      return (
        new Intl.DisplayNames([i18n.language], { type: "language" }).of(
          l.locale,
        ) ?? l.locale
      );
    } catch {
      return l.locale;
    }
  });

  return (
    <WarningIcon
      message={
        !names
          ? t("settings.general.shortcut.altGrConflict")
          : key === "space"
            ? t("settings.general.shortcut.altGrConflictSpace", {
                layouts: names.join(", "),
              })
            : t("settings.general.shortcut.altGrConflictLayout", {
                layouts: names.join(", "),
                key: formatKeyCombination(shift ? `shift+${key}` : key, osType),
              })
      }
      label={t("settings.general.shortcut.badge.risky")}
      action={{
        label: t("settings.shortcuts.altGrWarning.turnOff"),
        onClick: () => {
          navigateTo("shortcuts");
          highlightSetting(t("settings.shortcuts.altGrWarning.label"));
        },
      }}
    />
  );
};
