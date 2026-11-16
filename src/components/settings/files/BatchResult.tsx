import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { CircleAlert, CircleCheck, Loader2 } from "lucide-react";
import { Button } from "../../ui/Button";
import {
  useFileBatchStore,
  type BatchPart,
} from "../../../stores/fileBatchStore";
import { CopyButton, SavedTo, clock } from "./filesUi";

/** One file's text: its name and its own Copy; long texts fold. */
const PartCard: React.FC<{ part: BatchPart; index: number }> = ({
  part,
  index,
}) => {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const text = part.text?.trim() ?? "";
  const long = text.length > 600;
  return (
    <div className="flex flex-col gap-1.5 rounded-lg border border-border px-3 py-2.5">
      <div className="flex items-center justify-between gap-2">
        <p
          className="flex items-center gap-2 text-sm font-semibold min-w-0"
          title={part.path}
        >
          <span className="text-xs tabular-nums text-accent-text">
            {index + 1}.
          </span>
          <span className="truncate">{part.fileName}</span>
        </p>
        {text && (
          <CopyButton text={text} label={t("settings.files.transcribe.copy")} />
        )}
      </div>
      {part.status === "done" ? (
        <>
          <p
            className={`text-[13px] leading-[1.6] select-text cursor-text whitespace-pre-wrap ${
              long && !open ? "line-clamp-4" : ""
            }`}
          >
            {text || t("settings.files.transcribe.empty")}
          </p>
          {long && (
            <button
              type="button"
              onClick={() => setOpen((v) => !v)}
              className="self-start text-xs text-accent-text hover:underline cursor-pointer"
            >
              {open
                ? t("settings.files.batch.result.showLess")
                : t("settings.files.batch.result.showAll")}
            </button>
          )}
        </>
      ) : part.status === "failed" ? (
        <p className="text-xs text-err-text break-words">
          {t("settings.files.batch.result.failed", { error: part.error ?? "" })}
        </p>
      ) : (
        <p className="text-xs text-text-secondary">
          {t("settings.files.batch.result.notTranscribed")}
        </p>
      )}
    </div>
  );
};

/**
 * Files › several files: the progress while they are transcribed one after
 * another, then the combined text (Copy all) and each file's own text.
 */
export const BatchResult: React.FC = () => {
  const { t } = useTranslation();
  const phase = useFileBatchStore((s) => s.phase);
  const parts = useFileBatchStore((s) => s.parts);
  const current = useFileBatchStore((s) => s.current);
  const progress = useFileBatchStore((s) => s.progress);
  const combined = useFileBatchStore((s) => s.combined);
  const savedPath = useFileBatchStore((s) => s.savedPath);
  const saveError = useFileBatchStore((s) => s.saveError);
  const stopError = useFileBatchStore((s) => s.stopError);
  const cancelRequested = useFileBatchStore((s) => s.cancelRequested);
  const saving = useFileBatchStore((s) => s.saving);
  const cancel = useFileBatchStore((s) => s.cancel);
  const dismiss = useFileBatchStore((s) => s.dismiss);

  if (phase === "running") {
    const part = parts[current];
    const percent =
      progress && progress.total > 0
        ? Math.round((progress.done / progress.total) * 100)
        : 0;
    return (
      <div className="px-4 py-3 flex flex-col gap-2">
        <div className="flex items-center justify-between gap-3">
          <p className="flex items-center gap-2 text-sm font-semibold min-w-0">
            <Loader2 className="w-4 h-4 shrink-0 animate-spin text-accent" />
            <span className="truncate">
              {t("settings.files.batch.progress", {
                current: current + 1,
                total: parts.length,
                name: part?.fileName ?? "",
              })}
            </span>
          </p>
          <Button
            variant="secondary"
            size="sm"
            onClick={cancel}
            disabled={cancelRequested || saving}
          >
            {t("settings.files.transcribe.cancel")}
          </Button>
        </div>
        <div className="w-full h-1 bg-control-bottom rounded-full overflow-hidden">
          <div
            className="h-full bg-accent rounded-full transition-[width] duration-300"
            style={{
              width: `${((current + percent / 100) / Math.max(1, parts.length)) * 100}%`,
            }}
          />
        </div>
        {progress && (
          <p className="text-xs text-text-secondary tabular-nums">
            {t("settings.files.transcribe.progress", {
              done: clock(progress.done),
              total: clock(progress.total),
              percent,
            })}
          </p>
        )}
      </div>
    );
  }

  if (phase !== "done") return null;
  const done = parts.filter((p) => p.status === "done").length;
  const failed = parts.filter((p) => p.status === "failed").length;
  return (
    <div className="px-4 py-3 flex flex-col gap-3">
      <div className="flex items-center justify-between gap-3">
        <p className="flex items-center gap-2 text-sm font-semibold min-w-0">
          {failed > 0 || stopError || cancelRequested ? (
            <CircleAlert className="w-4 h-4 shrink-0 text-err-text" />
          ) : (
            <CircleCheck className="w-4 h-4 shrink-0 text-ok-text" />
          )}
          <span className="truncate">
            {t("settings.files.batch.result.title", {
              done,
              total: parts.length,
            })}
          </span>
        </p>
        <div className="flex items-center gap-2">
          {combined && (
            <CopyButton
              text={combined}
              label={t("settings.files.batch.result.copyAll")}
              labelled
            />
          )}
          <Button variant="secondary" size="sm" onClick={dismiss}>
            {t("settings.files.batch.result.clear")}
          </Button>
        </div>
      </div>
      {stopError && (
        <p className="text-xs text-err-text break-words">
          {t("settings.files.batch.result.stopped", { error: stopError })}
        </p>
      )}
      {cancelRequested && !stopError && (
        <p className="text-xs text-text-secondary">
          {t("settings.files.batch.result.cancelled")}
        </p>
      )}
      <p className="max-h-64 overflow-y-auto text-[13px] leading-[1.6] select-text cursor-text whitespace-pre-wrap bg-surface2 border border-border rounded-lg px-3 py-2.5">
        {combined || t("settings.files.transcribe.empty")}
      </p>
      {savedPath && <SavedTo path={savedPath} />}
      {saveError && (
        <p className="text-xs text-err-text break-all">
          {t("settings.files.transcribe.saveFailed", { error: saveError })}
        </p>
      )}
      <p className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
        {t("settings.files.batch.result.perFile")}
      </p>
      <div className="flex flex-col gap-2">
        {parts.map((part, index) => (
          <PartCard key={part.path} part={part} index={index} />
        ))}
      </div>
    </div>
  );
};
