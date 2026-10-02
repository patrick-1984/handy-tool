import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { Download, Trash2, UsersRound } from "lucide-react";
import { toast } from "sonner";
import { commands, type SpeakerModelStatus } from "@/bindings";
import {
  speakerDownloadPercent,
  useSpeakerModelStatus,
} from "@/hooks/useSpeakerModelStatus";
import { Button } from "../../ui/Button";
import { SettingContainer } from "../../ui/SettingContainer";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { translateNoteError } from "./noteErrors";

/** A thin bar with "Downloading… 42%" (or "Checking the download…"). */
export const SpeakerDownloadProgress: React.FC<{
  status: SpeakerModelStatus;
}> = ({ status }) => {
  const { t } = useTranslation();
  const verifying = status.state === "verifying";
  const percent = verifying ? 100 : speakerDownloadPercent(status);
  return (
    <div className="flex flex-col gap-1.5 w-full">
      <div className="w-full h-1 bg-control-bottom rounded-full overflow-hidden">
        <div
          className="h-full bg-accent rounded-full transition-[width] duration-300"
          style={{ width: `${percent}%` }}
        />
      </div>
      <p className="text-xs text-text-secondary tabular-nums">
        {verifying
          ? t("settings.notes.speakers.verifying")
          : t("settings.notes.speakers.downloading", { percent })}
      </p>
    </div>
  );
};

/**
 * Notes › Settings: the speaker models used by "Make note with speakers"
 * (status, download, delete).
 */
export const SpeakerDetectionSettings: React.FC = () => {
  const { t } = useTranslation();
  const status = useSpeakerModelStatus();
  const [deleting, setDeleting] = useState(false);

  const download = async () => {
    // Progress and the final state arrive as status events.
    const result = await commands.downloadSpeakerModels();
    if (result.status !== "ok") {
      console.error("Failed to download the speaker models:", result.error);
    }
  };

  const remove = async () => {
    setDeleting(true);
    try {
      const result = await commands.deleteSpeakerModels();
      if (result.status !== "ok") {
        toast.error(
          t("settings.notes.speakers.deleteError", {
            error: translateNoteError(result.error, t),
          }),
        );
      }
    } catch (error) {
      toast.error(
        t("settings.notes.speakers.deleteError", { error: String(error) }),
      );
    } finally {
      setDeleting(false);
    }
  };

  const failed = status?.state === "failed";
  let control: React.ReactNode = null;
  switch (status?.state) {
    case undefined:
      break;
    case "ready":
      control = (
        <div className="flex items-center gap-3">
          <span className="text-sm text-ok-text">
            {t("settings.notes.speakers.ready")}
          </span>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void remove()}
            disabled={deleting}
          >
            <Trash2 className="w-3.5 h-3.5" />
            {t("settings.notes.speakers.delete")}
          </Button>
        </div>
      );
      break;
    case "downloading":
    case "verifying":
      control = (
        <div className="w-[240px]">
          <SpeakerDownloadProgress status={status} />
        </div>
      );
      break;
    default:
      control = (
        <div className="flex items-center gap-3">
          <span
            className={`text-sm ${failed ? "text-err-text" : "text-text-secondary"}`}
            title={status?.error ?? undefined}
          >
            {failed
              ? t("settings.notes.speakers.failed")
              : t("settings.notes.speakers.notDownloaded")}
          </span>
          <Button variant="secondary" size="sm" onClick={() => void download()}>
            <Download className="w-3.5 h-3.5" />
            {t("settings.notes.speakers.download")}
          </Button>
        </div>
      );
  }

  return (
    <SettingsGroup
      icon={UsersRound}
      title={t("settings.notes.speakers.title")}
      description={t("settings.notes.speakers.description")}
    >
      <SettingContainer
        title={t("settings.notes.speakers.models.title")}
        description={t("settings.notes.speakers.models.description")}
        descriptionMode="tooltip"
        grouped={true}
      >
        {control}
      </SettingContainer>
    </SettingsGroup>
  );
};
