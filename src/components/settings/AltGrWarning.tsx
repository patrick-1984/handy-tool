import React from "react";
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
 * Anyone who never types with AltGr can switch the warning off: its tooltip
 * leads to the switch at the end of the Shortcuts page.
 */
export const AltGrWarning: React.FC<{ binding: string }> = ({ binding }) => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const navigateTo = useNavStore((state) => state.navigateTo);
  if (!isAltGrRiskyChord(binding)) return null;
  if (getSetting("altgr_warning_enabled") === false) return null;

  return (
    <WarningIcon
      message={t("settings.general.shortcut.altGrConflict")}
      className="text-amber-500"
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
