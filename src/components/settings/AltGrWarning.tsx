import React from "react";
import { AlertTriangle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { isAltGrRiskyChord } from "@/lib/utils/keyboard";

/**
 * Warns that a chord is one AltGr can type a character with.
 *
 * Windows reports AltGr as Ctrl+Alt, so `ctrl+alt+o` steals ó from anyone on a
 * Polish (Programmers) layout — the shortcut fires and the character never
 * arrives. The application's own defaults are kept clear of these chords
 * (`no_default_binding_collides_with_altgr`), but a user can still pick one by
 * hand, and until 1.4.0 one of the shipped defaults was `ctrl+alt+space`.
 */
export const AltGrWarning: React.FC<{ binding: string }> = ({ binding }) => {
  const { t } = useTranslation();
  if (!isAltGrRiskyChord(binding)) return null;

  const message = t("settings.general.shortcut.altGrConflict");
  return (
    <span
      title={message}
      aria-label={message}
      className="flex items-center text-amber-500"
    >
      <AlertTriangle className="h-4 w-4 shrink-0" />
    </span>
  );
};
