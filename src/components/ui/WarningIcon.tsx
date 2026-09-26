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
  className: string;
  action?: { label: string; onClick: () => void };
}> = ({ message, className, action }) => {
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
      className={`inline-flex items-center justify-center h-6 w-6 cursor-help ${className}`}
      onMouseEnter={show}
      onMouseLeave={hide}
      onClick={() => setOpen((o) => !o)}
    >
      <AlertTriangle className="h-4 w-4 shrink-0 pointer-events-none" />
      {open && (
        <Tooltip targetRef={ref} position="top">
          <p className="text-sm leading-relaxed">{message}</p>
          {action && (
            <button
              type="button"
              className="mt-2 text-sm font-medium text-logo-primary hover:underline cursor-pointer"
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
