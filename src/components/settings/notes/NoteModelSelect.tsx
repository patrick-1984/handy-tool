import React, { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, RefreshCw } from "lucide-react";
import { commands, type LlmProvider } from "@/bindings";
import { getOpenRouterPrices, resolvePrice } from "@/lib/openrouterPrices";
import {
  cheaperThanReference,
  formatPricePerMillion,
  formatTimes,
  priceIndex,
  recommendNoteModels,
  referencePrice,
  sortModels,
  type ModelPrice,
  type ModelSort,
  type RecommendedModel,
} from "@/lib/noteModels";
import type { OpenRouterModelPrice } from "@/bindings";
import { TEXT_FIELD } from "../../ui/controlClasses";

/** Kinds whose models can be priced from the OpenRouter catalogue. */
const PRICED_KINDS = ["openrouter", "anthropic", "gemini"];

interface Props {
  value: string;
  /** The provider whose models are listed (null = none chosen yet). */
  provider: LlmProvider | null;
  onCommit: (value: string) => void;
  placeholder?: string;
  className?: string;
  /** Show the chosen model's price under the field. */
  showSelectedPrice?: boolean;
  /** Every change of the field's text, typed or picked. */
  onChangeText?: (text: string) => void;
}

/**
 * The box the list must stay inside: the nearest ancestor that clips or
 * scrolls (the page's content area), else the window.
 */
const clipBounds = (el: HTMLElement) => {
  for (let p = el.parentElement; p; p = p.parentElement) {
    const style = getComputedStyle(p);
    if (/(auto|scroll|hidden)/.test(style.overflowX + style.overflowY)) {
      return p.getBoundingClientRect();
    }
  }
  return new DOMRect(0, 0, window.innerWidth, window.innerHeight);
};

/** The list's width; it opens towards the side with room for it. */
const LIST_WIDTH = 560;
/** The list's height (max-h-80). */
const LIST_HEIGHT = 320;

/**
 * The model list of the provider's catalogue and the OpenRouter prices,
 * loaded once per provider.
 */
