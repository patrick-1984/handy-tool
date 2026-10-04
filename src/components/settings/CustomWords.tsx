import React, { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { X } from "lucide-react";
import { ask, open } from "@tauri-apps/plugin-dialog";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { commands } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { SettingContainer } from "../ui/SettingContainer";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { parseCustomWords } from "../../lib/utils/customWords";

interface CustomWordsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/** Above this many words the chip list folds to its first chips. */
const COLLAPSED_COUNT = 12;

export const CustomWords: React.FC<CustomWordsProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const [newWord, setNewWord] = useState("");
    const [expanded, setExpanded] = useState(false);
    const [filter, setFilter] = useState("");
    const customWords = getSetting("custom_words") || [];

    // Adds every word in `text` (separated by spaces, commas, semicolons or
    // new lines). A lone duplicate keeps its own message; anything more ends
    // in one summary toast. Returns what happened so the caller can react.
    const addWords = (text: string): "empty" | "duplicate" | "done" => {
      const { added, skipped, duplicates } = parseCustomWords(
        text,
        customWords,
      );
      if (added.length === 0 && skipped === 0) return "empty";
      if (added.length === 0 && skipped === 1 && duplicates.length === 1) {
        toast.error(
          t("settings.advanced.customWords.duplicate", {
            word: duplicates[0],
          }),
        );
        return "duplicate";
      }
      if (added.length > 0) {
        updateSetting("custom_words", [...customWords, ...added]);
      }
      if (added.length > 1 || skipped > 0) {
        const title = t("settings.advanced.customWords.addedSummary", {
          added: added.length,
        });
        const description =
          skipped > 0
            ? t("settings.advanced.customWords.skippedSummary", { skipped })
            : undefined;
        if (added.length > 0) toast.success(title, { description });
        else toast.error(title, { description });
      }
      return "done";
    };

    // A lone duplicate stays in the field so it can be fixed.
    const handleAddWord = () => {
      if (addWords(newWord) === "done") setNewWord("");
    };

    const handleRemoveWord = (wordToRemove: string) => {
      updateSetting(
        "custom_words",
        customWords.filter((word) => word !== wordToRemove),
      );
    };

    const handleKeyPress = (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        handleAddWord();
      }
    };

    // A single-line field drops new lines from pasted text; turn them into
    // spaces so a pasted column of words stays whole.
    const handlePaste = (e: React.ClipboardEvent<HTMLInputElement>) => {
      const pasted = e.clipboardData.getData("text");
      if (!/[\r\n\t]/.test(pasted)) return;
      e.preventDefault();
      const input = e.currentTarget;
      const start = input.selectionStart ?? newWord.length;
      const end = input.selectionEnd ?? newWord.length;
      // Keep the edge spaces so the paste never glues onto a word beside it.
      const flat = pasted.replace(/[\r\n\t]+/g, " ");
      setNewWord(newWord.slice(0, start) + flat + newWord.slice(end));
    };

    const handleImport = async () => {
      try {
        const path = await open({
          multiple: false,
          directory: false,
          filters: [
            {
              name: t("settings.advanced.customWords.fileFilter"),
              extensions: ["txt", "csv", "md"],
            },
          ],
        });
        if (typeof path !== "string") return;
        const res = await commands.readTextFileForCount(path);
        if (res.status !== "ok") {
          toast.error(
            t("settings.advanced.customWords.importFailed", {
              error: res.error,
            }),
          );
          return;
        }
        if (addWords(res.data) === "empty") {
          toast.error(t("settings.advanced.customWords.importEmpty"));
        }
      } catch (e) {
        console.error("Failed to import custom words:", e);
        toast.error(
          t("settings.advanced.customWords.importFailed", { error: String(e) }),
        );
      }
    };

    const handleCopyAll = async () => {
      try {
        await writeText(customWords.join("\n"));
        toast.success(
          t("settings.advanced.customWords.copied", {
            count: customWords.length,
          }),
        );
      } catch (e) {
        console.error("Failed to copy custom words:", e);
      }
    };

    const handleRemoveAll = async () => {
      const confirmed = await ask(
        t("settings.advanced.customWords.removeAllMessage", {
          count: customWords.length,
        }),
        {
          title: t("settings.advanced.customWords.removeAllTitle"),
          kind: "warning",
        },
      );
      if (!confirmed) return;
      updateSetting("custom_words", []);
      setExpanded(false);
      setFilter("");
    };

    const busy = isUpdating("custom_words");
    const collapsible = customWords.length > COLLAPSED_COUNT;
    const showAll = expanded && collapsible;
    const query = filter.trim().toLowerCase();
    const visibleWords = useMemo(() => {
      if (!collapsible) return customWords;
      if (!showAll) return customWords.slice(0, COLLAPSED_COUNT);
      return query
        ? customWords.filter((w) => w.toLowerCase().includes(query))
        : customWords;
    }, [customWords, collapsible, showAll, query]);

    const chips = visibleWords.map((word) => (
      <span
        key={word}
        className="inline-flex items-center gap-0.5 h-[26px] ps-2.5 pe-0.5 rounded-sm border border-border bg-surface2 text-[13px] text-text"
      >
        {word}
        <button
          type="button"
          onClick={() => handleRemoveWord(word)}
          disabled={busy}
          aria-label={t("settings.advanced.customWords.remove", { word })}
          className="inline-flex items-center justify-center w-5 h-5 rounded-[3px] text-text-secondary hover:bg-hover hover:text-text transition-colors cursor-pointer disabled:cursor-not-allowed"
        >
          <X className="w-3 h-3" aria-hidden />
        </button>
      </span>
    ));

    const toggleButton = collapsible ? (
      <button
        type="button"
        onClick={() => {
          setExpanded(!showAll);
          setFilter("");
        }}
        aria-expanded={showAll}
        className="inline-flex items-center h-[26px] px-2 rounded-sm text-[13px] font-medium text-accent-text hover:bg-hover transition-colors cursor-pointer"
      >
        {showAll
          ? t("settings.advanced.customWords.showLess")
          : t("settings.advanced.customWords.showAll", {
              count: customWords.length,
            })}
      </button>
    ) : null;

    // One field holding the words as chips; words are added with Enter, many
    // at once when separated by spaces. A long list folds to its first chips.
    return (
      <SettingContainer
        title={t("settings.advanced.customWords.title")}
        description={t("settings.advanced.customWords.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
        layout="stacked"
      >
        <div className="flex flex-col gap-1.5">
          <div className="flex flex-wrap items-center gap-1.5 p-1.5 rounded-md border border-control-border border-b-control-bottom bg-control focus-within:shadow-[inset_0_-2px_0_var(--color-accent)]">
            {showAll ? (
              <div className="w-full max-h-60 overflow-y-auto flex flex-wrap items-center gap-1.5">
                {chips}
                {visibleWords.length === 0 && (
                  <span className="h-[26px] px-1.5 inline-flex items-center text-[13px] text-text-secondary">
                    {t("settings.advanced.customWords.noMatches")}
                  </span>
                )}
              </div>
            ) : (
              <>
                {chips}
                {toggleButton}
              </>
            )}
            <input
              type="text"
              value={newWord}
              onChange={(e) => setNewWord(e.target.value)}
              onKeyDown={handleKeyPress}
              onPaste={handlePaste}
              placeholder={t("settings.advanced.customWords.placeholderMulti")}
              aria-label={t("settings.advanced.customWords.placeholder")}
              disabled={busy}
              className="flex-1 min-w-40 h-[26px] px-1.5 bg-transparent text-[13px] text-text placeholder:text-text-secondary focus:outline-none"
            />
          </div>
          <div className="flex flex-wrap items-center gap-1">
            {showAll && (
              <>
                <Input
                  variant="compact"
                  type="search"
                  value={filter}
                  onChange={(e) => setFilter(e.target.value)}
                  placeholder={t("settings.advanced.customWords.filter")}
                  aria-label={t("settings.advanced.customWords.filter")}
                  className="w-44 text-[13px]"
                />
                {toggleButton}
              </>
            )}
            <div className="ms-auto flex flex-wrap items-center gap-1">
              <Button
                variant="ghost"
                size="sm"
                onClick={handleImport}
                disabled={busy}
              >
                {t("settings.advanced.customWords.importFile")}
              </Button>
              {customWords.length > 0 && (
                <>
                  <Button variant="ghost" size="sm" onClick={handleCopyAll}>
                    {t("settings.advanced.customWords.copyAll")}
                  </Button>
                  <Button
                    variant="danger-ghost"
                    size="sm"
                    onClick={handleRemoveAll}
                    disabled={busy}
                  >
                    {t("settings.advanced.customWords.removeAll")}
                  </Button>
                </>
              )}
            </div>
          </div>
        </div>
      </SettingContainer>
    );
  },
);
