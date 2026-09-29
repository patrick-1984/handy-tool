import type { ModelInfo } from "@/bindings";

/**
 * Engines never suggested at setup: they run elsewhere (need an API key) or
 * need extra software (FLM). They stay in "See all models".
 */
const NOT_SUGGESTED: string[] = [
  "ApiWhisper",
  "OpenRouterWhisper",
  "FlmWhisper",
];

/** Models tuned for particular languages: suggested only when one is picked. */
const SPECIALISED: Record<string, string[]> = {
  "breeze-asr": ["zh", "zh-Hans", "zh-Hant"],
};

/** Every model can take the picked languages (all of them, when several). */
export const understandsAll = (model: ModelInfo, languages: string[]) =>
  languages.every((code) => model.supported_languages.includes(code));

export interface ModelSuggestions {
  accurate?: ModelInfo;
  balanced?: ModelInfo;
  fast?: ModelInfo;
}

const best = (pool: ModelInfo[], score: (m: ModelInfo) => number) =>
  pool.reduce<ModelInfo | undefined>(
    (top, m) => (top === undefined || score(m) > score(top) ? m : top),
    undefined,
  );

/**
 * Three different models for the picked languages: the balanced one first (the
 * recommended model when it fits), then the most accurate and the fastest of
 * the rest. The setup selects the balanced one to start with; the user can
 * pick another.
 */
export const suggestModels = (
  models: ModelInfo[],
  languages: string[],
): ModelSuggestions => {
  const pool = models.filter(
    (m) =>
      !NOT_SUGGESTED.includes(m.engine_type) &&
      !m.is_custom &&
      understandsAll(m, languages) &&
      (!SPECIALISED[m.id] ||
        languages.some((code) => SPECIALISED[m.id].includes(code))),
  );
  // The recommended model (Parakeet V3: many languages) is the balanced pick
  // whenever it takes the languages; otherwise the best all-rounder, ties
  // going to the more accurate.
  const balanced =
    pool.find((m) => m.is_recommended) ??
    best(
      pool,
      (m) => m.accuracy_score + m.speed_score + m.accuracy_score * 0.0001,
    );
  const rest = pool.filter((m) => m !== balanced);
  const accurate = best(rest, (m) => m.accuracy_score + m.speed_score * 0.001);
  const fast = best(
    rest.filter((m) => m !== accurate),
    (m) => m.speed_score + m.accuracy_score * 0.001,
  );
  return { accurate, balanced, fast };
};

/** Every language some model on offer can take, as codes. */
export const offeredLanguages = (models: ModelInfo[]) =>
  new Set(models.flatMap((m) => m.supported_languages));
