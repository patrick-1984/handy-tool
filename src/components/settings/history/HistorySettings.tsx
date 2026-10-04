import React, {
  useState,
  useEffect,
  useCallback,
  useMemo,
  useRef,
} from "react";
import { useTranslation } from "react-i18next";
import { AudioPlayer } from "../../ui/AudioPlayer";
import { Button } from "../../ui/Button";
import {
  Copy,
  Star,
  Check,
  Trash2,
  FolderOpen,
  Search,
  X,
  HardDrive,
  Clock,
  NotebookPen,
  UsersRound,
} from "lucide-react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { readFile } from "@tauri-apps/plugin-fs";
import { commands, type HistoryEntry } from "@/bindings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { STICKY_TABS, TabBar } from "../../ui/TabBar";
import { TranscriptionCostReport } from "../advanced/TranscriptionCostReport";
import { useNavStore, type HistoryTab } from "@/stores/navStore";
import { CrashResilientRecording } from "../CrashResilientRecording";
import { PreserveTranscriptions } from "../PreserveTranscriptions";
import { HistoryLimit } from "../HistoryLimit";
import { RecordingRetentionPeriodSelector } from "../RecordingRetentionPeriod";
import { ICON_BUTTON } from "../../ui/controlClasses";
import { isHistoryEntryBusy, useNotesStore } from "@/stores/notesStore";
import { HistoryEntryNotes } from "../notes/HistoryEntryNotes";
import { NoteSkillPicker } from "../notes/NoteSkillPicker";
import { OutputLanguagePicker } from "../notes/OutputLanguagePicker";

const pad2 = (n: number) => String(n).padStart(2, "0");
/** A take's length as a clock reads it: "0:18", "12:05", "1:02:07". */
const fmtDuration = (s: number) => {
  // Round once so the components can't disagree across a 60s boundary.
  const t = Math.max(0, Math.round(s));
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  return h > 0 ? `${h}:${pad2(m)}:${pad2(t % 60)}` : `${m}:${pad2(t % 60)}`;
};

/**
 * When a take was recorded: "Today, 14:32" / "Yesterday, 17:48" (the words in
 * the app's language, from the browser), otherwise a short date and time.
 */
