import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { commands, type UpdaterStatus } from "@/bindings";

/**
 * The updater's current status, kept live by its "updater-status" event.
 * `refresh` asks the backend again (it recomputes the wait for an idle Handy).
 * A reply that arrives after a newer event is dropped.
 */
export function useUpdaterStatus(): [UpdaterStatus | null, () => void] {
  const [status, setStatus] = useState<UpdaterStatus | null>(null);
  const latest = useRef(0);

  const refresh = useCallback(() => {
    const request = ++latest.current;
    void commands.getUpdaterStatus().then((current) => {
      if (latest.current === request) setStatus(current);
    });
  }, []);

  useEffect(() => {
    let disposed = false;
    const unlisten = listen<UpdaterStatus>("updater-status", (event) => {
      latest.current++;
      setStatus(event.payload);
    });
    // Listen first, then fetch, so no change falls between the two.
    void unlisten.then(() => {
      if (!disposed) refresh();
    });
    return () => {
      disposed = true;
      latest.current++;
      void unlisten.then((stop) => stop());
    };
  }, [refresh]);

  return [status, refresh];
}
