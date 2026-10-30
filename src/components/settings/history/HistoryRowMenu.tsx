import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { LucideIcon } from "lucide-react";
import { MoreHorizontal } from "lucide-react";

export interface RowMenuItem {
  key: string;
  icon: LucideIcon;
  label: string;
  /** One line on what it does, shown under the label. */
  hint: string;
  onSelect: () => void;
  danger?: boolean;
  disabled?: boolean;
}

/** The "⋯" of a History row: its less frequent actions, each with an icon
 *  and a one-line explanation. Closes on choosing, Escape or a click outside. */
export const HistoryRowMenu: React.FC<{
  items: RowMenuItem[];
  buttonClassName: string;
}> = ({ items, buttonClassName }) => {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node))
        setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className="relative" ref={ref}>
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className={buttonClassName}
        title={t("settings.history.menu.more")}
        aria-label={t("settings.history.menu.more")}
        aria-haspopup="menu"
        aria-expanded={open}
      >
        <MoreHorizontal width={16} height={16} />
      </button>
      {open && (
        <div
          role="menu"
          className="absolute end-0 top-full mt-1 z-40 w-72 rounded-lg border border-border bg-surface shadow-float py-1"
        >
          {items.map((item) => (
            <button
              key={item.key}
              type="button"
              role="menuitem"
              disabled={item.disabled}
              onClick={() => {
                setOpen(false);
                item.onSelect();
              }}
              className={`w-full flex items-start gap-2.5 px-3 py-2 text-start transition-colors cursor-pointer disabled:cursor-not-allowed disabled:opacity-50 ${
                item.danger ? "hover:bg-err-bg" : "hover:bg-hover"
              }`}
            >
              <item.icon
                className={`w-4 h-4 mt-0.5 shrink-0 ${item.danger ? "text-err-text" : "text-text-secondary"}`}
                aria-hidden
              />
              <span className="min-w-0">
                <span
                  className={`block text-sm ${item.danger ? "text-err-text" : "text-text"}`}
                >
                  {item.label}
                </span>
                <span className="block text-xs leading-4 text-text-secondary">
                  {item.hint}
                </span>
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
};
