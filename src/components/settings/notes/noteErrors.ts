import type { TFunction } from "i18next";

/** Translation keys for the error codes the notes commands return. */
const NOTE_ERROR_KEYS: Record<string, string> = {
  note_empty_transcript: "settings.notes.errors.emptyTranscript",
  note_missing_provider: "settings.notes.errors.missingProvider",
  note_unsupported_provider: "settings.notes.errors.unsupportedProvider",
  note_missing_api_key: "settings.notes.errors.missingApiKey",
  note_missing_model: "settings.notes.errors.missingModel",
  note_empty: "settings.notes.errors.emptyNote",
  skill_missing: "settings.notes.errors.skillMissing",
  skill_empty: "settings.notes.errors.skillEmpty",
  skill_unsupported: "settings.notes.errors.skillUnsupported",
  skill_too_large: "settings.notes.errors.skillTooLarge",
  skill_unsafe_path: "settings.notes.errors.skillUnsafePath",
};

/**
 * A translated message for a notes error code. Other errors (network,
 * provider, file system) are shown as they come.
 */
export const translateNoteError = (error: string, t: TFunction): string => {
  const key = NOTE_ERROR_KEYS[error];
  return key ? t(key) : error;
};
