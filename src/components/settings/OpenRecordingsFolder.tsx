import React from "react";
import { useTranslation } from "react-i18next";
import { SettingContainer } from "../ui/SettingContainer";
import { commands } from "@/bindings";

interface OpenRecordingsFolderProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const OpenRecordingsFolder: React.FC<OpenRecordingsFolderProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();

  const handleOpen = async () => {
    await commands.openRecordingsFolder();
  };

  return (
    <SettingContainer
      title={t("settings.advanced.openRecordingsFolder.label")}
      description={t("settings.advanced.openRecordingsFolder.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    >
      <button
        type="button"
        onClick={handleOpen}
        className="px-3 py-1 text-sm font-semibold bg-mid-gray/10 border border-mid-gray/80 rounded hover:bg-logo-primary/10 hover:border-logo-primary cursor-pointer transition-all duration-150"
      >
        {t("settings.advanced.openRecordingsFolder.button")}
      </button>
    </SettingContainer>
  );
};
