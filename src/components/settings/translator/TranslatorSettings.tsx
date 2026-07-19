import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { FolderPlus, Trash2 } from "lucide-react";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { Dropdown } from "../../ui/Dropdown";
import { Button } from "../../ui/Button";
import { useSettings } from "../../../hooks/useSettings";
import { useSettingsStore } from "../../../stores/settingsStore";
import { useModelStore } from "../../../stores/modelStore";
import { getTranslatedModelName } from "../../../lib/utils/modelTranslation";
import {
  commands,
  type Result,
  type TranslatorPriority,
  type TranslatorStatus,
} from "@/bindings";

/**
 * Translator: watch folders and batch-transcribe new audio files into a .txt
 * next to each recording, sharing the engine with live dictation according to
 * the selected priority policy. Replaces the external FLMTray folder watch.
 */
export const TranslatorSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating, settings } = useSettings();
  const refreshSettings = useSettingsStore((s) => s.refreshSettings);
  const [status, setStatus] = useState<TranslatorStatus | null>(null);
  // Folder paths with an in-flight toggle/remove operation — used to disable
  // that row's controls and prevent overlapping requests against the same
  // folder (T-109).
  const [pendingFolders, setPendingFolders] = useState<Set<string>>(new Set());

  useEffect(() => {
    // Idempotent — populates the model list for the batch-model picker even
    // when the Models page hasn't been opened yet.
    useModelStore.getState().initialize();
    let disposed = false;
    // The `translator-status` event can arrive before the initial fetch
    // resolves (the worker publishes on every tick); without this guard the
    // late-resolving fetch would overwrite a newer event with a stale
    // snapshot.
    let eventArrived = false;
    commands.getTranslatorStatus().then((r) => {
      if (!disposed && !eventArrived && r.status === "ok") setStatus(r.data);
    });
    const unlisten = listen<TranslatorStatus>("translator-status", (e) => {
      eventArrived = true;
      setStatus(e.payload);
    });
    return () => {
      disposed = true;
      unlisten.then((f) => f());
    };
  }, []);

  const enabled = getSetting("translator_enabled") ?? false;
  const priority = (getSetting("translator_priority") ||
    "live_first") as TranslatorPriority;
  const translatorModel = (getSetting("translator_model") as string) ?? "";
  const folders = settings?.translator_folders ?? [];
  const models = useModelStore((s) => s.models);

  const priorityOptions = (
    ["live_first", "folder_first", "fifo"] as const
  ).map((value) => ({
    value,
    label: t(`settings.translator.priority.options.${value}`),
  }));

  // Only transcription-capable models are offered (the same registry the
  // Models page shows) — an LLM chat provider can't transcribe audio. Models
  // that can also translate to English are marked.
  const modelOptions = [
    { value: "", label: t("settings.translator.model.sameAsDictation") },
    ...models
      .filter((m) => m.is_downloaded)
      .map((m) => ({
        value: m.id,
        label: m.supports_translation
          ? t("settings.translator.model.translates", {
              name: getTranslatedModelName(m, t),
            })
          : getTranslatedModelName(m, t),
      })),
  ];

  const addFolder = async () => {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked !== "string" || !picked) return;
    const result = await commands.translatorAddFolder(picked);
    if (result.status === "error") {
      toast.error(
        t("settings.translator.folders.addFailed", { reason: result.error }),
      );
    }
    await refreshSettings();
  };

  // Both row operations are keyed by folder PATH (not array index): the
  // backend resolves the entry itself, so two overlapping operations across
  // rows can never be reinterpreted against a shifted index (T-109).
  const withFolderPending = async (
    path: string,
    op: () => Promise<Result<null, string>>,
  ) => {
    setPendingFolders((prev) => new Set(prev).add(path));
    try {
      const result = await op();
      if (result.status === "error") {
        toast.error(
          t("settings.translator.folders.operationFailed", {
            reason: result.error,
          }),
        );
      }
    } finally {
      setPendingFolders((prev) => {
        const next = new Set(prev);
        next.delete(path);
        return next;
      });
      await refreshSettings();
    }
  };

  const setFolderEnabled = (path: string, value: boolean) =>
    withFolderPending(path, () => commands.translatorSetFolderEnabled(path, value));

  const removeFolder = (path: string) =>
    withFolderPending(path, () => commands.translatorRemoveFolder(path));

  const statusLine = () => {
    if (!status || !status.enabled) {
      return t("settings.translator.status.disabled");
    }
    if (status.current_file) {
      // A file can briefly be "current" before its segment count is known
      // (silent/near-silent audio yields zero speech segments) — never
      // render a transient "segment 1/0".
      const total = status.current_total_segments;
      const base =
        total > 0
          ? t("settings.translator.status.working", {
              file: status.current_file,
              segment: Math.min(status.current_segment + 1, total),
              total,
            })
          : t("settings.translator.status.workingIndeterminate", {
              file: status.current_file,
            });
      return status.paused_reason
        ? `${base} — ${t(`settings.translator.status.paused.${status.paused_reason}`)}`
        : base;
    }
    if (status.queue_len > 0) {
      return status.paused_reason
        ? t(`settings.translator.status.paused.${status.paused_reason}`)
        : t("settings.translator.status.queued", { count: status.queue_len });
    }
    return t("settings.translator.status.idle");
  };

  return (
    <div className="w-full space-y-6">
      <SettingsGroup title={t("settings.translator.title")}>
        <ToggleSwitch
          checked={enabled}
          onChange={(value) => updateSetting("translator_enabled", value)}
          isUpdating={isUpdating("translator_enabled")}
          label={t("settings.translator.enabled.label")}
          description={t("settings.translator.enabled.description")}
          descriptionMode="tooltip"
          grouped={true}
        />
        <SettingContainer
          title={t("settings.translator.priority.title")}
          description={t("settings.translator.priority.description")}
          descriptionMode="tooltip"
          grouped={true}
        >
          <Dropdown
            options={priorityOptions}
            selectedValue={priority}
            onSelect={(value) =>
              updateSetting("translator_priority", value as TranslatorPriority)
            }
            disabled={isUpdating("translator_priority")}
          />
        </SettingContainer>
        <SettingContainer
          title={t("settings.translator.model.title")}
          description={t("settings.translator.model.description")}
          descriptionMode="tooltip"
          grouped={true}
        >
          <Dropdown
            options={modelOptions}
            selectedValue={translatorModel}
            onSelect={(value) => updateSetting("translator_model", value)}
            disabled={isUpdating("translator_model")}
          />
        </SettingContainer>
        <SettingContainer
          title={t("settings.translator.status.title")}
          description={t("settings.translator.status.description")}
          descriptionMode="tooltip"
          grouped={true}
        >
          <div className="flex flex-col items-end gap-1 text-sm">
            <span>{statusLine()}</span>
            {status && (status.done_count > 0 || status.failed_count > 0) && (
              <span className="text-xs text-text/50">
                {t("settings.translator.status.counters", {
                  done: status.done_count,
                  failed: status.failed_count,
                })}
              </span>
            )}
          </div>
        </SettingContainer>
      </SettingsGroup>

      <SettingsGroup title={t("settings.translator.folders.title")}>
        {folders.length === 0 && (
          <SettingContainer
            title={t("settings.translator.folders.emptyTitle")}
            description={t("settings.translator.folders.emptyDescription")}
            descriptionMode="tooltip"
            grouped={true}
          >
            <span className="text-sm text-text/50">
              {t("settings.translator.folders.none")}
            </span>
          </SettingContainer>
        )}
        {folders.map((folder, index) => {
          const folderName = folder.path.split(/[\\/]/).pop() || folder.path;
          const rowBusy = pendingFolders.has(folder.path);
          return (
            <SettingContainer
              key={`${folder.path}-${index}`}
              title={folderName}
              description={folder.path}
              descriptionMode="tooltip"
              grouped={true}
            >
              <div className="flex items-center gap-3">
                <label className="inline-flex items-center cursor-pointer">
                  <input
                    type="checkbox"
                    className="sr-only peer"
                    checked={folder.enabled}
                    disabled={rowBusy}
                    aria-label={t("settings.translator.folders.toggleAriaLabel", {
                      name: folderName,
                    })}
                    onChange={(e) => setFolderEnabled(folder.path, e.target.checked)}
                  />
                  <div className="relative w-11 h-6 bg-mid-gray/20 peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-logo-primary rounded-full peer peer-checked:after:translate-x-full rtl:peer-checked:after:-translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-background-ui peer-disabled:opacity-50"></div>
                </label>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => removeFolder(folder.path)}
                  disabled={rowBusy}
                  title={t("settings.translator.folders.remove")}
                  aria-label={t("settings.translator.folders.removeAriaLabel", {
                    name: folderName,
                  })}
                  className="text-logo-primary/85 hover:text-logo-primary hover:bg-logo-primary/10"
                >
                  <Trash2 className="w-3.5 h-3.5" />
                </Button>
              </div>
            </SettingContainer>
          );
        })}
        <SettingContainer
          title={t("settings.translator.folders.addTitle")}
          description={t("settings.translator.folders.addDescription")}
          descriptionMode="tooltip"
          grouped={true}
        >
          <Button variant="secondary" size="sm" onClick={addFolder}>
            <span className="flex items-center gap-1.5">
              <FolderPlus className="w-3.5 h-3.5" />
              {t("settings.translator.folders.add")}
            </span>
          </Button>
        </SettingContainer>
      </SettingsGroup>
    </div>
  );
};
