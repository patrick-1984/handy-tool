import React from "react";
import { useTranslation } from "react-i18next";
import { CircleCheck, Download, KeyRound, X } from "lucide-react";
import { Button } from "../../ui/Button";

/** The step segments: done and current filled, the current one longer. */
export const StepSegments: React.FC<{ step: number; total: number }> = ({
  step,
  total,
}) => (
  <span className="flex gap-1" aria-hidden>
    {Array.from({ length: total }, (_, i) => (
      <span
        key={i}
        className={`h-1 rounded-[2px] ${i === step - 1 ? "w-7" : "w-5"} ${i < step ? "bg-accent" : "bg-control-bottom"}`}
      />
    ))}
  </span>
);

/**
 * The frame of a feature setup on the Setups page (Jumper, Post-processing):
 * its name, "Step N of M" with segments and Close setup above a divider; the
 * step's heading and body; and under a second divider Back / Skip / the main
 * button, as in the basic setup.
 */
export const SetupFrame: React.FC<{
  name: string;
  /** 1-based; 0 = a closing screen without the counter. */
  step: number;
  total: number;
  heading: string;
  body?: string;
  children?: React.ReactNode;
  onClose: () => void;
  onBack?: () => void;
  onSkip?: () => void;
  primary: string;
  onPrimary: () => void;
  primaryDisabled?: boolean;
}> = ({
  name,
  step,
  total,
  heading,
  body,
  children,
  onClose,
  onBack,
  onSkip,
  primary,
  onPrimary,
  primaryDisabled = false,
}) => {
  const { t } = useTranslation();
  return (
    <div className="w-full flex flex-col gap-6">
      <header className="flex flex-wrap items-center gap-3 pb-4 border-b border-border">
        <span className="text-[13px] font-semibold">{name}</span>
        {step > 0 && (
          <>
            <span className="text-[13px] text-text-secondary tabular-nums">
              {t("setup.step", { current: step, total })}
            </span>
            <StepSegments step={step} total={total} />
          </>
        )}
        <button
          type="button"
          onClick={onClose}
          className="ms-auto inline-flex items-center gap-1.5 h-8 px-2.5 rounded-md text-[13px] text-text-secondary hover:bg-hover hover:text-text transition-colors cursor-pointer"
        >
          <X className="w-3.5 h-3.5" aria-hidden />
          {t("setup.closeSetup")}
        </button>
      </header>

      <div className="flex flex-col gap-1.5">
        <h2 className="font-display text-[22px] leading-tight font-semibold">
          {heading}
        </h2>
        {body && (
          <p className="max-w-[620px] text-sm leading-[1.6] text-text-secondary">
            {body}
          </p>
        )}
      </div>

      {children}

      <footer className="flex items-center gap-2 pt-4 border-t border-border">
        {onBack && (
          <Button variant="secondary" onClick={onBack}>
            {t("setup.back")}
          </Button>
        )}
        <span className="flex-1" />
        {onSkip && (
          <Button variant="ghost" onClick={onSkip}>
            {t("setup.skip")}
          </Button>
        )}
        <Button
          variant="primary"
          onClick={onPrimary}
          disabled={primaryDisabled}
        >
          {primary}
        </Button>
      </footer>
    </div>
  );
};

/**
 * A choice as a card: a radio, a title and a line. Selected is the radio's dot,
 * an accent border and a soft tint together (never colour alone); hover only
 * darkens the border. `compact` is the provider card: smaller, the line in
 * monospace on one line.
 */
export const ChoiceCard: React.FC<{
  selected: boolean;
  title: string;
  detail?: string;
  onClick: () => void;
  badge?: React.ReactNode;
  compact?: boolean;
}> = ({ selected, title, detail, onClick, badge, compact = false }) => (
  <button
    type="button"
    role="radio"
    aria-checked={selected}
    onClick={onClick}
    className={`w-full flex items-start rounded-lg border text-start cursor-pointer transition-colors ${
      compact ? "gap-2.5 px-3 py-2.5" : "gap-3 px-4 py-3"
    } ${
      selected
        ? "border-accent shadow-[inset_0_0_0_1px_var(--color-accent)] bg-accent-soft"
        : "border-border bg-surface hover:border-control-bottom hover:bg-control-hover"
    }`}
  >
    <span
      className={`flex-none mt-0.5 flex items-center justify-center rounded-full border-[1.5px] ${
        compact ? "w-4 h-4" : "w-[18px] h-[18px]"
      } ${selected ? "border-accent" : "border-control-bottom"}`}
      aria-hidden
    >
      {selected && (
        <span
          className={`rounded-full bg-accent ${compact ? "w-1.5 h-1.5" : "w-2 h-2"}`}
        />
      )}
    </span>
    <span className="flex-1 flex flex-col gap-0.5 min-w-0">
      <span className="text-sm font-semibold">{title}</span>
      {detail &&
        (compact ? (
          <span className="font-mono text-xs text-text-secondary truncate">
            {detail}
          </span>
        ) : (
          <span className="text-[13px] leading-[1.5] text-text-secondary">
            {detail}
          </span>
        ))}
    </span>
    {badge}
  </button>
);

/**
 * A provider's status as a badge: an icon, a word and a tint, never the tint
 * alone. Needs a model is the only amber one, because it is the only one that
 * blocks.
 */
export const StatusBadge: React.FC<{
  status: "ready" | "key" | "model";
  label: string;
}> = ({ status, label }) => {
  const Icon =
    status === "ready" ? CircleCheck : status === "key" ? KeyRound : Download;
  const tone =
    status === "ready"
      ? "bg-accent-soft text-accent-text"
      : status === "key"
        ? "bg-surface2 text-text-secondary"
        : "bg-warn-bg text-warn-text";
  return (
    <span
      className={`flex-none inline-flex items-center gap-1 h-[22px] ps-[5px] pe-[7px] rounded text-xs font-semibold ${tone}`}
    >
      <Icon className="w-3 h-3" aria-hidden />
      {label}
    </span>
  );
};
