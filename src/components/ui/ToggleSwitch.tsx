import React from "react";
import { useTranslation } from "react-i18next";
import { SettingContainer, type SettingIcon } from "./SettingContainer";

interface ToggleSwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  isUpdating?: boolean;
  label: string;
  description: string;
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  icon?: SettingIcon;
  /** A switch that reveals other settings: bold title plus a summary line. */
  lead?: boolean;
  /** No "switch it on to see more" (it cannot be switched on now). */
  noRevealHint?: boolean;
}

export const ToggleSwitch: React.FC<ToggleSwitchProps> = ({
  checked,
  onChange,
  disabled = false,
  isUpdating = false,
  label,
  description,
  descriptionMode = "tooltip",
  grouped = false,
  icon,
  lead,
  noRevealHint = false,
}) => {
  const { t } = useTranslation();
  return (
    <SettingContainer
      title={label}
      description={description}
      descriptionMode={descriptionMode}
      grouped={grouped}
      disabled={disabled}
      icon={icon}
      lead={lead}
      // Off, a switch that reveals settings says so: an empty space below it
      // otherwise reads as "there is nothing more".
      revealHint={
        lead && !checked && !noRevealHint ? t("settings.revealHint") : undefined
      }
    >
      <label
        className={`group inline-flex items-center gap-3 ${disabled || isUpdating ? "cursor-not-allowed" : "cursor-pointer"}`}
      >
        {/* The state in words, before the switch (Windows 11). */}
        <span
          className={`text-[13px] ${disabled ? "text-dis-text" : "text-text-secondary"}`}
          aria-hidden="true"
        >
          {checked ? t("common.on") : t("common.off")}
        </span>
        <input
          type="checkbox"
          value=""
          className="sr-only peer"
          checked={checked}
          disabled={disabled || isUpdating}
          onChange={(e) => onChange(e.target.checked)}
        />
        {/* Off: an outlined track with a grey knob; on: a cyan track with the
            knob at the end. The knob grows a little on hover. */}
        <div
          className={`relative w-10 h-5 shrink-0 rounded-full border transition-colors duration-150 border-text-secondary peer-checked:border-accent peer-checked:bg-accent peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-focus-ring peer-disabled:border-dis-text peer-disabled:peer-checked:bg-dis-text peer-disabled:peer-checked:border-dis-text after:content-[''] after:absolute after:top-1/2 after:-translate-y-1/2 after:start-[3px] after:h-3 after:w-3 after:rounded-full after:bg-text-secondary after:transition-all after:duration-150 peer-checked:after:start-[23px] peer-checked:after:bg-on-accent peer-disabled:after:bg-dis-text peer-disabled:peer-checked:after:bg-dis-bg ${disabled || isUpdating ? "" : "group-hover:after:h-3.5 group-hover:after:w-3.5 group-hover:peer-checked:after:start-[22px] group-hover:after:start-[2px]"}`}
        ></div>
      </label>
      {isUpdating && (
        <div className="absolute inset-0 flex items-center justify-center">
          <div className="w-4 h-4 border-2 border-accent border-t-transparent rounded-full animate-spin"></div>
        </div>
      )}
    </SettingContainer>
  );
};
