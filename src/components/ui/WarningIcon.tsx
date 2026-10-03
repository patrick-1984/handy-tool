import React, { useRef, useState } from "react";
import { CircleAlert, CircleHelp } from "lucide-react";
import { Tooltip } from "./Tooltip";

/** Long enough to move the mouse across the gap into the tooltip. */
const CLOSE_DELAY_MS = 250;

/**
 * A hint that explains itself on hover or click: a question mark when
 * something may not work, an exclamation mark when it needs a change to work.
 * Both are amber - a risk of not working, not an alarm. The whole square
 * is the hover target — a bare icon only reacts on its thin strokes, so its
 * explanation was easy to miss — and the explanation uses the app's own tooltip
 * like the (i) icons, instead of the slow native one. With an `action`, the
 * tooltip carries a button and stays open while the mouse moves onto it.
 */
export const WarningIcon: React.FC<{
  message: string;
  className?: string;
  action?: { label: string; onClick: () => void };
  /** A word next to the icon ("Attention"): a badge, not colour alone. */
  label?: string;
  /** "maybe" (?): it may not work. "change" (!): it needs a change to work. */
  kind?: "maybe" | "change";
}> = ({ message, className = "", action, label, kind = "maybe" }) => {
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
  const iconClass = `${label ? "h-3.5 w-3.5" : "h-4 w-4"} shrink-0 pointer-events-none`;
  return (
    <span
      ref={ref}
      role="img"
      aria-label={message}
      className={`inline-flex items-center justify-center cursor-help ${
        label
          ? "gap-1 h-6 px-2 rounded-sm text-xs font-medium bg-warn-bg text-warn-text"
          : "h-6 w-6 text-warn-text"
      } ${className}`}
      onMouseEnter={show}
      onMouseLeave={hide}
      onClick={() => setOpen((o) => !o)}
    >
      {kind === "change" ? (
        <CircleAlert className={iconClass} />
      ) : (
        <CircleHelp className={iconClass} />
      )}
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
