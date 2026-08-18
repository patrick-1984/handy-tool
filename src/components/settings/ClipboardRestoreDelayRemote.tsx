import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import { buildDelayOptions } from "../../lib/utils/delayOptions";
import type { ClipboardRestoreDelay } from "@/bindings";

interface ClipboardRestoreDelayRemoteProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/** Sentinel for "no override" — the backend maps an empty string to `None`. */
const INHERIT = "";

/**
 * Optional override of the clipboard restore wait, applied only when the
 * delivery target is a remote-desktop session.
 *
 * Unset by default, and that matters: the underlying setting is an `Option`
 * precisely because `ClipboardRestoreDelay.none` already means *zero
 * milliseconds*, so a plain enum could not express "leave it alone" and every
 * existing install would have been silently reset to no delay.
 *
 * The honest cost of raising it is in the description: a longer restore leaves
 * the transcript on the clipboard longer, widening the window in which
 * clipboard history and managers can capture it.
 */
export const ClipboardRestoreDelayRemoteSetting: React.FC<ClipboardRestoreDelayRemoteProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t, i18n } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const raw = getSetting("clipboard_restore_delay_remote");
    const selected = (raw ?? INHERIT) as string;

    const options = [
      {
        value: INHERIT,
        label: t("settings.advanced.clipboardRestoreDelayRemote.inherit"),
      },
      // `selected` is passed through so a legacy ms2500/ms5000 value still
      // shows its real label rather than collapsing to the placeholder.
      ...buildDelayOptions(t, i18n.language, selected as ClipboardRestoreDelay),
    ];

    return (
      <SettingContainer
        title={t("settings.advanced.clipboardRestoreDelayRemote.title")}
        description={t(
          "settings.advanced.clipboardRestoreDelayRemote.description",
        )}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <Dropdown
          options={options}
          selectedValue={selected}
          onSelect={(value) =>
            updateSetting(
              "clipboard_restore_delay_remote",
              (value === INHERIT
                ? null
                : value) as unknown as ClipboardRestoreDelay,
            )
          }
          disabled={isUpdating("clipboard_restore_delay_remote")}
        />
      </SettingContainer>
    );
  });
