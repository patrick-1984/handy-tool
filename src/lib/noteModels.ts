import type { Note, OpenRouterModelPrice } from "@/bindings";

/**
 * Model prices and comparisons for notes: the "Recommended for notes
 * (cheap)" group, price labels in the model picker, and the Saved notes
 * "Compare models" summary. Prices come live from the OpenRouter catalogue
 * (`getOpenRouterPrices`); nothing here hard-codes a price.
 */

/** The model the "at least 10x cheaper" bar is measured against. */
export const REFERENCE_MODEL_ID = "anthropic/claude-opus-5.5";

/** How many times cheaper than the reference a recommended model must be. */
export const CHEAPER_FACTOR = 10;

/**
 * Reputable, multilingual general models that summarise well. Only the ones
 * the live catalogue lists, and whose input AND output prices are both at
 * most 1/10 of the reference model's, are recommended. Ids verified against
 * OpenRouter's model list (October 2026).
 */
export const NOTE_MODEL_ALLOW_LIST = [
  "openai/gpt-6-luna",
  "google/gemini-3.1-flash-lite",
  "deepseek/deepseek-v4-flash",
  "deepseek/deepseek-v4-pro",
  "qwen/qwen3.8-flash",
  "mistralai/mistral-small-2603",
  "z-ai/glm-5.3-flash",
  "minimax/minimax-m3",
];

/** USD per 1M tokens. A negative price means "varies" (OpenRouter routers). */
export interface ModelPrice {
  input: number;
  output: number;
}

export interface RecommendedModel {
  id: string;
  price: ModelPrice;
  /** How many times cheaper than the reference on output; null when unknown. */
  cheaperBy: number | null;
}

// Rounding in the catalogue's per-token strings (0.4 can arrive as
// 0.39999999999999997) must not push a model over the bar.
const EPSILON = 1e-9;

/** Prices by model id and by canonical slug. */
export const priceIndex = (
  prices: OpenRouterModelPrice[],
): Map<string, ModelPrice> => {
  const index = new Map<string, ModelPrice>();
  for (const m of prices) {
    const price = { input: m.input_per_million, output: m.output_per_million };
    index.set(m.id, price);
    if (m.canonical_slug && !index.has(m.canonical_slug)) {
      index.set(m.canonical_slug, price);
    }
  }
  return index;
};

export const hasFixedPrice = (price: ModelPrice | null | undefined) =>
  price != null && price.input >= 0 && price.output >= 0;

/**
 * The reference model's price, when the catalogue lists it with a real
 * (non-zero, fixed) price.
 */
export const referencePrice = (
  index: Map<string, ModelPrice>,
): ModelPrice | null => {
  const price = index.get(REFERENCE_MODEL_ID);
  return price && price.input > 0 && price.output > 0 ? price : null;
};

/** How many times cheaper than the reference, on output price. */
export const cheaperThanReference = (
  price: ModelPrice | null | undefined,
  reference: ModelPrice | null,
): number | null => {
  if (!reference || !hasFixedPrice(price) || price!.output <= 0) return null;
  return reference.output / price!.output;
};

/**
 * "Recommended for notes (cheap)": the allow-listed models in the catalogue
 * (and, when given, in the provider's own model list) whose input and output
 * prices are both at most 1/CHEAPER_FACTOR of the reference model's,
 * cheapest output first. Without a reference price the bar can't be
 * checked, so every listed allow-list model is shown, without a ratio.
 */
export const recommendNoteModels = (
  prices: OpenRouterModelPrice[],
  available?: readonly string[] | null,
): RecommendedModel[] => {
  const index = priceIndex(prices);
  const reference = referencePrice(index);
  const availableSet = available ? new Set(available) : null;
  const recommended: RecommendedModel[] = [];
  for (const id of NOTE_MODEL_ALLOW_LIST) {
    if (availableSet && !availableSet.has(id)) continue;
    const price = index.get(id);
    if (!price || !hasFixedPrice(price)) continue;
    if (
      reference &&
      (price.input > reference.input / CHEAPER_FACTOR + EPSILON ||
        price.output > reference.output / CHEAPER_FACTOR + EPSILON)
    ) {
      continue;
    }
    recommended.push({
      id,
      price,
      cheaperBy: cheaperThanReference(price, reference),
    });
  }
  return recommended.sort(
    (a, b) => a.price.output - b.price.output || a.price.input - b.price.input,
  );
};

/** "40", "13", "6.7" (in the UI language): how many times cheaper. */
export const formatTimes = (times: number, locale = "en"): string =>
  new Intl.NumberFormat(locale, {
    maximumFractionDigits: times >= 10 ? 0 : 1,
  }).format(times);

/** US dollars in the UI language with the given fraction digits. */
const usd = (n: number, locale: string, min: number, max: number): string =>
  new Intl.NumberFormat(locale, {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: min,
    maximumFractionDigits: max,
  }).format(n);

/** Dollars with enough digits to tell cheap models apart: 4, 0.50, 0.028. */
const formatDollars = (n: number, locale: string): string => {
  if (n === 0) return usd(0, locale, 0, 0);
  // 0.09999999999999999 (a per-token string times a million) is $0.10.
  if (Number(n.toFixed(4)) >= 0.1) return usd(n, locale, 2, 2);
  // Two significant digits, without trailing zeros.
  const digits = Math.min(10, 1 - Math.floor(Math.log10(n)));
  return usd(n, locale, 0, digits);
};

