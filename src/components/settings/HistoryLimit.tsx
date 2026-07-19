import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import { Input } from "../ui/Input";
import { SettingContainer } from "../ui/SettingContainer";

const MAX_LIMIT = 9999;

interface HistoryLimitProps {
  descriptionMode?: "tooltip" | "inline";
  grouped?: boolean;
}

export const HistoryLimit: React.FC<HistoryLimitProps> = ({
  descriptionMode = "inline",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const historyLimit = (getSetting("history_limit") as number) ?? 5;

  // Local state + commit on blur/Enter: writing to the store on every
  // keystroke re-renders the settings tree and the input loses focus.
  const [value, setValue] = useState(String(historyLimit));

  useEffect(() => setValue(String(historyLimit)), [historyLimit]);

  const commit = () => {
    const parsed = parseInt(value, 10);
    if (!isNaN(parsed) && parsed >= 0 && parsed <= MAX_LIMIT) {
      if (parsed !== historyLimit) {
        updateSetting("history_limit", parsed);
      }
      setValue(String(parsed));
    } else {
      setValue(String(historyLimit));
    }
  };

  return (
    <SettingContainer
      title={t("settings.debug.historyLimit.title")}
      description={t("settings.debug.historyLimit.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      layout="horizontal"
    >
      <div className="flex items-center space-x-2">
        <Input
          type="number"
          min="0"
          max={MAX_LIMIT}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
          }}
          disabled={isUpdating("history_limit")}
          className="w-24"
        />
        <span className="text-sm text-text">
          {t("settings.debug.historyLimit.entries")}
        </span>
      </div>
    </SettingContainer>
  );
};
