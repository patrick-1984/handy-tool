import React from "react";
import { AlertTriangle } from "lucide-react";
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
    <span
      title={message}
      aria-label={message}
      className="flex items-center text-amber-500"
    >
      <AlertTriangle className="h-4 w-4 shrink-0" />
    </span>
  );
};
