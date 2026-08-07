import React, { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { commands, type UpdaterStatus } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { Button } from "../../ui/Button";

const RELEASES_URL =
  "https://github.com/patrick-1984/handy-tool/releases/latest";

interface AboutUpdateCheckProps {
  currentVersion: string;
}

export const AboutUpdateCheck: React.FC<AboutUpdateCheckProps> = ({
  currentVersion,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const silent = getSetting("automatic_silent_updates") ?? false;
  const [status, setStatus] = useState<UpdaterStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [manualResultVisible, setManualResultVisible] = useState(false);
  const [manualError, setManualError] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    void commands.getUpdaterStatus().then((current) => {
      if (!disposed) setStatus(current);
    });
    const unlisten = listen<UpdaterStatus>("updater-status", (event) => {
      setStatus(event.payload);
    });
    return () => {
      disposed = true;
      void unlisten.then((stop) => stop());
    };
  }, []);

  const checkNow = async (): Promise<void> => {
    setManualResultVisible(true);
    setManualError(null);
    setChecking(true);
    try {
      const result = await commands.checkForUpdates();
      if (result.status === "ok") {
        setStatus(result.data);
      } else {
        setManualError(result.error);
      }
    } catch (error) {
      setManualError(error instanceof Error ? error.message : String(error));
    } finally {
      setChecking(false);
    }
  };

  const install = (): void => {
    void commands.installAvailableUpdate();
  };

  const enableAutomatic = async (): Promise<void> => {
    if (!(getSetting("automatic_update_checks") ?? true)) {
      await updateSetting("automatic_update_checks", true);
    }
    await updateSetting("automatic_silent_updates", true);
  };

  const errorReason =
    manualError ??
    (status?.state === "failed" ? status.error_detail : null) ??
    t("sidebar.update.unknownError");
  const showResult = manualResultVisible && !checking;

  return (
    <div className="w-full space-y-3">
      <div className="flex flex-wrap gap-2">
        <Button
          type="button"
          variant="secondary"
          size="md"
          onClick={() => void openUrl(RELEASES_URL)}
        >
          {t("settings.about.updates.checkWeb")}
        </Button>
        <Button
          type="button"
          variant="secondary"
          size="md"
          onClick={() => void checkNow()}
          disabled={checking}
        >
          {checking && <RefreshCw className="h-4 w-4 animate-spin" />}
          {checking
            ? t("sidebar.update.checking")
            : t("settings.about.updates.checkApp")}
        </Button>
      </div>

      {showResult && manualError && (
        <div className="space-y-2 text-sm" role="status">
          <p>
            {t("settings.about.updates.checkFailed", { reason: errorReason })}
          </p>
          <div className="flex flex-wrap gap-2">
            <Button type="button" size="sm" onClick={() => void checkNow()}>
              {t("sidebar.update.tryAgain")}
            </Button>
            <span className="self-center text-xs text-mid-gray">
              {t("settings.about.updates.useWebHint")}
            </span>
          </div>
        </div>
      )}

      {showResult && !manualError && status?.state === "idle" && (
        <p className="text-sm" role="status">
          {t("settings.about.updates.latestVersion", {
            version: currentVersion,
          })}
        </p>
      )}

      {showResult && !manualError && status?.state === "available" && (
        <div className="space-y-2 text-sm" role="status">
          <p>
            {t("settings.about.updates.availableVersion", {
              version: status.version,
            })}
          </p>
          {status.portable ? (
            <>
              <p className="text-xs text-mid-gray">
                {t("sidebar.update.portableReplaceFolder")}
              </p>
              <div className="flex flex-wrap gap-2">
                <Button
                  type="button"
                  size="sm"
                  onClick={() => void openUrl(status.releases_url)}
                >
                  {t("sidebar.update.downloadPortableZip")}
                </Button>
                <Button
                  type="button"
                  variant="secondary"
                  size="sm"
                  onClick={() => setManualResultVisible(false)}
                >
                  {t("sidebar.update.remindLater")}
                </Button>
              </div>
            </>
          ) : (
            <>
              {status.waiting_for_idle && (
                <p className="text-xs text-amber-500">
                  {t("sidebar.update.pipelineBusy")}
                </p>
              )}
              <div className="flex flex-wrap gap-2">
                <Button type="button" size="sm" onClick={install}>
                  {status.waiting_for_idle
                    ? t("sidebar.update.installWhenIdle")
                    : t("sidebar.update.installRestartNow")}
                </Button>
                <Button
                  type="button"
                  variant="secondary"
                  size="sm"
                  onClick={() => setManualResultVisible(false)}
                >
                  {t("sidebar.update.remindLater")}
                </Button>
                {!silent && (
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    onClick={() => void enableAutomatic()}
                  >
                    {t("sidebar.update.enableAutomatic")}
                  </Button>
                )}
              </div>
            </>
          )}
        </div>
      )}

      {showResult && !manualError && status?.state === "failed" && (
        <div className="space-y-2 text-sm" role="status">
          <p>
            {t("settings.about.updates.checkFailed", { reason: errorReason })}
          </p>
          <Button type="button" size="sm" onClick={() => void checkNow()}>
            {t("sidebar.update.tryAgain")}
          </Button>
          <span className="ml-2 text-xs text-mid-gray">
            {t("settings.about.updates.useWebHint")}
          </span>
        </div>
      )}

      {showResult &&
        !manualError &&
        status &&
        ["downloading", "ready_to_restart", "installing"].includes(
          status.state,
        ) && (
          <p className="text-sm" role="status">
            {status.state === "downloading"
              ? t("sidebar.update.downloading", { version: status.version })
              : status.state === "installing"
                ? t("sidebar.update.installing")
                : t("sidebar.update.ready", { version: status.version })}
          </p>
        )}
    </div>
  );
};
