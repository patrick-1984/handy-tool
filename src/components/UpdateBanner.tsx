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

  useEffect(() => {
    let disposed = false;
    void commands.getUpdaterStatus().then((current) => {
      if (!disposed) setStatus(current);
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
          className="font-semibold underline"
        >
          {t("sidebar.update.downloadPortableZip")}
        </button>
      );
      secondary = (
        <button type="button" onClick={dismiss} className="underline">
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
          className="font-semibold underline"
        >
          {status.waiting_for_idle
            ? t("sidebar.update.installWhenIdle")
            : t("sidebar.update.installRestartNow")}
        </button>
      );
      secondary = (
        <>
          <button type="button" onClick={dismiss} className="underline">
            {t("sidebar.update.remindLater")}
          </button>
          {!silent && (
            <button
              type="button"
              onClick={() => void enableAutomatic()}
              className="underline"
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
        className="font-semibold underline disabled:no-underline disabled:opacity-60"
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
      <button type="button" onClick={retry} className="font-semibold underline">
        {t("sidebar.update.tryAgain")}
      </button>
    );
    secondary = (
      <button
        type="button"
        onClick={() => void openUrl(status.releases_url)}
        className="underline"
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
    <div className="mx-1 mt-1 rounded-lg border border-logo-primary/40 bg-logo-primary/10 p-2 text-xs">
      <div className="flex items-start gap-2">
        {status.state === "downloading" ? (
          <Download className="mt-0.5 h-4 w-4 shrink-0" />
        ) : (
          <RefreshCw
            className={`mt-0.5 h-4 w-4 shrink-0 ${status.state === "checking" ? "animate-spin" : ""}`}
          />
        )}
        <div className="min-w-0 flex-1">
          <p className="font-medium leading-snug">{title}</p>
          {detail && <p className="mt-1 break-words text-text/70">{detail}</p>}
          {(primary || secondary) && (
            <div className="mt-2 flex flex-wrap gap-x-2 gap-y-1">
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
