import React, { useRef, useState } from "react";
import { AlertTriangle } from "lucide-react";
import { Tooltip } from "./Tooltip";

/** Long enough to move the mouse across the gap into the tooltip. */
const CLOSE_DELAY_MS = 250;

/**
 * A warning triangle that explains itself on hover or click. The whole square
 * is the hover target — a bare icon only reacts on its thin strokes, so its
 * explanation was easy to miss — and the explanation uses the app's own tooltip
 * like the (i) icons, instead of the slow native one. With an `action`, the
 * tooltip carries a button and stays open while the mouse moves onto it.
 */
export const WarningIcon: React.FC<{
  message: string;
  className?: string;
  action?: { label: string; onClick: () => void };
  /** A word next to the triangle ("Conflict"): a badge, not colour alone. */
  label?: string;
  /** Badge colours: amber for a risk, red for a real clash. */
  tone?: "warn" | "error";
}> = ({ message, className = "", action, label, tone = "warn" }) => {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLSpanElement>(null);
  const closeTimer = useRef<number | undefined>(undefined);
  const show = () => {
    window.clearTimeout(closeTimer.current);
    setOpen(true);
  };
  // The tooltip is a portal but still a React child, so entering it counts as
  // entering this icon again and cancels the close.
  const hide = () => {
    window.clearTimeout(closeTimer.current);
    if (action) {
      closeTimer.current = window.setTimeout(
        () => setOpen(false),
        CLOSE_DELAY_MS,
      );
    } else {
      setOpen(false);
    }
  };
  return (
    <span
      ref={ref}
      role="img"
      aria-label={message}
      className={`inline-flex items-center justify-center cursor-help ${
        label
          ? `gap-1 h-6 px-2 rounded-sm text-xs font-medium ${tone === "error" ? "bg-err-bg text-err-text" : "bg-warn-bg text-warn-text"}`
          : "h-6 w-6"
      } ${className}`}
      onMouseEnter={show}
      onMouseLeave={hide}
      onClick={() => setOpen((o) => !o)}
    >
      <AlertTriangle
        className={`${label ? "h-3.5 w-3.5" : "h-4 w-4"} shrink-0 pointer-events-none`}
      />
      {label}
      {open && (
        <Tooltip targetRef={ref} position="top">
          <p className="text-[13px] leading-[18px]">{message}</p>
          {action && (
            <button
              type="button"
              className="mt-2.5 h-7 px-2.5 rounded-md border border-control-border border-b-control-bottom bg-control text-[13px] font-medium hover:bg-control-hover cursor-pointer"
              onClick={(e) => {
                e.stopPropagation();
                setOpen(false);
                action.onClick();
              }}
            >
              {action.label}
            </button>
          )}
        </Tooltip>
      )}
    </span>
  );
};
