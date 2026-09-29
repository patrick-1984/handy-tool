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

/** Copy, star and delete on a recording: icon buttons without a frame. */
const ICON_BUTTON =
  "inline-flex items-center justify-center h-7 w-7 rounded-md text-text-secondary hover:bg-hover hover:text-text transition-colors cursor-pointer";

/** Entries drawn at a time on the Recordings tab. */
const HISTORY_PAGE = 20;

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
  useEffect(() => setShownCount(HISTORY_PAGE), [matcher]);
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
      <div className="flex flex-col gap-3">
        {filteredEntries.slice(0, shownCount).map((entry) => (
          <HistoryEntryComponent
            key={entry.id}
            entry={entry}
            matcher={matcher}
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
          <div className="flex items-center justify-end">
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
  onToggleSaved,
  onCopyText,
  getAudioUrl,
  deleteAudio,
}) => {
  const { t, i18n } = useTranslation();
  const [showCopied, setShowCopied] = useState(false);

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
    <div className="card px-4 py-3 flex flex-col gap-2.5">
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
            className={`${ICON_BUTTON} hover:!bg-err-bg hover:!text-err-text`}
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
    </div>
  );
};
