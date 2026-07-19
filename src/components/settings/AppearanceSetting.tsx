import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import type { Theme } from "@/bindings";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";

interface AppearanceSettingProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

/**
 * T-204 — Light / Dark / System appearance selector.
 *
 * Persists `app_theme`. Resolution + live application happens elsewhere:
 * App.tsx resolves "system" via `matchMedia` (with a change listener so
 * System keeps tracking OS changes) and stamps `data-theme` on this window's
 * <html>, and the same resolved value drives the Sonner toaster. The
 * overlay/floating windows have no settings store of their own, so the Rust
 * side pushes the resolved theme into them directly on change
 * (`apply_theme_to_aux_windows` in lib.rs) and at startup.
 *
 * Not touched: the system tray icon, which follows the OS theme by design
 * regardless of this setting (see tray.rs `get_current_theme`).
 */
export const AppearanceSetting: React.FC<AppearanceSettingProps> = ({
  descriptionMode = "inline",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();

  const options = [
    {
      value: "system" as Theme,
      label: t("settings.advanced.appearance.options.system"),
    },
    {
      value: "light" as Theme,
      label: t("settings.advanced.appearance.options.light"),
    },
    {
      value: "dark" as Theme,
      label: t("settings.advanced.appearance.options.dark"),
    },
  ];

  const currentValue = getSetting("app_theme") ?? "system";

  return (
    <SettingContainer
      title={t("settings.advanced.appearance.title")}
      description={t("settings.advanced.appearance.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    >
      <Dropdown
        options={options}
        selectedValue={currentValue}
        onSelect={(value) => updateSetting("app_theme", value as Theme)}
        disabled={false}
      />
    </SettingContainer>
  );
};