/**
 * A per-1M price in the UI language: "$0.10", "$0.028" ("0,10 USD" in
 * Polish); null when it varies.
 */
export const formatPricePerMillion = (
  n: number,
  locale = "en",
): string | null => (n < 0 ? null : formatDollars(n, locale));

/**
 * What a note cost, in the UI language: "$0.0012" ("0,0012 USD" in
 * Polish); null when unknown.
 */
export const formatCost = (
  cost: number | null | undefined,
  locale = "en",
): string | null => {
  if (cost == null || !Number.isFinite(cost) || cost < 0) return null;
  if (cost === 0) return usd(0, locale, 0, 0);
  if (cost >= 1) return usd(cost, locale, 2, 2);
  if (cost >= 0.0001) return usd(cost, locale, 4, 4);
  return formatDollars(cost, locale);
};

/** "3.2 s" / "850 ms" in the UI language. */
export const formatDuration = (ms: number, locale: string): string =>
  ms < 1000
    ? new Intl.NumberFormat(locale, {
        style: "unit",
        unit: "millisecond",
        unitDisplay: "short",
        maximumFractionDigits: 0,
      }).format(ms)
    : new Intl.NumberFormat(locale, {
        style: "unit",
        unit: "second",
        unitDisplay: "short",
        maximumFractionDigits: 1,
      }).format(ms / 1000);

export type ModelSort = "name" | "price";

/**
 * Model ids sorted by name, or by price (input plus output per 1M, cheapest
 * first); models without a fixed price go last.
 */
export const sortModels = (
  ids: readonly string[],
  sort: ModelSort,
  priceOf: (id: string) => ModelPrice | null,
): string[] => {
  const byName = (a: string, b: string) => a.localeCompare(b);
  if (sort === "name") return [...ids].sort(byName);
  const key = (id: string) => {
    const price = priceOf(id);
    return hasFixedPrice(price)
      ? price!.input + price!.output
      : Number.POSITIVE_INFINITY;
  };
  return [...ids].sort((a, b) => key(a) - key(b) || byName(a, b));
};

/** One model's line in the Saved notes "Compare models" summary. */
export interface ModelSummary {
  model: string;
  notes: number;
  /** Average over the notes whose cost is known; null when none is. */
  averageCost: number | null;
  /** Sum of the known costs; null when none is known. */
  totalCost: number | null;
  /** Average generation time over the notes that recorded one. */
  averageMs: number | null;
}

/**
 * Per model: how many notes, their average and total cost, and the average
 * generation time. Cheapest average first; models without a known cost
 * last, then by name.
 */
export const summarizeByModel = (notes: readonly Note[]): ModelSummary[] => {
  const byModel = new Map<
    string,
    { notes: number; costs: number[]; times: number[] }
  >();
  for (const note of notes) {
    const entry = byModel.get(note.model) ?? {
      notes: 0,
      costs: [],
      times: [],
    };
    entry.notes += 1;
    if (note.cost_usd != null) entry.costs.push(note.cost_usd);
    if (note.duration_ms != null) entry.times.push(note.duration_ms);
    byModel.set(note.model, entry);
  }
  const sum = (values: number[]) => values.reduce((a, b) => a + b, 0);
  const summaries = [...byModel.entries()].map(([model, e]) => ({
    model,
    notes: e.notes,
    averageCost: e.costs.length > 0 ? sum(e.costs) / e.costs.length : null,
    totalCost: e.costs.length > 0 ? sum(e.costs) : null,
    averageMs: e.times.length > 0 ? sum(e.times) / e.times.length : null,
  }));
  return summaries.sort((a, b) => {
    if (a.averageCost === null || b.averageCost === null) {
      if (a.averageCost !== b.averageCost)
        return a.averageCost === null ? 1 : -1;
    } else if (a.averageCost !== b.averageCost) {
      return a.averageCost - b.averageCost;
    }
    return a.model.localeCompare(b.model);
  });
};

/**
 * Notes made from the same text (same source entry, same speakers flag),
 * so a note from "Try another model" sits next to the one it is compared
 * with. A note whose source entry was deleted keeps its old `history_id`,
 * while a note written from it afterwards has none, so a deleted source
 * groups by the text alone, like pasted text. Groups come newest first (by their newest note); inside a group the
 * oldest note comes first, so the original stays on the left.
 */
export const groupNotesBySource = (notes: readonly Note[]): Note[][] => {
  const groups = new Map<string, Note[]>();
  for (const note of notes) {
    const source = note.source_exists ? note.history_id : null;
    const key = `${source ?? "manual"}|${note.with_speakers ? 1 : 0}|${note.source_text}`;
    const group = groups.get(key);
    if (group) group.push(note);
    else groups.set(key, [note]);
  }
  return [...groups.values()]
    .map((group) => [...group].sort((a, b) => a.id - b.id))
    .sort(
      (a, b) =>
        Math.max(...b.map((n) => n.id)) - Math.max(...a.map((n) => n.id)),
    );
};
