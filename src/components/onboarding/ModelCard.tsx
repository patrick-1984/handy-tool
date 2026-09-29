import React from "react";
import { useTranslation } from "react-i18next";
import { Check, Download, Loader2, Trash2 } from "lucide-react";
import type { ModelInfo } from "@/bindings";
import { formatModelSize } from "../../lib/utils/format";
import {
  getTranslatedModelDescription,
  getTranslatedModelName,
} from "../../lib/utils/modelTranslation";
import { LANGUAGES } from "../../lib/constants/languages";
import { Button } from "../ui/Button";

// Get display text for model's language support
const getLanguageDisplayText = (
  supportedLanguages: string[],
  t: (key: string, options?: Record<string, unknown>) => string,
): string => {
  if (supportedLanguages.length === 1) {
    const langCode = supportedLanguages[0];
    const langName =
      LANGUAGES.find((l) => l.value === langCode)?.label || langCode;
    return t("modelSelector.capabilities.languageOnly", { language: langName });
  }
  return t("modelSelector.capabilities.multiLanguage");
};

export type ModelCardStatus =
  | "downloadable"
  | "downloading"
  | "extracting"
  | "switching"
  | "active"
  | "available";

interface ModelCardProps {
  model: ModelInfo;
  variant?: "default" | "featured";
  status?: ModelCardStatus;
  disabled?: boolean;
  className?: string;
  onSelect: (modelId: string) => void;
  onDownload?: (modelId: string) => void;
  onDelete?: (modelId: string) => void;
  onCancel?: (modelId: string) => void;
  downloadProgress?: number;
  downloadSpeed?: number; // MB/s
  showRecommended?: boolean;
  /** No buttons on the card (the setup picks with a click, downloads below). */
  hideActions?: boolean;
  /** In a picker (the setup): a radio at the top end; picked = accent edge and tint. */
  picked?: boolean;
}

/** A score as five segments, filled in cyan up to it. */
const ScoreBar: React.FC<{ label: string; score: number }> = ({
  label,
  score,
}) => {
  const filled = Math.round(Math.max(0, Math.min(1, score)) * 5);
  return (
    <span className="flex items-center gap-2">
      <span className="min-w-16 text-xs text-text-secondary capitalize">
        {label}
      </span>
      <span className="flex gap-[3px]" aria-label={`${label} ${filled}/5`}>
        {Array.from({ length: 5 }, (_, i) => (
          <span
            key={i}
            className={`h-1 w-2.5 rounded-full ${i < filled ? "bg-accent" : "bg-control-bottom"}`}
          />
        ))}
      </span>
    </span>
  );
};

/** A small badge at the card's top end. */
const Tag: React.FC<{
  tone?: "accent" | "plain";
  children: React.ReactNode;
}> = ({ tone = "plain", children }) => (
  <span
    className={`inline-flex items-center gap-1 h-6 px-2 rounded-full text-xs font-medium ${
      tone === "accent"
        ? "bg-accent-soft text-accent-text"
        : "bg-surface2 text-text-secondary"
    }`}
  >
    {children}
  </span>
);

/**
 * A model as a card of the redesign: name, size and languages, a one-line
 * description, accuracy and speed as five-segment bars, and its actions. The
 * active model has a 2px cyan edge and an "Active" badge; a download shows its
 * progress along the bottom.
 */
