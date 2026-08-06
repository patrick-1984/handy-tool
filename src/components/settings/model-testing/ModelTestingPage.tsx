import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { downloadDir, join } from "@tauri-apps/api/path";
import { Trash2, X, ImagePlus } from "lucide-react";
import {
  commands,
  type ChatOutcome,
  type LlmProvider,
  type ModelTestLibrary,
  type ModelTestRun,
  type NamedImage,
  type NamedText,
} from "@/bindings";
import { Dropdown } from "@/components/ui";
import { useNavStore } from "@/stores/navStore";
import { useSettings } from "../../../hooks/useSettings";
import { SavePromptButton } from "./SavePromptButton";

const actionButtonClass =
  "px-4 py-1.5 rounded-md bg-blue-600 text-white text-sm font-medium hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors cursor-pointer";
const secondaryButtonClass =
  "px-3 py-1.5 rounded-md border border-zinc-700 bg-zinc-800 text-zinc-100 text-sm hover:border-blue-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors cursor-pointer";
const textareaClass =
  "w-full rounded-md border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm text-zinc-100 placeholder-zinc-500 focus:border-blue-500 focus:outline-none font-mono";
const providerLinkClass =
  "text-xs text-logo-primary/85 hover:text-logo-primary hover:underline transition-colors cursor-pointer rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-logo-primary";

// Attached images are persisted (base64) into the prompt library, which lives in
// the single monolithic settings blob re-serialized on every settings change.
// Cap the size so a large image can't bloat that blob.
const MAX_IMAGE_MB = 4;
const MAX_IMAGE_BYTES = MAX_IMAGE_MB * 1024 * 1024;

const fmtTok = (n: number | null): string =>
  n == null ? "—" : n.toLocaleString();
