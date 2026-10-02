import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { commands, type SpeakerModelStatus } from "@/bindings";

/** Emitted by the backend while the speaker models download, and on delete. */
const STATUS_EVENT = "speaker-model-status";

/**
 * Download state of the speaker models used by "Make note with speakers",
 * kept current from the backend's progress events. null until loaded.
 */
export const useSpeakerModelStatus = (): SpeakerModelStatus | null => {
  const [status, setStatus] = useState<SpeakerModelStatus | null>(null);

  useEffect(() => {
    let active = true;
    const unlisten = listen<SpeakerModelStatus>(STATUS_EVENT, (event) => {
      if (active) setStatus(event.payload);
    });
    commands
      .getSpeakerModelStatus()
      .then((current) => {
        // An event that arrived meanwhile is at least as new.
        if (active) setStatus((previous) => previous ?? current);
      })
      .catch((error) => {
        console.error("Failed to get speaker model status:", error);
      });
    return () => {
      active = false;
      void unlisten.then((fn) => fn());
    };
  }, []);

  return status;
};

/** Download progress in whole percent. */
export const speakerDownloadPercent = (status: SpeakerModelStatus): number =>
  status.total > 0 ? Math.floor((status.downloaded / status.total) * 100) : 0;
