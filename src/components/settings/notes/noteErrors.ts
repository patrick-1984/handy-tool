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
  speakers_models_missing: "settings.notes.errors.speakersModelsMissing",
  speakers_models_in_use: "settings.notes.errors.speakersModelsInUse",
  speakers_models_downloading:
    "settings.notes.errors.speakersModelsDownloading",
  speakers_no_recording: "settings.notes.errors.speakersNoRecording",
  speakers_no_speech: "settings.notes.errors.speakersNoSpeech",
  speakers_no_model: "settings.notes.errors.speakersNoModel",
  speakers_failed: "settings.notes.errors.speakersFailed",
};

/**
 * A translated message for a notes error code. Other errors (network,
 * provider, file system) are shown as they come.
 */
export const translateNoteError = (error: string, t: TFunction): string => {
  const key = NOTE_ERROR_KEYS[error];
  return key ? t(key) : error;
};
