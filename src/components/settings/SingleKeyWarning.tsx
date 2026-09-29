import React from "react";
import { WarningIcon } from "@/components/ui/WarningIcon";
import { useTranslation } from "react-i18next";
import { isSingleTypingKey } from "@/lib/utils/keyboard";

/**
 * Warns that a shortcut is a single typing key without a modifier (e.g. "f"):
 * registered globally it fires every time that key is typed anywhere.
 */
export const SingleKeyWarning: React.FC<{ binding: string }> = ({
  binding,
}) => {
  const { t } = useTranslation();
  if (!isSingleTypingKey(binding)) return null;

  const message = t("settings.general.shortcut.singleKey");
  return (
    <WarningIcon
      message={message}
      label={t("settings.general.shortcut.badge.risky")}
    />
  );
};
