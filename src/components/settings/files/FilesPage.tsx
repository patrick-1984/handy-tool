import React, { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";
import {
  ArrowRight,
  Check,
  CircleCheck,
  Copy,
  Download,
  FileAudio,
  FileX,
  Folder,
  FolderOpen,
  ListOrdered,
  Loader2,
  Trash2,
} from "lucide-react";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { Button } from "../../ui/Button";
import { Dropdown } from "../../ui/Dropdown";
import { ResetButton } from "../../ui/ResetButton";
import { useSettings } from "../../../hooks/useSettings";
import { useModelStore } from "../../../stores/modelStore";
import { getTranslatedModelName } from "../../../lib/utils/modelTranslation";
import { formatModelSize } from "../../../lib/utils/format";
import { TranslatorSettings } from "../translator/TranslatorSettings";
import {
  commands,
  type EngineType,
  type FileJob,
  type FileTextSave,
  type FileTranscription,
  type ModelInfo,
} from "@/bindings";

/** The file types the backend reads (see decode.rs SUPPORTED_EXTENSIONS). */
const AUDIO_EXTENSIONS = [
  "wav",
  "mp3",
  "m4a",
  "mp4",
  "aac",
  "flac",
  "ogg",
  "opus",
];

/** Online engines: they have no second slot to run beside dictation. */
const EXTERNAL: EngineType[] = [
  "FlmWhisper",
  "ApiWhisper",
  "OpenRouterWhisper",
];

const SAVE_MODES: FileTextSave[] = [
  "ask",
  "handy_folder",
  "next_to_file",
  "dont_save",
];

/** Icon buttons without a frame: 32 px targets, named by their tooltip. */
const ICON_BUTTON =
  "inline-flex items-center justify-center h-8 w-8 rounded-md text-text-secondary hover:bg-hover hover:text-text transition-colors cursor-pointer";

/** 75.4 → "1:15", 3725 → "1:02:05". */
const clock = (secs: number): string => {
  const s = Math.max(0, Math.round(secs));
  const pad2 = (n: number) => String(n).padStart(2, "0");
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return h > 0 ? `${h}:${pad2(m)}:${pad2(s % 60)}` : `${m}:${pad2(s % 60)}`;
};

/**
 * A copy button that shows a tick for a moment after copying: an icon, or with
 * `labelled` a button with its label.
 */
const CopyButton: React.FC<{
  text: string;
  label: string;
  labelled?: boolean;
}> = ({ text, label, labelled = false }) => {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    await navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };
  if (labelled) {
    return (
      <Button variant="secondary" size="sm" onClick={copy}>
        {copied ? (
          <Check className="w-3.5 h-3.5" />
        ) : (
          <Copy className="w-3.5 h-3.5" />
        )}
        {label}
      </Button>
    );
  }
  return (
    <button
      type="button"
      className={ICON_BUTTON}
      title={label}
      aria-label={label}
      onClick={copy}
    >
      {copied ? (
        <Check width={16} height={16} />
      ) : (
        <Copy width={16} height={16} />
      )}
    </button>
  );
};

/**
 * A path on one line in the dialog: shortened in the middle when long, so its
 * start (the drive) and end (the folder) stay readable.
 */
const shortPath = (path: string, max = 42): string =>
  path.length <= max ? path : `${path.slice(0, 3)}…${path.slice(3 - max)}`;

/** "Text saved to <path>", the path in monospace. */
const SavedTo: React.FC<{ path: string }> = ({ path }) => {
  const { t } = useTranslation();
  const MARK = "⁣";
  const [before, after = ""] = t("settings.files.transcribe.savedTo", {
    path: MARK,
  }).split(MARK);
  return (
    <p className="text-xs text-text-secondary break-all">
      {before}
      <span className="font-mono">{path}</span>
      {after}
    </p>
  );
};

/**
 * Where should the text be saved? Asked when a file is picked (so a long file
 * can be left to run), with Remember my choice.
 */
