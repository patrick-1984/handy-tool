import React, { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Download, RefreshCw, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { commands, type UpdaterStatus } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";

export const UpdateBanner: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const silent = getSetting("automatic_silent_updates") ?? false;
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

  if (!status || ["idle", "disabled", "unsupported"].includes(status.state)) {
    return null;
  }
  if (dismissedVersion && dismissedVersion === status.version) return null;

  const install = (): void => {
    void commands.installAvailableUpdate();
  };
  const retry = (): void => {
    void commands.checkForUpdates();
  };
  const enableAutomatic = async (): Promise<void> => {
    if (!(getSetting("automatic_update_checks") ?? true)) {
      await updateSetting("automatic_update_checks", true);
    }
    await updateSetting("automatic_silent_updates", true);
  };
  const dismiss = (): void => {
    setDismissedVersion(status.version);
  };

  let title = t("sidebar.update.checking");
  let detail = "";
  let primary: React.ReactNode = null;
  let secondary: React.ReactNode = null;

  if (status.state === "available") {
    title = t("sidebar.update.available", { version: status.version });
    if (status.portable) {
      detail = t("sidebar.update.portableReplaceFolder");
      primary = (
        <button
          type="button"
          onClick={() => void openUrl(status.releases_url)}
          className="h-7 px-2.5 rounded-md bg-btn text-on-btn text-xs font-medium hover:bg-btn-hover cursor-pointer"
        >
          {t("sidebar.update.downloadPortableZip")}
        </button>
      );
      secondary = (
        <button
          type="button"
          onClick={dismiss}
          className="text-accent-text hover:underline cursor-pointer"
        >
          {t("sidebar.update.remindLater")}
        </button>
      );
    } else {
      detail = status.waiting_for_idle
        ? t("sidebar.update.pipelineBusy")
        : silent
          ? t("sidebar.update.silentOn")
          : t("sidebar.update.silentOff");
      primary = (
        <button
          type="button"
          onClick={install}
          className="h-7 px-2.5 rounded-md bg-btn text-on-btn text-xs font-medium hover:bg-btn-hover cursor-pointer"
        >
          {status.waiting_for_idle
            ? t("sidebar.update.installWhenIdle")
            : t("sidebar.update.installRestartNow")}
        </button>
      );
      secondary = (
        <>
          <button
            type="button"
            onClick={dismiss}
            className="text-accent-text hover:underline cursor-pointer"
          >
            {t("sidebar.update.remindLater")}
          </button>
          {!silent && (
            <button
              type="button"
              onClick={() => void enableAutomatic()}
              className="text-accent-text hover:underline cursor-pointer"
            >
              {t("sidebar.update.enableAutomatic")}
            </button>
          )}
        </>
      );
    }
  } else if (status.state === "downloading") {
    title = t("sidebar.update.downloading", { version: status.version });
    detail = t("sidebar.update.percent", {
      percent: status.progress_percent ?? 0,
    });
  } else if (status.state === "ready_to_restart") {
    title = t("sidebar.update.ready", { version: status.version });
    detail = status.waiting_for_idle
      ? t("sidebar.update.waitingForIdle")
      : t("sidebar.update.readyDetail");
    primary = (
      <button
        type="button"
        onClick={install}
        disabled={status.waiting_for_idle}
        className="h-7 px-2.5 rounded-md bg-btn text-on-btn text-xs font-medium hover:bg-btn-hover cursor-pointer disabled:bg-dis-bg disabled:text-dis-text disabled:cursor-not-allowed"
      >
        {status.waiting_for_idle
          ? t("sidebar.update.waiting")
          : t("sidebar.update.restartInstall")}
      </button>
    );
  } else if (status.state === "installing") {
    title = t("sidebar.update.installing");
    detail = t("sidebar.update.installingDetail");
  } else if (status.state === "failed") {
    title = t("sidebar.update.failed");
    detail = t("sidebar.update.failedDetail", {
      reason: status.error_detail ?? t("sidebar.update.unknownError"),
    });
    primary = (
      <button
        type="button"
        onClick={retry}
        className="h-7 px-2.5 rounded-md bg-btn text-on-btn text-xs font-medium hover:bg-btn-hover cursor-pointer"
      >
        {t("sidebar.update.tryAgain")}
      </button>
    );
    secondary = (
      <button
        type="button"
        onClick={() => void openUrl(status.releases_url)}
        className="text-accent-text hover:underline cursor-pointer"
      >
        {t(
          status.portable
            ? "sidebar.update.downloadPortableZip"
            : "sidebar.update.downloadInstaller",
        )}
      </button>
    );
  }

  return (
    <div className="mx-1 mt-2 rounded-lg border border-border bg-surface p-3 text-xs shadow-card">
      <div className="flex items-start gap-2">
        {status.state === "downloading" ? (
          <Download className="mt-0.5 h-4 w-4 shrink-0 text-accent-text" />
        ) : (
          <RefreshCw
            className={`mt-0.5 h-4 w-4 shrink-0 text-accent-text ${status.state === "checking" ? "animate-spin" : ""}`}
          />
        )}
        <div className="min-w-0 flex-1">
          <p className="text-[13px] font-semibold leading-snug">{title}</p>
          {detail && (
            <p className="mt-1 break-words text-text-secondary">{detail}</p>
          )}
          {(primary || secondary) && (
            <div className="mt-2.5 flex flex-wrap items-center gap-x-3 gap-y-1.5">
              {primary}
              {secondary}
            </div>
          )}
        </div>
        {status.state === "available" && silent && (
          <button
            type="button"
            aria-label={t("sidebar.update.later")}
            onClick={() => setDismissedVersion(status.version)}
          >
            <X className="h-3.5 w-3.5" />
          </button>
        )}
      </div>
    </div>
  );
};
