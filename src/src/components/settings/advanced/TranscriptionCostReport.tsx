import React, { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { save } from "@tauri-apps/plugin-dialog";
import { downloadDir, join } from "@tauri-apps/api/path";
import { Download, RefreshCw } from "lucide-react";
import { commands, type HistoryEntry } from "@/bindings";

// A recording contributes its duration always; cost only when the engine
// reported one (OpenRouter). Aggregations bucket by ISO-ish week (Monday),
// calendar month, and year.

interface Bucket {
  key: string;
  label: string;
  count: number;
  duration: number; // seconds
  cost: number; // USD
}

const pad = (n: number) => String(n).padStart(2, "0");

const fmtDuration = (seconds: number): string => {
  const t = Math.max(0, Math.round(seconds));
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  const s = t % 60;
  return `${pad(h)}:${pad(m)}:${pad(s)}`;
};

const fmtCost = (usd: number): string => `$${usd.toFixed(4)}`;

const fmtTimestamp = (unixSeconds: number): string => {
  const d = new Date(unixSeconds * 1000);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(
    d.getHours(),
  )}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
};

// Monday (local) of the week containing `d`, as YYYY-MM-DD.
const weekStart = (d: Date): string => {
  const copy = new Date(d.getFullYear(), d.getMonth(), d.getDate());
  const dow = (copy.getDay() + 6) % 7; // 0 = Monday
  copy.setDate(copy.getDate() - dow);
  return `${copy.getFullYear()}-${pad(copy.getMonth() + 1)}-${pad(copy.getDate())}`;
};

const bucketBy = (
  entries: HistoryEntry[],
  keyOf: (d: Date) => string,
  labelOf: (key: string) => string,
): Bucket[] => {
  const map = new Map<string, Bucket>();
  for (const e of entries) {
    const d = new Date(e.timestamp * 1000);
    const key = keyOf(d);
    const b = map.get(key) ?? {
      key,
      label: labelOf(key),
      count: 0,
      duration: 0,
      cost: 0,
    };
    b.count += 1;
    b.duration += e.duration_seconds ?? 0;
    b.cost += e.cost_usd ?? 0;
    map.set(key, b);
  }
  return Array.from(map.values()).sort((a, b) => (a.key < b.key ? 1 : -1));
};

const timestampSlug = (): string => {
  const d = new Date();
  return `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}-${pad(
    d.getHours(),
  )}${pad(d.getMinutes())}${pad(d.getSeconds())}`;
};

const csvCell = (v: string | number): string => {
  const s = String(v);
  return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
};

