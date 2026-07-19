import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import { useSettingsStore } from "../../stores/settingsStore";
import { useOsType } from "../../hooks/useOsType";
import { commands } from "@/bindings";

interface JumperSlotOptionsProps {
  /** 0 = hot slot (backed by the legacy anchor settings), 1–4 = static. */
  slot: number;
  grouped?: boolean;
}

/**
 * The two per-slot delivery options: keep the slot after a delivery into it,
 * and return focus to where you were afterwards. The hot slot's pair shows in
 * both flow groups on the General page; each static slot carries its own pair
 * on the Jumper page (turning keep off makes that slot one-shot).
 */
export const JumperSlotOptions: React.FC<JumperSlotOptionsProps> = React.memo(
  ({ slot, grouped = false }) => {
    const { t } = useTranslation();
    const osType = useOsType();
    const { getSetting, updateSetting, isUpdating, settings } = useSettings();
    const refreshSettings = useSettingsStore((s) => s.refreshSettings);

    if (osType !== "windows") {
      return null;
    }

    const isHot = slot === 0;
    const keep = isHot
      ? (getSetting("anchor_keep") ?? false)
      : (settings?.jumper_slot_keep?.[slot - 1] ?? true);
    const returnFocus = isHot
      ? (getSetting("anchor_return_focus") ?? true)
      : (settings?.jumper_slot_return_focus?.[slot - 1] ?? true);

    const setOption = async (option: "keep" | "return_focus", value: boolean) => {
      if (isHot) {
        updateSetting(
          option === "keep" ? "anchor_keep" : "anchor_return_focus",
          value,
        );
        return;
      }
      await commands.changeJumperSlotOption(slot, option, value);
      await refreshSettings();
    };

    const labelScope = isHot ? "hot" : "static";

    return (
      <>
        <ToggleSwitch
          checked={keep}
          onChange={(value) => setOption("keep", value)}
          isUpdating={isHot && isUpdating("anchor_keep")}
          label={t(`settings.jumper.slotOptions.keep.${labelScope}.label`)}
          description={t(
            `settings.jumper.slotOptions.keep.${labelScope}.description`,
          )}
          descriptionMode="tooltip"
          grouped={grouped}
        />
        <ToggleSwitch
          checked={returnFocus}
          onChange={(value) => setOption("return_focus", value)}
          isUpdating={isHot && isUpdating("anchor_return_focus")}
          label={t("settings.jumper.slotOptions.returnFocus.label")}
          description={t("settings.jumper.slotOptions.returnFocus.description")}
          descriptionMode="tooltip"
          grouped={grouped}
        />
      </>
    );
  },
);
