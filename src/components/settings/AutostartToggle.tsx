import React, { useEffect, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import { commands } from "@/bindings";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface AutostartToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const AutostartToggle: React.FC<AutostartToggleProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const [portable, setPortable] = useState<boolean | null>(null);

    const autostartEnabled = getSetting("autostart_enabled") ?? false;

    useEffect(() => {
      let active = true;
      void commands.isPortableMode().then((value) => {
        if (active) setPortable(value);
      });
      return () => {
        active = false;
      };
    }, []);

    const changeAutostart = async (enabled: boolean): Promise<void> => {
      if (!enabled || portable === false) {
        await updateSetting("autostart_enabled", enabled);
        return;
      }
      if (portable !== true) return;

      const confirmed = await ask(
        t("settings.advanced.autostart.portableConsentMessage"),
        {
          title: t("settings.advanced.autostart.portableConsentTitle"),
          kind: "warning",
        },
      );
      if (!confirmed) {
        await updateSetting("portable_autostart_consent", "declined");
        return;
      }
      await updateSetting("portable_autostart_consent", "granted");
      await updateSetting("autostart_enabled", true);
    };

    return (
      <ToggleSwitch
        checked={autostartEnabled}
        onChange={(enabled) => void changeAutostart(enabled)}
        disabled={portable === null}
        isUpdating={
          isUpdating("autostart_enabled") ||
          isUpdating("portable_autostart_consent")
        }
        label={t("settings.advanced.autostart.label")}
        description={t(
          portable
            ? "settings.advanced.autostart.portableDescription"
            : "settings.advanced.autostart.description",
        )}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);
