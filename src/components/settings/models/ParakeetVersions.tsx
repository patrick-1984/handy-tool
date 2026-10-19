import React, { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { ask } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { Check, Download, Loader2, Trash2, X } from "lucide-react";
import { Button } from "../../ui/Button";
import { SectionTitle } from "../../ui/SettingsGroup";
import { useModelStore } from "../../../stores/modelStore";
import { getTranslatedModelName } from "../../../lib/utils/modelTranslation";
import { formatModelSize } from "../../../lib/utils/format";
import { commands, type ModelInfo } from "@/bindings";
import { HuggingFaceParakeet } from "./HuggingFaceParakeet";

/** Handy's own Parakeet models (built in, from Handy's server). */
export const BUILT_IN_PARAKEET = [
  "parakeet-tdt-0.6b-v3",
  "parakeet-tdt-0.6b-v2",
];

/** Versions worth knowing about, on Hugging Face. Each is the moondream
 *  "parakeet-ultra" post-training of Parakeet TDT 0.6B v3 (same architecture
 *  and tokenizer, better accuracy), in the layout Handy's engine reads. */
const SUGGESTED: { repo: string; key: string; sizeMb: number }[] = [
  { repo: "Masterx/parakeet-tdt-0.6b-ultra-onnx", key: "ultra", sizeMb: 640 },
  {
    repo: "VoiceVibe/parakeet-ultra-nounk-onnx",
    key: "ultraNoUnk",
    sizeMb: 2430,
  },
];

const hfId = (repo: string) => `hf-${repo.replace("/", "--")}`;

/** A Parakeet model, any version: built in or from Hugging Face. */
export const isParakeetVersion = (m: ModelInfo) => m.engine_type === "Parakeet";

interface RowProps {
  title: string;
  detail: string;
  model?: ModelInfo;
  /** Not added yet: what Add does. */
  onAdd?: () => void;
  adding?: boolean;
}

/** One version: its name, then In use / Use / Download (with progress) /
 *  Add, and Delete for one from Hugging Face. */
const VersionRow: React.FC<RowProps> = ({
  title,
  detail,
  model,
  onAdd,
  adding,
}) => {
  const { t } = useTranslation();
  const currentModel = useModelStore((s) => s.currentModel);
  const downloading = useModelStore((s) => s.downloadingModels);
  const extracting = useModelStore((s) => s.extractingModels);
  const progress = useModelStore((s) => s.downloadProgress);
  const { selectModel, downloadModel, cancelDownload, deleteModel } =
    useModelStore();

  const id = model?.id;
  const inUse = id !== undefined && id === currentModel;
  const isDownloading = id !== undefined && id in downloading;
  const isExtracting = id !== undefined && id in extracting;

  const remove = async () => {
    if (!model) return;
    const confirmed = await ask(
      t("settings.models.deleteConfirm", { modelName: title }),
      { title: t("settings.models.deleteTitle"), kind: "warning" },
    );
    if (confirmed) await deleteModel(model.id);
  };

  return (
    <div
      className={`flex items-center justify-between gap-3 px-3 py-2.5 ${inUse ? "bg-accent-soft" : ""}`}
    >
      <div className="min-w-0">
        <p className="text-sm font-medium text-text">{title}</p>
        <p className="text-xs text-text-secondary">{detail}</p>
      </div>
      <div className="flex items-center gap-1.5 shrink-0">
        {inUse && (
          <span className="inline-flex items-center gap-1 text-xs font-medium text-accent-text">
            <Check className="w-3.5 h-3.5" />
            {t("settings.models.parakeet.inUse")}
          </span>
        )}
        {!model && onAdd && (
          <Button
            variant="secondary"
            size="sm"
            onClick={onAdd}
            disabled={adding}
          >
            {adding ? (
              <Loader2 className="w-3.5 h-3.5 animate-spin" />
            ) : (
              <Download className="w-3.5 h-3.5" />
            )}
            {t("settings.models.huggingFace.add")}
          </Button>
        )}
        {model && isDownloading && (
          <>
            <span className="text-xs tabular-nums text-text-secondary">
              {t("modelSelector.downloading", {
                percentage: Math.round(progress[model.id]?.percentage ?? 0),
              })}
            </span>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => void cancelDownload(model.id)}
            >
              {t("modelSelector.cancel")}
            </Button>
          </>
        )}
        {model && isExtracting && (
          <span className="text-xs text-text-secondary">
            {t("modelSelector.extractingGeneric")}
          </span>
        )}
        {model && !isDownloading && !isExtracting && !model.is_downloaded && (
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void downloadModel(model.id)}
          >
            <Download className="w-3.5 h-3.5" />
            {t("onboarding.download")}
          </Button>
        )}
        {model && model.is_downloaded && !inUse && (
          <Button
            variant="primary"
            size="sm"
            onClick={() => void selectModel(model.id)}
          >
            {t("settings.models.parakeet.use")}
          </Button>
        )}
        {model?.is_custom && !isDownloading && (
          <button
            type="button"
            onClick={() => void remove()}
            title={t("modelSelector.deleteModel", { modelName: title })}
            aria-label={t("modelSelector.deleteModel", { modelName: title })}
            className="inline-flex items-center justify-center h-7 w-7 rounded-md text-text-secondary hover:bg-err-bg hover:text-err-text cursor-pointer transition-colors"
          >
            <Trash2 className="w-3.5 h-3.5" />
          </button>
        )}
      </div>
    </div>
  );
};

