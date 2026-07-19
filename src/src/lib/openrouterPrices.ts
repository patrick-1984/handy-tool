import { commands, type OpenRouterModelPrice } from "@/bindings";

// OpenRouter exposes pass-through per-token pricing for every model it proxies
// (including Gemini/Anthropic, which don't publish prices via their own APIs).
// We cache the catalogue in localStorage so price look-ups work offline.

const CACHE_KEY = "handy.openrouterPrices";
const TTL_MS = 24 * 60 * 60 * 1000; // refresh at most once/day

interface Cached {
  ts: number;
  prices: OpenRouterModelPrice[];
}

let memo: OpenRouterModelPrice[] | null = null;

function readCache(): Cached | null {
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    return raw ? (JSON.parse(raw) as Cached) : null;
  } catch {
    return null;
  }
}

/**
 * Returns the OpenRouter price catalogue. Uses a fresh localStorage cache if
 * available; otherwise tries to refresh from the API and caches it. Offline (or
 * on error) it falls back to any cached copy, even if stale; never throws.
 */
export async function getOpenRouterPrices(): Promise<OpenRouterModelPrice[]> {
  const cached = readCache();
  const fresh = cached !== null && Date.now() - cached.ts < TTL_MS;
  if (fresh && cached) {
    memo = cached.prices;
    return cached.prices;
  }
  try {
    const res = await commands.fetchOpenrouterModelPrices();
    if (res.status === "ok") {
      memo = res.data;
      try {
        localStorage.setItem(
          CACHE_KEY,
          JSON.stringify({ ts: Date.now(), prices: res.data }),
        );
      } catch {
        // ignore quota errors
      }
      return res.data;
    }
  } catch {
    // offline / network error — fall through to cache
  }
  if (cached) {
    memo = cached.prices;
    return cached.prices;
  }
  return memo ?? [];
}

const VENDOR_PREFIX: Record<string, string> = {
  gemini: "google",
  anthropic: "anthropic",
  openrouter: "",
};

// Collapse version separators and drop a trailing -YYYYMMDD date suffix so e.g.
// native `claude-haiku-4-5` matches OpenRouter slug `anthropic/claude-haiku-4.5`.
function normalize(s: string): string {
  return s
    .toLowerCase()
    .replace(/-\d{8}$/, "")
    .replace(/[.\-_/]/g, "");
}

export interface ResolvedPrice {
  input: number;
  output: number;
}

/**
 * Find a model's per-1M pricing in the OpenRouter catalogue. For Gemini /
 * Anthropic the native model id is mapped to its OpenRouter slug (exact, then
 * normalized fuzzy match). Returns null if not found.
 */
export function resolvePrice(
  prices: OpenRouterModelPrice[],
  kind: string,
  modelId: string,
): ResolvedPrice | null {
  if (!modelId.trim()) return null;
  const vendor = VENDOR_PREFIX[kind] ?? "";
  const candidates = new Set<string>([modelId]);
  if (vendor && !modelId.includes("/")) candidates.add(`${vendor}/${modelId}`);

  // 1) exact match on id or canonical_slug
  for (const m of prices) {
    if (candidates.has(m.id) || candidates.has(m.canonical_slug)) {
      return { input: m.input_per_million, output: m.output_per_million };
    }
  }
  // 2) normalized fuzzy match
  const targets = Array.from(candidates).map(normalize);
  for (const m of prices) {
    if (
      targets.includes(normalize(m.id)) ||
      targets.includes(normalize(m.canonical_slug))
    ) {
      return { input: m.input_per_million, output: m.output_per_million };
    }
  }
  return null;
}

/** Convenience: fetch the catalogue then resolve one model. */
export async function resolveModelPrice(
  kind: string,
  modelId: string,
): Promise<ResolvedPrice | null> {
  const prices = await getOpenRouterPrices();
  return resolvePrice(prices, kind, modelId);
}