const ModelCard: React.FC<ModelCardProps> = ({
  model,
  variant = "default",
  status = "downloadable",
  disabled = false,
  className = "",
  onSelect,
  onDownload,
  onDelete,
  onCancel,
  downloadProgress,
  downloadSpeed,
  showRecommended = true,
  hideActions = false,
  picked,
}) => {
  const { t } = useTranslation();
  const isFeatured = variant === "featured";
  const isClickable =
    status === "available" || status === "active" || status === "downloadable";

  const displayName = getTranslatedModelName(model, t);
  const displayDescription = getTranslatedModelDescription(model, t);
  const meta = [
    formatModelSize(Number(model.size_mb)),
    model.supported_languages.length > 0
      ? getLanguageDisplayText(model.supported_languages, t)
      : null,
    model.supports_translation
      ? t("modelSelector.capabilities.translate")
      : null,
  ]
    .filter(Boolean)
    .join(" · ");

  const handleClick = () => {
    if (!isClickable || disabled) return;
    if (status === "downloadable" && onDownload) {
      onDownload(model.id);
    } else {
      onSelect(model.id);
    }
  };

  const stop =
    (action: () => void) => (e: React.MouseEvent | React.KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      action();
    };

  const edge =
    picked !== undefined
      ? picked
        ? "border border-accent shadow-[inset_0_0_0_1px_var(--color-accent)] bg-accent-soft p-4"
        : "border border-border bg-surface shadow-card p-4"
      : status === "active"
        ? "border-2 border-accent bg-surface shadow-card p-[15px]"
        : isFeatured
          ? "border-2 border-accent/40 bg-surface shadow-card p-[15px]"
          : "border border-border bg-surface shadow-card p-4";

  return (
    <div
      // Titled like a setting, so What's new can point at a model.
      data-setting-title={displayName}
      onClick={handleClick}
      onKeyDown={(e) => {
        if (e.key === "Enter" && isClickable) handleClick();
      }}
      role={isClickable ? "button" : undefined}
      aria-pressed={picked}
      tabIndex={isClickable ? 0 : undefined}
      className={`flex flex-col gap-3 rounded-lg text-start transition-colors duration-150 ${edge} ${
        isClickable && !disabled
          ? "cursor-pointer hover:border-accent/60"
          : disabled
            ? "cursor-not-allowed"
            : ""
      } ${className}`}
    >
      {/* Name and facts, badges at the end */}
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="text-[15px] leading-5 font-semibold text-text">
            {displayName}
          </h3>
          <p className="text-[13px] leading-[18px] text-text-secondary">
            {meta}
          </p>
        </div>
        <div className="flex flex-wrap justify-end gap-1.5 shrink-0">
          {status === "active" && (
            <Tag tone="accent">
              <Check className="w-3 h-3" />
              {t("modelSelector.active")}
            </Tag>
          )}
          {status === "switching" && (
            <Tag>
              <Loader2 className="w-3 h-3 animate-spin" />
              {t("modelSelector.switching")}
            </Tag>
          )}
          {showRecommended && model.is_recommended && status !== "active" && (
            <Tag tone="accent">{t("onboarding.recommended")}</Tag>
          )}
          {model.is_custom && <Tag>{t("modelSelector.custom")}</Tag>}
          {picked !== undefined && (
            <span
              className={`mt-0.5 flex items-center justify-center w-[18px] h-[18px] rounded-full border-[1.5px] ${picked ? "border-accent" : "border-control-bottom"}`}
              aria-hidden
            >
              {picked && <span className="w-2 h-2 rounded-full bg-accent" />}
            </span>
          )}
        </div>
      </div>

      {(model.accuracy_score > 0 || model.speed_score > 0) && (
        <div className="flex flex-wrap items-center gap-x-6 gap-y-1">
          <ScoreBar
            label={t("onboarding.modelCard.accuracy")}
            score={model.accuracy_score}
          />
          <ScoreBar
            label={t("onboarding.modelCard.speed")}
            score={model.speed_score}
          />
        </div>
      )}

      {displayDescription && (
        <p className="text-[13px] leading-[18px] text-text-secondary line-clamp-2">
          {status === "active" ? t("modelSelector.inUse") : displayDescription}
        </p>
      )}

      {/* Download progress, or the card's actions */}
      {status === "downloading" && downloadProgress !== undefined ? (
        <div className="flex flex-col gap-2">
          <div className="w-full h-1 bg-control-bottom rounded-full overflow-hidden">
            <div
              className="h-full bg-accent rounded-full transition-all duration-300"
              style={{ width: `${downloadProgress}%` }}
            />
          </div>
          <div className="flex items-center justify-between gap-2 text-xs text-text-secondary">
            <span className="tabular-nums">
              {t("modelSelector.downloading", {
                percentage: Math.round(downloadProgress),
              })}
              {downloadSpeed !== undefined &&
                downloadSpeed > 0 &&
                ` · ${t("modelSelector.downloadSpeed", {
                  speed: downloadSpeed.toFixed(1),
                })}`}
            </span>
            {onCancel && (
              <Button
                variant="secondary"
                size="sm"
                onClick={stop(() => onCancel(model.id))}
                aria-label={t("modelSelector.cancelDownload")}
              >
                {t("modelSelector.cancel")}
              </Button>
            )}
          </div>
        </div>
      ) : status === "extracting" ? (
        <div className="flex flex-col gap-2">
          <div className="w-full h-1 bg-control-bottom rounded-full overflow-hidden">
            <div className="h-full bg-accent rounded-full animate-pulse w-full" />
          </div>
          <p className="text-xs text-text-secondary">
            {t("modelSelector.extractingGeneric")}
          </p>
        </div>
      ) : (
        !hideActions &&
        (status === "downloadable" ||
          status === "available" ||
          (status === "active" && onDelete)) && (
          <div className="flex items-center gap-2">
            {status === "downloadable" && (
              <Button
                variant="secondary"
                size="sm"
                disabled={disabled}
                onClick={stop(() => onDownload?.(model.id))}
              >
                <Download className="w-3.5 h-3.5" />
                {t("onboarding.download")}
              </Button>
            )}
            {status === "available" && (
              <Button
                variant="secondary"
                size="sm"
                disabled={disabled}
                onClick={stop(() => onSelect(model.id))}
              >
                {t("modelSelector.useModel")}
              </Button>
            )}
            {onDelete && (status === "available" || status === "active") && (
              <button
                type="button"
                onClick={stop(() => onDelete(model.id))}
                title={t("modelSelector.deleteModel", {
                  modelName: displayName,
                })}
                aria-label={t("modelSelector.deleteModel", {
                  modelName: displayName,
                })}
                className="ms-auto inline-flex items-center justify-center h-7 w-7 rounded-md text-text-secondary hover:bg-err-bg hover:text-err-text cursor-pointer transition-colors"
              >
                <Trash2 className="w-3.5 h-3.5" />
              </button>
            )}
          </div>
        )
      )}
    </div>
  );
};

export default ModelCard;
