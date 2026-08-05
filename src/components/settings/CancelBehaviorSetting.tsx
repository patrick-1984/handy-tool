import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import type { CancelBehavior } from "@/bindings";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";

interface CancelBehaviorSettingProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

/**
 * What cancelling does to the take in progress. Governs EVERY cancel entry
 * point (the Escape shortcut, the tray "Cancel" item, the in-app cancel command
 * and `handy --cancel`), so it is deliberately rendered on all platforms — the
 * Escape *shortcut* row is Linux-gated in Debug settings because dynamic
 * hotkey registration is unstable there, but the tray and CLI paths still work.
 */
export const CancelBehaviorSetting: React.FC<CancelBehaviorSettingProps> = ({
  descriptionMode = "inline",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const options = [
    {
      value: "finish_silently" as CancelBehavior,
      label: t("settings.general.cancelBehavior.options.finishSilently"),
    },
    {
      value: "discard_recording" as CancelBehavior,
      label: t("settings.general.cancelBehavior.options.discardRecording"),
    },
  ];

  const currentValue = getSetting("cancel_behavior") ?? "finish_silently";

  return (
    <SettingContainer
      title={t("settings.general.cancelBehavior.title")}
      description={t("settings.general.cancelBehavior.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    >
      <Dropdown
        options={options}
        selectedValue={currentValue}
        onSelect={(value) =>
          updateSetting("cancel_behavior", value as CancelBehavior)
        }
        disabled={isUpdating("cancel_behavior")}
      />
    </SettingContainer>
  );
};
