import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { useSettings } from "../../../hooks/useSettings";

interface DirectTypingForRemoteProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * Type instead of pasting when the delivery target is a remote desktop.
 *
 * A clipboard paste method has to put the transcript on the local clipboard,
 * and RDP/Citrix clipboard redirection then copies it to the REMOTE machine's
 * clipboard — a separate clipboard on a separate OS, whose clipboard history
 * keeps it permanently. Restoring the local clipboard cannot retract any of
 * that. Typing never touches a clipboard on either side.
 *
 * On by default. Turning it off restores the old paste behaviour for remote
 * targets, along with the leak. It also stands down under
 * `Clipboard Handling = Copy to Clipboard`, which is defined by leaving the
 * transcript on the clipboard after delivery — redirection carries it across
 * regardless, so typing would cost its downsides for no privacy gain.
 */
export const DirectTypingForRemote: React.FC<DirectTypingForRemoteProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled = getSetting("direct_typing_for_remote_targets") ?? true;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(value) =>
          updateSetting("direct_typing_for_remote_targets", value)
        }
        isUpdating={isUpdating("direct_typing_for_remote_targets")}
        label={t("settings.jumper.directTypingForRemote.label")}
        description={t("settings.jumper.directTypingForRemote.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  });
