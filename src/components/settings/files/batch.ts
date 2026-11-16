/**
 * Files › several files (2.7.0): the pure parts - which files, in what order,
 * and how their texts become one.
 */
import type {
  AudioFileInfo,
  BatchSeparator,
  FileBatchOptions,
} from "@/bindings";

/** The file types the backend reads (see decode.rs SUPPORTED_EXTENSIONS). */
export const AUDIO_EXTENSIONS = [
  "wav",
  "mp3",
  "m4a",
  "mp4",
  "aac",
  "flac",
  "ogg",
  "opus",
];

export const fileNameOf = (path: string): string =>
  path.split(/[\/]/).pop() ?? path;

export const isAudioPath = (path: string): boolean =>
  AUDIO_EXTENSIONS.includes(path.split(".").pop()?.toLowerCase() ?? "");

/**
 * The same file picked or dropped twice counts once (Windows paths ignore
 * case). The first occurrence keeps its place.
 */
export const dedupePaths = (paths: string[], ignoreCase: boolean): string[] => {
  const seen = new Set<string>();
  return paths.filter((p) => {
    const key = ignoreCase ? p.toLowerCase() : p;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
};

/** Oldest first; files without a date last; ties by name. */
export const defaultOrder = (items: AudioFileInfo[]): AudioFileInfo[] =>
  [...items].sort((a, b) => {
    const ta = a.created_ms ?? Number.POSITIVE_INFINITY;
    const tb = b.created_ms ?? Number.POSITIVE_INFINITY;
    if (ta !== tb) return ta < tb ? -1 : 1;
    return a.file_name.localeCompare(b.file_name, undefined, {
      numeric: true,
    });
  });

export interface ResolvedBatchOptions {
  separator: BatchSeparator;
  numbered: boolean;
  names: boolean;
}

export const resolveBatchOptions = (
  options: FileBatchOptions | null | undefined,
): ResolvedBatchOptions => ({
  separator: options?.separator ?? "blank_line",
  numbered: options?.numbered ?? false,
  names: options?.names ?? false,
});

/** The divider line between parts. */
export const DIVIDER = "---";

const SEPARATORS: Record<BatchSeparator, string> = {
  none: "\n",
  blank_line: "\n\n",
  divider: `\n\n${DIVIDER}\n\n`,
};

/**
 * One text from the parts, in their order. Parts without text (failed, empty)
 * are left out, and the numbers count only the parts that are there.
 */
export const combineBatch = (
  parts: { fileName: string; text?: string | null }[],
  options: ResolvedBatchOptions,
): string =>
  parts
    .map((p) => ({ name: p.fileName, text: (p.text ?? "").trim() }))
    .filter((p) => p.text.length > 0)
    .map((p, i) => {
      const number = options.numbered ? `${i + 1}. ` : "";
      return options.names
        ? `${number}${p.name}\n${p.text}`
        : `${number}${p.text}`;
    })
    .join(SEPARATORS[options.separator]);
