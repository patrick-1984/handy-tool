import React, { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  commands,
  type ProviderCountResult,
  type LlmProvider,
} from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";

const PROVIDER_PREFIX = "provider:";

const LOCAL_TOKENIZERS = ["cl100k_base", "o200k_base", "estimate"];

const actionButtonClass =
  "px-4 py-1.5 rounded-md bg-blue-600 text-white text-sm font-medium hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors";
const secondaryButtonClass =
  "px-3 py-1.5 rounded-md border border-zinc-700 bg-zinc-800 text-zinc-100 text-sm hover:border-blue-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors cursor-pointer";

export const TokenCountPage: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const [text, setText] = useState("");
  const [fileName, setFileName] = useState<string | null>(null);
  const [activeOption, setActiveOption] = useState("cl100k_base");
  const [countingOption, setCountingOption] = useState<string | null>(null);
  const [result, setResult] = useState<{
    tokens: number;
    characters: number;
    words: number;
  } | null>(null);
  const [error, setError] = useState<string | null>(null);

  // "Count with all" sweep state
  const [sweeping, setSweeping] = useState(false);
  const [sweepResults, setSweepResults] = useState<ProviderCountResult[]>([]);
  const [sweepTotal, setSweepTotal] = useState(0);
  const [sweepDone, setSweepDone] = useState(false);
  const [showFailed, setShowFailed] = useState(false);
  const sweepingRef = useRef(false);

  const providers =
    (getSetting("llm_providers") as LlmProvider[] | undefined) ?? [];
  const enabledProviders = providers.filter((p) => p.enabled);

  const selectedProvider = activeOption.startsWith(PROVIDER_PREFIX)
    ? providers.find((p) => p.id === activeOption.slice(PROVIDER_PREFIX.length))
    : undefined;

  useEffect(() => {
    const unlistenPromise = listen<ProviderCountResult>(
      "token-count-progress",
      (event) => {
        if (!sweepingRef.current) return;
        setSweepResults((prev) => [...prev, event.payload]);
      },
    );
    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, []);

  const characters = text.length;
  const words = text.split(/\s+/).filter(Boolean).length;
  const busy = countingOption !== null || sweeping;

  /** Run a count with the given option (local tokenizer or provider:<id>). */
  const runCount = async (option: string) => {
    if (!text.trim() || busy) return;
    setActiveOption(option);
    setCountingOption(option);
    setError(null);
    setResult(null);
    try {
      if (option.startsWith(PROVIDER_PREFIX)) {
        const providerId = option.slice(PROVIDER_PREFIX.length);
        const res = await commands.countTokensViaProvider(providerId, text);
        if (res.status === "ok" && res.data.ok && res.data.tokens !== null) {
          setResult({ tokens: res.data.tokens, characters, words });
        } else {
          const message =
            res.status === "ok" ? (res.data.error ?? "") : res.error;
          setError(message);
        }
      } else {
        const res = await commands.countTokens(text, option);
        if (res.status === "ok") {
          setResult({ tokens: res.data, characters, words });
        } else {
          setError(res.error);
        }
      }
    } catch (e) {
      console.error("Token count error:", e);
      setError(String(e));
    } finally {
      setCountingOption(null);
    }
  };

  const handleCountAll = async (parallel: boolean) => {
    if (!text.trim() || busy) return;
    setSweeping(true);
    sweepingRef.current = true;
    setSweepResults([]);
    setSweepDone(false);
    setShowFailed(false);
    // Sweep covers the three built-in tokenizers plus every enabled provider
    setSweepTotal(LOCAL_TOKENIZERS.length + enabledProviders.length);
    setError(null);
    try {
      await commands.countTokensAllProviders(text, parallel);
    } catch (e) {
      console.error("Count-with-all error:", e);
    } finally {
      setSweeping(false);
      sweepingRef.current = false;
      setSweepDone(true);
    }
  };

  const handleCancelSweep = () => {
    commands.cancelTokenCountSweep().catch(console.error);
  };

  const handleOpenFile = async () => {
    try {
      const path = await open({ multiple: false, directory: false });
      if (typeof path !== "string") return;
      const res = await commands.readTextFileForCount(path);
      if (res.status !== "ok") {
        setError(res.error);
        return;
      }
      setText(res.data);
      setFileName(path.split(/[\\/]/).pop() ?? path);
      setResult(null);
      setError(null);
    } catch (e) {
      console.error("Failed to open file:", e);
      setError(String(e));
    }
  };

  const okResults = sweepResults.filter(
    (r) => r.ok && r.tokens !== null,
  ) as (ProviderCountResult & { tokens: number })[];
  const failedResults = sweepResults.filter((r) => !r.ok);
  const minTokens = useMemo(() => {
    // The estimate heuristic must not define the Δ baseline; use exact counts
    const exact = okResults.filter((r) => r.provider_id !== "builtin:estimate");
    const pool = exact.length ? exact : okResults;
    return pool.length ? Math.min(...pool.map((r) => r.tokens)) : 0;
  }, [okResults]);

  const formatDelta = (tokens: number) => {
    if (!minTokens || tokens === minTokens) return "—";
    const delta = ((tokens - minTokens) / minTokens) * 100;
    return `+${delta.toFixed(1)}%`;
  };

  const formatElapsed = (ms: number) =>
    ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`;

  const chipClass = (option: string, enabled: boolean) => {
    const base =
      "px-3 py-1.5 rounded-full border text-sm transition-colors whitespace-nowrap";
    if (!enabled) {
      return `${base} border-zinc-800 text-zinc-600 cursor-not-allowed`;
    }
    const selected = activeOption === option;
    const working = countingOption === option;
    return `${base} cursor-pointer ${
      selected
        ? "border-blue-500 bg-blue-600/20 text-zinc-100"
        : "border-zinc-700 bg-zinc-800 text-zinc-300 hover:border-blue-500"
    } ${working ? "animate-pulse" : ""}`;
  };

  return (
    <div className="w-full flex flex-col gap-4 h-full overflow-y-auto">
      <h2 className="text-lg font-semibold text-text">
        {t("tokenCount.title")}
      </h2>

      <textarea
        className="flex-1 min-h-[320px] w-full rounded-md border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm text-zinc-100 placeholder-zinc-500 focus:border-blue-500 focus:outline-none resize-none"
        placeholder={t("tokenCount.placeholder")}
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          setFileName(null);
        }}
      />

      {/* Layer 1: clickable counter chips — click = count immediately */}
      <div className="flex items-center gap-2 flex-wrap">
        {LOCAL_TOKENIZERS.map((id) => (
          <button
            key={id}
            onClick={() => runCount(id)}
            disabled={busy || !text.trim()}
            className={chipClass(id, true)}
            title={t(`tokenCount.options.${id}`)}
          >
            {t(`tokenCount.chips.${id}`)}
          </button>
        ))}
        {providers.map((p) => {
          const option = `${PROVIDER_PREFIX}${p.id}`;
          return (
            <button
              key={p.id}
              onClick={() => p.enabled && runCount(option)}
              disabled={!p.enabled || busy || !text.trim()}
              className={chipClass(option, !!p.enabled)}
              title={
                p.enabled
                  ? `${p.model} (${p.base_url || "local"})`
                  : t("tokenCount.chipDisabled")
              }
            >
              {p.name}
            </button>
          );
        })}
      </div>

      {/* Layer 2: bulk actions */}
      <div className="flex items-center gap-3 flex-wrap">
        <button
          onClick={() => handleCountAll(false)}
          disabled={busy || !text.trim()}
          className={actionButtonClass}
          title={t("tokenCount.countAllTitle")}
        >
          {sweeping
            ? t("tokenCount.counting", {
                done: sweepResults.length,
                total: sweepTotal,
              })
            : t("tokenCount.countAll")}
        </button>
        <button
          onClick={() => handleCountAll(true)}
          disabled={busy || !text.trim()}
          className={actionButtonClass}
          title={t("tokenCount.countAllParallelTitle")}
        >
          {t("tokenCount.countAllParallel")}
        </button>
        {sweeping && (
          <button onClick={handleCancelSweep} className={secondaryButtonClass}>
            {t("tokenCount.cancelSweep")}
          </button>
        )}
        <button
          onClick={handleOpenFile}
          disabled={busy}
          className={secondaryButtonClass}
        >
          {t("tokenCount.openFile")}
        </button>
      </div>

      <div className="text-xs text-text/50">
        {selectedProvider
          ? t("tokenCount.activeProvider", {
              name: selectedProvider.name,
              model: selectedProvider.model,
              url: selectedProvider.base_url || "local",
            })
          : t("tokenCount.activeLocal", {
              tokenizer: activeOption,
            })}
        {fileName && (
          <span className="ml-2">
            {t("tokenCount.loadedFile", { name: fileName })}
          </span>
        )}
      </div>

      {error && <div className="text-sm text-red-400">{error}</div>}

      {result && (
        <div className="flex items-center gap-4 text-sm text-zinc-300">
          <span>
            <strong>{result.tokens.toLocaleString()}</strong>{" "}
            {t("tokenCount.tokens")}
          </span>
          <span className="text-zinc-500">|</span>
          <span>
            {result.characters.toLocaleString()} {t("tokenCount.characters")}
          </span>
          <span className="text-zinc-500">|</span>
          <span>
            {result.words.toLocaleString()} {t("tokenCount.words")}
          </span>
        </div>
      )}

      {(sweepResults.length > 0 || sweeping) && (
        <div className="space-y-2 pb-4">
          <div className="text-sm text-zinc-300">
            {characters.toLocaleString()} {t("tokenCount.characters")}
            <span className="text-zinc-500 mx-2">|</span>
            {words.toLocaleString()} {t("tokenCount.words")}
          </div>
          <table className="w-full text-sm border border-mid-gray/20 rounded-lg overflow-hidden">
            <thead>
              <tr className="text-left text-xs text-text/60 uppercase tracking-wide bg-zinc-800/50">
                <th className="px-3 py-2">{t("tokenCount.table.provider")}</th>
                <th className="px-3 py-2">{t("tokenCount.table.model")}</th>
                <th className="px-3 py-2 text-right">
                  {t("tokenCount.table.tokens")}
                </th>
                <th className="px-3 py-2 text-right">
                  {t("tokenCount.table.delta")}
                </th>
                <th className="px-3 py-2 text-right">
                  {t("tokenCount.table.time")}
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-mid-gray/20">
              {okResults.map((r) => (
                <tr key={r.provider_id}>
                  <td className="px-3 py-1.5">{r.provider_name}</td>
                  <td className="px-3 py-1.5 text-text/70">{r.model}</td>
                  <td className="px-3 py-1.5 text-right font-medium">
                    {r.tokens.toLocaleString()}
                  </td>
                  <td className="px-3 py-1.5 text-right text-text/70">
                    {formatDelta(r.tokens)}
                  </td>
                  <td className="px-3 py-1.5 text-right text-text/50">
                    {formatElapsed(r.elapsed_ms)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {sweepDone && (
            <div className="text-xs text-text/50">
              {t("tokenCount.sweepSummary", {
                ok: okResults.length,
                total: sweepResults.length,
              })}
              {failedResults.length > 0 && (
                <button
                  onClick={() => setShowFailed((v) => !v)}
                  className="ml-2 underline cursor-pointer hover:text-text"
                >
                  {showFailed
                    ? t("tokenCount.hideFailed")
                    : t("tokenCount.showFailed", {
                        count: failedResults.length,
                      })}
                </button>
              )}
            </div>
          )}
          {showFailed && failedResults.length > 0 && (
            <ul className="text-xs text-text/50 space-y-0.5">
              {failedResults.map((r) => (
                <li key={r.provider_id}>
                  {r.provider_name} ({r.model}): {r.error}
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
};
