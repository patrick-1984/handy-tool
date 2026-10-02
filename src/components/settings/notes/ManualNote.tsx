import React from "react";
import { useTranslation } from "react-i18next";
import { Loader2, Settings2, Sparkles } from "lucide-react";
import { useSettings } from "@/hooks/useSettings";
import { useNavStore } from "@/stores/navStore";
import { noteSetupProblem, useNotesStore } from "@/stores/notesStore";
import { Alert } from "../../ui/Alert";
import { Button } from "../../ui/Button";
import { Textarea } from "../../ui/Textarea";
import {
  CopyNoteButton,
  DeleteNoteButton,
  NoteBody,
  NoteCard,
} from "./NoteParts";
import { translateNoteError } from "./noteErrors";

/** Notes › Manual note: any pasted or typed text into a note. */
export const ManualNote: React.FC = () => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const text = useNotesStore((state) => state.manualText);
  const generating = useNotesStore((state) => state.manualGenerating);
  const error = useNotesStore((state) => state.manualError);
  const result = useNotesStore((state) => state.manualResult);
  const setText = useNotesStore((state) => state.setManualText);
  const generate = useNotesStore((state) => state.generateManualNote);
  const setNotesTab = useNavStore((state) => state.setNotesTab);

  const problem = noteSetupProblem(settings);
  const hasText = text.trim() !== "";

  return (
    <div className="flex flex-col gap-4 min-w-0">
      <div className="card px-4 py-3 flex flex-col gap-3">
        <h3 className="text-sm">{t("settings.notes.manual.title")}</h3>
        <Textarea
          value={text}
          onChange={(event) => setText(event.target.value)}
          placeholder={t("settings.notes.manual.placeholder")}
          disabled={generating}
          className="w-full min-h-[180px] select-text"
        />
        <div className="flex flex-wrap items-center gap-3">
          <Button
            onClick={() => void generate()}
            disabled={!hasText || generating || problem !== null}
          >
            {generating ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <Sparkles className="w-4 h-4" />
            )}
            {t("settings.notes.manual.generate")}
          </Button>
          {problem && (
            <>
              <p className="text-[13px] text-text-secondary">
                {translateNoteError(problem, t)}
              </p>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => setNotesTab("settings")}
              >
                <Settings2 className="w-3.5 h-3.5" />
                {t("settings.notes.openSettings")}
              </Button>
            </>
          )}
        </div>
      </div>

      {error && (
        <Alert variant="error">
          <span className="font-medium">
            {t("settings.notes.inline.error")}
          </span>{" "}
          <span className="select-text break-words">
            {translateNoteError(error, t)}
          </span>
        </Alert>
      )}

      {generating && (
        <NoteCard>
          <p className="flex items-center gap-2 text-sm text-text-secondary">
            <Loader2 className="w-4 h-4 animate-spin text-accent shrink-0" />
            {t("settings.notes.inline.generating")}
          </p>
        </NoteCard>
      )}

      {result && !generating && (
        <NoteCard
          meta={
            <span className="text-xs text-text-secondary truncate">
              {t("settings.notes.manual.savedHint")}
            </span>
          }
          actions={
            <>
              <CopyNoteButton markdown={result.note_text} />
              <DeleteNoteButton noteId={result.id} />
            </>
          }
        >
          <NoteBody markdown={result.note_text} />
        </NoteCard>
      )}
    </div>
  );
};
