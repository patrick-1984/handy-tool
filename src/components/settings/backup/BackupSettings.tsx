import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { open, save } from "@tauri-apps/plugin-dialog";
import { downloadDir, join } from "@tauri-apps/api/path";
import { Archive, ArchiveRestore, Check, RotateCw } from "lucide-react";
import { commands, type RestoreReport } from "@/bindings";

const pad = (n: number) => String(n).padStart(2, "0");
const timestampSlug = (): string => {
  const d = new Date();
  return `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}-${pad(
    d.getHours(),
  )}${pad(d.getMinutes())}${pad(d.getSeconds())}`;
};

/**
 * Back up Handy's data as a .tar.gz. Downloaded models and large uncompressed
 * audio (.wav/.flac) are always excluded; the "full" profile keeps the small
 * compressed recordings, the "config" profile is settings + history only.
 */
export const BackupSettings: React.FC = () => {
  const { t } = useTranslation();
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [savedPath, setSavedPath] = useState<string | null>(null);
  // Restore: what to bring back from the archive, and the result report.
  const [restoreConfig, setRestoreConfig] = useState(true);
  const [restoreRecordings, setRestoreRecordings] = useState(true);
  const [restoreReport, setRestoreReport] = useState<RestoreReport | null>(
    null,
  );

  const run = async (profile: "config" | "full") => {
    setBusy(profile);
    setError(null);
    setSavedPath(null);
    try {
      const name = `handy-backup-${profile}-${timestampSlug()}.tar.gz`;
      let def = name;
      try {
        def = await join(await downloadDir(), name);
      } catch {
        // Downloads dir not resolvable — fall back to a bare filename.
      }
      const path = await save({
        defaultPath: def,
        filters: [{ name: "Backup", extensions: ["tar.gz", "gz"] }],
      });
      if (!path) return;
      const res = await commands.createBackup(profile, path);
      if (res.status === "ok") setSavedPath(res.data);
      else setError(res.error);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const handleRestore = async () => {
    if (!restoreConfig && !restoreRecordings) return;
    setBusy("restore");
    setError(null);
    setSavedPath(null);
    setRestoreReport(null);
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: "Backup", extensions: ["gz", "tar.gz"] }],
      });
      if (!path || typeof path !== "string") return;
      const res = await commands.restoreBackup(
        path,
        restoreConfig,
        restoreRecordings,
      );
      if (res.status === "ok") setRestoreReport(res.data);
      else setError(res.error);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const buttonClass =
    "flex items-center gap-2 px-3 py-2 rounded-md border border-zinc-700 bg-zinc-800 text-zinc-100 text-sm hover:border-blue-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors cursor-pointer";

  return (
    <div className="w-full space-y-4">
      <p className="text-sm text-text/60">{t("settings.backup.description")}</p>

      <div className="space-y-3">
        <div className="border border-mid-gray/20 rounded-lg p-3 space-y-2 bg-mid-gray/5">
          <p className="text-sm font-semibold">
            {t("settings.backup.configTitle")}
          </p>
          <p className="text-xs text-text/50">
            {t("settings.backup.configHint")}
          </p>
          <button
            type="button"
            className={buttonClass}
            disabled={busy !== null}
            onClick={() => run("config")}
          >
            <Archive className="w-4 h-4" />
            {t("settings.backup.exportConfig")}
          </button>
        </div>

        <div className="border border-mid-gray/20 rounded-lg p-3 space-y-2 bg-mid-gray/5">
          <p className="text-sm font-semibold">
            {t("settings.backup.fullTitle")}
          </p>
          <p className="text-xs text-text/50">
            {t("settings.backup.fullHint")}
          </p>
          <button
            type="button"
            className={buttonClass}
            disabled={busy !== null}
            onClick={() => run("full")}
          >
            <Archive className="w-4 h-4" />
            {t("settings.backup.exportFull")}
          </button>
        </div>

        <div className="border border-mid-gray/20 rounded-lg p-3 space-y-2 bg-mid-gray/5">
          <p className="text-sm font-semibold">
            {t("settings.backup.restoreTitle")}
          </p>
          <p className="text-xs text-text/50">
            {t("settings.backup.restoreHint")}
          </p>
          <div className="flex items-center gap-4 flex-wrap">
            <label className="flex items-center gap-1.5 text-xs text-text/70 cursor-pointer">
              <input
                type="checkbox"
                checked={restoreConfig}
                onChange={(e) => setRestoreConfig(e.target.checked)}
                className="w-3.5 h-3.5 accent-blue-600 cursor-pointer"
              />
              {t("settings.backup.restoreConfig")}
            </label>
            <label className="flex items-center gap-1.5 text-xs text-text/70 cursor-pointer">
              <input
                type="checkbox"
                checked={restoreRecordings}
                onChange={(e) => setRestoreRecordings(e.target.checked)}
                className="w-3.5 h-3.5 accent-blue-600 cursor-pointer"
              />
              {t("settings.backup.restoreRecordings")}
            </label>
          </div>
          <button
            type="button"
            className={buttonClass}
            disabled={busy !== null || (!restoreConfig && !restoreRecordings)}
            onClick={handleRestore}
          >
            <ArchiveRestore className="w-4 h-4" />
            {t("settings.backup.restoreButton")}
          </button>
          {restoreReport && (
            <div className="space-y-2 pt-1">
              <p className="flex items-center gap-1 text-sm text-green-400">
                <Check className="w-4 h-4" />
                {t("settings.backup.restoreDone", {
                  settings: restoreReport.settings_restored ? "✓" : "—",
                  history: restoreReport.history_restored ? "✓" : "—",
                  recordings: restoreReport.recordings_restored,
                })}
              </p>
              {restoreReport.errors.length > 0 && (
                <ul className="text-xs text-red-400 list-disc pl-4 space-y-0.5">
                  {restoreReport.errors.map((e, i) => (
                    <li key={i}>{e}</li>
                  ))}
                </ul>
              )}
              {restoreReport.restart_required && (
                <div className="space-y-1">
                  <p className="text-xs text-amber-400">
                    {t("settings.backup.restartHint")}
                  </p>
                  <button
                    type="button"
                    className={buttonClass}
                    onClick={() => commands.restartApp()}
                  >
                    <RotateCw className="w-4 h-4" />
                    {t("settings.backup.restartNow")}
                  </button>
                </div>
              )}
            </div>
          )}
        </div>
      </div>

      {savedPath && (
        <p className="flex items-center gap-1 text-sm text-green-400">
          <Check className="w-4 h-4" />
          {t("settings.backup.saved", { path: savedPath })}
        </p>
      )}
      {error && <p className="text-sm text-red-400">{error}</p>}
    </div>
  );
};
