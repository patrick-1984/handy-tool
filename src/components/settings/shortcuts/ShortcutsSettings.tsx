import { takeShortcutsAvailable } from "../../../lib/utils/keyboard";
import React from "react";
import { Mic, Keyboard, Crosshair, CircleHelp } from "lucide-react";
import { useTranslation } from "react-i18next";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { ShortcutInput } from "../ShortcutInput";
import { ShortcutRegistrationFailures } from "../ShortcutRegistrationFailures";
import { useShortcutConflicts } from "../ShortcutConflictWarning";
import { useSettings } from "../../../hooks/useSettings";
import { useOsType } from "../../../hooks/useOsType";
import { ShortcutKeeperGroup, ShortcutKeeperProvider } from "./ShortcutKeeper";

const JUMPER_SLOTS = [1, 2, 3, 4, 5, 6, 7, 8, 9];

/**
 * Every shortcut on one page, so two actions on the same keys are easy to spot.
 * The feature pages keep their own shortcut rows as well; both edit the same
 * binding. Rows are shown under the same conditions the backend registers them.
 */
export const ShortcutsSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const osType = useOsType();
  const conflicts = useShortcutConflicts();
  const postProcessEnabled = getSetting("post_process_enabled") ?? false;
  const pauseEnabled = getSetting("pause_button_enabled") ?? false;
  const undoWordEnabled = getSetting("undo_word_enabled") ?? false;

  const page = (
    <div className="w-full space-y-6">
      {conflicts.size > 0 && (
        <section className="rounded-lg border border-warn-border bg-warn-bg p-4">
          <h3 className="text-sm font-semibold text-warn-text">
            {t("settings.shortcuts.conflicts.title")}
          </h3>
          <p className="mt-1 text-xs text-text-secondary">
            {t("settings.shortcuts.conflicts.description")}
          </p>
        </section>
      )}
      <ShortcutRegistrationFailures />
      {/* Shortcut Keeper is Windows-only: its "Keep on this PC" boxes appear
          on this page's rows only (inside the provider). */}
      {osType === "windows" && <ShortcutKeeperGroup />}
      <SettingsGroup
        icon={Mic}
        title={t("settings.shortcuts.groups.dictation")}
      >
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
        {/* Take-only shortcuts: never registered on Linux (dynamic registration
            is unstable there); Pause and Undo word also need their option on. */}
        {takeShortcutsAvailable(osType) && (
          <ShortcutInput shortcutId="cancel" grouped={true} />
        )}
        {takeShortcutsAvailable(osType) && pauseEnabled && (
          <ShortcutInput shortcutId="pause" grouped={true} />
        )}
        {takeShortcutsAvailable(osType) && undoWordEnabled && (
          <ShortcutInput shortcutId="undo_word" grouped={true} />
        )}
        <ShortcutInput shortcutId="toggle_live_text_box" grouped={true} />
      </SettingsGroup>
      <SettingsGroup icon={Keyboard} title={t("sidebar.keyboardTyper")}>
        <ShortcutInput shortcutId="type_text" grouped={true} />
      </SettingsGroup>
      {/* The Jumper is Windows-only; its hotkeys are not registered elsewhere. */}
      {osType === "windows" && (
        <SettingsGroup icon={Crosshair} title={t("sidebar.jumper")}>
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
      <SettingsGroup
        icon={CircleHelp}
        title={t("settings.shortcuts.groups.warnings")}
      >
        <ToggleSwitch
          checked={getSetting("altgr_warning_enabled") ?? true}
          onChange={(value) => updateSetting("altgr_warning_enabled", value)}
          isUpdating={isUpdating("altgr_warning_enabled")}
          label={t("settings.shortcuts.altGrWarning.label")}
          description={t("settings.shortcuts.altGrWarning.description")}
          descriptionMode="tooltip"
          grouped={true}
        />
      </SettingsGroup>
    </div>
  );
  return osType === "windows" ? (
    <ShortcutKeeperProvider>{page}</ShortcutKeeperProvider>
  ) : (
    page
  );
};
