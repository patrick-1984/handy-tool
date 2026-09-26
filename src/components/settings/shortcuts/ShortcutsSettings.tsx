import React from "react";
import { useTranslation } from "react-i18next";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ShortcutInput } from "../ShortcutInput";
import { ShortcutRegistrationFailures } from "../ShortcutRegistrationFailures";
import { useShortcutConflicts } from "../ShortcutConflictWarning";
import { useSettings } from "../../../hooks/useSettings";
import { useOsType } from "../../../hooks/useOsType";

const JUMPER_SLOTS = [1, 2, 3, 4, 5, 6, 7, 8, 9];

/**
 * Every shortcut on one page, so two actions on the same keys are easy to spot.
 * The feature pages keep their own shortcut rows as well; both edit the same
 * binding. Rows are shown under the same conditions the backend registers them.
 */
export const ShortcutsSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const osType = useOsType();
  const conflicts = useShortcutConflicts();
  const postProcessEnabled = getSetting("post_process_enabled") ?? false;

  return (
    <div className="w-full space-y-6">
      {conflicts.size > 0 && (
        <section className="rounded-lg border border-red-500/30 bg-red-500/10 p-4">
          <h3 className="text-sm font-semibold text-red-400">
            {t("settings.shortcuts.conflicts.title")}
          </h3>
          <p className="mt-1 text-xs text-mid-gray">
            {t("settings.shortcuts.conflicts.description")}
          </p>
        </section>
      )}
      <ShortcutRegistrationFailures />
      <SettingsGroup title={t("settings.shortcuts.groups.dictation")}>
        <ShortcutInput shortcutId="transcribe" grouped={true} />
        <ShortcutInput shortcutId="transcribe_ptt" grouped={true} />
        <ShortcutInput shortcutId="transcribe_and_submit" grouped={true} />
        {postProcessEnabled && (
          <ShortcutInput
            shortcutId="transcribe_with_post_process"
            grouped={true}
          />
        )}
        <ShortcutInput shortcutId="paste_last" grouped={true} />
        {/* Cancel is never registered on Linux (dynamic registration is unstable there) */}
        {osType !== "linux" && (
          <ShortcutInput shortcutId="cancel" grouped={true} />
        )}
      </SettingsGroup>
      <SettingsGroup title={t("sidebar.keyboardTyper")}>
        <ShortcutInput shortcutId="type_text" grouped={true} />
      </SettingsGroup>
      {/* The Jumper is Windows-only; its hotkeys are not registered elsewhere. */}
      {osType === "windows" && (
        <SettingsGroup title={t("sidebar.jumper")}>
          <ShortcutInput shortcutId="anchor_set" grouped={true} />
          <ShortcutInput shortcutId="anchor_jump" grouped={true} />
          <ShortcutInput shortcutId="anchor_set_2" grouped={true} />
          <ShortcutInput shortcutId="anchor_jump_2" grouped={true} />
          {JUMPER_SLOTS.map((i) => (
            <React.Fragment key={i}>
              <ShortcutInput shortcutId={`jump_set_slot_${i}`} grouped={true} />
              <ShortcutInput shortcutId={`jump_slot_${i}`} grouped={true} />
            </React.Fragment>
          ))}
        </SettingsGroup>
      )}
    </div>
  );
};
