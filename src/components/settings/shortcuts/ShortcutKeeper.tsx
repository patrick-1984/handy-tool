import React, { createContext, useContext, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { MonitorDot } from "lucide-react";
import { commands, type KeepSupport, type RemoteKeysStatus } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { WarningIcon } from "../../ui/WarningIcon";

/**
 * Shortcut Keeper (Windows): keeps the shortcuts you tick on this PC while a
 * Remote Desktop session has the keyboard. The switch is a group on the
 * Shortcuts page; each shortcut row there gets a "Keep on this PC" box.
 */
interface Keeper {
  status: RemoteKeysStatus | null;
  support: Partial<{ [id: string]: KeepSupport }>;
}

const KeeperContext = createContext<Keeper | null>(null);

/** Provides the status and per-shortcut support to the rows inside it. */
export const ShortcutKeeperProvider: React.FC<{
  children: React.ReactNode;
}> = ({ children }) => {
  const { getSetting } = useSettings();
  const bindings = getSetting("bindings");
  const [status, setStatus] = useState<RemoteKeysStatus | null>(null);
  const [support, setSupport] = useState<Keeper["support"]>({});

  useEffect(() => {
    let disposed = false;
    const unlisten = listen<RemoteKeysStatus>("remote-keys-status", (event) =>
      setStatus(event.payload),
    );
    void unlisten.then(() =>
      commands.getRemoteKeysStatus().then((current) => {
        if (!disposed) setStatus(current);
      }),
    );
    return () => {
      disposed = true;
      void unlisten.then((stop) => stop());
    };
  }, []);

  // Whether a shortcut can be kept depends on its keys: ask again on a rebind.
  useEffect(() => {
    let disposed = false;
    void commands.getRemoteKeySupport().then((current) => {
      if (!disposed) setSupport(current);
    });
    return () => {
      disposed = true;
    };
  }, [bindings]);

  return (
    <KeeperContext.Provider value={{ status, support }}>
      {children}
    </KeeperContext.Provider>
  );
};

/** The switch, what it does, and what it is doing now. */
export const ShortcutKeeperGroup: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const keeper = useContext(KeeperContext);
  const state = keeper?.status?.state;
  const blocked = state === "unavailable" || state === "needs_restart";
  return (
    <SettingsGroup
      icon={MonitorDot}
      title={t("settings.shortcuts.keeper.title")}
      description={t("settings.shortcuts.keeper.summary")}
    >
      <ToggleSwitch
        checked={getSetting("remote_keys_enabled") ?? false}
        onChange={(value) => updateSetting("remote_keys_enabled", value)}
        isUpdating={isUpdating("remote_keys_enabled")}
        disabled={blocked}
        label={t("settings.shortcuts.keeper.label")}
        description={t("settings.shortcuts.keeper.description")}
        descriptionMode="tooltip"
        grouped={true}
      />
      {state && (
        <p
          role="status"
          className="px-4 py-2.5 text-xs leading-4 text-text-secondary"
        >
          {t(`settings.shortcuts.keeper.status.${state}`)}
        </p>
      )}
    </SettingsGroup>
  );
};

/**
 * "Keep on this PC" next to a shortcut on the Shortcuts page (rendered only
 * inside the provider, so the feature pages' rows stay as they are).
 */
export const KeepLocalCheckbox: React.FC<{ shortcutId: string }> = ({
  shortcutId,
}) => {
  const { t } = useTranslation();
  const { getSetting, refreshSettings } = useSettings();
  const keeper = useContext(KeeperContext);
  const [saving, setSaving] = useState(false);
  if (!keeper) return null;

  const on = getSetting("remote_keys_enabled") ?? false;
  const available =
    keeper.status !== null &&
    keeper.status.state !== "unavailable" &&
    keeper.status.state !== "needs_restart";
  const support = keeper.support[shortcutId];
  const possible = support?.category === "ok" || support?.category === "warn";
  const kept =
    possible &&
    (getSetting("remote_local_bindings") ?? []).includes(shortcutId);
  const reason =
    on && available && support && support.code
      ? t(`settings.shortcuts.keeper.reasons.${support.code}`)
      : null;
  // A warning matters once the box is ticked; a "cannot" reason always.
  const showReason = reason && (!possible || kept);

  const toggle = async (local: boolean): Promise<void> => {
    setSaving(true);
    try {
      await commands.setRemoteLocalBinding(shortcutId, local);
      await refreshSettings();
    } finally {
      setSaving(false);
    }
  };

  return (
    <span className="flex items-center gap-1">
      {showReason && <WarningIcon message={reason} />}
      <label
        className={`flex items-center gap-1.5 text-xs whitespace-nowrap ${
          on && available && possible
            ? "text-text-secondary cursor-pointer"
            : "text-dis-text cursor-not-allowed"
        }`}
        title={
          !on || !available
            ? t("settings.shortcuts.keeper.turnOnFirst")
            : t("settings.shortcuts.keeper.checkboxHint")
        }
      >
        <input
          type="checkbox"
          checked={kept}
          disabled={!on || !available || !possible || saving}
          onChange={(event) => void toggle(event.target.checked)}
          className="w-3.5 h-3.5 accent-accent cursor-pointer disabled:cursor-not-allowed"
        />
        {t("settings.shortcuts.keeper.checkbox")}
      </label>
    </span>
  );
};
