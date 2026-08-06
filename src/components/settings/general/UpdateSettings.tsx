import React, { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { commands, type UpdaterStatus } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { ToggleSwitch } from "@/components/ui/ToggleSwitch";
import { SettingContainer } from "@/components/ui/SettingContainer";

const pad = (value: number): string => String(value).padStart(2, "0");

const formatMinute = (minute: number): string => {
  const wrapped = ((minute % 1440) + 1440) % 1440;
  return `${pad(Math.floor(wrapped / 60))}:${pad(wrapped % 60)}`;
};

export const UpdateSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating, refreshSettings } =
    useSettings();
  const checks = getSetting("automatic_update_checks") ?? true;
  const silent = getSetting("automatic_silent_updates") ?? false;
  const storedTime = getSetting("silent_update_time_local") ?? "04:00";
  const jitter = getSetting("silent_update_jitter_minutes") ?? 30;
  const [time, setTime] = useState(storedTime);
  const [jitterInput, setJitterInput] = useState(String(jitter));
  const [status, setStatus] = useState<UpdaterStatus | null>(null);

  useEffect(() => setTime(storedTime), [storedTime]);
  useEffect(() => setJitterInput(String(jitter)), [jitter]);

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

  const windowText = useMemo(() => {
    const [hour = 4, minute = 0] = storedTime.split(":").map(Number);
    const center = hour * 60 + minute;
    return t("settings.general.updates.time.window", {
      start: formatMinute(center - jitter),
      end: formatMinute(center + jitter),
    });
  }, [jitter, storedTime, t]);

  const checkNow = async (): Promise<void> => {
    const result = await commands.checkForUpdates();
    if (result.status === "ok") setStatus(result.data);
  };

  const commitTime = (): void => {
    if (/^(?:[01]\d|2[0-3]):[0-5]\d$/.test(time)) {
      void updateSetting("silent_update_time_local", time);
    } else {
      setTime(storedTime);
    }
  };

  const commitJitter = (): void => {
    const value = Math.min(180, Math.max(0, Math.round(Number(jitterInput))));
    if (Number.isFinite(value)) {
      setJitterInput(String(value));
      void updateSetting("silent_update_jitter_minutes", value);
    } else {
      setJitterInput(String(jitter));
    }
  };

  const lastCheck = status?.last_checked_at
    ? new Date(status.last_checked_at).toLocaleString()
    : t("settings.general.updates.never");

  return (
    <>
      <ToggleSwitch
        checked={checks}
        onChange={(enabled) => {
          void updateSetting("automatic_update_checks", enabled).then(() => {
            if (!enabled) void refreshSettings();
          });
        }}
        isUpdating={isUpdating("automatic_update_checks")}
        label={t("settings.general.updates.checks.label")}
        description={t("settings.general.updates.checks.description")}
        grouped
      />
      <ToggleSwitch
        checked={silent}
        onChange={(enabled) =>
          void updateSetting("automatic_silent_updates", enabled)
        }
        disabled={!checks}
        isUpdating={isUpdating("automatic_silent_updates")}
        label={t("settings.general.updates.silent.label")}
        description={t("settings.general.updates.silent.description")}
        grouped
      />
      <SettingContainer
        title={t("settings.general.updates.time.label")}
        description={`${t("settings.general.updates.time.description")} ${windowText}`}
        descriptionMode="inline"
        grouped
        disabled={!checks || !silent}
      >
        <input
          type="time"
          value={time}
          disabled={!checks || !silent}
          onChange={(event) => setTime(event.target.value)}
          onBlur={commitTime}
          className="rounded-md border border-mid-gray/30 bg-background-ui px-2 py-1 text-sm disabled:opacity-50"
        />
      </SettingContainer>
      <SettingContainer
        title={t("settings.general.updates.jitter.label")}
        description={t("settings.general.updates.jitter.description")}
        descriptionMode="inline"
        grouped
        disabled={!checks || !silent}
      >
        <div className="flex items-center gap-2">
          <input
            type="number"
            min={0}
            max={180}
            value={jitterInput}
            disabled={!checks || !silent}
            onChange={(event) => setJitterInput(event.target.value)}
            onBlur={commitJitter}
            className="w-20 rounded-md border border-mid-gray/30 bg-background-ui px-2 py-1 text-sm disabled:opacity-50"
          />
          <span className="text-xs text-mid-gray">
            {t("settings.general.updates.jitter.minutes")}
          </span>
        </div>
      </SettingContainer>
      <SettingContainer
        title={t("settings.general.updates.checkNow")}
        description={t("settings.general.updates.lastChecked", {
          value: lastCheck,
        })}
        descriptionMode="inline"
        grouped
      >
        <button
          type="button"
          onClick={() => void checkNow()}
          disabled={status?.state === "checking"}
          className="rounded-md bg-logo-primary/80 px-3 py-1.5 text-sm font-medium hover:bg-logo-primary disabled:cursor-wait disabled:opacity-50"
        >
          {status?.state === "checking"
            ? t("settings.general.updates.checking")
            : t("settings.general.updates.checkNow")}
        </button>
      </SettingContainer>
      {status?.state === "unsupported" && (
        <p className="px-4 py-2 text-xs text-amber-500">
          {t("settings.general.updates.portableUnsupported")}
        </p>
      )}
      {status?.state === "idle" && status.last_checked_at && (
        <p className="px-4 py-2 text-xs text-mid-gray">
          {t("settings.general.updates.upToDate")}
        </p>
      )}
    </>
  );
};
