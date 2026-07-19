/**
 * History full-text search matching.
 *
 * "Assumed regex": if the query contains regex metacharacters and compiles,
 * it is used as a live regular expression; otherwise (plain text or an
 * invalid pattern) it falls back to a literal, escaped substring match.
 * All matching is case-insensitive.
 */

export interface SearchMatcher {
  /** Global case-insensitive regex used for matching and highlighting. */
  regex: RegExp;
  /** True when the query is being treated as a live regular expression. */
  isRegex: boolean;
}

const META_CHARS = /[\\^$.|?*+()[\]{}]/;

function escapeLiteral(text: string): string {
  return text.replace(/[\\^$.|?*+()[\]{}]/g, "\\$&");
}

export function buildMatcher(query: string): SearchMatcher | null {
  const trimmed = query.trim();
  if (!trimmed) return null;

  if (META_CHARS.test(trimmed)) {
    try {
      return { regex: new RegExp(trimmed, "giu"), isRegex: true };
    } catch {
      // Invalid pattern: treat as literal text below.
    }
  }
  return { regex: new RegExp(escapeLiteral(trimmed), "giu"), isRegex: false };
}

/** Test whether any of the given fields matches. Resets regex state. */
export function fieldsMatch(
  matcher: SearchMatcher,
  fields: (string | null | undefined)[],
): boolean {
  return fields.some((field) => {
    if (!field) return false;
    matcher.regex.lastIndex = 0;
    return matcher.regex.test(field);
  });
}

export interface SnippetResult {
  /** The (possibly truncated) text to display. */
  text: string;
  /** Whether text was cut before/after. */
  leadingEllipsis: boolean;
  trailingEllipsis: boolean;
}

/**
 * For long texts, return a window centered on the first match so the hit is
 * visible without rendering the full transcript.
 */
export function snippetAroundFirstMatch(
  text: string,
  matcher: SearchMatcher,
  maxLength = 320,
): SnippetResult {
  if (text.length <= maxLength) {
    return { text, leadingEllipsis: false, trailingEllipsis: false };
  }
  matcher.regex.lastIndex = 0;
  const first = matcher.regex.exec(text);
  if (!first || first.index < maxLength / 2) {
    return {
      text: text.slice(0, maxLength),
      leadingEllipsis: false,
      trailingEllipsis: true,
    };
  }
  const start = Math.max(0, first.index - 120);
  const end = Math.min(text.length, first.index + first[0].length + 200);
  return {
    text: text.slice(start, end),
    leadingEllipsis: start > 0,
    trailingEllipsis: end < text.length,
  };
}

export interface HighlightSegment {
  text: string;
  isMatch: boolean;
}

/** Split text into match / non-match segments for <mark> rendering. */
export function highlightSegments(
  text: string,
  matcher: SearchMatcher,
): HighlightSegment[] {
  const segments: HighlightSegment[] = [];
  let last = 0;
  let guard = 0;
  let m: RegExpExecArray | null;
  matcher.regex.lastIndex = 0;
  while ((m = matcher.regex.exec(text)) !== null && guard++ < 1000) {
    if (m[0] === "") {
      // Zero-length match (e.g. "a*"): advance to avoid an infinite loop.
      matcher.regex.lastIndex++;
      continue;
    }
    if (m.index > last) {
      segments.push({ text: text.slice(last, m.index), isMatch: false });
    }
    segments.push({ text: m[0], isMatch: true });
    last = m.index + m[0].length;
  }
  if (last < text.length) {
    segments.push({ text: text.slice(last), isMatch: false });
  }
  return segments;
}
