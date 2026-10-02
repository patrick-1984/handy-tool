import React, { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { FileText, Scale } from "lucide-react";
import type { Note } from "@/bindings";
import {
  formatCost,
  formatDuration,
  groupNotesBySource,
  summarizeByModel,
} from "@/lib/noteModels";
import { useNavStore } from "@/stores/navStore";
import { useNotesStore } from "@/stores/notesStore";
import { Button } from "../../ui/Button";
import { Dropdown } from "../../ui/Dropdown";
import {
  CollapsibleNote,
  CutShortNotice,
  CopyNoteButton,
  DeleteNoteButton,
  NoteGroup,
  NoteMeta,
  SpeakersBadge,
  TryAnotherModelButton,
  TryAnotherModelForm,
  noteDate,
} from "./NoteParts";

const SavedNote: React.FC<{ note: Note }> = ({ note }) => {
  const { t, i18n } = useTranslation();
  const goToHistoryEntry = useNavStore((state) => state.goToHistoryEntry);
  const [trying, setTrying] = useState(false);
  const comparing = useNotesStore(
    (state) => state.modelJobs[note.id]?.status === "generating",
  );

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
          <TryAnotherModelButton
            active={trying}
            disabled={comparing}
            onClick={() => setTrying((value) => !value)}
          />
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
      {trying && (
        <TryAnotherModelForm note={note} onClose={() => setTrying(false)} />
      )}
    </div>
  );
};

/**
 * "Compare models": per model, how many notes, their average cost and time,
 * and what they cost in total, with a filter for one model's notes.
 */
const CompareModels: React.FC<{
  notes: Note[];
  filter: string;
  onFilter: (model: string) => void;
}> = ({ notes, filter, onFilter }) => {
  const { t, i18n } = useTranslation();
  const summaries = useMemo(() => summarizeByModel(notes), [notes]);
  const unknown = t("settings.notes.usage.costUnknown");

  return (
    <div className="card px-4 py-3 flex flex-col gap-2">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="flex items-center gap-1.5 text-sm font-semibold">
          <Scale className="w-4 h-4 text-text-secondary" aria-hidden />
          {t("settings.notes.compare.title")}
        </h3>
        <div className="flex items-center gap-2">
          <span className="text-xs text-text-secondary">
            {t("settings.notes.compare.filter")}
          </span>
          <Dropdown
            className="w-[240px]"
            options={[
              { value: "", label: t("settings.notes.compare.allModels") },
              ...summaries.map((s) => ({
                value: s.model,
                label: s.model,
                hint: String(s.notes),
              })),
            ]}
            selectedValue={filter}
            onSelect={onFilter}
          />
        </div>
      </div>
      <div className="overflow-x-auto">
        <table className="w-full text-xs tabular-nums">
          <thead>
            <tr className="text-text-secondary text-start">
              <th className="py-1 pe-3 font-medium text-start">
                {t("settings.notes.compare.model")}
              </th>
              <th className="py-1 px-2 font-medium text-end">
                {t("settings.notes.compare.notes")}
              </th>
              <th className="py-1 px-2 font-medium text-end">
                {t("settings.notes.compare.averageCost")}
              </th>
              <th className="py-1 px-2 font-medium text-end">
                {t("settings.notes.compare.averageTime")}
              </th>
              <th className="py-1 ps-2 font-medium text-end">
                {t("settings.notes.compare.totalCost")}
              </th>
            </tr>
          </thead>
          <tbody>
            {summaries.map((s) => (
              <tr
                key={s.model}
                onClick={() => onFilter(filter === s.model ? "" : s.model)}
                className={`border-t border-border cursor-pointer hover:bg-hover ${
                  filter === s.model ? "bg-active" : ""
                }`}
              >
                <td className="py-1.5 pe-3 text-text break-all">{s.model}</td>
                <td className="py-1.5 px-2 text-end">{s.notes}</td>
                <td className="py-1.5 px-2 text-end whitespace-nowrap">
                  {formatCost(s.averageCost) ?? unknown}
                </td>
                <td className="py-1.5 px-2 text-end whitespace-nowrap">
                  {s.averageMs !== null
                    ? formatDuration(s.averageMs, i18n.language)
                    : "–"}
                </td>
                <td className="py-1.5 ps-2 text-end whitespace-nowrap">
                  {formatCost(s.totalCost) ?? unknown}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="text-xs text-text-secondary">
        {t("settings.notes.compare.hint")}
      </p>
    </div>
  );
};

/**
 * Notes › Saved notes: the model comparison, then every note, newest first;
 * notes made from the same text sit side by side.
 */
export const SavedNotes: React.FC = () => {
  const { t } = useTranslation();
  const notes = useNotesStore((state) => state.notes);
  const loading = useNotesStore((state) => state.notesLoading);
  const loadNotes = useNotesStore((state) => state.loadNotes);
  const [filter, setFilter] = useState("");

  // Reload on open, so "source deleted" is current.
  useEffect(() => {
    void loadNotes();
  }, [loadNotes]);

  // A filtered model whose notes are all gone shows everything again.
  const activeFilter =
    filter !== "" && notes.some((note) => note.model === filter) ? filter : "";
  const groups = useMemo(
    () =>
      groupNotesBySource(
        activeFilter === ""
          ? notes
          : notes.filter((note) => note.model === activeFilter),
      ),
    [notes, activeFilter],
  );

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
      <CompareModels notes={notes} filter={activeFilter} onFilter={setFilter} />
      {groups.map((group) => (
        <NoteGroup
          key={group[0].id}
          notes={group}
          renderNote={(note) => <SavedNote note={note} />}
        />
      ))}
    </div>
  );
};