const SaveTextDialog: React.FC<{
  file: string;
  folder: string;
  onChoose: (save: FileTextSave, remember: boolean) => void;
  onCancel: () => void;
}> = ({ file, folder, onChoose, onCancel }) => {
  const { t } = useTranslation();
  const [remember, setRemember] = useState(false);
  const cut = Math.max(file.lastIndexOf("\\"), file.lastIndexOf("/"));
  const recordingFolder = file.slice(0, cut);
  const name = file.slice(cut + 1);

  const firstRow = useRef<HTMLButtonElement>(null);
  useEffect(() => firstRow.current?.focus(), []);

  // Esc = Cancel.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);

  const choices: {
    save: FileTextSave;
    label: string;
    detail: string;
    icon: React.ComponentType<{ className?: string }>;
    path: boolean;
  }[] = [
    {
      save: "handy_folder",
      label: t("settings.files.ask.handyFolder"),
      detail: folder,
      icon: Folder,
      path: true,
    },
    {
      save: "next_to_file",
      label: t("settings.files.ask.nextToFile"),
      detail: recordingFolder,
      icon: FolderOpen,
      path: true,
    },
    {
      save: "dont_save",
      label: t("settings.files.ask.dontSave"),
      detail: t("settings.files.ask.dontSaveHint"),
      icon: FileX,
      path: false,
    },
  ];

  // Each row is the action itself; focus starts on the first one.
  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-scrim p-4"
      onClick={onCancel}
      role="presentation"
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="save-text-title"
        className="w-[440px] max-w-full rounded-lg bg-surface border border-border shadow-float overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-6 pt-5 pb-1 min-w-0">
          <h2
            id="save-text-title"
            className="font-display text-lg leading-6 font-semibold"
          >
            {t("settings.files.ask.title")}
          </h2>
          <p className="text-[13px] text-text-secondary truncate" title={file}>
            {name}
          </p>
        </div>
        <div className="px-6 pt-3 pb-4 flex flex-col gap-2">
          {choices.map((c, i) => (
            <button
              key={c.save}
              ref={i === 0 ? firstRow : undefined}
              type="button"
              onClick={() => onChoose(c.save, remember)}
              className="flex items-center gap-3 rounded-lg border border-border px-3.5 py-2.5 text-start cursor-pointer transition-colors hover:border-control-bottom hover:bg-control-hover"
            >
              <c.icon className="w-[18px] h-[18px] shrink-0 text-text-secondary" />
              <span className="flex-1 min-w-0 flex flex-col gap-0.5">
                <span className="text-sm font-semibold">{c.label}</span>
                <span
                  className={`text-xs text-text-secondary ${c.path ? "font-mono whitespace-nowrap" : ""}`}
                  title={c.path ? c.detail : undefined}
                >
                  {c.path ? shortPath(c.detail) : c.detail}
                </span>
              </span>
              <ArrowRight className="w-4 h-4 shrink-0 text-text-secondary rtl:-scale-x-100" />
            </button>
          ))}
        </div>
        <div className="flex items-center justify-between gap-4 px-6 py-3.5 bg-surface2 border-t border-border">
          <label className="flex flex-col gap-0.5 cursor-pointer">
            {/* The box centred on the first line; the hint under the words. */}
            <span className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={remember}
                onChange={(e) => setRemember(e.target.checked)}
                className="w-4 h-4 shrink-0 accent-accent cursor-pointer"
              />
              <span className="text-sm">
                {t("settings.files.ask.remember")}
              </span>
            </span>
            <span className="ps-6 text-xs text-text-secondary">
              {t("settings.files.ask.rememberHint")}
            </span>
          </label>
          <Button variant="secondary" onClick={onCancel}>
            {t("common.cancel")}
          </Button>
        </div>
      </div>
    </div>,
    document.body,
  );
};

