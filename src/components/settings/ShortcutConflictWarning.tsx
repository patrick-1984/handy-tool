import React from "react";
import { WarningIcon } from "@/components/ui/WarningIcon";
import { useTranslation } from "react-i18next";
import { useSettings } from "@/hooks/useSettings";
import { useOsType } from "@/hooks/useOsType";
import { normalizeChord } from "@/lib/utils/keyboard";

// Mirrors `is_jumper_binding` in src-tauri/src/shortcut/mod.rs.
const isJumperBinding = (id: string) =>
  id.startsWith("anchor_") ||
  id.startsWith("jump_slot_") ||
  id.startsWith("jump_set_slot_");

/**
 * For every shortcut that shares its keys with another, the ids of the others.
 *
 * Only shortcuts that can actually fire count: not ones switched off ("None"),
 * the post-processing hotkey while post-processing is off, the Windows-only
 * Jumper elsewhere, or Cancel on Linux (never registered there).
 */
export const useShortcutConflicts = (): Map<string, string[]> => {
  const { getSetting } = useSettings();
  const osType = useOsType();
  const bindings = getSetting("bindings") ?? {};
  const postProcessEnabled = getSetting("post_process_enabled") ?? false;
  const pauseEnabled = getSetting("pause_button_enabled") ?? false;
  const undoWordEnabled = getSetting("undo_word_enabled") ?? false;

  const idsByChord = new Map<string, string[]>();
  for (const [id, binding] of Object.entries(bindings)) {
    const chord = normalizeChord(binding?.current_binding ?? "");
    if (!chord) continue;
    if (id === "transcribe_with_post_process" && !postProcessEnabled) continue;
    if (isJumperBinding(id) && osType !== "windows") continue;
    if (id === "cancel" && osType === "linux") continue;
    if (id === "pause" && !pauseEnabled) continue;
    if (id === "undo_word" && !undoWordEnabled) continue;
    idsByChord.set(chord, [...(idsByChord.get(chord) ?? []), id]);
  }

  const conflicts = new Map<string, string[]>();
  for (const ids of idsByChord.values()) {
    if (ids.length < 2) continue;
    for (const id of ids) {
      conflicts.set(
        id,
        ids.filter((other) => other !== id),
      );
    }
  }
  return conflicts;
};

/**
 * Flags a shortcut that shares its keys with another one. Duplicates are
 * allowed, but only one of them can work: with the default (Tauri) keyboard
 * backend the second stays inactive until the first moves off the chord.
 */
export const ShortcutConflictWarning: React.FC<{ shortcutId: string }> = ({
  shortcutId,
}) => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const others = useShortcutConflicts().get(shortcutId);
  if (!others) return null;

  const bindings = getSetting("bindings") ?? {};
  const names = others
    .map((id) =>
      t(
        `settings.general.shortcut.bindings.${id}.name`,
        bindings[id]?.name ?? id,
      ),
    )
    .join(", ");
  const message = t("settings.general.shortcut.conflict", { names });
  return (
    <WarningIcon
      message={message}
      label={t("settings.general.shortcut.badge.conflict")}
      tone="error"
    />
  );
};
