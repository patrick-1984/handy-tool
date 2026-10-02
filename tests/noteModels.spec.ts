import { test, expect } from "@playwright/test";
import type { Note, OpenRouterModelPrice } from "../src/bindings";
import {
  NOTE_MODEL_ALLOW_LIST,
  formatCost,
  formatPricePerMillion,
  formatTimes,
  groupNotesBySource,
  recommendNoteModels,
  sortModels,
  summarizeByModel,
} from "../src/lib/noteModels";

/** A catalogue row, priced per 1M tokens like `fetch_openrouter_model_prices`. */
const price = (
  id: string,
  input: number,
  output: number,
): OpenRouterModelPrice => ({
  id,
  canonical_slug: `${id}-20260901`,
  name: id,
  input_per_million: input,
  output_per_million: output,
});

// Prices as OpenRouter listed them on 2026-10-02.
const CATALOGUE: OpenRouterModelPrice[] = [
  price("anthropic/claude-opus-5.5", 4, 20),
  price("openai/gpt-6-luna", 0.09999999999999999, 0.5),
  price("google/gemini-3.1-flash-lite", 0.25, 1.5),
  price("deepseek/deepseek-v4-flash", 0.028, 0.056),
  price("deepseek/deepseek-v4-pro", 0.2088, 0.4176),
  price("qwen/qwen3.8-flash", 0.15, 0.47),
  price("mistralai/mistral-small-2603", 0.15, 0.6),
  price("z-ai/glm-5.3-flash", 0.15, 0.5),
  price("minimax/minimax-m3", 0.3, 1.2),
  // Not on the allow-list: never recommended, however cheap.
  price("cheap/unknown-model", 0.01, 0.01),
  // Fails the 10x bar on output ($2.50 > $2.00): the old default.
  price("google/gemini-2.5-flash", 0.3, 2.5),
  // OpenRouter routers have no fixed price.
  price("openrouter/auto", -1_000_000, -1_000_000),
];

const note = (fields: Partial<Note>): Note => ({
  id: 1,
  history_id: null,
  timestamp: 1_790_000_000,
  source_text: "text",
  note_text: "# Note",
  skill_name: null,
  model: "m",
  with_speakers: false,
  cost_usd: null,
  truncated: false,
  prompt_tokens: null,
  completion_tokens: null,
  duration_ms: null,
  source_exists: false,
  ...fields,
});

test.describe("note model recommendations", () => {
  test("every allow-listed model passing the 10x bar is recommended, cheapest output first", () => {
    const ids = recommendNoteModels(CATALOGUE).map((m) => m.id);
    expect(ids).toEqual([
      "deepseek/deepseek-v4-flash",
      "deepseek/deepseek-v4-pro",
      "qwen/qwen3.8-flash",
      "openai/gpt-6-luna",
      "z-ai/glm-5.3-flash",
      "mistralai/mistral-small-2603",
      "minimax/minimax-m3",
      "google/gemini-3.1-flash-lite",
    ]);
    expect(ids).not.toContain("cheap/unknown-model");
    expect(new Set(ids)).toEqual(new Set(NOTE_MODEL_ALLOW_LIST));
  });

  test("the bar is computed from the live reference price", () => {
    // Opus gets cheaper: $2 / $10 makes the bar $0.20 / $1.00.
    const cheaperOpus = CATALOGUE.map((m) =>
      m.id === "anthropic/claude-opus-5.5" ? price(m.id, 2, 10) : m,
    );
    const ids = recommendNoteModels(cheaperOpus).map((m) => m.id);
    expect(ids).toContain("openai/gpt-6-luna");
    expect(ids).toContain("deepseek/deepseek-v4-flash");
    expect(ids).not.toContain("minimax/minimax-m3"); // $0.30 in
    expect(ids).not.toContain("google/gemini-3.1-flash-lite"); // $1.50 out
    expect(ids).not.toContain("deepseek/deepseek-v4-pro"); // $0.2088 in
  });

  test("a model exactly at the bar still counts", () => {
    const atBar = [
      price("anthropic/claude-opus-5.5", 4, 20),
      price("openai/gpt-6-luna", 0.4, 2.0000000000000004),
    ];
    expect(recommendNoteModels(atBar).map((m) => m.id)).toEqual([
      "openai/gpt-6-luna",
    ]);
  });

  test("the badge says how many times cheaper on output", () => {
    const byId = Object.fromEntries(
      recommendNoteModels(CATALOGUE).map((m) => [m.id, m.cheaperBy]),
    );
    expect(formatTimes(byId["openai/gpt-6-luna"]!)).toBe("40");
    expect(formatTimes(byId["deepseek/deepseek-v4-flash"]!)).toBe("357");
    expect(formatTimes(byId["google/gemini-3.1-flash-lite"]!)).toBe("13");
    expect(formatTimes(6.66)).toBe("6.7");
  });

  test("without a reference price, models are shown without a ratio", () => {
    const noOpus = CATALOGUE.filter(
      (m) => m.id !== "anthropic/claude-opus-5.5",
    );
    const recommended = recommendNoteModels(noOpus);
    expect(recommended.length).toBe(NOTE_MODEL_ALLOW_LIST.length);
    expect(recommended.every((m) => m.cheaperBy === null)).toBe(true);
  });

  test("only models the provider lists are recommended", () => {
    const ids = recommendNoteModels(CATALOGUE, [
      "openai/gpt-6-luna",
      "anthropic/claude-opus-5.5",
    ]).map((m) => m.id);
    expect(ids).toEqual(["openai/gpt-6-luna"]);
  });
});

