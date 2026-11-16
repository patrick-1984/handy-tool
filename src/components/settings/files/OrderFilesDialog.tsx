import React, { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { convertFileSrc } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { ArrowDown, ArrowUp, GripVertical, Pause, Play, X } from "lucide-react";
import { Button } from "../../ui/Button";
import { Dropdown } from "../../ui/Dropdown";
import { useFileBatchStore } from "../../../stores/fileBatchStore";
import { moveItem } from "../history/combine";
import type { BatchSeparator } from "@/bindings";
import type { ResolvedBatchOptions } from "./batch";

const pad2 = (n: number) => String(n).padStart(2, "0");
const clock = (s: number) => {
  const t = Math.max(0, Math.round(s));
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  return h > 0 ? `${h}:${pad2(m)}:${pad2(t % 60)}` : `${m}:${pad2(t % 60)}`;
};

const SEPARATORS: BatchSeparator[] = ["none", "blank_line", "divider"];

/** One player for the whole window: starting a file stops the other. */
let player: HTMLAudioElement | null = null;
const stopPlayer = () => {
  if (player) {
    player.pause();
    player.removeAttribute("src");
    player.load();
    player = null;
  }
};

interface Props {
  options: ResolvedBatchOptions;
  onOptions: (options: ResolvedBatchOptions) => void;
  /** Why Transcribe cannot start now (model not downloaded...), if it can't. */
  blocked: string | null;
  onTranscribe: () => void;
}

/**
 * Files › several files: the picked files in the order their texts will be
 * joined - by date first, then dragged or moved with the arrows. Each can be
 * listened to before it is placed, or removed.
 */
export const OrderFilesDialog: React.FC<Props> = ({
  options,
  onOptions,
  blocked,
  onTranscribe,
}) => {
  const { t, i18n } = useTranslation();
  const items = useFileBatchStore((s) => s.items);
  const setItems = useFileBatchStore((s) => s.setItems);
  const remove = useFileBatchStore((s) => s.remove);
  const closeOrder = useFileBatchStore((s) => s.closeOrder);
  const [playing, setPlaying] = useState<string | null>(null);

  const close = React.useCallback(() => {
    stopPlayer();
    closeOrder();
  }, [closeOrder]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [close]);
  useEffect(() => () => stopPlayer(), []);

  const toggle = (path: string, name: string) => {
    if (playing === path) {
      stopPlayer();
      setPlaying(null);
      return;
    }
    stopPlayer();
    const audio = new Audio(convertFileSrc(path));
    player = audio;
    audio.onended = () => {
      if (player === audio) setPlaying(null);
    };
    audio.onerror = () => {
      if (player !== audio) return;
      setPlaying(null);
      toast.error(t("settings.files.batch.order.playFailed", { name }));
    };
    setPlaying(path);
    audio.play().catch(() => {
      if (player === audio) setPlaying(null);
    });
  };

  // Dragging by the grip (pointer events: the window's own file drop handling
  // keeps HTML drag-and-drop from working inside it), as in History › Combine.
  const rowRefs = useRef<(HTMLLIElement | null)[]>([]);
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  useEffect(() => {
    if (dragIndex === null) return;
    const onMove = (e: PointerEvent) => {
      let target = 0;
      rowRefs.current.forEach((row, i) => {
        if (!row || i === dragIndex) return;
        const r = row.getBoundingClientRect();
        if (e.clientY > r.top + r.height / 2) target += 1;
      });
      if (target !== dragIndex) {
        setItems(
          moveItem(useFileBatchStore.getState().items, dragIndex, target),
        );
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
  }, [dragIndex, setItems]);

  const move = (from: number, to: number) => {
    if (to < 0 || to >= items.length) return;
    setItems(moveItem(items, from, to));
  };

  const when = (ms: number) =>
    new Intl.DateTimeFormat(i18n.language, {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(ms));

  const ICON =
    "inline-flex items-center justify-center h-7 w-7 rounded-md text-text-secondary hover:bg-hover hover:text-text cursor-pointer disabled:opacity-40 disabled:cursor-default disabled:hover:bg-transparent";

  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-scrim p-4"
      onClick={close}
      role="presentation"
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="order-files-title"
        className="w-[620px] max-w-full max-h-[88vh] flex flex-col rounded-lg bg-surface border border-border shadow-float overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start justify-between gap-3 px-5 pt-4 pb-3 border-b border-border">
          <div>
            <h2
              id="order-files-title"
              className="text-[15px] font-semibold text-text"
            >
              {t("settings.files.batch.order.title", { count: items.length })}
            </h2>
            <p className="text-[13px] leading-[18px] text-text-secondary">
              {t("settings.files.batch.order.description")}
            </p>
          </div>
          <button
            type="button"
            onClick={close}
            aria-label={t("common.cancel")}
            className="inline-flex items-center justify-center h-7 w-7 rounded-md text-text-secondary hover:bg-hover cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
        <ol className="overflow-y-auto px-3 py-2 space-y-1.5 select-none min-h-16">
          {items.length === 0 && (
            <li className="px-2 py-3 text-sm text-text-secondary">
              {t("settings.files.batch.order.empty")}
            </li>
          )}
          {items.map((item, index) => (
            <li
              key={item.path}
              ref={(el) => {
                rowRefs.current[index] = el;
              }}
              className={`flex items-center gap-2 rounded-md border px-2 py-1.5 ${
                dragIndex === index
                  ? "border-accent bg-accent-soft"
                  : "border-border bg-surface"
              }`}
            >
              <button
                type="button"
                onPointerDown={(e) => {
                  e.preventDefault();
                  setDragIndex(index);
                }}
                className="text-text-secondary cursor-grab active:cursor-grabbing touch-none"
                title={t("settings.files.batch.order.drag")}
                aria-label={t("settings.files.batch.order.drag")}
              >
                <GripVertical className="w-4 h-4" />
              </button>
              <span className="w-6 shrink-0 text-xs font-semibold tabular-nums text-accent-text text-end">
                {index + 1}.
              </span>
              <button
                type="button"
                className={ICON}
                onClick={() => toggle(item.path, item.file_name)}
                title={
                  playing === item.path
                    ? t("settings.files.batch.order.stop")
                    : t("settings.files.batch.order.play")
                }
                aria-label={
                  playing === item.path
                    ? t("settings.files.batch.order.stop")
                    : t("settings.files.batch.order.play")
                }
              >
                {playing === item.path ? (
                  <Pause className="w-4 h-4" />
                ) : (
                  <Play className="w-4 h-4" />
                )}
              </button>
              <div className="min-w-0 flex-1">
                <p
                  className="text-[13px] font-medium text-text truncate"
                  title={item.path}
                >
                  {item.file_name}
                </p>
                <p className="text-xs text-text-secondary tabular-nums">
                  {item.created_ms != null
                    ? when(item.created_ms)
                    : t("settings.files.batch.order.noDate")}
                  {" · "}
                  {item.duration_secs != null
                    ? clock(item.duration_secs)
                    : t("settings.files.batch.order.noLength")}
                </p>
              </div>
              <button
                type="button"
                className={ICON}
                onClick={() => move(index, index - 1)}
                disabled={index === 0}
                title={t("settings.files.batch.order.moveUp")}
                aria-label={t("settings.files.batch.order.moveUp")}
              >
                <ArrowUp className="w-4 h-4" />
              </button>
              <button
                type="button"
                className={ICON}
                onClick={() => move(index, index + 1)}
                disabled={index === items.length - 1}
                title={t("settings.files.batch.order.moveDown")}
                aria-label={t("settings.files.batch.order.moveDown")}
              >
                <ArrowDown className="w-4 h-4" />
              </button>
              <button
                type="button"
                className={ICON}
                onClick={() => {
                  if (playing === item.path) {
                    stopPlayer();
                    setPlaying(null);
                  }
                  remove(item.path);
                }}
                title={t("settings.files.batch.order.remove")}
                aria-label={t("settings.files.batch.order.remove")}
              >
                <X className="w-4 h-4" />
              </button>
            </li>
          ))}
        </ol>
        <div className="flex flex-col gap-2.5 px-5 py-3 border-t border-border">
          <p className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
            {t("settings.files.batch.options.title")}
          </p>
          <div className="flex items-center justify-between gap-3">
            <span className="text-sm">
              {t("settings.files.batch.options.separator.label")}
            </span>
            <Dropdown
              options={SEPARATORS.map((value) => ({
                value,
                label: t(`settings.files.batch.options.separator.${value}`),
              }))}
              selectedValue={options.separator}
              onSelect={(value) =>
                onOptions({ ...options, separator: value as BatchSeparator })
              }
            />
          </div>
          <label className="flex items-center gap-2 text-sm cursor-pointer">
            <input
              type="checkbox"
              checked={options.numbered}
              onChange={(e) =>
                onOptions({ ...options, numbered: e.target.checked })
              }
              className="w-4 h-4 accent-accent cursor-pointer"
            />
            {t("settings.files.batch.options.numbered")}
          </label>
          <label className="flex items-center gap-2 text-sm cursor-pointer">
            <input
              type="checkbox"
              checked={options.names}
              onChange={(e) =>
                onOptions({ ...options, names: e.target.checked })
              }
              className="w-4 h-4 accent-accent cursor-pointer"
            />
            {t("settings.files.batch.options.names")}
          </label>
        </div>
        <div className="flex items-center justify-between gap-2 px-5 py-3 bg-surface2 border-t border-border">
          <span className="text-xs text-err-text">{blocked ?? ""}</span>
          <div className="flex items-center gap-2">
            <Button variant="secondary" onClick={close}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="primary"
              onClick={() => {
                stopPlayer();
                setPlaying(null);
                onTranscribe();
              }}
              disabled={items.length === 0 || blocked !== null}
            >
              {t("settings.files.batch.order.transcribe", {
                count: items.length,
              })}
            </Button>
          </div>
        </div>
      </div>
    </div>,
    document.body,
  );
};
