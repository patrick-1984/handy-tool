import React, { useRef, useState } from "react";
import { AlertTriangle } from "lucide-react";
import { Tooltip } from "./Tooltip";

/**
 * A warning triangle that explains itself on hover or click. The whole square
 * is the hover target — a bare icon only reacts on its thin strokes, so its
 * explanation was easy to miss — and the explanation uses the app's own tooltip
 * like the (i) icons, instead of the slow native one.
 */
export const WarningIcon: React.FC<{ message: string; className: string }> = ({
  message,
  className,
}) => {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLSpanElement>(null);
  return (
    <span
      ref={ref}
      role="img"
      aria-label={message}
      className={`inline-flex items-center justify-center h-6 w-6 cursor-help ${className}`}
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
      onClick={() => setOpen((o) => !o)}
    >
      <AlertTriangle className="h-4 w-4 shrink-0 pointer-events-none" />
      {open && (
        <Tooltip targetRef={ref} position="top">
          <p className="text-sm leading-relaxed">{message}</p>
        </Tooltip>
      )}
    </span>
  );
};