/** Pick a model and a file, follow its progress, and copy the result. */
const TranscribeFile: React.FC<{ onFinished: () => void }> = ({
  onFinished,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const [job, setJob] = useState<FileJob | null>(null);
  const [folder, setFolder] = useState("");
  const [pendingFile, setPendingFile] = useState<string | null>(null);
  const saveMode = (getSetting("file_text_save") ?? "ask") as FileTextSave;
  const keepAudio = getSetting("file_keep_audio") ?? false;
  const chosenFolder = (getSetting("files_folder") as string) ?? "";

  const models = useModelStore((st) => st.models);
  const dictationModel = useModelStore((st) => st.currentModel);
  const downloading = useModelStore((st) => st.downloadingModels);
  const extracting = useModelStore((st) => st.extractingModels);
  const downloadProgress = useModelStore((st) => st.downloadProgress);
  useEffect(() => {
    // Idempotent: fills the model list even if the Models page wasn't opened.
    useModelStore.getState().initialize();
  }, []);

  // Any local model, and the dictation model even when it is an online one.
  // The one picked last is remembered; until then, the dictation model.
  const fileModels = models.filter(
    (m) => m.id === dictationModel || !EXTERNAL.includes(m.engine_type),
  );
  const savedModel = (getSetting("file_model") as string) ?? "";
  const modelId = fileModels.some((m) => m.id === savedModel)
    ? savedModel
    : dictationModel;
  const model = fileModels.find((m) => m.id === modelId);
  // The dictation model first, then the downloaded ones, then the rest.
  const rank = (m: ModelInfo) =>
    m.id === dictationModel ? 0 : m.is_downloaded ? 1 : 2;
  // Grouped under headings, each with its size; the ones to download have a
  // download glyph instead of "(not downloaded)".
  const GROUPS = ["dictation", "downloaded", "notDownloaded"];
  const modelOptions = [...fileModels]
    .sort((a, b) => rank(a) - rank(b))
    .map((m) => {
      const name = getTranslatedModelName(m, t);
      return {
        value: m.id,
        label: name,
        group: t(`settings.files.model.groups.${GROUPS[rank(m)]}`),
        hint: formatModelSize(Number(m.size_mb)),
        trailing: m.is_downloaded ? undefined : (
          <Download
            className="w-3.5 h-3.5 shrink-0 text-text-secondary"
            aria-hidden
          />
        ),
        selectedLabel:
          m.id === dictationModel
            ? t("settings.files.model.dictation", { name })
            : m.is_downloaded
              ? name
              : t("settings.files.model.notDownloaded", { name }),
      };
    });

  useEffect(() => {
    let disposed = false;
    let eventArrived = false;
    commands.getFileJob().then((j) => {
      if (!disposed && !eventArrived) setJob(j);
    });
    const unlisten = listen<FileJob>("file-job", (e) => {
      eventArrived = true;
      setJob(e.payload);
      if (e.payload.state === "done") onFinished();
    });
    return () => {
      disposed = true;
      unlisten.then((f) => f());
    };
  }, [onFinished]);

  useEffect(() => {
    commands.getFilesFolder().then((r) => {
      if (r.status === "ok") setFolder(r.data);
    });
  }, [chosenFolder]);

  const running = job?.state === "decoding" || job?.state === "transcribing";

  const chooseFile = async () => {
    const picked = await open({
      multiple: false,
      filters: [
        {
          name: t("settings.files.transcribe.audioFiles"),
          extensions: AUDIO_EXTENSIONS,
        },
      ],
    });
    if (typeof picked !== "string" || !picked) return;
    if (saveMode === "ask") setPendingFile(picked);
    else void start(picked, saveMode);
  };

  const start = async (path: string, save: FileTextSave) => {
    const result = await commands.transcribeFile(path, modelId, save);
    if (result.status === "error") {
      toast.error(
        t("settings.files.transcribe.startFailed", { reason: result.error }),
      );
    }
  };

  const onDialogChoice = (save: FileTextSave, remember: boolean) => {
    const path = pendingFile;
    setPendingFile(null);
    if (remember) updateSetting("file_text_save", save);
    if (path) void start(path, save);
  };
  const closeDialog = React.useCallback(() => setPendingFile(null), []);

  const modelState = () => {
    if (!model || model.is_downloaded) return null;
    if (model.id in extracting)
      return (
        <span className="text-xs text-text-secondary">
          {t("modelSelector.extractingGeneric")}
        </span>
      );
    if (model.id in downloading)
      return (
        <span className="text-xs text-text-secondary tabular-nums">
          {t("modelSelector.downloading", {
            percentage: Math.round(downloadProgress[model.id]?.percentage ?? 0),
          })}
        </span>
      );
    // Choose file waits for it, so downloading is the main action now.
    return (
      <Button
        variant="primary"
        onClick={() => void useModelStore.getState().downloadModel(model.id)}
      >
        <Download className="w-3.5 h-3.5" />
        {t("settings.files.model.downloadSize", {
          size: formatModelSize(Number(model.size_mb)),
        })}
      </Button>
    );
  };

  const chooseFolder = async () => {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked !== "string" || !picked) return;
    updateSetting("files_folder", picked);
  };

  const status = () => {
    if (!job) return null;
    switch (job.state) {
      case "decoding":
      case "transcribing": {
        const total = job.state === "transcribing" ? job.total_secs : 0;
        const done = job.state === "transcribing" ? job.done_secs : 0;
        const percent = total > 0 ? Math.round((done / total) * 100) : 0;
        return (
          <div className="px-4 py-3 flex flex-col gap-2">
            <div className="flex items-center justify-between gap-3">
              <p className="flex items-center gap-2 text-sm font-semibold min-w-0">
                <Loader2 className="w-4 h-4 shrink-0 animate-spin text-accent" />
                <span className="truncate">
                  {job.state === "decoding"
                    ? t("settings.files.transcribe.decoding", {
                        file: job.file_name,
                      })
                    : job.file_name}
                </span>
              </p>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => void commands.cancelFileJob()}
              >
                {t("settings.files.transcribe.cancel")}
              </Button>
            </div>
            <div className="w-full h-1 bg-control-bottom rounded-full overflow-hidden">
              <div
                className="h-full bg-accent rounded-full transition-[width] duration-300"
                style={{ width: `${percent}%` }}
              />
            </div>
            {job.state === "transcribing" && (
              <p className="text-xs text-text-secondary tabular-nums">
                {t("settings.files.transcribe.progress", {
                  done: clock(done),
                  total: clock(total),
                  percent,
                })}
              </p>
            )}
          </div>
        );
      }
      case "done": {
        const { entry } = job;
        return (
          <div className="px-4 py-3 flex flex-col gap-2">
            <div className="flex items-center justify-between gap-3">
              <p className="flex items-center gap-2 text-sm font-semibold min-w-0">
                <CircleCheck className="w-4 h-4 shrink-0 text-ok-text" />
                <span className="truncate">
                  {t("settings.files.transcribe.done", {
                    file: entry.file_name,
                  })}
                </span>
              </p>
              {entry.text && (
                <CopyButton
                  text={entry.text}
                  label={t("settings.files.transcribe.copy")}
                  labelled
                />
              )}
            </div>
            <p className="max-h-64 overflow-y-auto text-[13px] leading-[1.6] select-text cursor-text whitespace-pre-wrap bg-surface2 border border-border rounded-lg px-3 py-2.5">
              {entry.text || t("settings.files.transcribe.empty")}
            </p>
            {entry.text_path && <SavedTo path={entry.text_path} />}
            {entry.save_error && (
              <p className="text-xs text-err-text break-all">
                {t("settings.files.transcribe.saveFailed", {
                  error: entry.save_error,
                })}
              </p>
            )}
          </div>
        );
      }
      case "failed":
        return (
          <p className="px-4 py-3 text-sm text-err-text break-words">
            {t("settings.files.transcribe.failed", {
              file: job.file_name,
              error: job.error,
            })}
          </p>
        );
      case "cancelled":
        return (
          <p className="px-4 py-3 text-sm text-text-secondary">
            {t("settings.files.transcribe.cancelled", { file: job.file_name })}
          </p>
        );
    }
  };

  return (
    <>
      <SettingsGroup
        icon={FileAudio}
        title={t("settings.files.transcribe.title")}
      >
        <SettingContainer
          title={t("settings.files.model.title")}
          description={t("settings.files.model.description")}
          descriptionMode="tooltip"
          grouped={true}
        >
          <div className="flex items-center gap-2">
            {modelState()}
            <Dropdown
              options={modelOptions}
              selectedValue={modelId}
              onSelect={(value) => updateSetting("file_model", value)}
              disabled={running}
              // Wide enough for "Whisper Large V3 Turbo (not downloaded)".
              className="w-80 [&>button]:w-full"
            />
          </div>
        </SettingContainer>
        <SettingContainer
          title={t("settings.files.transcribe.choose.title")}
          description={t("settings.files.transcribe.choose.description")}
          descriptionMode="tooltip"
          grouped={true}
        >
          <Button
            variant="primary"
            onClick={chooseFile}
            disabled={running || !model?.is_downloaded}
          >
            {t("settings.files.transcribe.choose.button")}
          </Button>
        </SettingContainer>
        {status()}
        <SettingContainer
          title={t("settings.files.options.saveText.title")}
          description={t("settings.files.options.saveText.description")}
          descriptionMode="tooltip"
          grouped={true}
        >
          <Dropdown
            options={SAVE_MODES.map((value) => ({
              value,
              label: t(`settings.files.options.saveText.options.${value}`),
            }))}
            selectedValue={saveMode}
            onSelect={(value) =>
              updateSetting("file_text_save", value as FileTextSave)
            }
            disabled={isUpdating("file_text_save")}
          />
        </SettingContainer>
        <ToggleSwitch
          checked={keepAudio}
          onChange={(value) => updateSetting("file_keep_audio", value)}
          isUpdating={isUpdating("file_keep_audio")}
          label={t("settings.files.options.keepAudio.title")}
          description={t("settings.files.options.keepAudio.description")}
          descriptionMode="tooltip"
          grouped={true}
        />
        {(keepAudio || saveMode === "ask" || saveMode === "handy_folder") && (
          <SettingContainer
            title={t("settings.files.options.folder.title")}
            description={t("settings.files.options.folder.description")}
            descriptionMode="tooltip"
            grouped={true}
            layout="stacked"
          >
            <div className="flex items-center gap-2">
              <div className="flex-1 min-w-0 px-2 py-2 bg-surface2 border border-border rounded-lg text-xs font-mono break-all select-text cursor-text">
                {folder}
              </div>
              {chosenFolder && (
                <ResetButton
                  onClick={() => updateSetting("files_folder", "")}
                  ariaLabel={t("settings.files.options.folder.reset")}
                />
              )}
              <Button variant="secondary" size="sm" onClick={chooseFolder}>
                {t("settings.files.options.folder.change")}
              </Button>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => void commands.openFilesFolder()}
              >
                {t("common.open")}
              </Button>
            </div>
          </SettingContainer>
        )}
      </SettingsGroup>
      {pendingFile && (
        <SaveTextDialog
          file={pendingFile}
          folder={folder}
          onChoose={onDialogChoice}
          onCancel={closeDialog}
        />
      )}
    </>
  );
};

