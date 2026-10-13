import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  commands,
  type ProxyAuth,
  type ProxyMode,
  type ProxyTestResult,
  type SystemProxyInfo,
} from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { SettingContainer } from "../../ui/SettingContainer";
import { Dropdown } from "../../ui/Dropdown";
import { Button } from "../../ui/Button";
import { TEXT_FIELD } from "../../ui/controlClasses";

/**
 * Advanced settings › Proxy (Windows): the same choices as a browser's
 * connection settings - Windows' proxy settings, a manual proxy (address,
 * port, sites reached directly) or none - plus how to sign in to a proxy that
 * asks (the Windows account, or a user name and password) and a test.
 */
export const ProxySettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, refreshSettings } = useSettings();

  const mode = (getSetting("proxy_mode") ?? "system") as ProxyMode;
  const auth = (getSetting("proxy_auth") ?? "windows") as ProxyAuth;
  const savedHost = (getSetting("proxy_host") as string) ?? "";
  const savedPort = (getSetting("proxy_port") as number) ?? 8080;
  const savedBypass = (getSetting("proxy_bypass") as string) ?? "";
  const savedUser = (getSetting("proxy_username") as string) ?? "";
  const hasPassword = ((getSetting("proxy_password") as string) ?? "") !== "";

  // Local state + commit on blur (text inputs never write on every key).
  const [host, setHost] = useState(savedHost);
  const [port, setPort] = useState(String(savedPort));
  const [bypass, setBypass] = useState(savedBypass);
  const [user, setUser] = useState(savedUser);
  const [password, setPassword] = useState("");
  useEffect(() => setHost(savedHost), [savedHost]);
  useEffect(() => setPort(String(savedPort)), [savedPort]);
  useEffect(() => setBypass(savedBypass), [savedBypass]);
  useEffect(() => setUser(savedUser), [savedUser]);

  const [windows, setWindows] = useState<SystemProxyInfo | null>(null);
  useEffect(() => {
    commands.getSystemProxy().then(setWindows);
  }, []);

  const [testing, setTesting] = useState(false);
  const [result, setResult] = useState<ProxyTestResult | null>(null);
  const [passwordError, setPasswordError] = useState(false);

  const savePassword = async (value: string) => {
    const saved = await commands.setProxyPassword(value);
    setPasswordError(saved.status === "error");
    if (saved.status === "ok") {
      // Clear only what was saved: a newer entry typed meanwhile stays.
      setPassword((current) => (current === value ? "" : current));
      await refreshSettings();
    }
  };

  const runTest = async () => {
    setTesting(true);
    setResult(null);
    try {
      setResult(await commands.testProxyConnection());
    } finally {
      setTesting(false);
    }
  };

  const modeOptions = [
    { value: "system", label: t("settings.advanced.proxy.mode.system") },
    { value: "manual", label: t("settings.advanced.proxy.mode.manual") },
    { value: "off", label: t("settings.advanced.proxy.mode.off") },
  ];
  const authOptions = [
    { value: "windows", label: t("settings.advanced.proxy.auth.windows") },
    { value: "password", label: t("settings.advanced.proxy.auth.password") },
  ];

  const windowsSummary = () => {
    if (!windows) return "";
    const parts: string[] = [];
    if (windows.script)
      parts.push(
        t("settings.advanced.proxy.windows.script", { url: windows.script }),
      );
    if (windows.proxy)
      parts.push(
        t("settings.advanced.proxy.windows.proxy", { proxy: windows.proxy }),
      );
    if (parts.length === 0) return t("settings.advanced.proxy.windows.none");
    if (windows.bypass)
      parts.push(
        t("settings.advanced.proxy.windows.bypass", { list: windows.bypass }),
      );
    return parts.join(" · ");
  };

  const resultText = (r: ProxyTestResult) => {
    const route =
      r.route === "proxy"
        ? t("settings.advanced.proxy.test.viaProxy", { proxy: r.proxy ?? "" })
        : t(`settings.advanced.proxy.test.route.${r.route}`);
    if (!r.ok)
      return t("settings.advanced.proxy.test.failed", {
        route,
        error: r.error ?? "",
      });
    const signIn = r.sign_in
      ? t("settings.advanced.proxy.test.signedIn", { method: r.sign_in })
      : "";
    return t("settings.advanced.proxy.test.ok", {
      route,
      status: r.status ?? 0,
      signIn,
    });
  };

  return (
    <>
      <SettingContainer
        title={t("settings.advanced.proxy.mode.title")}
        description={t("settings.advanced.proxy.mode.description")}
        descriptionMode="tooltip"
        grouped={true}
      >
        <Dropdown
          options={modeOptions}
          selectedValue={mode}
          onSelect={(value) => updateSetting("proxy_mode", value as ProxyMode)}
        />
      </SettingContainer>

      {mode === "system" && (
        <p className="px-4 py-2 text-xs text-text-secondary">
          {t("settings.advanced.proxy.windows.label")} {windowsSummary()}
        </p>
      )}

      {mode === "manual" && (
        <>
          <SettingContainer
            title={t("settings.advanced.proxy.host.title")}
            description={t("settings.advanced.proxy.host.description")}
            descriptionMode="tooltip"
            grouped={true}
          >
            <div className="flex items-center gap-2">
              <input
                type="text"
                value={host}
                onChange={(e) => setHost(e.target.value)}
                onBlur={() => {
                  if (host.trim() !== savedHost)
                    updateSetting("proxy_host", host.trim());
                }}
                placeholder={t("settings.advanced.proxy.host.placeholder")}
                className={`w-48 ${TEXT_FIELD}`}
                spellCheck={false}
              />
              <span className="text-sm text-text-secondary">
                {t("settings.advanced.proxy.port")}
              </span>
              <input
                type="number"
                min={1}
                max={65535}
                value={port}
                onChange={(e) => setPort(e.target.value)}
                onBlur={() => {
                  const value = Number.parseInt(port, 10);
                  if (value >= 1 && value <= 65535) {
                    if (value !== savedPort) updateSetting("proxy_port", value);
                  } else {
                    setPort(String(savedPort));
                  }
                }}
                className={`w-20 ${TEXT_FIELD}`}
              />
            </div>
          </SettingContainer>
          <SettingContainer
            title={t("settings.advanced.proxy.bypass.title")}
            description={t("settings.advanced.proxy.bypass.description")}
            descriptionMode="tooltip"
            grouped={true}
          >
            <input
              type="text"
              value={bypass}
              onChange={(e) => setBypass(e.target.value)}
              onBlur={() => {
                if (bypass.trim() !== savedBypass)
                  updateSetting("proxy_bypass", bypass.trim());
              }}
              placeholder={t("settings.advanced.proxy.bypass.placeholder")}
              className={`w-full ${TEXT_FIELD}`}
              spellCheck={false}
            />
          </SettingContainer>
        </>
      )}

      {mode !== "off" && (
        <SettingContainer
          title={t("settings.advanced.proxy.auth.title")}
          description={t("settings.advanced.proxy.auth.description")}
          descriptionMode="tooltip"
          grouped={true}
        >
          <Dropdown
            options={authOptions}
            selectedValue={auth}
            onSelect={(value) =>
              updateSetting("proxy_auth", value as ProxyAuth)
            }
          />
        </SettingContainer>
      )}

      {mode !== "off" && auth === "password" && (
        <>
          <SettingContainer
            title={t("settings.advanced.proxy.username.title")}
            description={t("settings.advanced.proxy.username.description")}
            descriptionMode="tooltip"
            grouped={true}
          >
            <input
              type="text"
              value={user}
              onChange={(e) => setUser(e.target.value)}
              onBlur={() => {
                if (user.trim() !== savedUser)
                  updateSetting("proxy_username", user.trim());
              }}
              placeholder={t("settings.advanced.proxy.username.placeholder")}
              className={`w-full ${TEXT_FIELD}`}
              spellCheck={false}
              autoComplete="off"
            />
          </SettingContainer>
          <SettingContainer
            title={t("settings.advanced.proxy.password.title")}
            description={t("settings.advanced.proxy.password.description")}
            descriptionMode="tooltip"
            grouped={true}
          >
            <div className="flex items-center gap-2">
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                onBlur={() => {
                  if (password !== "") savePassword(password);
                }}
                placeholder={
                  hasPassword
                    ? t("settings.advanced.proxy.password.saved")
                    : t("settings.advanced.proxy.password.placeholder")
                }
                className={`w-48 ${TEXT_FIELD}`}
                autoComplete="new-password"
              />
              {hasPassword && (
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => savePassword("")}
                >
                  {t("settings.advanced.proxy.password.forget")}
                </Button>
              )}
            </div>
          </SettingContainer>
          {passwordError && (
            <p className="px-4 py-2 text-sm text-err-text">
              {t("settings.advanced.proxy.password.error")}
            </p>
          )}
        </>
      )}

      <SettingContainer
        title={t("settings.advanced.proxy.test.title")}
        description={t("settings.advanced.proxy.test.description")}
        descriptionMode="tooltip"
        grouped={true}
      >
        <Button
          variant="secondary"
          size="sm"
          onClick={runTest}
          disabled={testing}
        >
          {testing
            ? t("settings.advanced.proxy.test.running")
            : t("settings.advanced.proxy.test.button")}
        </Button>
      </SettingContainer>
      {result && (
        <p
          className={`px-4 py-2 text-sm break-words ${
            result.ok ? "text-text-secondary" : "text-err-text"
          }`}
        >
          {resultText(result)}
        </p>
      )}
    </>
  );
};
