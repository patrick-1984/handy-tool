import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import {
  commands,
  type AudioFileInfo,
  type FileJob,
  type FileTextSave,
} from "@/bindings";
import {
  combineBatch,
  defaultOrder,
  type ResolvedBatchOptions,
} from "@/components/settings/files/batch";

/**
 * Files › several files (2.7.0). The batch lives here, not in the page, so it
 * keeps going when another page is opened (Back, a pinned page...).
 */

export type PartStatus =
  | "pending"
  | "running"
  | "done"
  | "failed"
  | "cancelled";

export interface BatchPart {
  path: string;
  fileName: string;
  status: PartStatus;
  text?: string;
  error?: string;
}

type Phase = "idle" | "ordering" | "running" | "done";

interface FileBatchState {
  phase: Phase;
  /** The files in the Order files window, in their order. */
  items: AudioFileInfo[];
  parts: BatchPart[];
  /** The file being transcribed (index into parts). */
  current: number;
  /** Its progress, from the backend's file-job events. */
  progress: { done: number; total: number } | null;
  jobId: number | null;
  /** Every job this batch started (their single-file view stays hidden). */
  jobIds: number[];
  cancelRequested: boolean;
  combined: string;
  savedPath: string | null;
  saveError: string | null;
  /** Why the batch stopped before the end (an update, another job). */
  stopError: string | null;
  /** The combined text is being saved: too late to cancel. */
  saving: boolean;

  /** Open the Order files window for these (supported, distinct) files. */
  openOrder: (paths: string[]) => Promise<void>;
  /** Files dropped while the window is open join the end of the list. */
  addPaths: (paths: string[]) => Promise<void>;
  setItems: (items: AudioFileInfo[]) => void;
  remove: (path: string) => void;
  closeOrder: () => void;
  start: (
    modelId: string,
    save: FileTextSave,
    options: ResolvedBatchOptions,
  ) => Promise<void>;
  cancel: () => void;
  dismiss: () => void;
}

/** Bumped when the Order files window closes or a batch starts: a file-info
 *  answer that arrives later must not reopen or change it. */
let orderGeneration = 0;

/** Terminal file-job states by job id, and who waits for which. */
const finished = new Map<number, FileJob>();
const waiters = new Map<number, (job: FileJob) => void>();

const isTerminal = (job: FileJob) =>
  job.state === "done" || job.state === "failed" || job.state === "cancelled";

/** Errors that stop the whole batch rather than one file. */
const stopsBatch = (error: string) =>
  error.includes("update is being installed") ||
  error.includes("already being transcribed");

const infosFor = async (paths: string[]) => {
  try {
    return await commands.audioFilesInfo(paths);
  } catch {
    return paths.map(
      (path): AudioFileInfo => ({
        path,
        file_name: path.split(/[\\/]/).pop() ?? path,
        supported: true,
        created_ms: null,
        duration_secs: null,
      }),
    );
  }
};

