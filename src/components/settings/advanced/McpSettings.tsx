import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { Copy, RefreshCw, Check } from "lucide-react";
import { commands, type McpStatus } from "@/bindings";
import { SettingContainer } from "../../ui/SettingContainer";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { SECONDARY_BUTTON, TEXT_FIELD } from "../../ui/controlClasses";

const buttonClass = SECONDARY_BUTTON;

const Snippet: React.FC<{ text: string }> = ({ text }) => {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    await writeText(text);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };
  return (
    <div className="flex items-start gap-2 rounded-md border border-border bg-surface2 p-2">
      <pre className="flex-1 overflow-x-auto text-xs text-text whitespace-pre-wrap break-all font-mono">
        {text}
      </pre>
      <button
        type="button"
        onClick={copy}
        className="shrink-0 p-1 rounded-md text-text-secondary hover:text-text cursor-pointer"
        title="Copy"
      >
        {copied ? (
          <Check className="w-3.5 h-3.5 text-ok-text" />
        ) : (
          <Copy className="w-3.5 h-3.5" />
        )}
      </button>
    </div>
  );
};

export const McpSettings: React.FC = () => {
  const { t } = useTranslation();
  const [status, setStatus] = useState<McpStatus | null>(null);
  const [port, setPort] = useState("8765");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showToken, setShowToken] = useState(false);

  const load = async () => {
    const s = await commands.getMcpStatus();
    setStatus(s);
    setPort(String(s.port));
  };

  useEffect(() => {
    load().catch((e) => setError(String(e)));
  }, []);

  const run = async (
    fn: () => Promise<{ status: string; data?: McpStatus; error?: string }>,
  ) => {
    setBusy(true);
    setError(null);
    try {
      const res = await fn();
      if (res.status === "ok" && res.data) {
        setStatus(res.data);
        setPort(String(res.data.port));
      } else if (res.status === "error") {
        setError(res.error ?? "error");
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const toggleEnabled = () =>
    run(() => commands.setMcpEnabled(!(status?.enabled ?? false)));
  const commitPort = () => {
    const p = Number.parseInt(port, 10);
    if (Number.isFinite(p) && p >= 1024 && p <= 65535 && p !== status?.port) {
      run(() => commands.changeMcpPort(p));
    }
  };
  const regen = () => run(() => commands.regenerateMcpToken());
  const install = async () => {
    setBusy(true);
    setError(null);
    try {
      const res = await commands.installCli();
      if (res.status === "error") setError(res.error);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  if (!status) {
    return (
      <p className="px-4 py-3 text-sm text-text-secondary">
        {t("settings.advanced.mcp.loading")}
      </p>
    );
  }

  const base = `http://127.0.0.1:${status.port}/mcp`;
  const httpCmd = `claude mcp add --transport http handy ${base} --header "Authorization: Bearer ${status.token}"`;
  const stdioCmd = `claude mcp add handy -- handy mcp --stdio`;

  return (
    <>
      <ToggleSwitch
        checked={status.enabled}
        onChange={toggleEnabled}
        isUpdating={busy}
        label={t("settings.advanced.mcp.enable")}
        description={
          status.running
            ? t("settings.advanced.mcp.running")
            : t("settings.advanced.mcp.stopped")
        }
        descriptionMode="inline"
        grouped
      />

      <SettingContainer
        title={t("settings.advanced.mcp.port")}
        description=""
        descriptionMode="inline"
        grouped
      >
        <input
          type="number"
          min="1024"
          max="65535"
          value={port}
          disabled={busy}
          onChange={(e) => setPort(e.target.value)}
          onBlur={commitPort}
          className={`${TEXT_FIELD} w-28`}
        />
      </SettingContainer>

      <SettingContainer
        title={t("settings.advanced.mcp.token")}
        description=""
        descriptionMode="inline"
        grouped
      >
        <div className="flex items-center gap-2 flex-wrap justify-end">
          <code className="text-xs font-mono text-text bg-surface2 rounded-sm px-2 py-1">
            {showToken ? status.token || "—" : "••••••••"}
          </code>
          <button
            type="button"
            className={buttonClass}
            onClick={() => setShowToken((v) => !v)}
          >
            {showToken
              ? t("settings.advanced.mcp.hide")
              : t("settings.advanced.mcp.show")}
          </button>
          <button
            type="button"
            className={buttonClass}
            disabled={busy}
            onClick={regen}
          >
            <RefreshCw className="w-3.5 h-3.5" />
            {t("settings.advanced.mcp.regenerate")}
          </button>
        </div>
      </SettingContainer>

      {/* Connection snippets */}
      <div className="px-4 py-3 space-y-2">
        <p className="text-xs font-semibold text-text-secondary">
          {t("settings.advanced.mcp.claudeApp")}
        </p>
        <Snippet text={base} />
        <p className="text-xs text-text-secondary">
          {t("settings.advanced.mcp.authHeader")}
        </p>
        <Snippet text={`Authorization: Bearer ${status.token}`} />
        <p className="text-xs font-semibold text-text-secondary pt-1">
          {t("settings.advanced.mcp.claudeCodeStdio")}
        </p>
        <Snippet text={stdioCmd} />
        <p className="text-xs font-semibold text-text-secondary pt-1">
          {t("settings.advanced.mcp.claudeCodeHttp")}
        </p>
        <Snippet text={httpCmd} />
      </div>

      {/* CLI */}
      <SettingContainer
        title={t("settings.advanced.mcp.cliTitle")}
        description={
          status.cli_installed
            ? `${t("settings.advanced.mcp.cliHint")}

${status.cli_path}`
            : t("settings.advanced.mcp.cliHint")
        }
        descriptionMode="inline"
        grouped
      >
        <button
          type="button"
          className={`${buttonClass} shrink-0`}
          disabled={busy}
          onClick={install}
        >
          {status.cli_installed
            ? t("settings.advanced.mcp.reinstallCli")
            : t("settings.advanced.mcp.installCli")}
        </button>
      </SettingContainer>

      {error && <p className="px-4 py-3 text-sm text-err-text">{error}</p>}
    </>
  );
};
