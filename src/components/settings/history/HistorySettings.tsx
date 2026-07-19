import React, { useState, useEffect, useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { AudioPlayer } from "../../ui/AudioPlayer";
import { Button } from "../../ui/Button";
import { Copy, Star, Check, Trash2, FolderOpen, Search, X } from "lucide-react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { readFile } from "@tauri-apps/plugin-fs";
import { commands, type HistoryEntry } from "@/bindings";
import { formatDateTime } from "@/utils/dateFormat";

const pad2 = (n: number) => String(n).padStart(2, "0");
const fmtDuration = (s: number) => {
  // Round once so the components can't disagree across a 60s boundary.
  const t = Math.max(0, Math.round(s));
  return `${pad2(Math.floor(t / 3600))}:${pad2(Math.floor((t % 3600) / 60))}:${pad2(
    t % 60,
  )}`;
};

// "(HH:MM:SS · <model> · $cost)" — duration first, then which model produced it
// (local or OpenRouter, already labeled), then the real cost when known.
const detailBracket = (e: HistoryEntry): string | null => {
  const parts: string[] = [];
  if (e.duration_seconds != null) parts.push(fmtDuration(e.duration_seconds));
  if (e.model_used) parts.push(e.model_used);
  if (e.cost_usd != null) parts.push(`$${e.cost_usd.toFixed(4)}`);
  return parts.length ? `(${parts.join(" · ")})` : null;
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

export const HistorySettings: React.FC = () => {
  const { t } = useTranslation();
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
      <div className="px-4 py-3 text-center text-text/60">
        {t("settings.history.loading")}
      </div>
    );
  } else if (loadError) {
    body = (
      <div className="px-4 py-3 text-center text-danger">
        {t("settings.history.loadError", { error: loadError })}
      </div>
    );
  } else if (historyEntries.length === 0) {
    body = (
      <div className="px-4 py-3 text-center text-text/60">
        {t("settings.history.empty")}
      </div>
    );
  } else if (filteredEntries.length === 0) {
    body = (
      <div className="px-4 py-3 text-center text-text/60">
        {t("settings.history.search.noMatches")}
      </div>
    );
  } else {
    body = (
      <div className="divide-y divide-mid-gray/20">
        {filteredEntries.map((entry) => (
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
      </div>
    );
  }

  return (
    <div className="w-full space-y-6">
      <div className="space-y-2">
        <div className="px-4 flex items-center justify-between">
          <div>
            <h2 className="text-xs font-medium text-mid-gray uppercase tracking-wide">
              {t("settings.history.title")}
            </h2>
          </div>
          <OpenRecordingsButton
            onClick={openRecordingsFolder}
            label={t("settings.history.openFolder")}
          />
        </div>
        {showSearchBar && (
          <div className="px-4 flex items-center gap-2">
            <div className="relative flex-1">
              <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-4 h-4 text-text/40 pointer-events-none" />
              <input
                type="text"
                value={searchInput}
                onChange={(e) => setSearchInput(e.target.value)}
                placeholder={t("settings.history.search.placeholder")}
                className="w-full rounded-md border border-mid-gray/30 bg-background pl-8 pr-8 py-1.5 text-sm focus:border-logo-primary focus:outline-none"
              />
              {searchInput && (
                <button
                  onClick={() => setSearchInput("")}
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-text/40 hover:text-text cursor-pointer"
                  title={t("settings.history.search.clear")}
                >
                  <X className="w-4 h-4" />
                </button>
              )}
            </div>
            {matcher && (
              <span className="text-xs text-text/60 whitespace-nowrap">
                {t("settings.history.search.matches", {
                  matched: filteredEntries.length,
                  total: historyEntries.length,
                })}
                {matcher.isRegex && (
                  <span
                    className="ml-1.5 px-1 py-0.5 rounded bg-logo-primary/15 text-logo-primary font-mono"
                    title={t("settings.history.search.regexActive")}
                  >
                    {t("settings.history.search.regexBadge")}
                  </span>
                )}
              </span>
            )}
          </div>
        )}
        <div className="bg-background border border-mid-gray/20 rounded-lg overflow-visible">
          {body}
        </div>
      </div>
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
            className="bg-logo-primary/30 text-inherit rounded-sm px-0.5"
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

  const formattedDate = formatDateTime(String(entry.timestamp), i18n.language);

  return (
    <div className="px-4 py-2 pb-5 flex flex-col gap-3">
      <div className="flex justify-between items-center">
        <p className="text-sm font-medium">
          {formattedDate}
          {detailBracket(entry) && (
            <span className="text-text/50 font-normal">
              {" "}
              {detailBracket(entry)}
            </span>
          )}
        </p>
        <div className="flex items-center gap-1">
          <button
            onClick={handleCopyText}
            className="text-text/50 hover:text-logo-primary  hover:border-logo-primary transition-colors cursor-pointer"
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
            className={`p-2 rounded-md transition-colors cursor-pointer ${
              entry.saved
                ? "text-logo-primary hover:text-logo-primary/80"
                : "text-text/50 hover:text-logo-primary"
            }`}
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
            className="text-text/50 hover:text-logo-primary transition-colors cursor-pointer"
            title={t("settings.history.delete")}
          >
            <Trash2 width={16} height={16} />
          </button>
        </div>
      </div>
      <p className="italic text-text/90 text-sm pb-2 select-text cursor-text">
        <HighlightedText text={entry.transcription_text} matcher={matcher} />
      </p>
      <AudioPlayer onLoadRequest={handleLoadAudio} className="w-full" />
    </div>
  );
};
