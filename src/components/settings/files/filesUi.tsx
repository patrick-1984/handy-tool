import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, Copy } from "lucide-react";
import { Button } from "../../ui/Button";

/** Icon buttons without a frame: 32 px targets, named by their tooltip. */
export const ICON_BUTTON =
  "inline-flex items-center justify-center h-8 w-8 rounded-md text-text-secondary hover:bg-hover hover:text-text transition-colors cursor-pointer";

/** 75.4 → "1:15", 3725 → "1:02:05". */
export const clock = (secs: number): string => {
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
export const CopyButton: React.FC<{
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

/** "Text saved to <path>", the path in monospace. */
export const SavedTo: React.FC<{ path: string }> = ({ path }) => {
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