export const TranscriptionCostReport: React.FC = () => {
  const { t } = useTranslation();
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = async () => {
    try {
      const res = await commands.getHistoryEntries();
      if (res.status === "ok") setEntries(res.data);
      else setError(res.error);
    } catch (e) {
      setError(String(e));
    }
  };

  useEffect(() => {
    load();
  }, []);

  const { days, weeks, months, years, total } = useMemo(() => {
    const days = bucketBy(
      entries,
      (d) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`,
      (key) => key,
    ).slice(0, 7);
    const weeks = bucketBy(
      entries,
      (d) => weekStart(d),
      (key) => t("settings.advanced.costReport.weekOf", { date: key }),
    ).slice(0, 4);
    const months = bucketBy(
      entries,
      (d) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}`,
      (key) => key,
    ).slice(0, 12);
    const years = bucketBy(
      entries,
      (d) => `${d.getFullYear()}`,
      (key) => key,
    );
    const total: Bucket = {
      key: "all",
      label: t("settings.advanced.costReport.allTime"),
      count: entries.length,
      duration: entries.reduce((s, e) => s + (e.duration_seconds ?? 0), 0),
      cost: entries.reduce((s, e) => s + (e.cost_usd ?? 0), 0),
    };
    return { days, weeks, months, years, total };
  }, [entries, t]);

  const buildCsv = (): string => {
    const lines: string[] = [];
    const push = (cells: (string | number)[]) =>
      lines.push(cells.map(csvCell).join(","));

    push([
      t("settings.advanced.costReport.title"),
      fmtTimestamp(Date.now() / 1000),
    ]);
    lines.push("");

    // Per-recording rows (oldest→newest for a natural ledger).
    push([t("settings.advanced.costReport.recordings")]);
    push([
      t("settings.advanced.costReport.colTimestamp"),
      t("settings.advanced.costReport.colDuration"),
      t("settings.advanced.costReport.colCost"),
    ]);
    [...entries]
      .sort((a, b) => a.timestamp - b.timestamp)
      .forEach((e) =>
        push([
          fmtTimestamp(e.timestamp),
          fmtDuration(e.duration_seconds ?? 0),
          (e.cost_usd ?? 0).toFixed(6),
        ]),
      );
    lines.push("");

    const section = (title: string, buckets: Bucket[]) => {
      push([title]);
      push([
        t("settings.advanced.costReport.colPeriod"),
        t("settings.advanced.costReport.colCount"),
        t("settings.advanced.costReport.colDuration"),
        t("settings.advanced.costReport.colCost"),
      ]);
      buckets.forEach((b) =>
        push([b.label, b.count, fmtDuration(b.duration), b.cost.toFixed(6)]),
      );
      lines.push("");
    };
    section(t("settings.advanced.costReport.daily"), days);
    section(t("settings.advanced.costReport.weekly"), weeks);
    section(t("settings.advanced.costReport.monthly"), months);
    section(t("settings.advanced.costReport.yearly"), years);
    push([
      t("settings.advanced.costReport.total"),
      total.count,
      fmtDuration(total.duration),
      total.cost.toFixed(6),
    ]);
    return lines.join("\n");
  };

  const handleDownload = async () => {
    setError(null);
    try {
      const name = `transcription cost report-${timestampSlug()}.csv`;
      let def = name;
      try {
        def = await join(await downloadDir(), name);
      } catch {
        // Downloads dir not resolvable — fall back to a bare filename.
      }
      const path = await save({
        defaultPath: def,
        filters: [{ name: "CSV", extensions: ["csv"] }],
      });
      if (!path) return;
      const res = await commands.writeTextFile(path, buildCsv());
      if (res.status === "error") setError(res.error);
    } catch (e) {
      setError(String(e));
    }
  };

  // Recompute durations for older recordings that predate duration tracking
  // (reads each audio file), then reload so the summaries are correct.
  const handleRecalc = async () => {
    setBusy(true);
    setError(null);
    try {
      const res = await commands.backfillHistoryDurations();
      if (res.status === "error") setError(res.error);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const renderTable = (title: string, buckets: Bucket[]) => (
    <div className="space-y-1">
      <p className="text-xs font-semibold text-text/70">{title}</p>
      {buckets.length === 0 ? (
        <p className="text-xs text-text/40">
          {t("settings.advanced.costReport.empty")}
        </p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-xs border-collapse">
            <thead>
              <tr className="text-left text-text/50 border-b border-mid-gray/20">
                <th className="px-2 py-1">
                  {t("settings.advanced.costReport.colPeriod")}
                </th>
                <th className="px-2 py-1 text-right">
                  {t("settings.advanced.costReport.colCount")}
                </th>
                <th className="px-2 py-1 text-right">
                  {t("settings.advanced.costReport.colDuration")}
                </th>
                <th className="px-2 py-1 text-right">
                  {t("settings.advanced.costReport.colCost")}
                </th>
              </tr>
            </thead>
            <tbody>
              {buckets.map((b) => (
                <tr key={b.key} className="border-b border-mid-gray/10">
                  <td className="px-2 py-1">{b.label}</td>
                  <td className="px-2 py-1 text-right tabular-nums">
                    {b.count}
                  </td>
                  <td className="px-2 py-1 text-right tabular-nums">
                    {fmtDuration(b.duration)}
                  </td>
                  <td className="px-2 py-1 text-right tabular-nums">
                    {fmtCost(b.cost)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );

  return (
    <div className="space-y-3 pt-2">
      <div className="flex items-center gap-2 flex-wrap">
        <span className="text-sm font-semibold flex-1">
          {t("settings.advanced.costReport.title")}
        </span>
        <button
          type="button"
          onClick={handleRecalc}
          disabled={busy}
          className="flex items-center gap-1 px-3 py-1.5 rounded-md border border-zinc-700 bg-zinc-800 text-zinc-100 text-sm hover:border-blue-500 disabled:opacity-50 transition-colors cursor-pointer"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${busy ? "animate-spin" : ""}`} />
          {t("settings.advanced.costReport.recalc")}
        </button>
        <button
          type="button"
          onClick={handleDownload}
          className="flex items-center gap-1 px-3 py-1.5 rounded-md border border-zinc-700 bg-zinc-800 text-zinc-100 text-sm hover:border-blue-500 transition-colors cursor-pointer"
        >
          <Download className="w-3.5 h-3.5" />
          {t("settings.advanced.costReport.download")}
        </button>
      </div>
      <p className="text-xs text-text/50">
        {t("settings.advanced.costReport.description")}
      </p>

      <div className="grid grid-cols-1 gap-3">
        {renderTable(t("settings.advanced.costReport.daily"), days)}
        {renderTable(t("settings.advanced.costReport.weekly"), weeks)}
        {renderTable(t("settings.advanced.costReport.monthly"), months)}
        {renderTable(t("settings.advanced.costReport.yearly"), years)}
      </div>

      <div className="flex items-center gap-4 border-t border-mid-gray/20 pt-2 text-sm">
        <span className="font-semibold">
          {t("settings.advanced.costReport.total")}
        </span>
        <span className="text-text/70 tabular-nums">
          {t("settings.advanced.costReport.totalLine", {
            count: total.count,
            duration: fmtDuration(total.duration),
            cost: fmtCost(total.cost),
          })}
        </span>
      </div>

      {error && <p className="text-sm text-red-400">{error}</p>}
    </div>
  );
};