test.describe("note model prices", () => {
  test("price labels keep cheap prices apart", () => {
    expect(formatPricePerMillion(4)).toBe("$4.00");
    expect(formatPricePerMillion(0.5)).toBe("$0.50");
    expect(formatPricePerMillion(0.09999999999999999)).toBe("$0.10");
    expect(formatPricePerMillion(0.028)).toBe("$0.028");
    expect(formatPricePerMillion(0)).toBe("$0");
    expect(formatPricePerMillion(-1_000_000)).toBeNull();
  });

  test("note costs", () => {
    expect(formatCost(0.0012)).toBe("$0.0012");
    expect(formatCost(0.00004)).toBe("$0.00004");
    expect(formatCost(1.5)).toBe("$1.50");
    expect(formatCost(0)).toBe("$0");
    expect(formatCost(null)).toBeNull();
  });

  test("sorting by price puts unpriced models last", () => {
    const index = new Map(
      CATALOGUE.map((m) => [
        m.id,
        { input: m.input_per_million, output: m.output_per_million },
      ]),
    );
    const sorted = sortModels(
      [
        "openrouter/auto",
        "no/price",
        "openai/gpt-6-luna",
        "cheap/unknown-model",
      ],
      "price",
      (id) => index.get(id) ?? null,
    );
    expect(sorted).toEqual([
      "cheap/unknown-model",
      "openai/gpt-6-luna",
      "no/price",
      "openrouter/auto",
    ]);
    expect(sortModels(["b", "a"], "name", () => null)).toEqual(["a", "b"]);
  });
});

test.describe("model comparison", () => {
  test("per model: notes, average and total cost, average time", () => {
    const summaries = summarizeByModel([
      note({ id: 1, model: "a", cost_usd: 0.002, duration_ms: 3000 }),
      note({ id: 2, model: "a", cost_usd: 0.004, duration_ms: 1000 }),
      note({ id: 3, model: "b", cost_usd: 0.0005, duration_ms: 500 }),
      note({ id: 4, model: "c" }),
      note({ id: 5, model: "a", duration_ms: 2000 }),
    ]);
    expect(summaries.map((s) => s.model)).toEqual(["b", "a", "c"]);
    const a = summaries[1];
    expect(a.notes).toBe(3);
    expect(a.averageCost).toBeCloseTo(0.003);
    expect(a.totalCost).toBeCloseTo(0.006);
    expect(a.averageMs).toBe(2000);
    const c = summaries[2];
    expect(c.averageCost).toBeNull();
    expect(c.totalCost).toBeNull();
    expect(c.averageMs).toBeNull();
  });

  test("notes from the same text sit together, the original first", () => {
    const groups = groupNotesBySource([
      note({
        id: 5,
        history_id: 7,
        source_exists: true,
        source_text: "meeting",
        model: "b",
      }),
      note({ id: 4, history_id: 8, source_exists: true, source_text: "other" }),
      note({
        id: 3,
        history_id: 7,
        source_exists: true,
        source_text: "meeting",
        model: "a",
      }),
      note({
        id: 2,
        history_id: 7,
        source_exists: true,
        source_text: "[Person 1]: meeting",
        with_speakers: true,
      }),
    ]);
    expect(groups.map((g) => g.map((n) => n.id))).toEqual([[3, 5], [4], [2]]);
  });

  test("a note of a deleted source sits next to the one written from it", () => {
    const groups = groupNotesBySource([
      // Written with Try another model after the entry was deleted.
      note({ id: 9, history_id: null, source_text: "meeting", model: "b" }),
      note({ id: 8, history_id: 3, source_exists: true, source_text: "x" }),
      // Its entry (7) is gone; the note keeps the old id.
      note({ id: 6, history_id: 7, source_text: "meeting", model: "a" }),
    ]);
    expect(groups.map((g) => g.map((n) => n.id))).toEqual([[6, 9], [8]]);
  });
});

test.describe("note numbers in the UI language", () => {
  test("costs, prices and ratios follow the locale", () => {
    expect(formatCost(0.0012, "pl")).toBe("0,0012\u00a0USD");
    expect(formatPricePerMillion(0.5, "pl")).toBe("0,50\u00a0USD");
    expect(formatPricePerMillion(0.028, "de")).toBe("0,028\u00a0$");
    expect(formatTimes(6.66, "pl")).toBe("6,7");
    expect(formatCost(0.0012, "en")).toBe("$0.0012");
  });
});
