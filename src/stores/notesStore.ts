import { create } from "zustand";
import {
  commands,
  type AppSettings,
  type LlmProvider,
  type Note,
  type NoteSkill,
} from "@/bindings";
import { useSettingsStore } from "./settingsStore";

/**
 * A note being made for a History entry, shown inline under the entry.
 * `text` is what the note is made from, so Try again can resend it;
 * `withSpeakers` marks a note made from a speaker-labelled transcript.
 * `fallbackText` is the entry's own text, for a normal note when speaker
 * detection can't run.
 */
export type HistoryNoteJob =
  /** "Make note with speakers": transcribing the recording with labels. */
  | { status: "identifying" }
  /** The speaker models must be downloaded first. */
  | {
      status: "speakerModels";
      downloading: boolean;
      error: string | null;
      fallbackText: string;
    }
  /** Speaker detection failed; `error` is an error code or message. */
  | { status: "speakersError"; error: string; fallbackText: string }
  | { status: "generating"; withSpeakers: boolean }
  /** The provider, its key or the model is missing; `error` is its code. */
  | { status: "needsSetup"; error: string; text: string; withSpeakers: boolean }
  | { status: "error"; error: string; text: string; withSpeakers: boolean };

/** Whether a job is running, so the entry's note buttons stay disabled. */
export const isHistoryJobBusy = (job: HistoryNoteJob | undefined): boolean =>
  job?.status === "identifying" ||
  job?.status === "generating" ||
  (job?.status === "speakerModels" && job.downloading);

/** Error code of a speaker transcription without the speaker models. */
const MODELS_MISSING_ERROR = "speakers_models_missing";

/** Error codes that mean the note settings are incomplete. */
export const SETUP_ERRORS = new Set([
  "note_missing_provider",
  "note_unsupported_provider",
  "note_missing_api_key",
  "note_missing_model",
]);

/** Provider kinds that can write notes (mirrors CHAT_KINDS in notes.rs). */
export const NOTE_PROVIDER_KINDS = [
  "openrouter",
  "openai_compatible",
  "anthropic",
  "gemini",
];
/** Kinds that need an API key (mirrors KEY_REQUIRED_KINDS in notes.rs). */
const KEY_REQUIRED_KINDS = ["openrouter", "anthropic", "gemini"];

/**
 * The provider notes are written with: the chosen one, else the first
 * enabled OpenRouter provider, else the first OpenRouter provider (mirrors
 * `AppSettings::note_provider`).
 */
export const noteProvider = (
  settings: AppSettings | null,
): LlmProvider | null => {
  const providers = settings?.llm_providers ?? [];
  const ref = settings?.note_provider_ref ?? "";
  if (ref) return providers.find((p) => p.id === ref) ?? null;
  const openrouter = providers.filter((p) => p.kind === "openrouter");
  return openrouter.find((p) => p.enabled) ?? openrouter[0] ?? null;
};

/**
 * What still keeps notes from being written, as the error code the backend
 * would return, or null when the settings are complete.
 */
export const noteSetupProblem = (
  settings: AppSettings | null,
): string | null => {
  const provider = noteProvider(settings);
  if (!provider) return "note_missing_provider";
  if (!NOTE_PROVIDER_KINDS.includes(provider.kind)) {
    return "note_unsupported_provider";
  }
  if (
    KEY_REQUIRED_KINDS.includes(provider.kind) &&
    (provider.api_key ?? "").trim() === ""
  ) {
    return "note_missing_api_key";
  }
  const model = (settings?.note_model ?? "").trim() || provider.model.trim();
  if (!model) return "note_missing_model";
  return null;
};

interface NotesStore {
  manualText: string;
  manualGenerating: boolean;
  manualError: string | null;
  /** The note produced by the last manual generation. */
  manualResult: Note | null;

  /** All notes, for the Saved notes tab. */
  notes: Note[];
  notesLoading: boolean;

  /** Notes per History entry id, newest first, for the inline cards. */
  historyNotes: Record<number, Note[]>;
  historyJobs: Record<number, HistoryNoteJob>;

  skills: NoteSkill[];

  setManualText: (text: string) => void;
  generateManualNote: () => Promise<void>;

  /**
   * Make a note from `text` and show it under the History entry. "Make
   * note" passes the entry's text; a speaker-labelled transcript passes
   * `withSpeakers`.
   */
  generateHistoryNote: (
    historyId: number,
    text: string,
    withSpeakers?: boolean,
  ) => Promise<void>;
  /**
   * "Make note with speakers": transcribe the entry's recording with speaker
   * labels, then make the note from that. Asks for the speaker model
   * download first when it is missing. `fallbackText` is the entry's text,
   * offered as a normal note if speaker detection fails.
   */
  makeHistoryNoteWithSpeakers: (
    historyId: number,
    fallbackText: string,
  ) => Promise<void>;
  /** Download the speaker models from an entry's card, then continue. */
  downloadSpeakerModelsForHistory: (historyId: number) => Promise<void>;
  dismissHistoryJob: (historyId: number) => void;
  /** Load the notes of History entries whose notes aren't loaded yet. */
  loadNotesForHistory: (historyIds: number[]) => Promise<void>;

