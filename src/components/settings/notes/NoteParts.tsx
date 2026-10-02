import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Check,
  ChevronDown,
  ChevronUp,
  Copy,
  NotebookPen,
  Scissors,
  Trash2,
  UsersRound,
} from "lucide-react";
import { toast } from "sonner";
import type { Note } from "@/bindings";
import { useNotesStore } from "@/stores/notesStore";
import { ICON_BUTTON } from "../../ui/controlClasses";
import { Markdown } from "./markdown";

/**
 * The tinted "Note" card: the accent colour marks everything that is a note
 * (or a note being written), so it reads apart from the transcript above it.
 */
export const NoteCard: React.FC<{
  actions?: React.ReactNode;
  meta?: React.ReactNode;
  children: React.ReactNode;
}> = ({ actions, meta, children }) => {
  const { t } = useTranslation();
  return (
    <div className="rounded-lg border border-accent/30 border-s-[3px] border-s-accent bg-accent-soft/50 ps-3 pe-2 py-2 flex flex-col gap-1.5">
      <div className="flex items-center justify-between gap-2 min-h-7">
        <div className="flex items-center gap-2 min-w-0">
          <span className="flex items-center gap-1 text-xs font-semibold uppercase tracking-[0.06em] text-accent-text shrink-0">
            <NotebookPen className="w-3.5 h-3.5" aria-hidden />
            {t("settings.notes.inline.label")}
          </span>
          {meta}
        </div>
        <div className="flex items-center gap-0.5 shrink-0">{actions}</div>
      </div>
      {children}
    </div>
  );
};

/** When a note was written, as a short date and time. */
export const noteDate = (timestamp: number, locale: string): string => {
  const date = new Date(timestamp * 1000);
  if (isNaN(date.getTime())) return String(timestamp);
  return new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
};

/** "Skill · model · $cost": what wrote the note. */
export const NoteMeta: React.FC<{ note: Note }> = ({ note }) => {
  const { t } = useTranslation();
  const parts = [
    note.skill_name ?? t("settings.notes.defaultSkill"),
    note.model,
    ...(note.cost_usd != null ? [`$${note.cost_usd.toFixed(4)}`] : []),
  ];
  const text = parts.join(" · ");
  return (
    <span className="text-xs text-text-secondary truncate" title={text}>
      {text}
    </span>
  );
};

/** Marks a note made from a speaker-labelled transcript. */
export const SpeakersBadge: React.FC = () => {
  const { t } = useTranslation();
  return (
    <span className="inline-flex items-center gap-1 shrink-0 rounded-full bg-accent-soft px-2 py-0.5 text-xs font-medium text-accent-text">
      <UsersRound className="w-3 h-3" aria-hidden />
      {t("settings.notes.speakersBadge")}
    </span>
  );
};

/** Says a note ends early because the model hit its length limit. */
export const CutShortNotice: React.FC<{ note: Note }> = ({ note }) => {
  const { t } = useTranslation();
  if (!note.truncated) return null;
  return (
    <p className="flex items-center gap-1.5 text-xs text-warn-text">
      <Scissors className="w-3.5 h-3.5 shrink-0" aria-hidden />
      {t("settings.notes.cutShort")}
    </p>
  );
};

/** Copies the raw Markdown and briefly shows a check mark. */
export const CopyNoteButton: React.FC<{ markdown: string }> = ({
  markdown,
}) => {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(markdown);
    } catch (error) {
      console.error("Failed to copy note:", error);
      toast.error(t("settings.notes.copyError"));
      return;
    }
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <button
      type="button"
      onClick={handleCopy}
      className={ICON_BUTTON}
      title={t("settings.notes.copy")}
    >
      {copied ? (
        <Check width={16} height={16} />
      ) : (
        <Copy width={16} height={16} />
      )}
    </button>
  );
};

/** Deletes a note, with a toast when that fails. */
export const DeleteNoteButton: React.FC<{ noteId: number }> = ({ noteId }) => {
  const { t } = useTranslation();
  const deleteNote = useNotesStore((state) => state.deleteNote);

  const handleDelete = async () => {
    if (!(await deleteNote(noteId))) {
      toast.error(t("settings.notes.deleteError"));
    }
  };

  return (
    <button
      type="button"
      onClick={handleDelete}
      className={`${ICON_BUTTON} hover:!bg-err-bg hover:!text-err-text`}
      title={t("settings.notes.delete")}
    >
      <Trash2 width={16} height={16} />
    </button>
  );
};

/** A note's rendered body; selectable so parts can be copied by hand. */
export const NoteBody: React.FC<{ markdown: string }> = ({ markdown }) => (
  <div className="select-text cursor-text break-words min-w-0">
    <Markdown markdown={markdown} />
  </div>
);

/**
 * A note body clamped to a few lines, with Show more when it is longer.
 * `footer` sits on the toggle's row, at the other end.
 */
export const CollapsibleNote: React.FC<{
  markdown: string;
  footer?: React.ReactNode;
}> = ({ markdown, footer }) => {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const [overflows, setOverflows] = useState(false);
  const bodyRef = useRef<HTMLDivElement>(null);

  // Only clamp notes taller than the collapsed height.
  useEffect(() => {
    const body = bodyRef.current;
    if (!body) return;
    const measure = () => {
      if (!expanded) setOverflows(body.scrollHeight > body.clientHeight + 1);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(body);
    return () => observer.disconnect();
  }, [expanded, markdown]);

  return (
    <>
      <div
        ref={bodyRef}
        className={
          expanded
            ? ""
            : `max-h-48 overflow-hidden ${
                overflows
                  ? "[mask-image:linear-gradient(to_bottom,black_70%,transparent)]"
                  : ""
              }`
        }
      >
        <NoteBody markdown={markdown} />
      </div>
      {(overflows || footer) && (
        <div className="flex flex-wrap items-center justify-between gap-2">
          {overflows ? (
            <button
              type="button"
              onClick={() => setExpanded((value) => !value)}
              className="flex items-center gap-1 text-xs text-accent-text hover:underline cursor-pointer"
            >
              {expanded ? (
                <ChevronUp className="w-3.5 h-3.5" aria-hidden />
              ) : (
                <ChevronDown className="w-3.5 h-3.5" aria-hidden />
              )}
              {expanded
                ? t("settings.notes.showLess")
                : t("settings.notes.showMore")}
            </button>
          ) : (
            <span />
          )}
          {footer}
        </div>
      )}
    </>
  );
};