const useModelCatalogue = (provider: LlmProvider | null) => {
  const [ids, setIds] = useState<string[]>([]);
  const [prices, setPrices] = useState<OpenRouterModelPrice[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(false);
  const [fetched, setFetched] = useState(false);
  const providerId = provider?.id ?? null;
  const kind = provider?.kind ?? "";

  // Prices come from the cached OpenRouter catalogue (refreshed once a day),
  // so the chosen model's price shows without opening the list.
  useEffect(() => {
    if (!PRICED_KINDS.includes(kind)) {
      setPrices([]);
      return;
    }
    let cancelled = false;
    void getOpenRouterPrices().then((list) => {
      if (!cancelled) setPrices(list);
    });
    return () => {
      cancelled = true;
    };
  }, [kind]);

  useEffect(() => {
    setIds([]);
    setFetched(false);
    setError(false);
  }, [providerId]);

  const load = async () => {
    if (!providerId) return;
    setLoading(true);
    setError(false);
    try {
      const result = await commands.listProviderModels(providerId);
      if (result.status === "ok") {
        setIds(result.data);
        setFetched(true);
        return;
      }
      throw new Error(result.error);
    } catch {
      // OpenRouter's catalogue lists the same models, so fall back to it.
      const catalogue =
        kind === "openrouter" ? await getOpenRouterPrices() : [];
      if (catalogue.length > 0) {
        setIds(catalogue.map((m) => m.id));
        setFetched(true);
      } else {
        setError(true);
      }
    } finally {
      setLoading(false);
    }
  };

  return { ids, prices, loading, error, fetched, load };
};

/** "$0.10 / $0.50", or "price varies". */
const PriceLabel: React.FC<{ price: ModelPrice }> = ({ price }) => {
  const { t } = useTranslation();
  const input = formatPricePerMillion(price.input);
  const output = formatPricePerMillion(price.output);
  if (input === null || output === null) {
    return <>{t("settings.notes.modelPicker.priceVaries")}</>;
  }
  if (price.input === 0 && price.output === 0) {
    return <>{t("settings.notes.modelPicker.free")}</>;
  }
  return (
    <>
      {input} / {output}
    </>
  );
};

/** "≈40x cheaper than Opus 5.5". */
const CheaperBadge: React.FC<{ times: number }> = ({ times }) => {
  const { t } = useTranslation();
  return (
    <span className="shrink-0 rounded-full border border-ok-text/40 px-1.5 py-px text-[11px] font-medium text-ok-text whitespace-nowrap">
      {t("settings.notes.modelPicker.cheaper", { times: formatTimes(times) })}
    </span>
  );
};

/**
 * The note model picker: every model the provider lists, with its price per
 * 1M input / output tokens (OpenRouter's catalogue), a search box, sorting by
 * name or price, and "Recommended for notes (cheap)" on top for OpenRouter.
 * Any model id can also be typed. Commits on selection or on blur.
 */
export const NoteModelSelect: React.FC<Props> = ({
  value,
  provider,
  onCommit,
  placeholder,
  className,
  showSelectedPrice = false,
  onChangeText,
}) => {
  const { t } = useTranslation();
  const { ids, prices, loading, error, fetched, load } =
    useModelCatalogue(provider);
  const [query, setQuery] = useState(value);
  const [open, setOpen] = useState(false);
  // Until the user types, the list is not filtered by the current value.
  const [touched, setTouched] = useState(false);
  const [sort, setSort] = useState<ModelSort>("price");
  const blurTimer = useRef<number | null>(null);
  const fieldRef = useRef<HTMLDivElement>(null);
  const [placement, setPlacement] = useState({
    alignEnd: false,
    openUp: false,
    width: LIST_WIDTH,
  });
  const listRef = useRef<HTMLDivElement>(null);
  // The latest text and whether Escape cancelled it, for the blur timer.
  const queryRef = useRef(value);
  const cancelledRef = useRef(false);
  // The click that focused the field must not undo its select-all.
  const justFocusedRef = useRef(false);
  const kind = provider?.kind ?? "";

  useEffect(() => {
    setQuery(value);
    queryRef.current = value;
  }, [value]);
  useEffect(
    () => () => {
      if (blurTimer.current) window.clearTimeout(blurTimer.current);
    },
    [],
  );

  const index = useMemo(() => priceIndex(prices), [prices]);
  const reference = useMemo(() => referencePrice(index), [index]);
  const priceOf = useMemo(() => {
    if (kind === "openrouter") return (id: string) => index.get(id) ?? null;
    if (PRICED_KINDS.includes(kind)) {
      return (id: string) => resolvePrice(prices, kind, id);
    }
    return () => null;
  }, [kind, index, prices]);

  const recommended: RecommendedModel[] = useMemo(
    () =>
      kind === "openrouter" && prices.length > 0
        ? recommendNoteModels(prices, fetched ? ids : null)
        : [],
    [kind, prices, ids, fetched],
  );

  const q = touched ? query.trim().toLowerCase() : "";
  const matches = (id: string) => q === "" || id.toLowerCase().includes(q);
  const shownRecommended = recommended.filter((m) => matches(m.id));
  const shownAll = useMemo(
    () =>
      sortModels(
        ids.filter((id) => q === "" || id.toLowerCase().includes(q)),
        sort,
        priceOf,
      ),
    [ids, q, sort, priceOf],
  );
  const hasPrices = prices.length > 0 && PRICED_KINDS.includes(kind);

  // A new search starts at the top of the list.
  useEffect(() => {
    if (listRef.current) listRef.current.scrollTop = 0;
  }, [q, sort]);

  const changeQuery = (text: string) => {
    setQuery(text);
    queryRef.current = text;
    onChangeText?.(text);
  };

  const commit = (next: string) => {
    if (next !== value) onCommit(next);
  };

  const select = (id: string) => {
    changeQuery(id);
    setTouched(false);
    setOpen(false);
    commit(id);
  };

  const handleFocus = (e: React.FocusEvent<HTMLInputElement>) => {
    if (blurTimer.current) window.clearTimeout(blurTimer.current);
    cancelledRef.current = false;
    setTouched(false);
    setOpen(true);
    const field = fieldRef.current;
    if (field) {
      // Open to the right when it fits, else to the left; upwards when
      // there is no room below but more above. Never past the content area.
      const rect = field.getBoundingClientRect();
      const bounds = clipBounds(field);
      const width = Math.min(LIST_WIDTH, bounds.width - 16);
      const fitsRight = rect.left + width <= bounds.right - 8;
      const fitsLeft = rect.right - width >= bounds.left + 8;
      const below = bounds.bottom - rect.bottom;
      const above = rect.top - bounds.top;
      setPlacement({
        alignEnd: !fitsRight && fitsLeft,
        openUp: below < LIST_HEIGHT + 8 && above > below,
        width:
          fitsRight || fitsLeft
            ? width
            : Math.max(240, bounds.right - 8 - rect.left),
      });
    }
    e.target.select();
    justFocusedRef.current = true;
    if (!fetched && !loading && provider) void load();
  };

  const handleBlur = () => {
    // Let a click inside the list land before closing and committing.
    blurTimer.current = window.setTimeout(() => {
      setOpen(false);
      setTouched(false);
      if (cancelledRef.current) {
        // Escape: back to the saved model, nothing committed.
        changeQuery(value);
        return;
      }
      commit(queryRef.current.trim());
    }, 150);
  };

  const keepOpen = (e: React.MouseEvent) => e.preventDefault();

  const row = (id: string, price: ModelPrice | null, times: number | null) => (
    <button
      key={id}
      type="button"
      onMouseDown={(e) => {
        e.preventDefault();
        select(id);
      }}
      className={`w-full min-h-8 flex items-center gap-2 px-2 py-1 text-sm text-start rounded-md hover:bg-hover cursor-pointer ${
        id === value ? "bg-active text-accent-text" : "text-text"
      }`}
    >
      <span className="truncate min-w-0 flex-1" title={id}>
        {id}
      </span>
      {times !== null && <CheaperBadge times={times} />}
      {price && (
        <span className="shrink-0 text-xs tabular-nums text-text-secondary whitespace-nowrap">
          <PriceLabel price={price} />
        </span>
      )}
    </button>
  );

  const heading = (text: string, extra?: React.ReactNode) => (
    <div className="flex items-center justify-between gap-2 px-2 pt-2 pb-1">
      <span className="text-xs font-semibold uppercase tracking-[0.06em] text-text-secondary">
        {text}
      </span>
      {extra}
    </div>
  );

  const selectedPrice = value ? priceOf(value) : null;
  const selectedTimes =
    kind === "openrouter"
      ? cheaperThanReference(selectedPrice, reference)
      : null;

  return (
    <div className={className}>
      <div ref={fieldRef} className="relative flex items-center gap-1">
        <div className="relative flex-1 min-w-0">
          <input
            type="text"
            value={query}
            onChange={(e) => {
              changeQuery(e.target.value);
              setTouched(true);
              setOpen(true);
            }}
            onFocus={handleFocus}
            onMouseUp={(e) => {
              if (justFocusedRef.current) e.preventDefault();
              justFocusedRef.current = false;
            }}
            onBlur={handleBlur}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.stopPropagation();
                cancelledRef.current = true;
                setOpen(false);
                e.currentTarget.blur();
              } else if (e.key === "Enter") {
                e.currentTarget.blur();
              }
            }}
            placeholder={placeholder}
            aria-label={t("settings.notes.modelPicker.search")}
            className={`${TEXT_FIELD} w-full pe-6`}
          />
          <ChevronDown className="pointer-events-none absolute end-1.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-text-secondary" />
        </div>
        <button
          type="button"
          onClick={() => void load()}
          disabled={loading || !provider}
          className="p-1.5 rounded-md border border-border text-text-secondary hover:text-text hover:border-accent disabled:opacity-50 transition-colors cursor-pointer shrink-0"
          title={t("settings.modelPicker.refresh")}
        >
          <RefreshCw className={`w-4 h-4 ${loading ? "animate-spin" : ""}`} />
        </button>

        {open && (
          <div
            ref={listRef}
            onMouseDown={keepOpen}
            style={{ width: placement.width }}
            className={`absolute ${placement.alignEnd ? "end-0" : "start-0"} ${placement.openUp ? "bottom-full mb-1" : "top-full mt-1"} z-30 max-h-80 overflow-y-auto rounded-lg border border-border bg-surface shadow-float p-1`}
          >
            <div className="flex flex-wrap items-center justify-between gap-2 px-2 py-1">
              <span className="text-xs text-text-secondary">
                {hasPrices
                  ? t("settings.notes.modelPicker.priceHeader")
                  : t("settings.notes.modelPicker.noPrices")}
              </span>
              <div
                className="flex items-center gap-1 text-xs"
                role="group"
                aria-label={t("settings.notes.modelPicker.sort")}
              >
                <span className="text-text-secondary">
                  {t("settings.notes.modelPicker.sort")}
                </span>
                {(["price", "name"] as ModelSort[]).map((option) => (
                  <button
                    key={option}
                    type="button"
                    disabled={option === "price" && !hasPrices}
                    aria-pressed={sort === option}
                    onClick={() => setSort(option)}
                    className={`px-1.5 py-0.5 rounded-md cursor-pointer disabled:cursor-not-allowed disabled:text-dis-text ${
                      sort === option && (option === "name" || hasPrices)
                        ? "bg-accent-soft text-accent-text"
                        : "text-text-secondary hover:bg-hover"
                    }`}
                  >
                    {option === "price"
                      ? t("settings.notes.modelPicker.sortPrice")
                      : t("settings.notes.modelPicker.sortName")}
                  </button>
                ))}
              </div>
            </div>

            {shownRecommended.length > 0 && (
              <>
                {heading(t("settings.notes.modelPicker.recommended"))}
                {shownRecommended.map((m) => row(m.id, m.price, m.cheaperBy))}
              </>
            )}

            {loading && (
              <div className="px-2 py-1.5 text-xs text-text-secondary">
                {t("settings.modelPicker.loading")}
              </div>
            )}
            {error && !loading && (
              <div className="px-2 py-1.5 text-xs text-err-text">
                {t("settings.modelPicker.error")}
              </div>
            )}
            {!loading && !error && fetched && (
              <>
                {heading(
                  t("settings.notes.modelPicker.all", { count: ids.length }),
                )}
                {shownAll.map((id) => row(id, priceOf(id), null))}
                {shownAll.length === 0 && (
                  <div className="px-2 py-1.5 text-xs text-text-secondary">
                    {t("settings.modelPicker.noMatches")}
                  </div>
                )}
              </>
            )}
          </div>
        )}
      </div>

      {showSelectedPrice && selectedPrice && (
        <p className="mt-1 flex flex-wrap items-center gap-1.5 text-xs text-text-secondary">
          <span className="tabular-nums">
            {t("settings.notes.modelPicker.selectedPrice")}{" "}
            <PriceLabel price={selectedPrice} />
          </span>
          {selectedTimes !== null && <CheaperBadge times={selectedTimes} />}
        </p>
      )}
    </div>
  );
};