  loadNotes: () => Promise<void>;
  deleteNote: (id: number) => Promise<boolean>;

  loadSkills: () => Promise<void>;
}

/** Newest first, without duplicates. */
const mergeNotes = (...lists: Note[][]): Note[] => {
  const byId = new Map<number, Note>();
  for (const note of lists.flat()) byId.set(note.id, note);
  return [...byId.values()].sort((a, b) => b.id - a.id);
};

const withoutKey = <T>(record: Record<number, T>, key: number) => {
  const { [key]: _removed, ...rest } = record;
  return rest;
};

/** History entry ids whose notes were requested, so each loads once. */
const requestedHistoryIds = new Set<number>();

export const useNotesStore = create<NotesStore>()((set, get) => ({
  manualText: "",
  manualGenerating: false,
  manualError: null,
  manualResult: null,
  notes: [],
  notesLoading: false,
  historyNotes: {},
  historyJobs: {},
  skills: [],

  setManualText: (text) => set({ manualText: text }),

  generateManualNote: async () => {
    const { manualText, manualGenerating } = get();
    if (manualGenerating || manualText.trim() === "") return;

    set({ manualGenerating: true, manualError: null, manualResult: null });
    try {
      const result = await commands.generateNote(manualText, null, false);
      if (result.status === "ok") {
        set((state) => ({
          manualResult: result.data,
          notes: mergeNotes([result.data], state.notes),
        }));
      } else {
        set({ manualError: result.error });
      }
    } catch (error) {
      set({ manualError: String(error) });
    } finally {
      set({ manualGenerating: false });
    }
  },

  generateHistoryNote: async (historyId, text, withSpeakers = false) => {
    const current = get().historyJobs[historyId];
    // "Make note with speakers" continues here from its identifying step.
    const continuesSpeakers = withSpeakers && current?.status === "identifying";
    if (isHistoryJobBusy(current) && !continuesSpeakers) return;

    const setJob = (job: HistoryNoteJob) =>
      set((state) => ({
        historyJobs: { ...state.historyJobs, [historyId]: job },
      }));

    setJob({ status: "generating", withSpeakers });
    try {
      const result = await commands.generateNote(text, historyId, withSpeakers);
      if (result.status === "ok") {
        const note = result.data;
        set((state) => ({
          historyJobs: withoutKey(state.historyJobs, historyId),
          historyNotes: {
            ...state.historyNotes,
            [historyId]: mergeNotes(
              [note],
              state.historyNotes[historyId] ?? [],
            ),
          },
          notes: mergeNotes([note], state.notes),
        }));
      } else if (SETUP_ERRORS.has(result.error)) {
        setJob({
          status: "needsSetup",
          error: result.error,
          text,
          withSpeakers,
        });
      } else {
        setJob({ status: "error", error: result.error, text, withSpeakers });
      }
    } catch (error) {
      setJob({ status: "error", error: String(error), text, withSpeakers });
    }
  },

  makeHistoryNoteWithSpeakers: async (historyId, fallbackText) => {
    if (isHistoryJobBusy(get().historyJobs[historyId])) return;

    const setJob = (job: HistoryNoteJob) =>
      set((state) => ({
        historyJobs: { ...state.historyJobs, [historyId]: job },
      }));
    setJob({ status: "identifying" });
    try {
      const result =
        await commands.transcribeHistoryEntryWithSpeakers(historyId);
      if (result.status === "ok") {
        await get().generateHistoryNote(historyId, result.data, true);
      } else if (result.error === MODELS_MISSING_ERROR) {
        setJob({
          status: "speakerModels",
          downloading: false,
          error: null,
          fallbackText,
        });
      } else {
        setJob({ status: "speakersError", error: result.error, fallbackText });
      }
    } catch (error) {
      setJob({ status: "speakersError", error: String(error), fallbackText });
    }
  },

  downloadSpeakerModelsForHistory: async (historyId) => {
    const job = get().historyJobs[historyId];
    if (job?.status !== "speakerModels" || job.downloading) return;
    const { fallbackText } = job;

    // Update the card only while it is still showing (not dismissed or
    // replaced by another note).
    const updateCard = (downloading: boolean, error: string | null) =>
      set((state) =>
        state.historyJobs[historyId]?.status === "speakerModels"
          ? {
              historyJobs: {
                ...state.historyJobs,
                [historyId]: {
                  status: "speakerModels",
                  downloading,
                  error,
                  fallbackText,
                },
              },
            }
          : {},
      );

    updateCard(true, null);
    let error: string | null = null;
    try {
      const result = await commands.downloadSpeakerModels();
      if (result.status !== "ok") error = result.error;
    } catch (e) {
      error = String(e);
    }
    if (get().historyJobs[historyId]?.status !== "speakerModels") return;
    if (error !== null) {
      updateCard(false, error);
      return;
    }
    // Downloaded: continue with the note.
    set((state) => ({
      historyJobs: withoutKey(state.historyJobs, historyId),
    }));
    await get().makeHistoryNoteWithSpeakers(historyId, fallbackText);
  },

  dismissHistoryJob: (historyId) =>
    set((state) => ({
      historyJobs: withoutKey(state.historyJobs, historyId),
    })),

  loadNotesForHistory: async (historyIds) => {
    const ids = historyIds.filter((id) => !requestedHistoryIds.has(id));
    if (ids.length === 0) return;
    ids.forEach((id) => requestedHistoryIds.add(id));

    try {
      const result = await commands.getNotesForHistoryIds(ids);
      if (result.status !== "ok") throw new Error(result.error);
      const fetched = new Map<number, Note[]>();
      for (const note of result.data) {
        if (note.history_id === null) continue;
        fetched.set(note.history_id, [
          ...(fetched.get(note.history_id) ?? []),
          note,
        ]);
      }
      set((state) => {
        const historyNotes = { ...state.historyNotes };
        for (const [id, notes] of fetched) {
          // Keep notes generated while the request was running.
          historyNotes[id] = mergeNotes(notes, historyNotes[id] ?? []);
        }
        return { historyNotes };
      });
    } catch (error) {
      console.error("Failed to load notes for history:", error);
      ids.forEach((id) => requestedHistoryIds.delete(id));
    }
  },

  loadNotes: async () => {
    set({ notesLoading: true });
    try {
      const result = await commands.getNotes();
      if (result.status === "ok") {
        set({ notes: result.data });
      } else {
        console.error("Failed to load notes:", result.error);
      }
    } catch (error) {
      console.error("Failed to load notes:", error);
    } finally {
      set({ notesLoading: false });
    }
  },

  deleteNote: async (id) => {
    const { notes, historyNotes, manualResult } = get();
    const removed =
      notes.find((n) => n.id === id) ??
      Object.values(historyNotes)
        .flat()
        .find((n) => n.id === id);
    const wasManualResult = manualResult?.id === id;
    set((state) => {
      const sourceId = removed?.history_id;
      return {
        notes: state.notes.filter((n) => n.id !== id),
        historyNotes:
          sourceId != null && state.historyNotes[sourceId]
            ? {
                ...state.historyNotes,
                [sourceId]: state.historyNotes[sourceId].filter(
                  (n) => n.id !== id,
                ),
              }
            : state.historyNotes,
        manualResult: state.manualResult?.id === id ? null : state.manualResult,
      };
    });
    try {
      const result = await commands.deleteNote(id);
      if (result.status === "ok") return true;
      console.error("Failed to delete note:", result.error);
    } catch (error) {
      console.error("Failed to delete note:", error);
    }
    // Put back only the note that failed to delete, so changes made while
    // the request was running are kept.
    if (removed) {
      set((state) => {
        const sourceId = removed.history_id;
        return {
          notes: mergeNotes([removed], state.notes),
          historyNotes:
            sourceId != null
              ? {
                  ...state.historyNotes,
                  [sourceId]: mergeNotes(
                    [removed],
                    state.historyNotes[sourceId] ?? [],
                  ),
                }
              : state.historyNotes,
          manualResult:
            wasManualResult && state.manualResult === null
              ? removed
              : state.manualResult,
        };
      });
    }
    return false;
  },

  loadSkills: async () => {
    try {
      const result = await commands.getNoteSkills();
      if (result.status !== "ok") {
        console.error("Failed to load skills:", result.error);
        return;
      }
      set({ skills: result.data });
      // The backend drops a selected skill that no longer exists; pick up
      // that change so the pickers and the backend agree.
      const settingsStore = useSettingsStore.getState();
      const selectedId = settingsStore.settings?.note_skill_id;
      if (selectedId && !result.data.some((s) => s.id === selectedId)) {
        await settingsStore.refreshSettings();
      }
    } catch (error) {
      console.error("Failed to load skills:", error);
    }
  },
}));

// Once the provider, key and model are set, drop the "settings missing"
// cards so they don't keep showing a stale message under History entries.
useSettingsStore.subscribe((state) => {
  const { historyJobs } = useNotesStore.getState();
  const stale = Object.entries(historyJobs).filter(
    ([, job]) => job.status === "needsSetup",
  );
  if (stale.length === 0 || noteSetupProblem(state.settings) !== null) return;
  useNotesStore.setState((notesState) => {
    const jobs = { ...notesState.historyJobs };
    for (const [id] of stale) {
      if (jobs[Number(id)]?.status === "needsSetup") delete jobs[Number(id)];
    }
    return { historyJobs: jobs };
  });
});
