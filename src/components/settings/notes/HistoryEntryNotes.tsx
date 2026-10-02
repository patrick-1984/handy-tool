import React from "react";
import { useTranslation } from "react-i18next";
import {
  Download,
  Loader2,
  NotebookPen,
  RotateCcw,
  Settings2,
  X,
} from "lucide-react";
import type { Note } from "@/bindings";
import { useNavStore } from "@/stores/navStore";
import { useNotesStore, type HistoryNoteJob } from "@/stores/notesStore";
import { useSpeakerModelStatus } from "@/hooks/useSpeakerModelStatus";
import { Button } from "../../ui/Button";
import { ICON_BUTTON } from "../../ui/controlClasses";
import {
  CollapsibleNote,
  CopyNoteButton,
  DeleteNoteButton,
  NoteCard,
  NoteMeta,
  SpeakersBadge,
  noteDate,
} from "./NoteParts";
import { translateNoteError } from "./noteErrors";
import { SpeakerDownloadProgress } from "./SpeakerDetectionSettings";

/** A small text link to Notes › Settings. */
export const NoteSettingsLink: React.FC = () => {
  const { t } = useTranslation();
  const openNotes = useNavStore((state) => state.openNotes);
  return (
    <button
      type="button"
      onClick={() => openNotes("settings")}
      className="px-1.5 text-xs text-accent-text hover:underline cursor-pointer whitespace-nowrap"
    >
      {t("settings.notes.openSettings")}
    </button>
  );
};

const DismissButton: React.FC<{ historyId: number }> = ({ historyId }) => {
  const { t } = useTranslation();
  const dismiss = useNotesStore((state) => state.dismissHistoryJob);
  return (
    <button
      type="button"
      onClick={() => dismiss(historyId)}
      className={ICON_BUTTON}
      title={t("settings.notes.inline.dismiss")}
    >
      <X width={16} height={16} />
    </button>
  );
};

/** A spinner with a status line, while a step of the job runs. */
export const NoteProgressCard: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => (
  <NoteCard>
    <p className="flex items-center gap-2 text-sm text-text-secondary">
      <Loader2 className="w-4 h-4 animate-spin text-accent shrink-0" />
      {children}
    </p>
  </NoteCard>
);

/** "Make a normal note" from the entry's own text. */
const NormalNoteButton: React.FC<{ historyId: number; text: string }> = ({
  historyId,
  text,
}) => {
  const { t } = useTranslation();
  const generate = useNotesStore((state) => state.generateHistoryNote);
  return (
    <Button
      variant="secondary"
      size="sm"
      onClick={() => void generate(historyId, text)}
      disabled={text.trim() === ""}
    >
      <NotebookPen className="w-3.5 h-3.5" />
      {t("settings.notes.speakers.normalNote")}
    </Button>
  );
};

/** The speaker models are missing: download them here, then continue. */
const SpeakerModelsCard: React.FC<{
  historyId: number;
  job: Extract<HistoryNoteJob, { status: "speakerModels" }>;
}> = ({ historyId, job }) => {
  const { t } = useTranslation();
  const status = useSpeakerModelStatus();
  const download = useNotesStore(
    (state) => state.downloadSpeakerModelsForHistory,
  );
  const inProgress =
    status?.state === "downloading" || status?.state === "verifying";

  return (
    <NoteCard
      actions={
        job.downloading ? undefined : <DismissButton historyId={historyId} />
      }
    >
      <p className="text-sm">{t("settings.notes.speakers.needsDownload")}</p>
      <p className="text-xs text-text-secondary">
        {t("settings.notes.speakers.needsDownloadHint")}
      </p>
      {job.downloading && status && inProgress ? (
        <SpeakerDownloadProgress status={status} />
      ) : job.downloading ? (
        <p className="flex items-center gap-2 text-xs text-text-secondary">
          <Loader2 className="w-3.5 h-3.5 animate-spin text-accent" />
          {t("settings.notes.speakers.starting")}
        </p>
      ) : (
        <>
          {job.error && (
            <p className="text-sm text-err-text select-text break-words">
              {t("settings.notes.speakers.downloadError", {
                error: translateNoteError(job.error, t),
              })}
            </p>
          )}
          <div className="flex flex-wrap gap-2">
            <Button
              variant="primary"
              size="sm"
              onClick={() => void download(historyId)}
            >
              <Download className="w-3.5 h-3.5" />
              {t("settings.notes.speakers.download")}
            </Button>
            <NormalNoteButton historyId={historyId} text={job.fallbackText} />
          </div>
        </>
      )}
    </NoteCard>
  );
};

