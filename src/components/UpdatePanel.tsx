import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type UpdaterStatus } from "@/bindings";
import { UpdateStatusCard, isLiveUpdateState } from "./UpdateStatusCard";

/**
 * Opens from the version at the bottom of the sidebar. With an update under
 * way it shows its card; otherwise "Update now" checks (as Check now does) and
 * says so when there is no new version.
 */
export const UpdatePanel: React.FC<{
  id: string;
  status: UpdaterStatus | null;
  onClose: () => void;
}> = ({ id, status, onClose }) => {
  const { t } = useTranslation();
  const [checking, setChecking] = useState(false);
  const [noNewVersion, setNoNewVersion] = useState(false);

  const updateNow = async (): Promise<void> => {
    setChecking(true);
    setNoNewVersion(false);
    try {
      const result = await commands.checkForUpdates();
      setNoNewVersion(result.status === "ok" && result.data.state === "idle");
    } finally {
      setChecking(false);
    }
  };

  const live = isLiveUpdateState(status);
  const buttonClass =
    "h-7 px-2.5 rounded-md text-xs font-medium cursor-pointer disabled:cursor-not-allowed";

  return (
    <div id={id} className="mb-2">
      {isLiveUpdateState(status) ? (
        <UpdateStatusCard status={status} />
      ) : (
        noNewVersion && (
          <p className="mx-1 text-xs text-text-secondary">
            {t("sidebar.update.noNewVersion")}
          </p>
        )
      )}
      <div className="mx-1 mt-2 flex items-center gap-2">
        {!live && (
          <button
            type="button"
            onClick={() => void updateNow()}
            disabled={checking}
            className={`${buttonClass} bg-btn text-on-btn hover:bg-btn-hover disabled:bg-dis-bg disabled:text-dis-text`}
          >
            {checking
              ? t("sidebar.update.checking")
              : t("sidebar.update.updateNow")}
          </button>
        )}
        <button
          type="button"
          onClick={onClose}
          className={`${buttonClass} border border-control-border border-b-control-bottom bg-control hover:bg-control-hover`}
        >
          {t("common.close")}
        </button>
      </div>
    </div>
  );
};
