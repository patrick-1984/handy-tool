/** Longest custom word accepted (the matcher ignores longer candidates). */
export const MAX_CUSTOM_WORD_LENGTH = 50;

/**
 * Cleans one custom word: trims it and drops the characters `<>"'&`.
 * Returns null when nothing usable is left, the word holds a space, or it is
 * longer than {@link MAX_CUSTOM_WORD_LENGTH}.
 */
export const sanitizeCustomWord = (raw: string): string | null => {
  const word = raw.trim().replace(/[<>"'&]/g, "");
  if (!word || /\s/.test(word) || word.length > MAX_CUSTOM_WORD_LENGTH) {
    return null;
  }
  return word;
};

/**
 * Splits pasted or typed text into custom words. Words may be separated by
 * spaces, tabs, new lines, commas or semicolons. List markup (bullets like
 * `-`, `*` or `1.`, and `**bold**` or `` `code` `` around a word) is ignored, so
 * a Markdown list (say, one a chat assistant wrote) pastes cleanly. Each word
 * is sanitized; words already in `existing` or repeated in the text are
 * skipped, and so are invalid ones (too long, or empty once cleaned).
 */
export const parseCustomWords = (
  text: string,
  existing: readonly string[],
): { added: string[]; skipped: number; duplicates: string[] } => {
  const seen = new Set(existing);
  const added: string[] = [];
  const duplicates: string[] = [];
  let skipped = 0;
  for (const raw of text.split(/[\s,;]+/)) {
    const token = raw.replace(/^[*`]+|[*`]+$/g, "");
    if (!/[\p{L}\p{N}]/u.test(token) || /^\d+[.)]$/.test(token)) continue;
    const word = sanitizeCustomWord(token);
    if (word === null) {
      skipped += 1;
    } else if (seen.has(word)) {
      skipped += 1;
      duplicates.push(word);
    } else {
      seen.add(word);
      added.push(word);
    }
  }
  return { added, skipped, duplicates };
};
