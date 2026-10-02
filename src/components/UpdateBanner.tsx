import React, { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { RefreshCw, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { commands, type UpdaterStatus } from "@/bindings";
import { UpdateStatusCard, isLiveUpdateState } from "./UpdateStatusCard";

export const UpdateBanner: React.FC<{
  /** Hide the update card (not a failed-update notice) while the version's panel shows it. */
  suppressStatus?: boolean;
}> = ({ suppressStatus = false }) => {
  const { t } = useTranslation();
  const [status, setStatus] = useState<UpdaterStatus | null>(null);
  const [dismissedVersion, setDismissedVersion] = useState<string | null>(null);
  // Outcome of the PREVIOUS update attempt, resolved by the backend at startup.
  // Without this the failure mode is invisible: the installer is launched, the
  // process exits, and the app comes back on the old version reporting nothing at
  // all - indistinguishable from "already up to date".
  const [failedUpdate, setFailedUpdate] = useState<{
    expected: string;
    actual: string;
  } | null>(null);
  const [outcomeDismissed, setOutcomeDismissed] = useState(false);

  useEffect(() => {
    let disposed = false;
    void commands.getUpdaterStatus().then((current) => {
      if (!disposed) setStatus(current);
    });
    void commands
      .takeUpdateOutcome()
      .then((outcome) => {
        if (disposed || !outcome) return;
        if (typeof outcome === "object" && "blocked" in outcome) {
          const b = (
            outcome as { blocked: { expected: string; actual: string } }
          ).blocked;
          setFailedUpdate({ expected: b.expected, actual: b.actual });
        }
      })
      .catch(() => {
        /* a missing outcome is normal - most launches follow no update at all */
      });
    const unlisten = listen<UpdaterStatus>("updater-status", (event) => {
      setStatus(event.payload);
      if (event.payload.state === "checking") {
        setDismissedVersion(null);
      }
    });
    return () => {
      disposed = true;
      void unlisten.then((stop) => stop());
    };
  }, []);

  // A failed update outranks any pending-update banner: telling someone an update
  // is available when their last one silently did not apply is worse than useless.
  if (failedUpdate && !outcomeDismissed) {
    return (
      <div className="flex items-center gap-3 border-b border-warn-border bg-warn-bg px-4 py-2 text-[13px]">
        <RefreshCw className="h-4 w-4 shrink-0 text-warn-text" />
        <span className="flex-1">
          {t("updater.failed.message", {
            expected: failedUpdate.expected,
            actual: failedUpdate.actual,
          })}
        </span>
        <button
          type="button"
          onClick={() =>
            void openUrl(
              "https://github.com/patrick-1984/handy-tool/releases/latest",
            )
          }
          className="h-7 rounded-md border border-control-border border-b-control-bottom bg-control px-2.5 hover:bg-control-hover cursor-pointer"
        >
          {t("updater.failed.download")}
        </button>
        <button
          type="button"
          aria-label={t("sidebar.update.later")}
          onClick={() => setOutcomeDismissed(true)}
          className="rounded-md p-1 hover:bg-hover cursor-pointer"
        >
          <X className="h-4 w-4" />
        </button>
      </div>
    );
  }

  // The panel above the version shows the same card while it is open.
  if (suppressStatus || !isLiveUpdateState(status)) return null;
  if (dismissedVersion && dismissedVersion === status.version) return null;

  return (
    <UpdateStatusCard
      status={status}
      onDismiss={() => setDismissedVersion(status.version)}
    />
  );
};