/** One transcribed file: name, when, length and model, and its text. */
const LogEntry: React.FC<{
  entry: FileTranscription;
  onDelete: () => void;
}> = ({ entry, onDelete }) => {
  const { t, i18n } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const when = new Intl.DateTimeFormat(i18n.language, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(entry.id));
  const long = entry.text.length > 280;
  return (
    <div className="px-4 py-3 flex flex-col gap-2">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p
            className="text-sm font-semibold truncate"
            title={entry.source_path}
          >
            {entry.file_name}
          </p>
          <p className="text-[13px] text-text-secondary">
            {[when, clock(entry.duration_secs), entry.model].join(" · ")}
          </p>
        </div>
        <div className="flex items-center gap-1 shrink-0">
          {entry.text && (
            <CopyButton
              text={entry.text}
              label={t("settings.files.transcribe.copy")}
            />
          )}
          {entry.text_path && (
            <button
              type="button"
              className={ICON_BUTTON}
              title={t("settings.files.log.reveal")}
              aria-label={t("settings.files.log.reveal")}
              onClick={() => void revealItemInDir(entry.text_path!)}
            >
              <FolderOpen width={16} height={16} />
            </button>
          )}
          <button
            type="button"
            className={`${ICON_BUTTON} hover:!bg-err-bg hover:!text-err-text`}
            title={t("settings.files.log.delete")}
            aria-label={t("settings.files.log.delete")}
            onClick={onDelete}
          >
            <Trash2 width={16} height={16} />
          </button>
        </div>
      </div>
      <p
        className={`text-sm select-text cursor-text whitespace-pre-wrap ${expanded ? "" : "line-clamp-3"}`}
      >
        {entry.text || t("settings.files.transcribe.empty")}
      </p>
      {long && (
        <button
          type="button"
          className="self-start text-[13px] font-medium text-accent-text hover:underline cursor-pointer"
          onClick={() => setExpanded((v) => !v)}
        >
          {expanded
            ? t("settings.files.log.showLess")
            : t("settings.files.log.showMore")}
        </button>
      )}
    </div>
  );
};

/**
 * Files: transcribe an audio file you pick, the list of files transcribed so
 * far, and the watched folders (moved here from More › Translator).
 */
export const FilesPage: React.FC = () => {
  const { t } = useTranslation();
  const [log, setLog] = useState<FileTranscription[]>([]);

  const loadLog = React.useCallback(() => {
    commands.getFileTranscriptions().then(setLog);
  }, []);
  useEffect(loadLog, [loadLog]);

  const deleteEntry = async (id: number) => {
    const result = await commands.deleteFileTranscription(id);
    if (result.status === "error") toast.error(result.error);
    loadLog();
  };

  return (
    <div className="w-full space-y-6">
      <TranscribeFile onFinished={loadLog} />
      <TranslatorSettings />
      <SettingsGroup icon={ListOrdered} title={t("settings.files.log.title")}>
        {log.length === 0 ? (
          <p className="px-4 py-3 text-sm text-text-secondary">
            {t("settings.files.log.empty")}
          </p>
        ) : (
          log.map((entry) => (
            <LogEntry
              key={entry.id}
              entry={entry}
              onDelete={() => void deleteEntry(entry.id)}
            />
          ))
        )}
      </SettingsGroup>
    </div>
  );
};
