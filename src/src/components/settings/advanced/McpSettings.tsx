import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { Copy, RefreshCw, Check } from "lucide-react";
import { commands, type McpStatus } from "@/bindings";

const buttonClass =
  "px-3 py-1.5 rounded-md border border-zinc-700 bg-zinc-800 text-zinc-100 text-sm hover:border-blue-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors cursor-pointer";

const Snippet: React.FC<{ text: string }> = ({ text }) => {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    await writeText(text);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };
  return (
    <div className="flex items-start gap-2 rounded-md border border-zinc-700 bg-zinc-900 p-2">
      <pre className="flex-1 overflow-x-auto text-xs text-zinc-200 whitespace-pre-wrap break-all font-mono">
        {text}
      </pre>
      <button
        type="button"
        onClick={copy}
        className="shrink-0 p-1 rounded-md text-text/50 hover:text-text cursor-pointer"
        title="Copy"
      >
        {copied ? (
          <Check className="w-3.5 h-3.5 text-green-400" />
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
      <p className="text-sm text-text/50">
        {t("settings.advanced.mcp.loading")}
      </p>
    );
  }

  const base = `http://127.0.0.1:${status.port}/mcp`;
  const httpCmd = `claude mcp add --transport http handy ${base} --header "Authorization: Bearer ${status.token}"`;
  const stdioCmd = `claude mcp add handy -- handy mcp --stdio`;

  return (
    <div className="space-y-4">
      <p className="text-sm text-text/60">
        {t("settings.advanced.mcp.description")}
      </p>

      {/* Enable + status */}
      <div className="flex items-center gap-3 flex-wrap">
        <label className="flex items-center gap-2 text-sm cursor-pointer">
          <input
            type="checkbox"
            checked={status.enabled}
            disabled={busy}
            onChange={toggleEnabled}
            className="w-4 h-4 accent-blue-600 cursor-pointer"
          />
          {t("settings.advanced.mcp.enable")}
        </label>
        <span
          className={`text-xs px-2 py-0.5 rounded-full ${
            status.running
              ? "bg-green-600/20 text-green-400"
              : "bg-mid-gray/20 text-text/50"
          }`}
        >
          {status.running
            ? t("settings.advanced.mcp.running")
            : t("settings.advanced.mcp.stopped")}
        </span>
      </div>

      {/* Port */}
      <div className="flex items-center gap-2">
        <span className="text-xs text-text/60 w-24 shrink-0">
          {t("settings.advanced.mcp.port")}
        </span>
        <input
          type="number"
          min="1024"
          max="65535"
          value={port}
          disabled={busy}
          onChange={(e) => setPort(e.target.value)}
          onBlur={commitPort}
          className="rounded-md border border-zinc-700 bg-zinc-800 px-2 py-1 text-sm text-zinc-100 w-28 focus:border-blue-500 focus:outline-none"
        />
      </div>

      {/* Token */}
      <div className="flex items-center gap-2 flex-wrap">
        <span className="text-xs text-text/60 w-24 shrink-0">
          {t("settings.advanced.mcp.token")}
        </span>
        <code className="text-xs font-mono text-zinc-200 bg-zinc-900 rounded px-2 py-1">
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
          <RefreshCw className="w-3.5 h-3.5 inline mr-1" />
          {t("settings.advanced.mcp.regenerate")}
        </button>
      </div>

      {/* Connection snippets */}
      <div className="space-y-2">
        <p className="text-xs font-semibold text-text/70">
          {t("settings.advanced.mcp.claudeApp")}
        </p>
        <Snippet text={base} />
        <p className="text-xs text-text/50">
          {t("settings.advanced.mcp.authHeader")}
        </p>
        <Snippet text={`Authorization: Bearer ${status.token}`} />
        <p className="text-xs font-semibold text-text/70 pt-1">
          {t("settings.advanced.mcp.claudeCodeStdio")}
        </p>
        <Snippet text={stdioCmd} />
        <p className="text-xs font-semibold text-text/70 pt-1">
          {t("settings.advanced.mcp.claudeCodeHttp")}
        </p>
        <Snippet text={httpCmd} />
      </div>

      {/* CLI */}
      <div className="space-y-2 pt-2 border-t border-mid-gray/20">
        <p className="text-xs font-semibold text-text/70">
          {t("settings.advanced.mcp.cliTitle")}
        </p>
        <p className="text-xs text-text/50">
          {t("settings.advanced.mcp.cliHint")}
        </p>
        <div className="flex items-center gap-2 flex-wrap">
          <button
            type="button"
            className={buttonClass}
            disabled={busy}
            onClick={install}
          >
            {status.cli_installed
              ? t("settings.advanced.mcp.reinstallCli")
              : t("settings.advanced.mcp.installCli")}
          </button>
          {status.cli_installed && (
            <span
              className="text-xs text-text/40 truncate"
              title={status.cli_path}
            >
              {status.cli_path}
            </span>
          )}
        </div>
      </div>

      {error && <p className="text-sm text-red-400">{error}</p>}
    </div>
  );
};
