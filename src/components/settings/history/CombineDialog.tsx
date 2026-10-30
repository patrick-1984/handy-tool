import React, { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Check, Copy, GripVertical, X } from "lucide-react";
import { Button } from "../../ui/Button";
import { useSettings } from "../../../hooks/useSettings";
import type { CombineSeparator, HistoryEntry } from "@/bindings";
import {
  combineTakes,
  moveItem,
  sortedQueue,
  takeText,
  type Affixes,
  type QueueOrder,
} from "./combine";

const pad2 = (n: number) => String(n).padStart(2, "0");
const clock = (s: number) => {
  const t = Math.max(0, Math.round(s));
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  return h > 0 ? `${h}:${pad2(m)}:${pad2(t % 60)}` : `${m}:${pad2(t % 60)}`;
};

interface Props {
  /** The selected recordings, in the order they were clicked. */
  ids: number[];
  entries: HistoryEntry[];
  onRemove: (id: number) => void;
  onClose: () => void;
}

/** History › Combine: the selected recordings as one text. The queue starts
 *  in the order they were clicked; it can be sorted by time either way or
 *  rearranged by dragging a row by its grip. */
export const CombineDialog: React.FC<Props> = ({
  ids,
  entries,
  onRemove,
  onClose,
}) => {
  const { t, i18n } = useTranslation();
  const { getSetting } = useSettings();
  const byId = useMemo(() => new Map(entries.map((e) => [e.id, e])), [entries]);
  const [order, setOrder] = useState<QueueOrder>("custom");
  // The custom (clicked / dragged) order; new selections join at the end.
  const [custom, setCustom] = useState<number[]>(ids);
  useEffect(() => {
    setCustom((prev) => [
      ...prev.filter((id) => ids.includes(id)),
      ...ids.filter((id) => !prev.includes(id)),
    ]);
  }, [ids]);
  const queue = sortedQueue(custom, byId, order).filter((id) => byId.has(id));
  const [preferSpeakers, setPreferSpeakers] = useState(true);
  const [copied, setCopied] = useState(false);
  const anySpeakers = queue.some((id) => byId.get(id)?.diarized_text?.trim());

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  // Dragging by the grip (pointer events: the window's own file drop handling
  // keeps HTML drag-and-drop from working inside it).
  const rowRefs = useRef<(HTMLLIElement | null)[]>([]);
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  useEffect(() => {
    if (dragIndex === null) return;
    const onMove = (e: PointerEvent) => {
      // The place among the OTHER rows: the pointer passes a row's middle
      // to move past it; within its own row the dragged one stays put.
      const rows = rowRefs.current;
      let target = 0;
      rows.forEach((row, i) => {
        if (!row || i === dragIndex) return;
        const r = row.getBoundingClientRect();
        if (e.clientY > r.top + r.height / 2) target += 1;
      });
      if (target !== dragIndex) {
        setCustom((prev) => moveItem(prev, dragIndex, target));
        setDragIndex(target);
      }
    };
    const onUp = () => setDragIndex(null);
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
  }, [dragIndex]);

  const startDrag = (index: number) => {
    // Dragging works on the order shown: it becomes the custom order.
    setCustom(queue);
    setOrder("custom");
    setDragIndex(index);
  };

  const affixes = (flow: "output" | "combine"): Affixes => ({
    prefixEnabled: (getSetting(`${flow}_prefix_enabled`) as boolean) ?? false,
    prefixText: (getSetting(`${flow}_prefix_text`) as string) ?? "",
    prefixNewline: (getSetting(`${flow}_prefix_newline`) as boolean) ?? true,
    suffixEnabled: (getSetting(`${flow}_suffix_enabled`) as boolean) ?? false,
    suffixText: (getSetting(`${flow}_suffix_text`) as string) ?? "",
    suffixNewline: (getSetting(`${flow}_suffix_newline`) as boolean) ?? true,
  });

  const copy = async () => {
    const text = combineTakes(
      queue.map((id) => byId.get(id)!).filter(Boolean),
      {
        takeAffixes: (getSetting("combine_keep_take_affixes") as boolean)
          ? affixes("output")
          : null,
        combineAffixes: affixes("combine"),
        separator: ((getSetting("combine_separator") as CombineSeparator) ??
          "blank_line") as CombineSeparator,
        preferSpeakers,
      },
    );
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
      toast.success(
        t("settings.history.combine.copied", { count: queue.length }),
      );
    } catch (error) {
      toast.error(
        t("settings.history.combine.copyFailed", { error: String(error) }),
      );
    }
  };

  const when = (ts: number) =>
    new Intl.DateTimeFormat(i18n.language, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(ts * 1000));

  const orders: QueueOrder[] = ["custom", "oldest", "newest"];

  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-scrim p-4"
      onClick={onClose}
      role="presentation"
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="combine-title"
        className="w-[560px] max-w-full max-h-[85vh] flex flex-col rounded-lg bg-surface border border-border shadow-float overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start justify-between gap-3 px-5 pt-4 pb-3 border-b border-border">
          <div>
            <h2
              id="combine-title"
              className="text-[15px] font-semibold text-text"
            >
              {t("settings.history.combine.title", { count: queue.length })}
            </h2>
            <p className="text-[13px] leading-[18px] text-text-secondary">
              {t("settings.history.combine.description")}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label={t("settings.history.combine.close")}
            className="inline-flex items-center justify-center h-7 w-7 rounded-md text-text-secondary hover:bg-hover cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
        <div className="flex items-center justify-between gap-2 px-5 py-2.5 border-b border-border">
          <div
            className="inline-flex rounded-md border border-border p-0.5 text-xs"
            role="group"
          >
            {orders.map((o) => (
              <button
                key={o}
                type="button"
                onClick={() => setOrder(o)}
                aria-pressed={order === o}
                className={`px-2 py-0.5 rounded cursor-pointer transition-colors ${
                  order === o
                    ? "bg-accent-soft text-accent-text font-medium"
                    : "text-text-secondary hover:bg-hover"
                }`}
              >
                {t(`settings.history.combine.order.${o}`)}
              </button>
            ))}
          </div>
          {anySpeakers && (
            <label className="inline-flex items-center gap-1.5 text-xs text-text-secondary cursor-pointer">
              <input
                type="checkbox"
                checked={preferSpeakers}
                onChange={(e) => setPreferSpeakers(e.target.checked)}
                className="accent-[var(--color-accent)]"
              />
              {t("settings.history.combine.useSpeakers")}
            </label>
          )}
        </div>
        <ol className="overflow-y-auto px-3 py-2 space-y-1.5 select-none">
          {queue.map((id, index) => {
            const entry = byId.get(id)!;
            return (
              <li
                key={id}
                ref={(el) => {
                  rowRefs.current[index] = el;
                }}
                className={`flex items-start gap-2 rounded-md border px-2 py-1.5 ${
                  dragIndex === index
                    ? "border-accent bg-accent-soft"
                    : "border-border bg-surface"
                }`}
              >
                <button
                  type="button"
                  onPointerDown={(e) => {
                    e.preventDefault();
                    startDrag(index);
                  }}
                  className="mt-0.5 text-text-secondary cursor-grab active:cursor-grabbing touch-none"
                  title={t("settings.history.combine.drag")}
                  aria-label={t("settings.history.combine.drag")}
                >
                  <GripVertical className="w-4 h-4" />
                </button>
                {/* The number it has in the History list (the click order),
                    so a moved row keeps it: 3, 1, 2 after rearranging. */}
                <span className="mt-0.5 w-5 shrink-0 text-xs font-semibold tabular-nums text-accent-text text-end">
                  {ids.indexOf(id) + 1}.
                </span>
                <div className="min-w-0 flex-1">
                  <p className="text-xs text-text-secondary tabular-nums">
                    {entry.name ? (
                      <span className="font-medium text-text">
                        {entry.name} ·{" "}
                      </span>
                    ) : null}
                    {when(Number(entry.timestamp))}
                    {entry.duration_seconds != null &&
                      ` · ${clock(entry.duration_seconds)}`}
                  </p>
                  <p className="text-[13px] leading-[18px] text-text line-clamp-2">
                    {takeText(entry, preferSpeakers) ||
                      t("settings.history.combine.noText")}
                  </p>
                </div>
                <button
                  type="button"
                  onClick={() => onRemove(id)}
                  className="mt-0.5 text-text-secondary hover:text-text cursor-pointer"
                  title={t("settings.history.combine.remove")}
                  aria-label={t("settings.history.combine.remove")}
                >
                  <X className="w-3.5 h-3.5" />
                </button>
              </li>
            );
          })}
        </ol>
        <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-border">
          <Button variant="secondary" onClick={onClose}>
            {t("settings.history.combine.close")}
          </Button>
          <Button
            variant="primary"
            onClick={() => void copy()}
            disabled={queue.length === 0}
          >
            {copied ? (
              <Check className="w-4 h-4" />
            ) : (
              <Copy className="w-4 h-4" />
            )}
            {copied
              ? t("settings.history.combine.copiedShort")
              : t("settings.history.combine.copy")}
          </Button>
        </div>
      </div>
    </div>,
    document.body,
  );
};
