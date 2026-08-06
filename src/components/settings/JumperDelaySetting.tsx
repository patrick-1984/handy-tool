import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import { buildDelayOptions } from "../../lib/utils/delayOptions";
import type { JumperPasteDelay, JumperSubmitDelay } from "@/bindings";

interface JumperDelaySettingProps {
  /** "paste" edits the pre-paste settle (shared by both transcription flows);
   * "submit" edits the pre-Enter settle (Transcribe & Submit only). */
  kind: "paste" | "submit";
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * Post-jump delay for the Windows Jumper, split into a LOCAL value and a
 * REMOTE value shown side by side. On a real jump the app waits the remote
 * value when the target matches the remote-desktop classifier (see the Jumper
 * page), else the local value; when you're already in the target it waits
 * neither. The paste variant is shared by Transcribe and Transcribe & Submit
 * (both jump + paste); the submit variant applies only to the Enter key.
 * Windows-only — renders nothing elsewhere (the Jumper is Windows-only).
 */
export const JumperDelaySetting: React.FC<JumperDelaySettingProps> = React.memo(
  ({ kind, descriptionMode = "tooltip", grouped = true }) => {
    const { t, i18n } = useTranslation();
    const osType = useOsType();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    if (osType !== "windows") return null;

    const localKey: "jumper_paste_delay" | "jumper_submit_delay" =
      kind === "paste" ? "jumper_paste_delay" : "jumper_submit_delay";
    const remoteKey:
      | "jumper_paste_delay_remote"
      | "jumper_submit_delay_remote" =
      kind === "paste"
        ? "jumper_paste_delay_remote"
        : "jumper_submit_delay_remote";
    const localDefault = "ms250";
    const remoteDefault = kind === "paste" ? "ms1000" : "ms500";

    const localValue = (getSetting(localKey) || localDefault) as
      | JumperPasteDelay
      | JumperSubmitDelay;
    const remoteValue = (getSetting(remoteKey) || remoteDefault) as
      | JumperPasteDelay
      | JumperSubmitDelay;

    // Built per dropdown so each keeps its own off-scale value visible.
    const localOptions = buildDelayOptions(t, i18n.language, localValue);
    const remoteOptions = buildDelayOptions(t, i18n.language, remoteValue);

    const titleKey =
      kind === "paste"
        ? "settings.general.transcribeAndSubmit.jumperPasteDelay.title"
        : "settings.general.transcribeAndSubmit.jumperSubmitDelay.title";
    const descKey =
      kind === "paste"
        ? "settings.general.transcribeAndSubmit.jumperPasteDelay.description"
        : "settings.general.transcribeAndSubmit.jumperSubmitDelay.description";

    return (
      <SettingContainer
        title={t(titleKey)}
        description={t(descKey)}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <div className="flex items-end gap-4">
          <div className="flex flex-col items-start gap-1">
            <span className="text-xs text-mid-gray">
              {t("settings.general.transcribeAndSubmit.jumperDelay.local")}
            </span>
            <Dropdown
              options={localOptions}
              selectedValue={localValue}
              onSelect={(value) =>
                updateSetting(
                  localKey,
                  value as JumperPasteDelay & JumperSubmitDelay,
                )
              }
              disabled={isUpdating(localKey)}
            />
          </div>
          <div className="flex flex-col items-start gap-1">
            <span className="text-xs text-mid-gray">
              {t("settings.general.transcribeAndSubmit.jumperDelay.remote")}
            </span>
            <Dropdown
              options={remoteOptions}
              selectedValue={remoteValue}
              onSelect={(value) =>
                updateSetting(
                  remoteKey,
                  value as JumperPasteDelay & JumperSubmitDelay,
                )
              }
              disabled={isUpdating(remoteKey)}
            />
          </div>
        </div>
      </SettingContainer>
    );
  },
);
