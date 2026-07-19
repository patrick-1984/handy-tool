import React, { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import { commands, type ModelUnloadTimeout } from "@/bindings";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { Input } from "../ui/Input";

interface ModelUnloadTimeoutProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const ModelUnloadTimeoutSetting: React.FC<ModelUnloadTimeoutProps> = ({
  descriptionMode = "inline",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { settings, getSetting, updateSetting } = useSettings();

  const timeoutOptions = [
    {
      value: "never" as ModelUnloadTimeout,
      label: t("settings.advanced.modelUnload.options.never"),
    },
    {
      value: "immediately" as ModelUnloadTimeout,
      label: t("settings.advanced.modelUnload.options.immediately"),
    },
    {
      value: "min2" as ModelUnloadTimeout,
      label: t("settings.advanced.modelUnload.options.min2"),
    },
    {
      value: "min5" as ModelUnloadTimeout,
      label: t("settings.advanced.modelUnload.options.min5"),
    },
    {
      value: "min10" as ModelUnloadTimeout,
      label: t("settings.advanced.modelUnload.options.min10"),
    },
    {
      value: "min15" as ModelUnloadTimeout,
      label: t("settings.advanced.modelUnload.options.min15"),
    },
    {
      value: "hour1" as ModelUnloadTimeout,
      label: t("settings.advanced.modelUnload.options.hour1"),
    },
    {
      value: "custom" as ModelUnloadTimeout,
      label: t("settings.advanced.modelUnload.options.custom"),
    },
  ];

  const debugTimeoutOptions = [
    ...timeoutOptions,
    {
      value: "sec5" as ModelUnloadTimeout,
      label: t("settings.advanced.modelUnload.options.sec5"),
    },
  ];

  const handleChange = async (event: React.ChangeEvent<HTMLSelectElement>) => {
    const newTimeout = event.target.value as ModelUnloadTimeout;

    try {
      await commands.setModelUnloadTimeout(newTimeout);
      updateSetting("model_unload_timeout", newTimeout);
    } catch (error) {
      console.error("Failed to update model unload timeout:", error);
    }
  };

  const currentValue = getSetting("model_unload_timeout") ?? "never";
  const customSeconds = getSetting("model_unload_custom_seconds") ?? 300;

  // Local state + commit-on-blur (repo text-input rule). Unit is derived from
  // the stored seconds: largest unit that divides it cleanly.
  const [amount, setAmount] = useState("5");
  const [unit, setUnit] = useState<"s" | "m" | "h">("m");
  useEffect(() => {
    if (customSeconds % 3600 === 0) {
      setUnit("h");
      setAmount(String(customSeconds / 3600));
    } else if (customSeconds % 60 === 0) {
      setUnit("m");
      setAmount(String(customSeconds / 60));
    } else {
      setUnit("s");
      setAmount(String(customSeconds));
    }
  }, [customSeconds]);

  const commitCustom = (nextUnit?: "s" | "m" | "h") => {
    const u = nextUnit ?? unit;
    const n = Math.max(1, Math.floor(Number(amount) || 0));
    const secs = u === "h" ? n * 3600 : u === "m" ? n * 60 : n;
    updateSetting("model_unload_custom_seconds", secs);
  };

  const unitOptions = (["s", "m", "h"] as const).map((u) => ({
    value: u,
    label: t(`settings.advanced.modelUnload.units.${u}`),
  }));

  const options = useMemo(() => {
    return settings?.debug_mode === true ? debugTimeoutOptions : timeoutOptions;
  }, [settings]);

  return (
    <SettingContainer
      title={t("settings.advanced.modelUnload.title")}
      description={t("settings.advanced.modelUnload.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    >
      <div className="flex items-center gap-2">
        <Dropdown
          options={options}
          selectedValue={currentValue}
          onSelect={(value) =>
            handleChange({
              target: { value },
            } as React.ChangeEvent<HTMLSelectElement>)
          }
          disabled={false}
        />
        {currentValue === "custom" && (
          <>
            <Input
              type="number"
              min={1}
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
              onBlur={() => commitCustom()}
              className="w-20"
            />
            <Dropdown
              options={unitOptions}
              selectedValue={unit}
              onSelect={(value) => {
                setUnit(value as "s" | "m" | "h");
                commitCustom(value as "s" | "m" | "h");
              }}
              disabled={false}
            />
          </>
        )}
      </div>
    </SettingContainer>
  );
};