export const useFileBatchStore = create<FileBatchState>()((set, get) => {
  const setPart = (index: number, patch: Partial<BatchPart>) =>
    set((s) => ({
      parts: s.parts.map((p, i) => (i === index ? { ...p, ...patch } : p)),
    }));

  const waitFor = (jobId: number): Promise<FileJob> =>
    new Promise((resolve) => {
      const done = finished.get(jobId);
      if (done) {
        finished.delete(jobId);
        resolve(done);
        return;
      }
      waiters.set(jobId, resolve);
      // An end that came before anyone listened is still in the backend.
      void commands.getFileJob().then((job) => {
        if (job && job.job_id === jobId && isTerminal(job)) {
          const waiter = waiters.get(jobId);
          if (waiter) {
            waiters.delete(jobId);
            waiter(job);
          }
        }
      });
    });

  return {
    phase: "idle",
    items: [],
    parts: [],
    current: 0,
    progress: null,
    jobId: null,
    jobIds: [],
    cancelRequested: false,
    combined: "",
    savedPath: null,
    saveError: null,
    stopError: null,
    saving: false,

    openOrder: async (paths) => {
      const gen = ++orderGeneration;
      const infos = await infosFor(paths);
      // Closed, reopened or started meanwhile: this answer is stale.
      if (gen !== orderGeneration || get().phase === "running") return;
      set({
        phase: "ordering",
        items: defaultOrder(infos.filter((i) => i.supported)),
      });
    },
    addPaths: async (paths) => {
      const gen = orderGeneration;
      const infos = await infosFor(paths);
      if (gen !== orderGeneration || get().phase !== "ordering") return;
      set((s) => {
        const known = new Set(s.items.map((i) => i.path.toLowerCase()));
        const fresh = infos.filter(
          (i) => i.supported && !known.has(i.path.toLowerCase()),
        );
        return { items: [...s.items, ...fresh] };
      });
    },
    setItems: (items) => set({ items }),
    remove: (path) =>
      set((s) => ({ items: s.items.filter((i) => i.path !== path) })),
    closeOrder: () => {
      orderGeneration++;
      set({ phase: "idle", items: [] });
    },

    start: async (modelId, save, options) => {
      const items = get().items;
      if (items.length === 0 || get().phase === "running") return;
      orderGeneration++;
      finished.clear();
      waiters.clear();
      // Claimed at once, so a second click cannot start a second batch.
      set({
        phase: "running",
        parts: items.map((i) => ({
          path: i.path,
          fileName: i.file_name,
          status: "pending",
        })),
        current: 0,
        progress: null,
        jobId: null,
        jobIds: [],
        cancelRequested: false,
        combined: "",
        savedPath: null,
        saveError: null,
        stopError: null,
      });
      let token: number | null = null;
      let unlisten: (() => void) | null = null;
      try {
        const hold = await commands.beginFileBatch();
        if (hold.status === "error") throw new Error(hold.error);
        token = hold.data;
        unlisten = await listen<FileJob>("file-job", (e) => {
          const job = e.payload;
          if (job.job_id === get().jobId && job.state === "transcribing") {
            set({ progress: { done: job.done_secs, total: job.total_secs } });
          }
          if (!isTerminal(job)) return;
          const waiter = waiters.get(job.job_id);
          if (waiter) {
            waiters.delete(job.job_id);
            waiter(job);
          } else {
            finished.set(job.job_id, job);
          }
        });
        for (let i = 0; i < items.length; i++) {
          if (get().cancelRequested) break;
          set({ current: i, progress: null, jobId: null });
          setPart(i, { status: "running" });
          // "dont_save": the combined text is saved once, at the end.
          const started = await commands.transcribeFile(
            items[i].path,
            modelId,
            "dont_save",
          );
          if (started.status === "error") {
            setPart(i, { status: "failed", error: started.error });
            if (stopsBatch(started.error)) {
              set({ stopError: started.error });
              break;
            }
            continue;
          }
          set((s) => ({
            jobId: started.data,
            jobIds: [...s.jobIds, started.data],
          }));
          // Cancel pressed while it was starting.
          if (get().cancelRequested) {
            void commands.cancelFileJob(started.data);
          }
          const job = await waitFor(started.data);
          if (job.state === "done") {
            setPart(i, { status: "done", text: job.entry.text });
          } else if (job.state === "failed") {
            setPart(i, { status: "failed", error: job.error });
          } else {
            setPart(i, { status: "cancelled" });
            break;
          }
        }

        const parts = get().parts.map((p) =>
          p.status === "pending" || p.status === "running"
            ? { ...p, status: "cancelled" as const }
            : p,
        );
        const combined = combineBatch(parts, options);
        set({ parts, combined });
        // Saved once - while the hold still keeps an update from starting -
        // and only when every file was tried and some text came out.
        const complete = !get().cancelRequested && !get().stopError;
        if (complete && combined && save !== "dont_save" && save !== "ask") {
          set({ saving: true });
          const saved = await commands.saveBatchText(
            combined,
            items[0].path,
            save,
          );
          if (saved.status === "ok") set({ savedPath: saved.data });
          else set({ saveError: saved.error });
        }
      } catch (error) {
        set({
          stopError: error instanceof Error ? error.message : String(error),
        });
      } finally {
        unlisten?.();
        if (token !== null) {
          await commands.endFileBatch(token, modelId).catch(() => undefined);
        }
        // Whatever happened, the batch ends: no spinner left behind.
        set((s) => ({
          phase: "done",
          saving: false,
          jobId: null,
          progress: null,
          parts: s.parts.map((p) =>
            p.status === "pending" || p.status === "running"
              ? { ...p, status: "cancelled" as const }
              : p,
          ),
        }));
      }
    },

    cancel: () => {
      set({ cancelRequested: true });
      const jobId = get().jobId;
      if (jobId !== null) void commands.cancelFileJob(jobId);
    },

    dismiss: () =>
      set({
        phase: "idle",
        items: [],
        parts: [],
        combined: "",
        savedPath: null,
        saveError: null,
        stopError: null,
      }),
  };
});