const recordedAt = (timestamp: number, locale: string): string => {
  const date = new Date(timestamp * 1000);
  if (isNaN(date.getTime())) return String(timestamp);
  const time = new Intl.DateTimeFormat(locale, {
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
  const startOfDay = (d: Date) =>
    new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const daysAgo = Math.round(
    (startOfDay(new Date()) - startOfDay(date)) / 86_400_000,
  );
  if (daysAgo === 0 || daysAgo === 1) {
    const day = new Intl.RelativeTimeFormat(locale, { numeric: "auto" }).format(
      -daysAgo,
      "day",
    );
    return `${day.charAt(0).toLocaleUpperCase(locale)}${day.slice(1)}, ${time}`;
  }
  return new Intl.DateTimeFormat(locale, {
    year:
      date.getFullYear() === new Date().getFullYear() ? undefined : "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
};

// "<model> · $cost" after the length: which model produced it (local or
// OpenRouter, already labeled), then the real cost when known.
const detailParts = (e: HistoryEntry): string[] => {
  const parts: string[] = [];
  if (e.model_used) parts.push(e.model_used);
  if (e.cost_usd != null) parts.push(`$${e.cost_usd.toFixed(4)}`);
  return parts;
};
import { useOsType } from "@/hooks/useOsType";
import {
  buildMatcher,
  fieldsMatch,
  highlightSegments,
  snippetAroundFirstMatch,
  type SearchMatcher,
} from "@/utils/historySearch";

interface OpenRecordingsButtonProps {
  onClick: () => void;
  label: string;
}

const OpenRecordingsButton: React.FC<OpenRecordingsButtonProps> = ({
  onClick,
  label,
}) => (
  <Button
    onClick={onClick}
    variant="secondary"
    size="sm"
    className="flex items-center gap-2"
    title={label}
  >
    <FolderOpen className="w-4 h-4" />
    <span>{label}</span>
  </Button>
);

/** Entries drawn at a time on the Recordings tab. */
const HISTORY_PAGE = 20;

/**
 * Scroll a History entry to the middle of the view, and keep it there while
 * the list above it still changes height (notes, "Show more" rows and players
 * drawn after the first layout). Stops at the user's first scroll, click or
 * key, once the list has not changed for a moment, or after a few seconds.
 * Returns a function that stops it early.
 */
const keepEntryCentered = (
  id: number,
  list: HTMLElement | null,
): (() => void) => {
  const center = (behavior: ScrollBehavior) =>
    document
      .getElementById(`history-entry-${id}`)
      ?.scrollIntoView({ behavior, block: "center" });
  center("smooth");
  if (!list) return () => {};

  const userEvents = ["wheel", "touchstart", "pointerdown", "keydown"];
  let quiet = 0;
  const stop = () => {
    observer.disconnect();
    window.clearTimeout(quiet);
    window.clearTimeout(limit);
    userEvents.forEach((type) => window.removeEventListener(type, stop, true));
  };
  const settle = () => {
    window.clearTimeout(quiet);
    quiet = window.setTimeout(stop, 600);
  };
  let first = true;
  const observer = new ResizeObserver(() => {
    // The first call only reports the size the list already has.
    if (first) {
      first = false;
    } else {
      center("auto");
    }
    settle();
  });
  observer.observe(list);
  const limit = window.setTimeout(stop, 4000);
  userEvents.forEach((type) => window.addEventListener(type, stop, true));
  return stop;
};

export const HistorySettings: React.FC = () => {
  const { t } = useTranslation();
  const tab = useNavStore((state) => state.historyTab);
  const setTab = useNavStore((state) => state.setHistoryTab);
  const osType = useOsType();
  const [historyEntries, setHistoryEntries] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [searchInput, setSearchInput] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");

  useEffect(() => {
    const handle = setTimeout(() => setDebouncedSearch(searchInput), 200);
    return () => clearTimeout(handle);
  }, [searchInput]);

  const matcher = useMemo(
    () => buildMatcher(debouncedSearch),
    [debouncedSearch],
  );

  const filteredEntries = useMemo(() => {
    if (!matcher) return historyEntries;
    return historyEntries.filter((entry) =>
      fieldsMatch(matcher, [
        entry.title,
        entry.transcription_text,
        entry.post_processed_text,
      ]),
    );
  }, [historyEntries, matcher]);

  // Drawing every entry at once (each with its own audio player) made the page
  // lag; draw a page at a time and add the next as the list nears its end.
  const [shownCount, setShownCount] = useState(HISTORY_PAGE);
  useEffect(() => {
    // Not while "Go to transcript" is drawing the list down to its entry.
    if (useNavStore.getState().focusHistoryId === null) {
      setShownCount(HISTORY_PAGE);
    }
  }, [matcher]);
  const moreRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = moreRef.current;
    if (!el) return;
    const observer = new IntersectionObserver(
      (seen) => {
        if (seen.some((e) => e.isIntersecting)) {
          setShownCount((n) => n + HISTORY_PAGE);
        }
      },
      { rootMargin: "400px" },
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, [filteredEntries, shownCount, tab]);

  const shownEntries = useMemo(
    () => filteredEntries.slice(0, shownCount),
    [filteredEntries, shownCount],
  );

  // Each shown entry's notes, under it.
  const loadNotesForHistory = useNotesStore(
    (state) => state.loadNotesForHistory,
  );
  useEffect(() => {
    if (tab !== "recordings" || shownEntries.length === 0) return;
    void loadNotesForHistory(shownEntries.map((entry) => entry.id));
  }, [tab, shownEntries, loadNotesForHistory]);

  // "Go to transcript" (Notes › Saved notes): clear the search, draw the list
  // down to the entry, then scroll to it and mark it as the note's source.
  const focusHistoryId = useNavStore((state) => state.focusHistoryId);
  const clearFocusHistoryId = useNavStore((state) => state.clearFocusHistoryId);
  const [sourceId, setSourceId] = useState<number | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const stopCenteringRef = useRef<(() => void) | null>(null);
  useEffect(() => () => stopCenteringRef.current?.(), []);
  useEffect(() => {
    if (focusHistoryId === null || loading) return;
    if (searchInput !== "" || matcher !== null) {
      // The list redraws unfiltered first; this runs again after.
      setSearchInput("");
      setDebouncedSearch("");
      return;
    }
    const index = historyEntries.findIndex((e) => e.id === focusHistoryId);
    if (index === -1) {
      clearFocusHistoryId();
      return;
    }
    if (index >= shownCount) {
      setShownCount(index + 1);
      return;
    }
    setSourceId(focusHistoryId);
    clearFocusHistoryId();
    // The notes of the entries drawn above it come in afterwards and push it
    // down: scroll once they are in, and keep it centered while the list
    // still settles.
    const target = focusHistoryId;
    void loadNotesForHistory(shownEntries.map((entry) => entry.id)).then(() =>
      requestAnimationFrame(() => {
        stopCenteringRef.current?.();
        stopCenteringRef.current = keepEntryCentered(target, listRef.current);
      }),
    );
  }, [
    focusHistoryId,
    loading,
    searchInput,
    matcher,
    historyEntries,
    shownCount,
    shownEntries,
    loadNotesForHistory,
    clearFocusHistoryId,
  ]);

  const loadHistoryEntries = useCallback(async () => {
    try {
      const result = await commands.getHistoryEntries();
      if (result.status === "ok") {
        setHistoryEntries(result.data);
        setLoadError(null);
      } else {
        // A silent empty list hides real failures — show them.
        setLoadError(String(result.error));
      }
    } catch (error) {
      console.error("Failed to load history entries:", error);
      setLoadError(String(error));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadHistoryEntries();

    // Listen for history update events
    const setupListener = async () => {
      const unlisten = await listen("history-updated", () => {
        console.log("History updated, reloading entries...");
        loadHistoryEntries();
      });

      // Return cleanup function
      return unlisten;
    };

    let unlistenPromise = setupListener();

    return () => {
      unlistenPromise.then((unlisten) => {
        if (unlisten) {
          unlisten();
        }
      });
    };
  }, [loadHistoryEntries]);

  const toggleSaved = async (id: number) => {
    try {
      await commands.toggleHistoryEntrySaved(id);
      // No need to reload here - the event listener will handle it
    } catch (error) {
      console.error("Failed to toggle saved status:", error);
    }
  };

  const copyToClipboard = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
    } catch (error) {
      console.error("Failed to copy to clipboard:", error);
    }
  };

  const getAudioUrl = useCallback(
    async (fileName: string) => {
      try {
        const result = await commands.getAudioFilePath(fileName);
        if (result.status === "ok") {
          if (osType === "linux") {
            const fileData = await readFile(result.data);
            const ext = fileName.split(".").pop()?.toLowerCase();
            const mimeType =
              ext === "opus" || ext === "ogg" ? "audio/ogg" : "audio/wav";
            const blob = new Blob([fileData], { type: mimeType });

            return URL.createObjectURL(blob);
          }

          return convertFileSrc(result.data, "asset");
        }
        return null;
      } catch (error) {
        console.error("Failed to get audio file path:", error);
        return null;
      }
    },
    [osType],
  );

  const deleteAudioEntry = async (id: number) => {
    try {
      await commands.deleteHistoryEntry(id);
    } catch (error) {
      console.error("Failed to delete audio entry:", error);
      throw error;
    }
  };

  const openRecordingsFolder = async () => {
    try {
      await commands.openRecordingsFolder();
    } catch (error) {
      console.error("Failed to open recordings folder:", error);
    }
  };

  const showSearchBar = !loading && historyEntries.length > 0;

  let body: React.ReactNode;
  if (loading) {
    body = (
      <div className="card px-4 py-8 text-center text-[13px] text-text-secondary">
        {t("settings.history.loading")}
      </div>
    );
  } else if (loadError) {
    body = (
      <div className="card px-4 py-8 text-center text-[13px] text-err-text">
        {t("settings.history.loadError", { error: loadError })}
      </div>
    );
  } else if (historyEntries.length === 0) {
    body = (
      <div className="card px-4 py-8 text-center text-[13px] text-text-secondary">
        {t("settings.history.empty")}
      </div>
    );
  } else if (filteredEntries.length === 0) {
    body = (
      <div className="card px-4 py-8 text-center text-[13px] text-text-secondary">
        {t("settings.history.search.noMatches")}
      </div>
    );
  } else {
    body = (
      <div ref={listRef} className="flex flex-col gap-3">
        {shownEntries.map((entry) => (
          <HistoryEntryComponent
            key={entry.id}
            entry={entry}
            matcher={matcher}
            isNoteSource={entry.id === sourceId}
            onToggleSaved={() => toggleSaved(entry.id)}
            onCopyText={() => copyToClipboard(entry.transcription_text)}
            getAudioUrl={getAudioUrl}
            deleteAudio={deleteAudioEntry}
          />
        ))}
        {shownCount < filteredEntries.length && (
          <div ref={moreRef} className="h-px" aria-hidden />
        )}
      </div>
    );
  }

  return (
    <div className="w-full space-y-4">
      <div className={STICKY_TABS}>
        <TabBar<HistoryTab>
          tabs={[
            {
              id: "recordings",
              label: t("settings.history.tabs.recordings"),
            },
            {
              id: "statistics",
              label: t("settings.history.tabs.statistics"),
            },
            {
              id: "settings",
              label: t("settings.history.tabs.settings"),
            },
          ]}
          active={tab}
          onSelect={setTab}
        />
      </div>
      {tab === "recordings" && (
        <div className="space-y-2">
          <div className="flex flex-wrap items-center justify-end gap-3">
            <NoteSkillPicker />
            <OutputLanguagePicker />
            <OpenRecordingsButton
              onClick={openRecordingsFolder}
              label={t("settings.history.openFolder")}
            />
          </div>
          {showSearchBar && (
            <div className="flex items-center gap-2">
              <div className="relative flex-1 max-w-md">
                <Search className="absolute start-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-text-secondary pointer-events-none" />
                <input
                  type="text"
                  value={searchInput}
                  onChange={(e) => setSearchInput(e.target.value)}
                  placeholder={t("settings.history.search.placeholder")}
                  className="w-full h-8 rounded-md border border-control-border border-b-control-bottom bg-control ps-8 pe-8 text-sm placeholder:text-text-secondary hover:bg-control-hover focus:outline-none focus:bg-control focus:shadow-[inset_0_-2px_0_var(--color-accent)]"
                />
                {searchInput && (
                  <button
                    onClick={() => setSearchInput("")}
                    className="absolute end-2 top-1/2 -translate-y-1/2 text-text-secondary hover:text-text cursor-pointer"
                    title={t("settings.history.search.clear")}
                  >
                    <X className="w-4 h-4" />
                  </button>
                )}
              </div>
              {matcher && (
                <span className="text-xs text-text-secondary whitespace-nowrap">
                  {t("settings.history.search.matches", {
                    matched: filteredEntries.length,
                    total: historyEntries.length,
                  })}
                  {matcher.isRegex && (
                    <span
                      className="ms-1.5 px-1 py-0.5 rounded-sm bg-accent-soft text-accent-text font-mono"
                      title={t("settings.history.search.regexActive")}
                    >
                      {t("settings.history.search.regexBadge")}
                    </span>
                  )}
                </span>
              )}
            </div>
          )}
          {body}
        </div>
      )}
      {/* The same figures as Providers' cost report, without the money. */}
      {tab === "statistics" && <TranscriptionCostReport variant="stats" />}
      {/* What History keeps and for how long (formerly Advanced › History). */}
      {tab === "settings" && (
        <SettingsGroup
          icon={HardDrive}
          title={t("settings.advanced.groups.history")}
        >
          <CrashResilientRecording descriptionMode="tooltip" grouped={true} />
          <PreserveTranscriptions descriptionMode="tooltip" grouped={true} />
          <HistoryLimit descriptionMode="tooltip" grouped={true} />
          <RecordingRetentionPeriodSelector
            descriptionMode="tooltip"
            grouped={true}
          />
        </SettingsGroup>
      )}
    </div>
  );
};

interface HistoryEntryProps {
  entry: HistoryEntry;
  matcher: SearchMatcher | null;
  /** Reached through "Go to transcript": outlined and labelled. */
  isNoteSource: boolean;
  onToggleSaved: () => void;
  onCopyText: () => void;
  getAudioUrl: (fileName: string) => Promise<string | null>;
  deleteAudio: (id: number) => Promise<void>;
}

const HighlightedText: React.FC<{
  text: string;
  matcher: SearchMatcher | null;
}> = ({ text, matcher }) => {
  if (!matcher) return <>{text}</>;
  const snippet = snippetAroundFirstMatch(text, matcher);
  const segments = highlightSegments(snippet.text, matcher);
  return (
    <>
      {snippet.leadingEllipsis && <>&hellip;</>}
      {segments.map((segment, i) =>
        segment.isMatch ? (
          <mark
            key={i}
            className="bg-accent-soft text-inherit rounded-sm px-0.5"
          >
            {segment.text}
          </mark>
        ) : (
          <React.Fragment key={i}>{segment.text}</React.Fragment>
        ),
      )}
      {snippet.trailingEllipsis && <>&hellip;</>}
    </>
  );
};

const HistoryEntryComponent: React.FC<HistoryEntryProps> = ({
  entry,
  matcher,
  isNoteSource,
  onToggleSaved,
  onCopyText,
  getAudioUrl,
  deleteAudio,
}) => {
  const { t, i18n } = useTranslation();
  const [showCopied, setShowCopied] = useState(false);
  const noteBusy = useNotesStore((state) =>
    isHistoryEntryBusy(state, entry.id),
  );
  const generateNote = useNotesStore((state) => state.generateHistoryNote);
  const makeNoteWithSpeakers = useNotesStore(
    (state) => state.makeHistoryNoteWithSpeakers,
  );
  // Speaker detection listens to the recording, so it needs one.
  const hasRecording = entry.file_name !== "" && entry.audio_purged_at == null;

  const handleLoadAudio = useCallback(
    () => getAudioUrl(entry.file_name),
    [getAudioUrl, entry.file_name],
  );

  const handleCopyText = () => {
    onCopyText();
    setShowCopied(true);
    setTimeout(() => setShowCopied(false), 2000);
  };

  const handleDeleteEntry = async () => {
    try {
      await deleteAudio(entry.id);
    } catch (error) {
      console.error("Failed to delete entry:", error);
      alert("Failed to delete entry. Please try again.");
    }
  };

  const formattedDate = recordedAt(Number(entry.timestamp), i18n.language);

  return (
    <div
      id={`history-entry-${entry.id}`}
      className={`card px-4 py-3 flex flex-col gap-2.5 ${
        isNoteSource ? "outline-2 outline-accent -outline-offset-1" : ""
      }`}
    >
      {isNoteSource && (
        <p className="flex items-center gap-1 text-xs font-semibold uppercase tracking-[0.06em] text-accent-text">
          <NotebookPen className="w-3.5 h-3.5" aria-hidden />
          {t("settings.notes.inline.sourceLabel")}
        </p>
      )}
      <div className="flex justify-between items-center">
        <p className="flex flex-wrap items-center gap-x-1.5 text-[13px] text-text-secondary">
          <span className="font-semibold text-text">{formattedDate}</span>
          {entry.duration_seconds != null && (
            <>
              <span aria-hidden>·</span>
              <Clock className="w-3 h-3" aria-hidden />
              <span className="tabular-nums">
                {fmtDuration(entry.duration_seconds)}
              </span>
            </>
          )}
          {detailParts(entry).map((part) => (
            <React.Fragment key={part}>
              <span aria-hidden>·</span>
              <span>{part}</span>
            </React.Fragment>
          ))}
        </p>
        <div className="flex items-center gap-1">
          <button
            onClick={handleCopyText}
            className={ICON_BUTTON}
            title={t("settings.history.copyToClipboard")}
          >
            {showCopied ? (
              <Check width={16} height={16} />
            ) : (
              <Copy width={16} height={16} />
            )}
          </button>
          {/* The note is made from the text shown above (transcription_text). */}
          <button
            onClick={() =>
              void generateNote(entry.id, entry.transcription_text)
            }
            disabled={noteBusy || entry.transcription_text.trim() === ""}
            className={ICON_BUTTON}
            title={t("settings.notes.inline.makeNote")}
          >
            <NotebookPen width={16} height={16} />
          </button>
          {hasRecording && (
            <button
              onClick={() =>
                void makeNoteWithSpeakers(entry.id, entry.transcription_text)
              }
              disabled={noteBusy}
              className={ICON_BUTTON}
              title={t("settings.notes.inline.makeNoteWithSpeakers")}
            >
              <UsersRound width={16} height={16} />
            </button>
          )}
          <button
            onClick={onToggleSaved}
            className={`${ICON_BUTTON} ${entry.saved ? "!text-accent" : ""}`}
            title={
              entry.saved
                ? t("settings.history.unsave")
                : t("settings.history.save")
            }
          >
            <Star
              width={16}
              height={16}
              fill={entry.saved ? "currentColor" : "none"}
            />
          </button>
          <button
            onClick={handleDeleteEntry}
            disabled={noteBusy}
            className={`${ICON_BUTTON} enabled:hover:!bg-err-bg enabled:hover:!text-err-text`}
            title={t("settings.history.delete")}
          >
            <Trash2 width={16} height={16} />
          </button>
        </div>
      </div>
      <p className="text-sm select-text cursor-text">
        <HighlightedText text={entry.transcription_text} matcher={matcher} />
      </p>
      <AudioPlayer onLoadRequest={handleLoadAudio} className="w-full" />
      <HistoryEntryNotes historyId={entry.id} />
    </div>
  );
};