/** Models › Parakeet › versions: which Parakeet runs - built in, or a newer
 *  or fine-tuned one from Hugging Face. Opened from a Parakeet card. */
export const ParakeetVersions: React.FC<{ onClose: () => void }> = ({
  onClose,
}) => {
  const { t } = useTranslation();
  const models = useModelStore((s) => s.models);
  const [adding, setAdding] = useState<string | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const byId = (id: string) => models.find((m) => m.id === id);
  const builtIn = BUILT_IN_PARAKEET.map(byId).filter(
    (m): m is ModelInfo => m !== undefined,
  );
  const suggestedIds = new Set(SUGGESTED.map((s) => hfId(s.repo)));
  const added = models.filter(
    (m) =>
      m.id.startsWith("hf-") && isParakeetVersion(m) && !suggestedIds.has(m.id),
  );

  const add = async (repo: string) => {
    setAdding(repo);
    const result = await commands.addHfParakeetModel(repo);
    if (result.status === "error") {
      setAdding(null);
      toast.error(result.error);
      return;
    }
    await useModelStore.getState().loadModels();
    setAdding(null);
    await useModelStore.getState().downloadModel(result.data.id);
  };

  const detailOf = (m: ModelInfo) =>
    [formatModelSize(Number(m.size_mb)), m.url?.replace("hf://", "")]
      .filter(Boolean)
      .join(" · ");

  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-scrim p-4"
      onClick={onClose}
      role="presentation"
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="parakeet-versions-title"
        className="w-[640px] max-w-full max-h-[85vh] flex flex-col rounded-lg bg-surface border border-border shadow-float overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start justify-between gap-3 px-5 pt-4 pb-3 border-b border-border">
          <div>
            <h2
              id="parakeet-versions-title"
              className="text-[15px] font-semibold text-text"
            >
              {t("settings.models.parakeet.title")}
            </h2>
            <p className="text-[13px] leading-[18px] text-text-secondary">
              {t("settings.models.parakeet.description")}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label={t("settings.models.parakeet.close")}
            className="inline-flex items-center justify-center h-7 w-7 rounded-md text-text-secondary hover:bg-hover cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
        <div className="overflow-y-auto px-5 py-4 space-y-5">
          <div className="space-y-2">
            <SectionTitle title={t("settings.models.parakeet.builtIn")} />
            <div className="card divide-y divide-border">
              {builtIn.map((m) => (
                <VersionRow
                  key={m.id}
                  model={m}
                  title={getTranslatedModelName(m, t)}
                  detail={[
                    formatModelSize(Number(m.size_mb)),
                    t(
                      `settings.models.parakeet.builtInDetail.${m.id === "parakeet-tdt-0.6b-v2" ? "v2" : "v3"}`,
                    ),
                  ].join(" · ")}
                />
              ))}
            </div>
          </div>
          <div className="space-y-2">
            <SectionTitle
              title={t("settings.models.parakeet.fromHuggingFace")}
            />
            <div className="card divide-y divide-border">
              {SUGGESTED.map((s) => (
                <VersionRow
                  key={s.repo}
                  model={byId(hfId(s.repo))}
                  title={t(`settings.models.parakeet.suggested.${s.key}.title`)}
                  detail={[
                    formatModelSize(s.sizeMb),
                    t(`settings.models.parakeet.suggested.${s.key}.detail`),
                    s.repo,
                  ].join(" · ")}
                  onAdd={() => void add(s.repo)}
                  adding={adding === s.repo}
                />
              ))}
              {added.map((m) => (
                <VersionRow
                  key={m.id}
                  model={m}
                  title={getTranslatedModelName(m, t)}
                  detail={detailOf(m)}
                />
              ))}
            </div>
          </div>
          <HuggingFaceParakeet />
        </div>
      </div>
    </div>,
    document.body,
  );
};
