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
  Captions,
  Loader2,
  Users,
  Pencil,
  Tag,
  Combine,
  CircleHelp,
} from "lucide-react";
import { toast } from "sonner";
import { convertFileSrc } from "@tauri-apps/api/core";
import { toSingleOggStream } from "@/lib/utils/oggChain";
import { listen } from "@tauri-apps/api/event";
import { readFile } from "@tauri-apps/plugin-fs";
import { commands, type HistoryEntry } from "@/bindings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { STICKY_TABS, TabBar } from "../../ui/TabBar";
import { TranscriptionCostReport } from "../advanced/TranscriptionCostReport";
import { useNavStore, type HistoryTab } from "@/stores/navStore";
import { CrashResilientRecording } from "../CrashResilientRecording";
import { PreserveTranscriptions } from "../PreserveTranscriptions";
import { RecordingTextFile } from "../RecordingTextFile";
import { HistoryLimit } from "../HistoryLimit";
import { RecordingRetentionPeriodSelector } from "../RecordingRetentionPeriod";
import { HistoryRowMenu, type RowMenuItem } from "./HistoryRowMenu";
import { HistoryFiltersBar } from "./HistoryFiltersBar";
import { CombineDialog } from "./CombineDialog";
import {
  NO_FILTER,
  isNarrowed,
  matchesFilter,
  type HistoryFilter,
} from "./historyFilters";

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
import {
  buildMatcher,
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

/** Copy, star and "⋯" on a recording: icon buttons without a frame. */
const ICON_BUTTON =
  "inline-flex items-center justify-center h-7 w-7 rounded-md text-text-secondary hover:bg-hover hover:text-text transition-colors cursor-pointer";

/** Entries drawn at a time on the Recordings tab. */
const HISTORY_PAGE = 20;

/** "Transcribe again with speakers" refused before sending: the message and
 *  whether it is fixed in Advanced settings › STT server. */
const speakersErrorMessage = (
  error: string,
  t: (key: string, o?: Record<string, unknown>) => string,
): { message: string; toSettings: boolean } => {
  const [code, detail = "", extra = ""] = error.split("|");
  switch (code) {
    case "STT_SERVER_NOT_SET":
      return {
        message: t("settings.history.speakers.errors.notSet"),
        toSettings: true,
      };
    case "STT_SPEAKERS_MODEL_NOT_SET":
      return {
        message: t("settings.history.speakers.errors.modelNotSet"),
        toSettings: true,
      };
    case "STT_SERVER_UNREACHABLE":
      return {
        message: t("settings.history.speakers.errors.unreachable", {
          error: detail,
        }),
        toSettings: true,
      };
    case "STT_SPEAKERS_MODEL_MISSING":
      return {
        message: t("settings.history.speakers.errors.modelMissing", {
          model: detail,
          available: extra || "—",
        }),
        toSettings: true,
      };
    default:
      return {
        message: t("settings.history.transcribeAgainFailed", { error }),
        toSettings: false,
      };
  }
};

export const HistorySettings: React.FC = () => {
  const { t } = useTranslation();
  const tab = useNavStore((state) => state.historyTab);
  const setTab = useNavStore((state) => state.setHistoryTab);
  const startSetup = useNavStore((state) => state.startSetup);
  const [historyEntries, setHistoryEntries] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [searchInput, setSearchInput] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");
  const [filter, setFilter] = useState<HistoryFilter>(NO_FILTER);
  // Recordings picked for Combine, in the order they were clicked.
  const [selected, setSelected] = useState<number[]>([]);
  const [combineOpen, setCombineOpen] = useState(false);
  // The last recording removed from the selection closes Combine for good
  // (selecting again must not reopen it by itself).
  useEffect(() => {
    if (selected.length === 0) setCombineOpen(false);
  }, [selected.length]);
  const onList = tab === "recordings" || tab === "favorites";

  useEffect(() => {
    const handle = setTimeout(() => setDebouncedSearch(searchInput), 200);
    return () => clearTimeout(handle);
  }, [searchInput]);

  const matcher = useMemo(
    () => buildMatcher(debouncedSearch),
    [debouncedSearch],
  );

  const filteredEntries = useMemo(() => {
    const now = Date.now() / 1000;
    const f: HistoryFilter = {
      ...filter,
      matcher,
      favoritesOnly: tab === "favorites" || filter.favoritesOnly,
    };
    return historyEntries.filter((entry) => matchesFilter(entry, f, now));
  }, [historyEntries, matcher, filter, tab]);

  // A recording deleted meanwhile leaves the selection.
  useEffect(() => {
    setSelected((ids) =>
      ids.filter((id) => historyEntries.some((e) => e.id === id)),
    );
  }, [historyEntries]);

  const toggleSelect = (id: number) =>
    setSelected((ids) =>
      ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id],
    );

  // Drawing every entry at once (each with its own audio player) made the page
  // lag; draw a page at a time and add the next as the list nears its end.
  const [shownCount, setShownCount] = useState(HISTORY_PAGE);
  useEffect(() => setShownCount(HISTORY_PAGE), [matcher, filter, tab]);
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
    const unlistenPromise = listen("history-updated", () => {
      loadHistoryEntries();
    });
    return () => {
      unlistenPromise.then((unlisten) => unlisten());
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

  const getAudioUrl = useCallback(async (fileName: string) => {
    try {
      const result = await commands.getAudioFilePath(fileName);
      if (result.status === "ok") {
        // Play from memory (a blob URL) on every platform: WebView2 streams
        // the asset protocol as a barely seekable resource, so the slider
        // could not move to an arbitrary point of a minutes-long take.
        try {
          const fileData = await readFile(result.data);
          const ext = fileName.split(".").pop()?.toLowerCase();
          const isOgg = ext === "opus" || ext === "ogg";
          const mimeType = isOgg ? "audio/ogg" : "audio/wav";
          // A take longer than 10 minutes is glued from chunks as a chained
          // Ogg file, which the web view plays only up to the end of the
          // first chunk: hand it one continuous stream instead.
          const audio = isOgg ? toSingleOggStream(fileData) : fileData;
          const blob = new Blob([audio], { type: mimeType });

          return URL.createObjectURL(blob);
        } catch (error) {
          console.error("Failed to read audio file, streaming it:", error);
        }

        return convertFileSrc(result.data, "asset");
      }
      return null;
    } catch (error) {
      console.error("Failed to get audio file path:", error);
      return null;
    }
  }, []);

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
  const narrowed = matcher !== null || isNarrowed(filter);

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
        {tab === "favorites" && !narrowed
          ? t("settings.history.favorites.empty")
          : t("settings.history.search.noMatches")}
      </div>
    );
  } else {
    body = (
      <div className="flex flex-col gap-3">
        {filteredEntries.slice(0, shownCount).map((entry) => (
          <HistoryEntryComponent
            key={entry.id}
            entry={entry}
            matcher={matcher}
            selectedIndex={selected.indexOf(entry.id)}
            onToggleSelect={() => toggleSelect(entry.id)}
            onToggleSaved={() => toggleSaved(entry.id)}
            onCopyText={(text) => copyToClipboard(text)}
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
              id: "favorites",
              label: t("settings.history.tabs.favorites"),
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
      {onList && (
        <div className="space-y-2">
          <div className="flex items-center justify-between gap-2">
            <button
              type="button"
              onClick={() => startSetup("history")}
              className="inline-flex items-center gap-1.5 h-8 px-2 rounded-md text-[13px] text-accent-text hover:bg-hover cursor-pointer"
            >
              <CircleHelp className="w-3.5 h-3.5" aria-hidden />
              {t("settings.history.showMeHow")}
            </button>
            <OpenRecordingsButton
              onClick={openRecordingsFolder}
              label={t("settings.history.openFolder")}
            />
          </div>
          {showSearchBar && (
            <div className="flex flex-wrap items-center gap-2">
              <div className="relative flex-1 min-w-[220px] max-w-md">
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
              <HistoryFiltersBar
                filter={filter}
                onChange={setFilter}
                showFavorites={tab === "recordings"}
              />
              {narrowed && (
                <span className="text-xs text-text-secondary whitespace-nowrap">
                  {t("settings.history.search.matches", {
                    matched: filteredEntries.length,
                    total: historyEntries.length,
                  })}
                  {matcher?.isRegex && (
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
          {selected.length > 0 && (
            <div className="sticky top-12 z-20 flex items-center justify-between gap-2 rounded-lg border border-accent/40 bg-accent-soft px-3 py-2">
              <span className="text-sm text-accent-text">
                {t("settings.history.combine.selected", {
                  count: selected.length,
                })}
              </span>
              <span className="flex items-center gap-2">
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => setSelected([])}
                >
                  {t("settings.history.combine.clear")}
                </Button>
                <Button
                  variant="primary"
                  size="sm"
                  onClick={() => setCombineOpen(true)}
                >
                  <Combine className="w-3.5 h-3.5" />
                  {t("settings.history.combine.open")}
                </Button>
              </span>
            </div>
          )}
          {body}
        </div>
      )}
      {combineOpen && selected.length > 0 && (
        <CombineDialog
          ids={selected}
          entries={historyEntries}
          onRemove={(id) => setSelected((ids) => ids.filter((x) => x !== id))}
          onClose={() => setCombineOpen(false)}
        />
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
          <RecordingTextFile descriptionMode="tooltip" grouped={true} />
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
  /** Place in the Combine queue (0-based), or -1 when not selected. */
  selectedIndex: number;
  onToggleSelect: () => void;
  onToggleSaved: () => void;
  /** Copies the text the row shows (plain, or with speakers). */
  onCopyText: (text: string) => void;
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

/** A recording's name: shown after its details, edited in place. */
const NameField: React.FC<{
  entry: HistoryEntry;
  editing: boolean;
  setEditing: (v: boolean) => void;
  matcher: SearchMatcher | null;
}> = ({ entry, editing, setEditing, matcher }) => {
  const { t } = useTranslation();
  const [value, setValue] = useState(entry.name ?? "");
  useEffect(() => {
    if (editing) setValue(entry.name ?? "");
  }, [editing, entry.name]);
  const save = async () => {
    const result = await commands.renameHistoryEntry(entry.id, value);
    if (result.status === "error") {
      toast.error(t("settings.history.name.failed", { error: result.error }));
      return;
    }
    setEditing(false);
  };
  if (editing) {
    return (
      <span className="inline-flex items-center gap-1">
        <input
          autoFocus
          type="text"
          value={value}
          // 120 characters as the backend counts them (code points, so an
          // emoji is one, not two UTF-16 units).
          onChange={(e) =>
            setValue(Array.from(e.target.value).slice(0, 120).join(""))
          }
          onKeyDown={(e) => {
            if (e.key === "Enter") void save();
            if (e.key === "Escape") setEditing(false);
          }}
          placeholder={t("settings.history.name.placeholder")}
          className="h-6 w-48 rounded border border-control-border bg-control px-1.5 text-[13px] text-text focus:outline-none focus:shadow-[inset_0_-2px_0_var(--color-accent)]"
          aria-label={t("settings.history.name.label")}
        />
        <button
          type="button"
          onClick={() => void save()}
          className="inline-flex items-center justify-center h-6 w-6 rounded text-accent-text hover:bg-hover cursor-pointer"
          title={t("settings.history.name.save")}
          aria-label={t("settings.history.name.save")}
        >
          <Check className="w-3.5 h-3.5" />
        </button>
        <button
          type="button"
          onClick={() => setEditing(false)}
          className="inline-flex items-center justify-center h-6 w-6 rounded text-text-secondary hover:bg-hover cursor-pointer"
          title={t("settings.history.name.cancel")}
          aria-label={t("settings.history.name.cancel")}
        >
          <X className="w-3.5 h-3.5" />
        </button>
      </span>
    );
  }
  if (!entry.name) return null;
  return (
    <button
      type="button"
      onClick={() => setEditing(true)}
      className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 bg-accent-soft text-accent-text text-[12px] font-medium cursor-pointer hover:opacity-90"
      title={t("settings.history.name.edit")}
    >
      <Tag className="w-3 h-3" aria-hidden />
      <HighlightedText text={entry.name} matcher={matcher} />
    </button>
  );
};

const HistoryEntryComponent: React.FC<HistoryEntryProps> = ({
  entry,
  matcher,
  selectedIndex,
  onToggleSelect,
  onToggleSaved,
  onCopyText,
  getAudioUrl,
  deleteAudio,
}) => {
  const { t, i18n } = useTranslation();
  const navigateTo = useNavStore((s) => s.navigateTo);
  const [showCopied, setShowCopied] = useState(false);
  const [transcribing, setTranscribing] = useState<null | "text" | "speakers">(
    null,
  );
  const [transcribeError, setTranscribeError] = useState<string | null>(null);
  const [naming, setNaming] = useState(false);
  // A take with speakers shows them by default; "Text" shows it without.
  const speakerText = entry.diarized_text?.trim() || null;
  const [showSpeakers, setShowSpeakers] = useState(true);
  const shownSpeakers = speakerText !== null && showSpeakers;
  const shownText = shownSpeakers ? speakerText : entry.transcription_text;
  const noText = entry.transcription_text.trim() === "";
  // The audio is still there (History's clean-up can delete it, keeping the text).
  const hasAudio = entry.audio_purged_at == null;
  const selected = selectedIndex >= 0;

  // "Transcribe again": reads the recording again with the dictation model
  // (a take recorded without a model, or whose transcription failed). The
  // row's text is replaced; asks first when it has text.
  const handleTranscribeAgain = async () => {
    if (
      !noText &&
      !window.confirm(t("settings.history.transcribeAgainConfirm"))
    )
      return;
    setTranscribing("text");
    setTranscribeError(null);
    try {
      const result = await commands.retranscribeHistoryEntry(entry.id);
      if (result.status === "error") setTranscribeError(result.error);
    } finally {
      setTranscribing(null);
    }
  };

  // "Transcribe again with speakers": the whole recording to the STT server's
  // speakers model. A missing or unreachable server says what to set up.
  const handleTranscribeSpeakers = async () => {
    if (!noText && !window.confirm(t("settings.history.speakers.confirm")))
      return;
    setTranscribing("speakers");
    setTranscribeError(null);
    try {
      const result = await commands.retranscribeHistoryEntrySpeakers(entry.id);
      if (result.status === "error") {
        const { message, toSettings } = speakersErrorMessage(result.error, t);
        toast.error(message, {
          duration: 10000,
          action: toSettings
            ? {
                label: t("settings.history.speakers.openSettings"),
                onClick: () => navigateTo("sttServer"),
              }
            : undefined,
        });
        if (!toSettings) setTranscribeError(result.error);
      } else if (!result.data.speakers) {
        toast.info(t("settings.history.speakers.oneVoice"));
      } else {
        setShowSpeakers(true);
      }
    } finally {
      setTranscribing(null);
    }
  };

  const handleLoadAudio = useCallback(
    () => getAudioUrl(entry.file_name),
    [getAudioUrl, entry.file_name],
  );

  const handleCopyText = () => {
    onCopyText(shownText);
    setShowCopied(true);
    setTimeout(() => setShowCopied(false), 2000);
  };

  const handleDeleteEntry = async () => {
    if (!window.confirm(t("settings.history.deleteConfirm"))) return;
    try {
      await deleteAudio(entry.id);
    } catch (error) {
      console.error("Failed to delete entry:", error);
      toast.error(t("settings.history.deleteFailed"));
    }
  };

  const menuItems: RowMenuItem[] = [
    ...(hasAudio
      ? [
          {
            key: "again",
            icon: Captions,
            label: t("settings.history.menu.again.label"),
            hint: t("settings.history.menu.again.hint"),
            onSelect: () => void handleTranscribeAgain(),
            disabled: transcribing !== null,
          },
          {
            key: "speakers",
            icon: Users,
            label: t("settings.history.menu.speakers.label"),
            hint: t("settings.history.menu.speakers.hint"),
            onSelect: () => void handleTranscribeSpeakers(),
            disabled: transcribing !== null,
          },
        ]
      : []),
    {
      key: "name",
      icon: Pencil,
      label: entry.name
        ? t("settings.history.menu.rename.label")
        : t("settings.history.menu.name.label"),
      hint: t("settings.history.menu.name.hint"),
      onSelect: () => setNaming(true),
    },
    {
      key: "delete",
      icon: Trash2,
      label: t("settings.history.menu.delete.label"),
      hint: t("settings.history.menu.delete.hint"),
      onSelect: () => void handleDeleteEntry(),
      danger: true,
    },
  ];

  const formattedDate = recordedAt(Number(entry.timestamp), i18n.language);

  return (
    <div
      className={`card px-4 py-3 flex flex-col gap-2.5 ${
        entry.saved ? "border-s-[3px] border-s-accent" : ""
      } ${selected ? "ring-1 ring-accent" : ""}`}
    >
      <div className="flex justify-between items-center gap-2">
        <div className="flex flex-wrap items-center gap-x-1.5 gap-y-1 text-[13px] text-text-secondary min-w-0">
          <label
            className="inline-flex items-center cursor-pointer me-0.5"
            title={t("settings.history.combine.select")}
          >
            <input
              type="checkbox"
              checked={selected}
              onChange={onToggleSelect}
              className="accent-[var(--color-accent)]"
              aria-label={t("settings.history.combine.select")}
            />
            {selected && (
              <span className="ms-1 text-[11px] font-semibold tabular-nums text-accent-text">
                {selectedIndex + 1}
              </span>
            )}
          </label>
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
          <NameField
            entry={entry}
            editing={naming}
            setEditing={setNaming}
            matcher={matcher}
          />
        </div>
        <div className="flex items-center gap-1 shrink-0">
          {transcribing !== null && (
            <Loader2
              width={16}
              height={16}
              className="animate-spin text-text-secondary"
              aria-label={t("settings.history.menu.working")}
            />
          )}
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
          <HistoryRowMenu items={menuItems} buttonClassName={ICON_BUTTON} />
        </div>
      </div>
      {noText ? (
        <p className="text-sm text-text-secondary italic">
          {hasAudio
            ? t("settings.history.noTranscript")
            : t("settings.history.noTranscriptNoAudio")}
        </p>
      ) : (
        <>
          {speakerText !== null && (
            <div
              className="inline-flex self-start rounded-md border border-border p-0.5 text-xs"
              role="group"
              aria-label={t("settings.history.view.label")}
            >
              {([false, true] as const).map((speakers) => (
                <button
                  key={String(speakers)}
                  type="button"
                  onClick={() => setShowSpeakers(speakers)}
                  aria-pressed={showSpeakers === speakers}
                  className={`px-2 py-0.5 rounded cursor-pointer transition-colors ${
                    showSpeakers === speakers
                      ? "bg-accent-soft text-accent-text font-medium"
                      : "text-text-secondary hover:bg-hover"
                  }`}
                >
                  {speakers
                    ? t("settings.history.view.speakers")
                    : t("settings.history.view.text")}
                </button>
              ))}
            </div>
          )}
          <p
            className={`text-sm select-text cursor-text ${shownSpeakers ? "whitespace-pre-line" : ""}`}
          >
            <HighlightedText text={shownText} matcher={matcher} />
          </p>
        </>
      )}
      {transcribeError && (
        <p className="text-xs text-err-text">
          {t("settings.history.transcribeAgainFailed", {
            error: transcribeError,
          })}
        </p>
      )}
      <AudioPlayer
        onLoadRequest={handleLoadAudio}
        knownDuration={entry.duration_seconds}
        className="w-full"
      />
    </div>
  );
};
