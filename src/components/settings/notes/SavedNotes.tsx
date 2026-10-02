import React, { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { FileText } from "lucide-react";
import type { Note } from "@/bindings";
import { useNavStore } from "@/stores/navStore";
import { useNotesStore } from "@/stores/notesStore";
import { Button } from "../../ui/Button";
import {
  CollapsibleNote,
  CutShortNotice,
  CopyNoteButton,
  DeleteNoteButton,
  NoteMeta,
  SpeakersBadge,
  noteDate,
} from "./NoteParts";

const SavedNote: React.FC<{ note: Note }> = ({ note }) => {
  const { t, i18n } = useTranslation();
  const goToHistoryEntry = useNavStore((state) => state.goToHistoryEntry);

  return (
    <div className="card px-4 py-3 flex flex-col gap-2">
      <div className="flex justify-between items-start gap-2">
        <div className="min-w-0 flex flex-col gap-0.5">
          <p className="flex flex-wrap items-center gap-2 text-[13px]">
            <span className="font-semibold">
              {noteDate(note.timestamp, i18n.language)}
            </span>
            {note.with_speakers && <SpeakersBadge />}
          </p>
          <NoteMeta note={note} />
        </div>
        <div className="flex items-center gap-0.5 shrink-0">
          <CopyNoteButton markdown={note.note_text} />
          <DeleteNoteButton noteId={note.id} />
        </div>
      </div>
      <CutShortNotice note={note} />
      <CollapsibleNote
        markdown={note.note_text}
        footer={
          note.history_id !== null && (
            <Button
              variant="secondary"
              size="sm"
              disabled={!note.source_exists}
              onClick={() => {
                if (note.history_id !== null) goToHistoryEntry(note.history_id);
              }}
            >
              <FileText className="w-3.5 h-3.5" />
              {note.source_exists
                ? t("settings.notes.saved.goToTranscript")
                : t("settings.notes.saved.sourceDeleted")}
            </Button>
          )
        }
      />
    </div>
  );
};

/** Notes › Saved notes: every note, newest first. */
export const SavedNotes: React.FC = () => {
  const { t } = useTranslation();
  const notes = useNotesStore((state) => state.notes);
  const loading = useNotesStore((state) => state.notesLoading);
  const loadNotes = useNotesStore((state) => state.loadNotes);

  // Reload on open, so "source deleted" is current.
  useEffect(() => {
    void loadNotes();
  }, [loadNotes]);

  if (notes.length === 0) {
    return (
      <div className="card px-4 py-8 text-center text-[13px] text-text-secondary">
        {loading
          ? t("settings.notes.saved.loading")
          : t("settings.notes.saved.empty")}
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-3">
      {notes.map((note) => (
        <SavedNote key={note.id} note={note} />
      ))}
    </div>
  );
};
