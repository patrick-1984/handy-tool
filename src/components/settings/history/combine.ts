import type { CombineSeparator, HistoryEntry } from "@/bindings";

/** A prefix/suffix pair as the settings hold it (same rules as Rust's
 *  `apply_affixes`: empty text gets nothing; an enabled but blank affix adds
 *  nothing, not even its newline). */
export interface Affixes {
  prefixEnabled: boolean;
  prefixText: string;
  prefixNewline: boolean;
  suffixEnabled: boolean;
  suffixText: string;
  suffixNewline: boolean;
}

export const applyAffixes = (text: string, a: Affixes): string => {
  if (text === "") return text;
  let out = "";
  if (a.prefixEnabled && a.prefixText !== "") {
    out += a.prefixText + (a.prefixNewline ? "\n" : "");
  }
  out += text;
  if (a.suffixEnabled && a.suffixText !== "") {
    out += (a.suffixNewline ? "\n" : "") + a.suffixText;
  }
  return out;
};

export const SEPARATORS: Record<CombineSeparator, string> = {
  blank_line: "\n\n",
  new_line: "\n",
  space: " ",
};

export interface CombineOptions {
  /** Each take's Transcribe prefix/suffix, or null to leave them out. */
  takeAffixes: Affixes | null;
  /** The combined text's own beginning and end. */
  combineAffixes: Affixes;
  separator: CombineSeparator;
  /** Use a take's text with speakers when it has one. */
  preferSpeakers: boolean;
}

/** The text one take contributes. */
export const takeText = (
  entry: HistoryEntry,
  preferSpeakers: boolean,
): string => {
  const speakers = entry.diarized_text?.trim();
  if (preferSpeakers && speakers) return speakers;
  // The same text the row shows and its Copy copies.
  return entry.transcription_text.trim();
};

/** The takes in queue order as one string. Takes without text are skipped. */
export const combineTakes = (
  entries: HistoryEntry[],
  o: CombineOptions,
): string => {
  const parts = entries
    .map((e) => takeText(e, o.preferSpeakers))
    .filter((t) => t !== "")
    .map((t) => (o.takeAffixes ? applyAffixes(t, o.takeAffixes) : t));
  return applyAffixes(parts.join(SEPARATORS[o.separator]), o.combineAffixes);
};

/** Queue order: as clicked (custom, including drags), oldest first, newest first. */
export type QueueOrder = "custom" | "oldest" | "newest";

export const sortedQueue = (
  ids: number[],
  byId: Map<number, HistoryEntry>,
  order: QueueOrder,
): number[] => {
  if (order === "custom") return ids;
  const ts = (id: number) => Number(byId.get(id)?.timestamp ?? 0);
  return [...ids].sort((a, b) =>
    order === "oldest" ? ts(a) - ts(b) : ts(b) - ts(a),
  );
};

/** Moves the item at `from` to `to` (both indexes in the current list). */
export const moveItem = <T>(list: T[], from: number, to: number): T[] => {
  if (from === to || from < 0 || from >= list.length) return list;
  const next = [...list];
  const [item] = next.splice(from, 1);
  next.splice(Math.max(0, Math.min(to, next.length)), 0, item);
  return next;
};