const JobCard: React.FC<{ historyId: number; job: HistoryNoteJob }> = ({
  historyId,
  job,
}) => {
  const { t } = useTranslation();
  const generate = useNotesStore((state) => state.generateHistoryNote);
  const makeWithSpeakers = useNotesStore(
    (state) => state.makeHistoryNoteWithSpeakers,
  );
  const openNotes = useNavStore((state) => state.openNotes);

  if (job.status === "identifying") {
    return (
      <NoteProgressCard>
        {t("settings.notes.speakers.identifying")}
      </NoteProgressCard>
    );
  }

  if (job.status === "speakerModels") {
    return <SpeakerModelsCard historyId={historyId} job={job} />;
  }

  if (job.status === "speakersError") {
    return (
      <NoteCard
        actions={
          <>
            <NoteSettingsLink />
            <DismissButton historyId={historyId} />
          </>
        }
      >
        <p className="text-sm text-err-text select-text break-words">
          <span className="font-medium">
            {t("settings.notes.speakers.error")}
          </span>{" "}
          {translateNoteError(job.error, t)}
        </p>
        <div className="flex flex-wrap gap-2">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void makeWithSpeakers(historyId, job.fallbackText)}
          >
            <RotateCcw className="w-3.5 h-3.5" />
            {t("settings.notes.inline.retry")}
          </Button>
          <NormalNoteButton historyId={historyId} text={job.fallbackText} />
        </div>
      </NoteCard>
    );
  }

  if (job.status === "generating") {
    return (
      <NoteProgressCard>
        {t("settings.notes.inline.generating")}
      </NoteProgressCard>
    );
  }

  const retry = () => void generate(historyId, job.text, job.withSpeakers);
  const retryButton = (
    <Button variant="secondary" size="sm" onClick={retry}>
      <RotateCcw className="w-3.5 h-3.5" />
      {t("settings.notes.inline.retry")}
    </Button>
  );

  if (job.status === "needsSetup") {
    return (
      <NoteCard actions={<DismissButton historyId={historyId} />}>
        <p className="text-sm">{translateNoteError(job.error, t)}</p>
        <div className="flex flex-wrap gap-2">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => openNotes("settings")}
          >
            <Settings2 className="w-3.5 h-3.5" />
            {t("settings.notes.openSettings")}
          </Button>
          {retryButton}
        </div>
      </NoteCard>
    );
  }

  return (
    <NoteCard
      actions={
        <>
          <NoteSettingsLink />
          <DismissButton historyId={historyId} />
        </>
      }
    >
      <p className="text-sm text-err-text select-text break-words">
        <span className="font-medium">{t("settings.notes.inline.error")}</span>{" "}
        {translateNoteError(job.error, t)}
      </p>
      <div>{retryButton}</div>
    </NoteCard>
  );
};

const InlineNote: React.FC<{ note: Note }> = ({ note }) => {
  const { i18n } = useTranslation();
  return (
    <NoteCard
      meta={
        <>
          {note.with_speakers && <SpeakersBadge />}
          <span
            className="text-xs text-text-secondary whitespace-nowrap"
            title={noteDate(note.timestamp, i18n.language)}
          >
            {noteDate(note.timestamp, i18n.language)}
          </span>
          <span className="text-xs text-text-secondary" aria-hidden>
            ·
          </span>
          <NoteMeta note={note} />
        </>
      }
      actions={
        <>
          <CopyNoteButton markdown={note.note_text} />
          <DeleteNoteButton noteId={note.id} />
          <NoteSettingsLink />
        </>
      }
    >
      <CollapsibleNote markdown={note.note_text} />
    </NoteCard>
  );
};

/**
 * The notes of a History entry, under it: the note being written (or what
 * went wrong) first, then the saved notes, newest first.
 */
export const HistoryEntryNotes: React.FC<{ historyId: number }> = ({
  historyId,
}) => {
  const notes = useNotesStore((state) => state.historyNotes[historyId]);
  const job = useNotesStore((state) => state.historyJobs[historyId]);

  if (!job && (!notes || notes.length === 0)) return null;

  return (
    <div className="flex flex-col gap-2">
      {job && <JobCard historyId={historyId} job={job} />}
      {notes?.map((note) => (
        <InlineNote key={note.id} note={note} />
      ))}
    </div>
  );
};
