import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import type { AnchorAction } from "@/bindings";

type AnchorActionKey =
  | "anchor_action_output_idle"
  | "anchor_action_output_stop"
  | "anchor_action_submit_idle"
  | "anchor_action_submit_stop";

const COMMAND_KEYS: Record<AnchorActionKey, string> = {
  anchor_action_output_idle: "output_idle",
  anchor_action_output_stop: "output_stop",
  anchor_action_submit_idle: "submit_idle",
  anchor_action_submit_stop: "submit_stop",
};

export { COMMAND_KEYS as ANCHOR_ACTION_COMMAND_KEYS };

const SLOT_KEYS: Record<AnchorActionKey, string> = {
  anchor_action_output_idle: "anchor_action_output_idle_slot",
  anchor_action_output_stop: "anchor_action_output_stop_slot",
  anchor_action_submit_idle: "anchor_action_submit_idle_slot",
  anchor_action_submit_stop: "anchor_action_submit_stop_slot",
};

interface AnchorActionSettingProps {
  settingKey: AnchorActionKey;
  /** "idle" (nothing running) or "stop" (transcription in progress). */
  moment: "idle" | "stop";
  grouped?: boolean;
}

/**
 * One of the four per-flow jump-slot side-actions (Windows-only, like the
 * Jumper itself): what the shortcut additionally does — and to which slot —
 * when pressed. Jump at "stop" means deliver the text into that slot's field.
 */
export const AnchorActionSetting: React.FC<AnchorActionSettingProps> =
  React.memo(({ settingKey, moment, grouped = false }) => {
    const { t } = useTranslation();
    const osType = useOsType();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    if (osType !== "windows") {
      return null;
    }

    const slotKey = SLOT_KEYS[settingKey] as
      | "anchor_action_output_idle_slot"
      | "anchor_action_output_stop_slot"
      | "anchor_action_submit_idle_slot"
      | "anchor_action_submit_stop_slot";

    const selected = (getSetting(settingKey) || "none") as AnchorAction;
    const selectedSlot = String(getSetting(slotKey) ?? 0);
    const options = (["none", "jump", "set", "clear"] as const).map(
      (value) => ({
        value,
        label: t(`settings.general.anchor.actions.options.${value}`),
      }),
    );
    // Hot 2 listed directly under Hot 1, then the static slots 1–9.
    const slotOptions = [0, 10, 1, 2, 3, 4, 5, 6, 7, 8, 9].map((slot) => ({
      value: String(slot),
      label:
        slot === 0
          ? t("settings.jumper.slotNames.hot")
          : slot === 10
            ? t("settings.jumper.slotNames.hot2")
            : t("settings.jumper.slotNames.static", { index: slot }),
    }));

    return (
      <SettingContainer
        title={t(`settings.general.anchor.actions.${moment}.title`)}
        description={t(`settings.general.anchor.actions.${moment}.description`)}
        descriptionMode="tooltip"
        grouped={grouped}
      >
        <div className="flex items-center gap-2">
          {selected !== "none" && (
            <Dropdown
              options={slotOptions}
              selectedValue={selectedSlot}
              onSelect={(value) => updateSetting(slotKey, Number(value))}
              disabled={isUpdating(slotKey)}
            />
          )}
          <Dropdown
            options={options}
            selectedValue={selected}
            onSelect={(value) =>
              updateSetting(settingKey, value as AnchorAction)
            }
            disabled={isUpdating(settingKey)}
          />
        </div>
      </SettingContainer>
    );
  });