const fmtTime = (ms: number): string =>
  ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${ms}ms`;
const fmtCost = (o: ChatOutcome): string =>
  o.cost_usd == null
    ? "—"
    : `${o.cost_is_real ? "" : "~"}$${o.cost_usd.toFixed(4)}`;
const totalCost = (run: ModelTestRun): number =>
  run.outcomes.reduce((sum, o) => sum + (o.cost_usd ?? 0), 0);

export const ModelTestingPage: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const navigateTo = useNavStore((state) => state.navigateTo);

  const providers =
    (getSetting("llm_providers") as LlmProvider[] | undefined) ?? [];
  // Only providers that can run chat completions in the tester.
  const indexed = providers
    .map((p, idx) => ({ p, idx }))
    .filter(
      ({ p }) =>
        // Enabled, chat-capable providers only — disabled/unconfigured slots
        // (e.g. unused OpenRouter seats) must not appear here.
        p.enabled &&
        p.kind !== "openai_local" &&
        p.kind !== "apple_intelligence",
    );

  const [runIds, setRunIds] = useState<string[]>([]);
  const [judgeIds, setJudgeIds] = useState<string[]>([]);
  const [mainPrompt, setMainPrompt] = useState("");
  const [judgePrompt, setJudgePrompt] = useState("");
  // Temperature + thinking are configured separately for the runner models and
  // for the judge panel (e.g. thinking off for runners, on for the judge).
  const [temperature, setTemperature] = useState(0.3);
  const [judgeTemperature, setJudgeTemperature] = useState(0.3);

  const [running, setRunning] = useState(false);
  const [phase, setPhase] = useState<"idle" | "main" | "judge">("idle");
  // Live activity feed shown during a run (one line per model/judge as it
  // finishes, plus phase markers).
  const [statusLog, setStatusLog] = useState<string[]>([]);
  const [mainRun, setMainRun] = useState<ModelTestRun | null>(null);
  const [judgeRun, setJudgeRun] = useState<ModelTestRun | null>(null);
  // Wall-clock the main run completed at, captured once (not recomputed every
  // render) so the report header reflects run time, not the latest re-render.
  const [reportedAt, setReportedAt] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [thinking, setThinking] = useState<"auto" | "on" | "off">("auto");
  const [judgeThinking, setJudgeThinking] = useState<"auto" | "on" | "off">(
    "auto",
  );
  const [imageDataUrl, setImageDataUrl] = useState<string | null>(null);
  const [imageName, setImageName] = useState<string | null>(null);
  const [dragOver, setDragOver] = useState(false);
  const [savePath, setSavePath] = useState<string | null>(() =>
    localStorage.getItem("handy.modelTestSavePath"),
  );
  // Prompt-library selection. The prompt fields always show the editable text;
  // these ids just track which saved prompt / preset (if any) is currently
  // loaded, so the pickers reflect it and editing marks the field "custom".
  const [selectedModelPromptId, setSelectedModelPromptId] = useState<
    string | null
  >(null);
  const [selectedJudgePromptId, setSelectedJudgePromptId] = useState<
    string | null
  >(null);
  const [selectedPresetId, setSelectedPresetId] = useState<string | null>(null);
  const runningRef = useRef(false);
  // Monotonic run token: bumped on every run and on cancel, so a resolved
  // (possibly partial) result from a cancelled/superseded run is discarded.
  const runIdRef = useRef(0);
  const phaseRef = useRef<"idle" | "main" | "judge">("idle");
  const fileInputRef = useRef<HTMLInputElement>(null);

  const thinkingValue: boolean | null =
    thinking === "auto" ? null : thinking === "on";
  const judgeThinkingValue: boolean | null =
    judgeThinking === "auto" ? null : judgeThinking === "on";

  const pushStatus = (line: string) => setStatusLog((log) => [...log, line]);

  useEffect(() => {
    const unlisten = listen<ChatOutcome>("model-test-progress", (e) => {
      if (!runningRef.current) return;
      const o = e.payload;
      const role = phaseRef.current === "judge" ? "judge" : "model";
      const line = o.ok
        ? `✓ ${role}: ${o.provider_name} (${o.model}) — ${fmtTime(o.elapsed_ms)}`
        : `✗ ${role}: ${o.provider_name} (${o.model}) — ${o.error ?? "failed"}`;
      setStatusLog((log) => [...log, line]);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const toggle = (
    list: string[],
    setList: (next: string[]) => void,
    id: string,
  ) => {
    setList(list.includes(id) ? list.filter((x) => x !== id) : [...list, id]);
  };

  const labelFor = (id: string): string => {
    const idx = providers.findIndex((p) => p.id === id);
    const p = providers[idx];
    if (!p) return id;
    return `#${idx + 1} ${p.name}${p.model ? ` · ${p.model}` : ""}`;
  };

  // --- Prompt library (saved model prompts, judge prompts, combined presets) ---
  const library =
    (getSetting("model_test_library") as ModelTestLibrary | undefined) ?? {};
  const modelPrompts = library.model_prompts ?? [];
  const judgePrompts = library.judge_prompts ?? [];
  const presets = library.presets ?? [];
  const newId = () =>
    Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
  const saveLibrary = (next: ModelTestLibrary) =>
    updateSetting("model_test_library", next);

  const currentImage = (): NamedImage | null =>
    imageDataUrl
      ? { name: imageName ?? "image", data_url: imageDataUrl }
      : null;

  const saveModelPrompt = (name: string) =>
    saveLibrary({
      ...library,
      model_prompts: [
        ...modelPrompts,
        { id: newId(), name, text: mainPrompt, image: currentImage() },
      ],
    });
  const saveJudgePrompt = (name: string) =>
    saveLibrary({
      ...library,
      judge_prompts: [
        ...judgePrompts,
        { id: newId(), name, text: judgePrompt },
      ],
    });
  // A preset references its saved model/judge prompts. Reuse the currently
  // selected saved prompts when present; otherwise create new saved entries
  // (named after the preset) so the preset's parts always exist as selectable
  // saved prompts. The raw text is also stored as a deletion fallback.
  const savePreset = (name: string) => {
    let nextModelPrompts = modelPrompts;
    let nextJudgePrompts = judgePrompts;
    let mpId = selectedModelPromptId;
    let jpId = selectedJudgePromptId;
    if (!mpId && mainPrompt.trim()) {
      mpId = newId();
      nextModelPrompts = [
        ...modelPrompts,
        { id: mpId, name, text: mainPrompt, image: currentImage() },
      ];
    }
    if (!jpId && judgePrompt.trim()) {
      jpId = newId();
      nextJudgePrompts = [
        ...judgePrompts,
        { id: jpId, name, text: judgePrompt },
      ];
    }
    const presetId = newId();
    saveLibrary({
      ...library,
      model_prompts: nextModelPrompts,
      judge_prompts: nextJudgePrompts,
      presets: [
        ...presets,
        {
          id: presetId,
          name,
          model_prompt_id: mpId,
          judge_prompt_id: jpId,
          model_prompt: mainPrompt,
          judge_prompt: judgePrompt,
        },
      ],
    });
    setSelectedModelPromptId(mpId);
    setSelectedJudgePromptId(jpId);
    setSelectedPresetId(presetId);
  };

  // Load a saved prompt into its field (incl. the model prompt's image).
  const applyModelPrompt = (p: NamedText) => {
    setMainPrompt(p.text);
    setSelectedModelPromptId(p.id);
    setImageDataUrl(p.image?.data_url ?? null);
    setImageName(p.image?.name ?? null);
  };
  const applyJudgePrompt = (p: NamedText) => {
    setJudgePrompt(p.text);
    setSelectedJudgePromptId(p.id);
  };

  const pickModelPrompt = (id: string | null) => {
    const p = modelPrompts.find((x) => x.id === id);
    if (!p) return;
    applyModelPrompt(p);
    setSelectedPresetId(null);
  };
  const pickJudgePrompt = (id: string | null) => {
    const p = judgePrompts.find((x) => x.id === id);
    if (!p) return;
    applyJudgePrompt(p);
    setSelectedPresetId(null);
  };
  // Selecting a preset selects its parts in the prompt pickers (resolving the
  // referenced saved prompts) and loads their text/image; legacy presets fall
  // back to their stored raw text.
  const pickPreset = (id: string | null) => {
    const p = presets.find((x) => x.id === id);
    if (!p) return;
    const mp = p.model_prompt_id
      ? modelPrompts.find((x) => x.id === p.model_prompt_id)
      : undefined;
    if (mp) {
      applyModelPrompt(mp);
    } else {
      setMainPrompt(p.model_prompt ?? "");
      setSelectedModelPromptId(null);
      setImageDataUrl(null);
      setImageName(null);
    }
    const jp = p.judge_prompt_id
      ? judgePrompts.find((x) => x.id === p.judge_prompt_id)
      : undefined;
    if (jp) {
      applyJudgePrompt(jp);
    } else {
      setJudgePrompt(p.judge_prompt ?? "");
      setSelectedJudgePromptId(null);
    }
    setSelectedPresetId(p.id);
  };

  // Editing a field (text or image) detaches it from the loaded saved prompt.
  const markModelCustom = () => {
    setSelectedModelPromptId(null);
    setSelectedPresetId(null);
  };
  const markJudgeCustom = () => {
    setSelectedJudgePromptId(null);
    setSelectedPresetId(null);
  };
  const deleteModelPrompt = (id: string) => {
    saveLibrary({
      ...library,
      model_prompts: modelPrompts.filter((p) => p.id !== id),
    });
    if (selectedModelPromptId === id) markModelCustom();
  };
  const deleteJudgePrompt = (id: string) => {
    saveLibrary({
      ...library,
      judge_prompts: judgePrompts.filter((p) => p.id !== id),
    });
    if (selectedJudgePromptId === id) markJudgeCustom();
  };
  const deletePreset = (id: string) => {
    saveLibrary({ ...library, presets: presets.filter((p) => p.id !== id) });
    if (selectedPresetId === id) setSelectedPresetId(null);
  };

  // "in $X · out $Y /1M" for a provider with a non-zero configured price.
  const costLabel = (p: LlmProvider): string => {
    const ci = p.cost_input_per_million ?? 0;
    const co = p.cost_output_per_million ?? 0;
    if (ci <= 0 && co <= 0) return "";
    return t("modelTesting.costPerM", { input: ci, output: co });
  };

  // Build the judge request so EVERY candidate answer is visible AND the
  // arbiter instructions live in the USER message (not only the system prompt).
  // Small/local models (LM Studio, FLM) routinely down-weight or ignore system
  // prompts, which made them "not see the other answers" and return junk; cloud
  // models that honour system prompts were fine. Putting the task + numbered
  // answers together in one user message fixes both.
  const buildJudgePrompt = (
    arbiter: string,
    input: string,
    outcomes: ChatOutcome[],
  ): { system: string; user: string } => {
    const answered = outcomes.filter((o) => o.ok);
    const numbered = answered
      .map(
        (o, i) =>
          `<answer index="${i + 1}" provider="${o.provider_name}" model="${o.model}">\n${o.content}\n</answer>`,
      )
      .join("\n\n");
    const user = [
      "You are judging multiple candidate answers to the same prompt.",
      `There are ${answered.length} answers, labelled <answer index="1"> through <answer index="${answered.length}">. Read and weigh ALL of them — do not judge only the first.`,
      "",
      "# Evaluation task",
      arbiter.trim(),
      "",
      "# Original prompt given to the models",
      `<original_prompt>\n${input.trim()}\n</original_prompt>`,
      "",
      `# Candidate answers (${answered.length})`,
      `<answers count="${answered.length}">`,
      numbered,
      "</answers>",
    ].join("\n");
    const system = `You are an impartial evaluator comparing ${answered.length} candidate answers. Consider every answer, then complete the user's evaluation task.`;
    return { system, user };
  };

  const summaryTableMd = (rows: ChatOutcome[]): string => {
    const head =
      "| Model | Input tok | Output tok | Cost | Time |\n|---|---:|---:|---:|---:|";
    const body = rows
      .map(
        (o) =>
          `| ${o.provider_name} (${o.model})${o.ok ? "" : " — FAILED"} | ${fmtTok(
            o.input_tokens,
          )} | ${fmtTok(o.output_tokens)} | ${fmtCost(o)} | ${fmtTime(
            o.elapsed_ms,
          )} |`,
      )
      .join("\n");
    return `${head}\n${body}`;
  };

  const answerBlocksMd = (rows: ChatOutcome[]): string =>
    rows
      .map(
        (o) =>
          `### ${o.provider_name} (${o.model})\n\n${
            o.ok ? o.content : `**Error:** ${o.error ?? "unknown"}`
          }`,
      )
      .join("\n\n");

  const buildReport = (): string => {
    if (!mainRun) return "";
    const out: string[] = [];
    out.push(`# Model Testing — ${reportedAt ?? new Date().toLocaleString()}`);
    out.push("");
    out.push("## Input");
    out.push("");
    out.push("```");
    out.push(mainPrompt.trim());
    out.push("```");
    out.push("");
    out.push("## Summary");
    out.push("");
    out.push(summaryTableMd(mainRun.outcomes));
    out.push("");
    out.push(
      `**Round-trip (longest):** ${fmtTime(mainRun.round_trip_ms)} · **Total cost:** $${totalCost(
        mainRun,
      ).toFixed(4)}`,
    );
    out.push(
      `**Model params:** temperature ${temperature.toFixed(2)} · thinking ${thinking}`,
    );
    out.push("");
    if (judgeRun) {
      out.push("## Judge Panel");
      out.push("");
      out.push("> **Arbiter prompt:**");
      judgePrompt
        .trim()
        .split("\n")
        .forEach((l) => out.push(`> ${l}`));
      out.push("");
      out.push(summaryTableMd(judgeRun.outcomes));
      out.push("");
      out.push(
        `**Round-trip (longest):** ${fmtTime(judgeRun.round_trip_ms)} · **Total cost:** $${totalCost(
          judgeRun,
        ).toFixed(4)}`,
      );
      out.push(
        `**Judge params:** temperature ${judgeTemperature.toFixed(2)} · thinking ${judgeThinking}`,
      );
      out.push("");
      out.push(answerBlocksMd(judgeRun.outcomes));
      out.push("");
    }
    out.push("## Answers");
    out.push("");
    out.push(answerBlocksMd(mainRun.outcomes));
    out.push("");
    return out.join("\n");
  };

  const report = buildReport();

  const handleRun = async () => {
    if (!mainPrompt.trim() || runIds.length === 0 || running) return;
    const myRun = ++runIdRef.current;
    setRunning(true);
    runningRef.current = true;
    setError(null);
    setMainRun(null);
    setJudgeRun(null);
    setReportedAt(null);
    setStatusLog([]);
    setPhase("main");
    phaseRef.current = "main";
    pushStatus(t("modelTesting.status.mainStart", { count: runIds.length }));
    try {
      const mainResult = await commands.runModelTest(
        null,
        mainPrompt,
        runIds,
        temperature,
        thinkingValue,
        imageDataUrl,
      );
      // Discard a cancelled/superseded run's result.
      if (runIdRef.current !== myRun) return;
      if (mainResult.status !== "ok") {
        setError(mainResult.error);
        return;
      }
      setMainRun(mainResult.data);
      setReportedAt(new Date().toLocaleString());

      const okCount = mainResult.data.outcomes.filter((o) => o.ok).length;
      const doJudge =
        judgeIds.length > 0 && judgePrompt.trim().length > 0 && okCount > 0;
      if (doJudge) {
        setPhase("judge");
        phaseRef.current = "judge";
        pushStatus(
          t("modelTesting.status.judgeStart", { count: judgeIds.length }),
        );
        const { system: judgeSystem, user: judgeUser } = buildJudgePrompt(
          judgePrompt,
          mainPrompt,
          mainResult.data.outcomes,
        );
        const judgeResult = await commands.runModelTest(
          judgeSystem,
          judgeUser,
          judgeIds,
          judgeTemperature,
          judgeThinkingValue,
          null,
        );
        if (runIdRef.current !== myRun) return;
        if (judgeResult.status === "ok") {
          setJudgeRun(judgeResult.data);
        } else {
          setError(judgeResult.error);
        }
      }
      if (runIdRef.current === myRun) pushStatus(t("modelTesting.status.done"));
    } catch (e) {
      if (runIdRef.current === myRun) setError(String(e));
    } finally {
      // Only the still-current run resets shared UI state (a newer run or a
      // cancel manages its own).
      if (runIdRef.current === myRun) {
        setRunning(false);
        runningRef.current = false;
        setPhase("idle");
        phaseRef.current = "idle";
      }
    }
  };

  const handleCancel = async () => {
    // Invalidate the in-flight run so its resolved result is ignored, then
    // tell the backend to stop dispatching further providers.
    runIdRef.current += 1;
    runningRef.current = false;
    setRunning(false);
    setPhase("idle");
    phaseRef.current = "idle";
    pushStatus(t("modelTesting.status.cancelled"));
    await commands.cancelModelTest();
  };

  const handleCopy = async () => {
    if (!report) return;
    await writeText(report);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };

  const readImageFile = (file: File) => {
    if (!file.type.startsWith("image/")) return;
    if (file.size > MAX_IMAGE_BYTES) {
      setError(t("modelTesting.image.tooLarge", { mb: MAX_IMAGE_MB }));
      return;
    }
    setError(null);
    const reader = new FileReader();
    reader.onload = () => {
      setImageDataUrl(reader.result as string);
      setImageName(file.name);
      // A user-chosen image diverges from any loaded saved prompt.
      markModelCustom();
    };
    reader.readAsDataURL(file);
  };

  const slugify = (s: string) =>
    s
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .split("-")
      .filter(Boolean)
      .slice(0, 2)
      .join("-")
      .slice(0, 40);

  const timestamp = () => {
    const d = new Date();
    const p = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(
      d.getHours(),
    )}${p(d.getMinutes())}${p(d.getSeconds())}`;
  };

  // <preset>-<ts>.md if a preset was used, else custom-<short-slug>-<ts>.md
  const defaultFileName = () => {
    const ts = timestamp();
    const preset = selectedPresetId
      ? presets.find((p) => p.id === selectedPresetId)
      : null;
    if (preset) return `${slugify(preset.name) || "preset"}-${ts}.md`;
    const slug = slugify(mainPrompt);
    return slug ? `custom-${slug}-${ts}.md` : `custom-${ts}.md`;
  };

  const writeReportTo = async (path: string) => {
    const res = await commands.writeTextFile(path, report);
    if (res.status === "error") {
      setError(res.error);
      return;
    }
    setSavePath(path);
    try {
      localStorage.setItem("handy.modelTestSavePath", path);
    } catch {
      // ignore quota errors
    }
  };

  const handleSaveAs = async () => {
    if (!report) return;
    try {
      let def = defaultFileName();
      try {
        def = await join(await downloadDir(), def);
      } catch {
        // Downloads dir not resolvable — fall back to a bare filename
      }
      const path = await save({
        defaultPath: def,
        filters: [{ name: "Markdown", extensions: ["md"] }],
      });
      if (path) await writeReportTo(path);
    } catch (e) {
      setError(String(e));
    }
  };

  const handleSave = async () => {
    if (!report || !savePath) return;
    await writeReportTo(savePath);
  };

  const renderSummary = (run: ModelTestRun) => (
    <div className="space-y-2">
      <div className="overflow-x-auto">
        <table className="w-full text-sm border-collapse">
          <thead>
            <tr className="text-left text-text/50 border-b border-mid-gray/20">
              <th className="px-2 py-1.5">{t("modelTesting.colModel")}</th>
              <th className="px-2 py-1.5 text-right">
                {t("modelTesting.colInput")}
              </th>
              <th className="px-2 py-1.5 text-right">
                {t("modelTesting.colOutput")}
              </th>
              <th className="px-2 py-1.5 text-right">
                {t("modelTesting.colCost")}
              </th>
              <th className="px-2 py-1.5 text-right">
                {t("modelTesting.colTime")}
              </th>
            </tr>
          </thead>
          <tbody>
            {run.outcomes.map((o) => (
              <tr
                key={o.provider_id}
                className={`border-b border-mid-gray/10 ${
                  o.ok ? "" : "text-red-400"
                }`}
              >
                <td className="px-2 py-1.5">
                  {o.provider_name} ({o.model})
                </td>
                <td className="px-2 py-1.5 text-right tabular-nums">
                  {fmtTok(o.input_tokens)}
                </td>
                <td className="px-2 py-1.5 text-right tabular-nums">
                  {fmtTok(o.output_tokens)}
                </td>
                <td className="px-2 py-1.5 text-right tabular-nums">
                  {fmtCost(o)}
                </td>
                <td className="px-2 py-1.5 text-right tabular-nums">
                  {fmtTime(o.elapsed_ms)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="text-xs text-text/60">
        {`${t("modelTesting.roundTrip")}: ${fmtTime(run.round_trip_ms)} · ${t(
          "modelTesting.totalCost",
        )}: $${totalCost(run).toFixed(4)}`}
      </p>
    </div>
  );

  const renderAnswers = (run: ModelTestRun) => (
    <div className="space-y-2">
      {run.outcomes.map((o) => (
        <div
          key={o.provider_id}
          className="border border-mid-gray/20 rounded-lg p-3 space-y-1 bg-mid-gray/5"
        >
          <p className="text-sm font-semibold">
            {o.provider_name} ({o.model})
          </p>
          {o.ok ? (
            <pre className="whitespace-pre-wrap break-words text-sm text-text/90 font-sans">
              {o.content}
            </pre>
          ) : (
            <p className="text-sm text-red-400">
              {t("modelTesting.errorPrefix")}: {o.error}
            </p>
          )}
        </div>
      ))}
    </div>
  );

  return (
    <div className="w-full space-y-4">
      <p className="text-sm text-text/60">{t("modelTesting.description")}</p>

      {/* Provider selection */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <span className="text-sm font-semibold">
            {t("modelTesting.providersLabel")}
          </span>
          <div className="flex gap-2">
            <button
              type="button"
              className={providerLinkClass}
              onClick={() => navigateTo("advanced", "providers")}
            >
              {t("modelTesting.configureProviders")}
            </button>
            <button
              type="button"
              className={secondaryButtonClass}
              onClick={() => setRunIds(indexed.map(({ p }) => p.id))}
            >
              {t("modelTesting.selectAll")}
            </button>
            <button
              type="button"
              className={secondaryButtonClass}
              onClick={() => {
                setRunIds([]);
                setJudgeIds([]);
              }}
            >
              {t("modelTesting.clear")}
            </button>
          </div>
        </div>
        {indexed.length === 0 ? (
          <div className="flex items-center gap-2 text-sm text-text/50">
            <span>{t("modelTesting.noProviders")}</span>
            <button
              type="button"
              className={providerLinkClass}
              onClick={() => navigateTo("advanced", "providers")}
            >
              {t("modelTesting.configureProviders")}
            </button>
          </div>
        ) : (
          <div className="space-y-1">
            {indexed.map(({ p }) => (
              <div
                key={p.id}
                className="flex items-center gap-2 border border-mid-gray/20 rounded-md px-3 py-1.5 bg-mid-gray/5"
              >
                <span className="text-sm truncate flex-1">
                  {labelFor(p.id)}
                </span>
                {costLabel(p) && (
                  <span className="text-[11px] text-text/40 tabular-nums shrink-0">
                    {costLabel(p)}
                  </span>
                )}
                <div className="flex gap-3 shrink-0">
                  <label className="flex items-center gap-1.5 text-xs text-text/70 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={runIds.includes(p.id)}
                      onChange={() => toggle(runIds, setRunIds, p.id)}
                      className="w-3.5 h-3.5 accent-blue-600 cursor-pointer"
                    />
                    {t("modelTesting.run")}
                  </label>
                  <label className="flex items-center gap-1.5 text-xs text-text/70 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={judgeIds.includes(p.id)}
                      onChange={() => toggle(judgeIds, setJudgeIds, p.id)}
                      className="w-3.5 h-3.5 accent-purple-600 cursor-pointer"
                    />
                    {t("modelTesting.judge")}
                  </label>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Preset: a saved model-prompt + judge-prompt pair */}
      <div className="flex items-center gap-1.5 flex-wrap">
        <span className="text-xs text-text/50">
          {t("modelTesting.library.preset")}
        </span>
        <Dropdown
          selectedValue={selectedPresetId}
          options={presets.map((p) => ({ value: p.id, label: p.name }))}
          onSelect={pickPreset}
          placeholder={t("modelTesting.library.presetPlaceholder")}
          className="min-w-[180px]"
        />
        {selectedPresetId && (
          <button
            type="button"
            title={t("modelTesting.library.delete")}
            onClick={() => deletePreset(selectedPresetId)}
            className="p-1 rounded-md border border-zinc-700 text-text/50 hover:text-red-400 hover:border-red-400 transition-colors cursor-pointer"
          >
            <Trash2 className="w-3.5 h-3.5" />
          </button>
        )}
        <SavePromptButton
          onSave={savePreset}
          disabled={!mainPrompt.trim() && !judgePrompt.trim()}
        />
      </div>

      {/* Model prompt */}
      <div className="space-y-1">
        <div className="flex items-center gap-1.5 flex-wrap">
          <span className="text-sm font-semibold flex-1">
            {t("modelTesting.mainPromptLabel")}
          </span>
          <Dropdown
            selectedValue={selectedModelPromptId}
            options={modelPrompts.map((p) => ({
              value: p.id,
              label: p.image ? `${p.name} 🖼 ${p.image.name}` : p.name,
            }))}
            onSelect={pickModelPrompt}
            placeholder={t("modelTesting.library.pick")}
            className="min-w-[160px]"
          />
          {selectedModelPromptId && (
            <button
              type="button"
              title={t("modelTesting.library.delete")}
              onClick={() => deleteModelPrompt(selectedModelPromptId)}
              className="p-1 rounded-md border border-zinc-700 text-text/50 hover:text-red-400 hover:border-red-400 transition-colors cursor-pointer"
            >
              <Trash2 className="w-3.5 h-3.5" />
            </button>
          )}
          <SavePromptButton
            onSave={saveModelPrompt}
            disabled={!mainPrompt.trim()}
          />
        </div>
        <textarea
          value={mainPrompt}
          onChange={(e) => {
            setMainPrompt(e.target.value);
            markModelCustom();
          }}
          placeholder={t("modelTesting.mainPromptPlaceholder")}
          rows={6}
          className={textareaClass}
        />
      </div>

      {/* Image attachment for runners (vision-capable models) */}
      <div className="space-y-1">
        <span className="text-xs text-text/50">
          {t("modelTesting.image.label")}
        </span>
        <input
          ref={fileInputRef}
          type="file"
          accept="image/*"
          className="hidden"
          onChange={(e) => {
            const f = e.target.files?.[0];
            if (f) readImageFile(f);
            e.target.value = "";
          }}
        />
        {imageDataUrl ? (
          <div className="flex items-center gap-2 rounded-md border border-zinc-700 bg-zinc-800/60 p-2">
            <img
              src={imageDataUrl}
              alt={imageName ?? ""}
              className="h-12 w-12 rounded object-cover"
            />
            <span className="text-sm flex-1 truncate text-text/80">
              {imageName}
            </span>
            <button
              type="button"
              onClick={() => {
                setImageDataUrl(null);
                setImageName(null);
                markModelCustom();
              }}
              className="flex items-center gap-1 text-xs text-text/50 hover:text-red-400 cursor-pointer"
            >
              <X className="w-3.5 h-3.5" />
              {t("modelTesting.image.remove")}
            </button>
          </div>
        ) : (
          <div
            onClick={() => fileInputRef.current?.click()}
            onDragOver={(e) => {
              e.preventDefault();
              setDragOver(true);
            }}
            onDragLeave={() => setDragOver(false)}
            onDrop={(e) => {
              e.preventDefault();
              setDragOver(false);
              const f = e.dataTransfer.files?.[0];
              if (f) readImageFile(f);
            }}
            className={`flex items-center justify-center gap-2 rounded-md border border-dashed px-3 py-3 text-sm cursor-pointer transition-colors ${
              dragOver
                ? "border-blue-500 bg-blue-600/10 text-text"
                : "border-zinc-700 text-text/50 hover:border-blue-500"
            }`}
          >
            <ImagePlus className="w-4 h-4" />
            {t("modelTesting.image.drop")}
          </div>
        )}
      </div>

      {/* Judge prompt */}
      <div className="space-y-1">
        <div className="flex items-center gap-1.5 flex-wrap">
          <span className="text-sm font-semibold flex-1">
            {t("modelTesting.judgePromptLabel")}
          </span>
          <Dropdown
            selectedValue={selectedJudgePromptId}
            options={judgePrompts.map((p) => ({ value: p.id, label: p.name }))}
            onSelect={pickJudgePrompt}
            placeholder={t("modelTesting.library.pick")}
            className="min-w-[160px]"
          />
          {selectedJudgePromptId && (
            <button
              type="button"
              title={t("modelTesting.library.delete")}
              onClick={() => deleteJudgePrompt(selectedJudgePromptId)}
              className="p-1 rounded-md border border-zinc-700 text-text/50 hover:text-red-400 hover:border-red-400 transition-colors cursor-pointer"
            >
              <Trash2 className="w-3.5 h-3.5" />
            </button>
          )}
          <SavePromptButton
            onSave={saveJudgePrompt}
            disabled={!judgePrompt.trim()}
          />
        </div>
        <textarea
          value={judgePrompt}
          onChange={(e) => {
            setJudgePrompt(e.target.value);
            markJudgeCustom();
          }}
          placeholder={t("modelTesting.judgePromptPlaceholder")}
          rows={4}
          className={textareaClass}
        />
        {/* Judge temperature + thinking (independent of the models') */}
        <div className="flex items-center gap-3 flex-wrap pt-1">
          <span className="text-xs text-text/50 w-16">
            {t("modelTesting.judgeParams")}
          </span>
          <input
            type="range"
            min="0"
            max="1"
            step="0.05"
            value={judgeTemperature}
            onChange={(e) =>
              setJudgeTemperature(Number.parseFloat(e.target.value))
            }
            className="w-36 accent-purple-600 cursor-pointer"
          />
          <span className="text-sm tabular-nums w-10 text-text/70">
            {judgeTemperature.toFixed(2)}
          </span>
          <Dropdown
            selectedValue={judgeThinking}
            options={[
              { value: "auto", label: t("modelTesting.thinking.auto") },
              { value: "on", label: t("modelTesting.thinking.on") },
              { value: "off", label: t("modelTesting.thinking.off") },
            ]}
            onSelect={(v) =>
              setJudgeThinking((v ?? "auto") as "auto" | "on" | "off")
            }
            className="min-w-[120px]"
          />
        </div>
      </div>

      {/* Model temperature + thinking + run */}
      <div className="flex items-center gap-3 flex-wrap">
        <span className="text-sm font-semibold">
          {t("modelTesting.modelParams")}
        </span>
        <input
          type="range"
          min="0"
          max="1"
          step="0.05"
          value={temperature}
          onChange={(e) => setTemperature(Number.parseFloat(e.target.value))}
          className="w-40 accent-blue-600 cursor-pointer"
        />
        <span className="text-sm tabular-nums w-10 text-text/70">
          {temperature.toFixed(2)}
        </span>
        <span className="text-sm font-semibold">
          {t("modelTesting.thinking.label")}
        </span>
        <Dropdown
          selectedValue={thinking}
          options={[
            { value: "auto", label: t("modelTesting.thinking.auto") },
            { value: "on", label: t("modelTesting.thinking.on") },
            { value: "off", label: t("modelTesting.thinking.off") },
          ]}
          onSelect={(v) => setThinking((v ?? "auto") as "auto" | "on" | "off")}
          className="min-w-[120px]"
        />
        <div className="flex-1" />
        {running ? (
          <button className={secondaryButtonClass} onClick={handleCancel}>
            {t("modelTesting.cancelButton")}
          </button>
        ) : null}
        <button
          className={actionButtonClass}
          onClick={handleRun}
          disabled={running || !mainPrompt.trim() || runIds.length === 0}
        >
          {t("modelTesting.runButton")}
        </button>
      </div>

      {(running || statusLog.length > 0) && (
        <div className="rounded-md border border-mid-gray/20 bg-mid-gray/5 p-2 space-y-1 max-h-40 overflow-y-auto">
          <div className="flex items-center gap-2 text-sm text-text/70">
            {running && (
              <span className="inline-block w-3 h-3 rounded-full border-2 border-logo-primary border-t-transparent animate-spin" />
            )}
            <span>
              {running
                ? phase === "judge"
                  ? t("modelTesting.runningJudge")
                  : t("modelTesting.runningMain")
                : t("modelTesting.status.activity")}
            </span>
          </div>
          {statusLog.length > 0 && (
            <div className="font-mono text-xs text-text/60 space-y-0.5">
              {statusLog.map((line, i) => (
                <div key={i}>{line}</div>
              ))}
            </div>
          )}
        </div>
      )}

      {error && <p className="text-sm text-red-400">{error}</p>}

      {/* Results */}
      {mainRun && (
        <div className="space-y-3 pt-2 border-t border-mid-gray/20">
          <h2 className="text-base font-semibold">
            {t("modelTesting.summaryTitle")}
          </h2>
          {renderSummary(mainRun)}

          {judgeRun && (
            <>
              <h2 className="text-base font-semibold">
                {t("modelTesting.judgePanelTitle")}
              </h2>
              {renderSummary(judgeRun)}
              {renderAnswers(judgeRun)}
            </>
          )}

          <h2 className="text-base font-semibold">
            {t("modelTesting.answersTitle")}
          </h2>
          {renderAnswers(mainRun)}

          {/* Artifact */}
          <div className="space-y-2 pt-2 border-t border-mid-gray/20">
            <div className="flex items-center gap-2 flex-wrap">
              <span className="text-sm font-semibold">
                {t("modelTesting.artifactLabel")}
              </span>
              {savePath && (
                <span
                  className="text-xs text-text/40 truncate max-w-[260px]"
                  title={savePath}
                >
                  {savePath}
                </span>
              )}
              <div className="flex-1" />
              <button className={secondaryButtonClass} onClick={handleCopy}>
                {copied
                  ? t("modelTesting.copied")
                  : t("modelTesting.copyMarkdown")}
              </button>
              <button
                className={secondaryButtonClass}
                onClick={handleSave}
                disabled={!savePath}
                title={
                  savePath
                    ? t("modelTesting.saveTo", { path: savePath })
                    : t("modelTesting.saveDisabled")
                }
              >
                {t("modelTesting.save")}
              </button>
              <button className={secondaryButtonClass} onClick={handleSaveAs}>
                {t("modelTesting.saveAs")}
              </button>
            </div>
            <textarea
              readOnly
              value={report}
              rows={12}
              className={textareaClass}
            />
          </div>
        </div>
      )}
    </div>
  );
};
