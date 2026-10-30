import type { HistoryEntry } from "@/bindings";
import { fieldsMatch, type SearchMatcher } from "@/utils/historySearch";

/** When a recording was made. */
export type DateRange = "any" | "3h" | "12h" | "24h" | "7d" | "30d" | "custom";
export const DATE_RANGES: DateRange[] = [
  "any",
  "3h",
  "12h",
  "24h",
  "7d",
  "30d",
  "custom",
];
const RANGE_SECONDS: Record<Exclude<DateRange, "any" | "custom">, number> = {
  "3h": 3 * 3600,
  "12h": 12 * 3600,
  "24h": 24 * 3600,
  "7d": 7 * 86400,
  "30d": 30 * 86400,
};

/** How long a recording is. The buckets follow how long takes really are
 *  (2,570 takes: under 10 s 24 %, 10–30 s 35 %, 30 s–1 min 18 %, 1–2 min
 *  12 %, 2–5 min 7 %, 5–10 min 2 %, over 10 min 1 %). */
export type LengthBucket =
  | "any"
  | "under10s"
  | "10to30s"
  | "30sTo1m"
  | "1to2m"
  | "2to5m"
  | "5to10m"
  | "over10m";
export const LENGTH_BUCKETS: LengthBucket[] = [
  "any",
  "under10s",
  "10to30s",
  "30sTo1m",
  "1to2m",
  "2to5m",
  "5to10m",
  "over10m",
];
const LENGTH_RANGE: Record<Exclude<LengthBucket, "any">, [number, number]> = {
  under10s: [0, 10],
  "10to30s": [10, 30],
  "30sTo1m": [30, 60],
  "1to2m": [60, 120],
  "2to5m": [120, 300],
  "5to10m": [300, 600],
  over10m: [600, Infinity],
};

export interface HistoryFilter {
  matcher: SearchMatcher | null;
  favoritesOnly: boolean;
  date: DateRange;
  /** Custom range, as `YYYY-MM-DD` (local days, both included); "" = open. */
  customFrom: string;
  customTo: string;
  length: LengthBucket;
}

export const NO_FILTER: HistoryFilter = {
  matcher: null,
  favoritesOnly: false,
  date: "any",
  customFrom: "",
  customTo: "",
  length: "any",
};

/** Local midnight of a `YYYY-MM-DD` day (plus `addDays` calendar days), in
 *  seconds; null when blank/invalid. Calendar days, not 86 400 s: a day with
 *  a daylight-saving change is 23 or 25 hours long. */
const dayStart = (day: string, addDays = 0): number | null => {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(day);
  if (!m) return null;
  const t = new Date(
    Number(m[1]),
    Number(m[2]) - 1,
    Number(m[3]) + addDays,
  ).getTime();
  return isNaN(t) ? null : t / 1000;
};

/** Whether a filter narrows the list at all (besides the search). */
export const isNarrowed = (f: HistoryFilter): boolean =>
  f.favoritesOnly ||
  f.length !== "any" ||
  (f.date !== "any" &&
    (f.date !== "custom" || f.customFrom !== "" || f.customTo !== ""));

export const matchesFilter = (
  entry: HistoryEntry,
  f: HistoryFilter,
  nowSeconds: number,
): boolean => {
  if (f.favoritesOnly && !entry.saved) return false;
  const ts = Number(entry.timestamp);
  if (f.date === "custom") {
    const from = dayStart(f.customFrom);
    // "to" includes the whole day: up to the next day's midnight.
    const end = dayStart(f.customTo, 1);
    if (from !== null && ts < from) return false;
    if (end !== null && ts >= end) return false;
  } else if (f.date !== "any") {
    if (ts < nowSeconds - RANGE_SECONDS[f.date]) return false;
  }
  if (f.length !== "any") {
    const d = entry.duration_seconds;
    if (d == null) return false;
    const [min, max] = LENGTH_RANGE[f.length];
    if (d < min || d >= max) return false;
  }
  if (f.matcher) {
    return fieldsMatch(f.matcher, [
      entry.name,
      entry.title,
      entry.transcription_text,
      entry.post_processed_text,
      entry.diarized_text,
    ]);
  }
  return true;
};
