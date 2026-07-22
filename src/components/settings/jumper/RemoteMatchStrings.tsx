import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useSettings } from "../../../hooks/useSettings";
import { Input } from "../../ui/Input";
import { Button } from "../../ui/Button";
import { SettingContainer } from "../../ui/SettingContainer";

interface RemoteMatchStringsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * Editable list of substrings that classify a jump target as a REMOTE desktop
 * session (RDP/Citrix). Matched case-insensitively against the anchor's
 * app/window-class/control-class; a match uses the longer "Remote" jump delays
 * and shows a badge. Mirrors the custom-words chip UX, but allows spaces and
 * punctuation because valid window identities contain them (e.g.
 * "Citrix.DesktopViewer.App"). An empty list simply treats every target as
 * local. No enable toggle — it is always on.
 */
export const RemoteMatchStrings: React.FC<RemoteMatchStringsProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const [newEntry, setNewEntry] = useState("");
    const entries =
      (getSetting("jumper_remote_match_strings") as string[] | undefined) || [];

    const handleAdd = () => {
      const trimmed = newEntry.trim();
      if (!trimmed || trimmed.length > 100) {
        return;
      }
      if (entries.some((e) => e.toLowerCase() === trimmed.toLowerCase())) {
        toast.error(
          t("settings.jumper.remoteMatch.duplicate", { value: trimmed }),
        );
        return;
      }
      updateSetting("jumper_remote_match_strings", [...entries, trimmed]);
      setNewEntry("");
    };

    const handleRemove = (value: string) => {
      updateSetting(
        "jumper_remote_match_strings",
        entries.filter((e) => e !== value),
      );
    };

    const handleKeyPress = (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        handleAdd();
      }
    };

    return (
      <>
        <SettingContainer
          title={t("settings.jumper.remoteMatch.title")}
          description={t("settings.jumper.remoteMatch.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <div className="flex items-center gap-2">
            <Input
              type="text"
              className="max-w-56"
              value={newEntry}
              onChange={(e) => setNewEntry(e.target.value)}
              onKeyDown={handleKeyPress}
              placeholder={t("settings.jumper.remoteMatch.placeholder")}
              variant="compact"
              disabled={isUpdating("jumper_remote_match_strings")}
            />
            <Button
              onClick={handleAdd}
              disabled={
                !newEntry.trim() ||
                newEntry.trim().length > 100 ||
                isUpdating("jumper_remote_match_strings")
              }
              variant="primary"
              size="md"
            >
              {t("settings.jumper.remoteMatch.add")}
            </Button>
          </div>
        </SettingContainer>
        {entries.length > 0 && (
          <div
            className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-mid-gray/20"} flex flex-wrap gap-1`}
          >
            {entries.map((value) => (
              <Button
                key={value}
                onClick={() => handleRemove(value)}
                disabled={isUpdating("jumper_remote_match_strings")}
                variant="secondary"
                size="sm"
                className="inline-flex items-center gap-1 cursor-pointer"
                aria-label={t("settings.jumper.remoteMatch.remove", { value })}
              >
                <span>{value}</span>
                <svg
                  className="w-3 h-3"
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M6 18L18 6M6 6l12 12"
                  />
                </svg>
              </Button>
            ))}
          </div>
        )}
      </>
    );
  },
);
