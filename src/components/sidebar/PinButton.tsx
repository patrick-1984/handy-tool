import React from "react";
import { useTranslation } from "react-i18next";
import { Pin, PinOff } from "lucide-react";
import { useSettings } from "@/hooks/useSettings";
import { samePinTarget } from "@/lib/sidebarLayout";
import { useTogglePin } from "./useSidebarLayout";

/**
 * Pin the page to the sidebar (or unpin it), next to the "?" in its title.
 * A pin says where it goes; a star would only say "favourite".
 */
export const PinButton: React.FC<{
  section: string;
  historyTab?: string | null;
}> = ({ section, historyTab }) => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const toggle = useTogglePin();
  const pinned = (settings?.sidebar_layout?.shortcuts ?? []).some((s) =>
    samePinTarget(s, section, historyTab),
  );
  const label = pinned ? t("sidebar.pinned.unpin") : t("sidebar.pinned.pin");
  return (
    <button
      type="button"
      onClick={() => toggle(section, historyTab)}
      title={label}
      aria-label={label}
      aria-pressed={pinned}
      className={`inline-flex items-center justify-center h-8 w-8 rounded-md transition-colors cursor-pointer hover:bg-hover ${
        pinned ? "text-accent-text" : "text-text-secondary hover:text-text"
      }`}
    >
      {pinned ? (
        <PinOff width={18} height={18} />
      ) : (
        <Pin width={18} height={18} />
      )}
    </button>
  );
};
