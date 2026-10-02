import React from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Download, RefreshCw, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { commands, type UpdaterStatus } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { useOsType } from "@/hooks/useOsType";

/** The states this card shows; the others (idle, disabled, unsupported) show nothing. */
export const isLiveUpdateState = (
  status: UpdaterStatus | null,
): status is UpdaterStatus =>
  !!status && !["idle", "disabled", "unsupported"].includes(status.state);

/**
 * One update in progress: what it is and what you can do. Shared by the
 * sidebar's update banner and the panel above the version. It only renders
 * the status it is given; the caller owns fetching and dismissal.
 */
export const UpdateStatusCard: React.FC<{
  status: UpdaterStatus;
  /** "Remind me later" and the ×; the panel has its own Close instead. */
  onDismiss?: () => void;
}> = ({ status, onDismiss }) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const osType = useOsType();
  const silent = getSetting("automatic_silent_updates") ?? false;
  // In-place installs use the Windows installer; elsewhere offer the download.
  const canInstall = osType === "windows" && !status.portable;

  if (!isLiveUpdateState(status)) return null;

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
  const remindLater = onDismiss && (
    <button
      type="button"
      onClick={onDismiss}
      className="text-accent-text hover:underline cursor-pointer"
    >
      {t("sidebar.update.remindLater")}
    </button>
  );
  const primaryClass =
    "h-7 px-2.5 rounded-md bg-btn text-on-btn text-xs font-medium hover:bg-btn-hover cursor-pointer";

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
          className={primaryClass}
        >
          {t("sidebar.update.downloadPortableZip")}
        </button>
      );
      secondary = remindLater;
    } else if (!canInstall) {
      primary = (
        <button
          type="button"
          onClick={() => void openUrl(status.releases_url)}
          className={primaryClass}
        >
          {t("sidebar.update.downloadInstaller")}
        </button>
      );
      secondary = remindLater;
    } else {
      detail = status.waiting_for_idle
        ? t("sidebar.update.pipelineBusy")
        : silent
          ? t("sidebar.update.silentOn")
          : t("sidebar.update.silentOff");
      primary = (
        <button type="button" onClick={install} className={primaryClass}>
          {status.waiting_for_idle
            ? t("sidebar.update.installWhenIdle")
            : t("sidebar.update.installRestartNow")}
        </button>
      );
      secondary = (
        <>
          {remindLater}
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
    // Stays clickable while waiting: the backend installs only once Handy is
    // idle, and a scheduled wait that ran out would otherwise leave it stuck.
    if (canInstall) {
      primary = (
        <button type="button" onClick={install} className={primaryClass}>
          {status.waiting_for_idle
            ? t("sidebar.update.installWhenIdle")
            : t("sidebar.update.restartInstall")}
        </button>
      );
    }
  } else if (status.state === "installing") {
    title = t("sidebar.update.installing");
    detail = t("sidebar.update.installingDetail");
  } else if (status.state === "failed") {
    title = t("sidebar.update.failed");
    detail = t("sidebar.update.failedDetail", {
      reason: status.error_detail ?? t("sidebar.update.unknownError"),
    });
    primary = (
      <button type="button" onClick={retry} className={primaryClass}>
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
        {status.state === "available" && silent && onDismiss && (
          <button
            type="button"
            aria-label={t("sidebar.update.later")}
            onClick={onDismiss}
          >
            <X className="h-3.5 w-3.5" />
          </button>
        )}
      </div>
    </div>
  );
};
