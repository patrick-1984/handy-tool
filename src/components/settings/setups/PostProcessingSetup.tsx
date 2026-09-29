import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2 } from "lucide-react";
import { SetupFrame, ChoiceCard, StatusBadge } from "./SetupFrame";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { Textarea } from "../../ui/Textarea";
import { Dropdown } from "../../ui/Dropdown";
import { ShortcutInput } from "../ShortcutInput";
import { useSettings } from "../../../hooks/useSettings";
import { commands, type LLMPrompt, type LlmProvider } from "@/bindings";

type Step = "prompt" | "provider" | "shortcut" | "try" | "done";
const STEPS: Step[] = ["prompt", "provider", "shortcut", "try", "done"];
const OWN = "__own__";

/** Online providers need a key; a server on this PC (LM Studio, FLM) does not. */
const needsKey = (p: LlmProvider) =>
  p.kind !== "openai_compatible" ||
  !/\/\/(localhost|127\.0\.0\.1|\[::1\])/.test(p.base_url ?? "");

/**
 * The Post-processing setup: what the AI should do with your words (a ready
 * prompt or your own), which AI (with its key and model right here), the
 * shortcut, and a try on a sample sentence. Turns post-processing on.
 */
export const PostProcessingSetup: React.FC<{ onClose: () => void }> = ({
  onClose,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, refreshSettings } = useSettings();
  const [step, setStep] = useState<Step>("prompt");
  const index = STEPS.indexOf(step);
  const next = () => setStep(STEPS[index + 1]);
  const back = () => setStep(STEPS[index - 1]);

  /* ─── prompt ─── */
  const prompts = (getSetting("post_process_prompts") as LLMPrompt[]) ?? [];
  const [promptId, setPromptId] = useState<string>(
    (getSetting("post_process_selected_prompt_id") as string) ||
      prompts[0]?.id ||
      OWN,
  );
  const [ownName, setOwnName] = useState("");
  const [ownText, setOwnText] = useState("");
  const savePrompt = async () => {
    let id = promptId;
    if (promptId === OWN) {
      const result = await commands.addPostProcessPrompt(
        ownName.trim() || t("setup.postProcessing.prompt.ownDefaultName"),
        ownText.trim(),
      );
      if (result.status !== "ok") return;
      id = result.data.id;
      setPromptId(id);
      await refreshSettings();
    }
    updateSetting("post_process_selected_prompt_id", id);
    updateSetting("post_process_enabled", true);
    next();
  };

  /* ─── provider ─── */
  const providers = (
    (getSetting("llm_providers") as LlmProvider[]) ?? []
  ).filter((p) => p.kind !== "openai_local");
  const [providerId, setProviderId] = useState<string>(
    (getSetting("post_process_provider_ref") as string) ||
      providers[0]?.id ||
      "",
  );
  const provider = providers.find((p) => p.id === providerId);
  const [edits, setEdits] = useState<
    Record<string, { api_key: string; base_url: string; model: string }>
  >({});
  const edit = provider
    ? (edits[provider.id] ?? {
        api_key: provider.api_key ?? "",
        base_url: provider.base_url ?? "",
        model: provider.model ?? "",
      })
    : null;
  const setEdit = (field: "api_key" | "base_url" | "model", value: string) =>
    provider &&
    edit &&
    setEdits((e) => ({ ...e, [provider.id]: { ...edit, [field]: value } }));
  const [models, setModels] = useState<string[] | null>(null);
  const [listing, setListing] = useState(false);
  const [listError, setListError] = useState<string | null>(null);
  // The edits into the registry (the same one Advanced › Providers shows).
  const commitProvider = async () => {
    if (!provider || !edit) return;
    const all = (getSetting("llm_providers") as LlmProvider[]) ?? [];
    await updateSetting(
      "llm_providers",
      all.map((p) => (p.id === provider.id ? { ...p, ...edit } : p)),
    );
  };
  const listModels = async () => {
    if (!provider) return;
    setListing(true);
    setListError(null);
    await commitProvider();
    const result = await commands.listProviderModels(provider.id);
    setListing(false);
    if (result.status === "ok") setModels(result.data);
    else setListError(result.error);
  };
  const saveProvider = async () => {
    await commitProvider();
    updateSetting("post_process_provider_ref", providerId);
    next();
  };
  const ready = (p: LlmProvider) => {
    const e = edits[p.id] ?? p;
    return !!e.model?.trim() && (!needsKey(p) || !!e.api_key?.trim());
  };

  /* ─── try ─── */
  const [sample, setSample] = useState(() =>
    t("setup.postProcessing.try.sample"),
  );
  const [result, setResult] = useState<string | null>(null);
  const [tryError, setTryError] = useState<string | null>(null);
  const [trying, setTrying] = useState(false);
  const tryIt = async () => {
    setTrying(true);
    setResult(null);
    setTryError(null);
    const r = await commands.postProcessSample(sample);
    setTrying(false);
    if (r.status === "ok") setResult(r.data);
    else setTryError(r.error);
  };

  // Where these live in the app, in its own words ("More › Post-processing").
  const pages = {
    postProcessingPage: `${t("sidebar.more")} › ${t("settings.advanced.tabs.postProcessing")}`,
    providersPage: `${t("sidebar.more")} › ${t("settings.advanced.tabs.providers")}`,
  };

  const common = {
    name: t("setup.postProcessing.name"),
    step: step === "done" ? 0 : index + 1,
    total: STEPS.length - 1,
    onClose,
    onBack: index > 0 && step !== "done" ? back : undefined,
  };

  switch (step) {
    case "prompt":
      return (
        <SetupFrame
          {...common}
          heading={t("setup.postProcessing.prompt.title")}
          body={t("setup.postProcessing.prompt.body", pages)}
          primary={t("setup.saveNext")}
          onPrimary={() => void savePrompt()}
          primaryDisabled={promptId === OWN && !ownText.trim()}
          onSkip={next}
        >
          <div className="flex flex-col gap-2">
            {prompts.map((p) => (
              <ChoiceCard
                key={p.id}
                selected={promptId === p.id}
                title={p.name}
                detail={p.prompt.split("\n")[0].slice(0, 140)}
                onClick={() => setPromptId(p.id)}
              />
            ))}
            <ChoiceCard
              selected={promptId === OWN}
              title={t("setup.postProcessing.prompt.own")}
              detail={t("setup.postProcessing.prompt.ownDetail")}
              onClick={() => setPromptId(OWN)}
            />
            {promptId === OWN && (
              <div className="flex flex-col gap-2 ps-1">
                <Input
                  value={ownName}
                  onChange={(e) => setOwnName(e.target.value)}
                  placeholder={t("setup.postProcessing.prompt.ownDefaultName")}
                />
                <Textarea
                  value={ownText}
                  onChange={(e) => setOwnText(e.target.value)}
                  placeholder={t("setup.postProcessing.prompt.ownPlaceholder")}
                  rows={4}
                />
              </div>
            )}
          </div>
        </SetupFrame>
      );
    case "provider":
      return (
        <SetupFrame
          {...common}
          heading={t("setup.postProcessing.provider.title")}
          body={t("setup.postProcessing.provider.body", pages)}
          primary={t("setup.saveNext")}
          onPrimary={() => void saveProvider()}
          primaryDisabled={!provider}
          onSkip={next}
        >
          <div className="grid grid-cols-2 gap-2">
            {providers.map((p) => (
              <ChoiceCard
                key={p.id}
                compact
                selected={providerId === p.id}
                title={p.name}
                detail={
                  (edits[p.id]?.model ?? p.model) ||
                  t("setup.postProcessing.provider.noModel")
                }
                onClick={() => {
                  setProviderId(p.id);
                  setModels(null);
                  setListError(null);
                }}
                badge={
                  ready(p) ? (
                    <StatusBadge
                      status="ready"
                      label={t("setup.postProcessing.provider.ready")}
                    />
                  ) : needsKey(p) ? (
                    <StatusBadge
                      status="key"
                      label={t("setup.postProcessing.provider.needsKey")}
                    />
                  ) : (
                    <StatusBadge
                      status="model"
                      label={t("setup.postProcessing.provider.needsModel")}
                    />
                  )
                }
              />
            ))}
          </div>
          {provider && edit && (
            <SettingsGroup title={provider.name}>
              {needsKey(provider) && (
                <label className="flex items-center gap-3 px-4 py-3">
                  <span className="w-24 shrink-0 text-sm">
                    {t("setup.postProcessing.provider.key")}
                  </span>
                  <Input
                    type="password"
                    value={edit.api_key}
                    onChange={(e) => setEdit("api_key", e.target.value)}
                    className="flex-1"
                  />
                </label>
              )}
              {provider.kind === "openai_compatible" && (
                <label className="flex items-center gap-3 px-4 py-3">
                  <span className="w-24 shrink-0 text-sm">
                    {t("setup.postProcessing.provider.address")}
                  </span>
                  <Input
                    value={edit.base_url}
                    onChange={(e) => setEdit("base_url", e.target.value)}
                    className="flex-1"
                  />
                </label>
              )}
              <div className="flex items-center gap-3 px-4 py-3">
                <span className="w-24 shrink-0 text-sm">
                  {t("setup.postProcessing.provider.model")}
                </span>
                {models && models.length > 0 ? (
                  <Dropdown
                    options={models.map((m) => ({ value: m, label: m }))}
                    selectedValue={edit.model || null}
                    onSelect={(value) => setEdit("model", value)}
                    className="flex-1 [&>button]:w-full"
                  />
                ) : (
                  <Input
                    value={edit.model}
                    onChange={(e) => setEdit("model", e.target.value)}
                    className="flex-1"
                  />
                )}
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => void listModels()}
                  disabled={listing}
                >
                  {listing && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
                  {t("setup.postProcessing.provider.list")}
                </Button>
              </div>
              {listError && (
                <p className="px-4 pb-3 text-[13px] text-err-text">
                  {t("setup.postProcessing.provider.listFailed", {
                    reason: listError,
                  })}
                </p>
              )}
            </SettingsGroup>
          )}
        </SetupFrame>
      );
    case "shortcut":
      return (
        <SetupFrame
          {...common}
          heading={t("setup.postProcessing.shortcut.title")}
          body={t("setup.postProcessing.shortcut.body")}
          primary={t("setup.saveNext")}
          onPrimary={next}
          onSkip={next}
        >
          <SettingsGroup>
            <ShortcutInput shortcutId="transcribe_with_post_process" grouped />
          </SettingsGroup>
        </SetupFrame>
      );
    case "try":
      return (
        <SetupFrame
          {...common}
          heading={t("setup.postProcessing.try.title")}
          body={t("setup.postProcessing.try.body")}
          primary={t("setup.next")}
          onPrimary={next}
        >
          <div className="flex flex-col gap-3">
            <Textarea
              value={sample}
              onChange={(e) => setSample(e.target.value)}
              rows={3}
            />
            <div>
              <Button
                variant="primary"
                size="sm"
                onClick={() => void tryIt()}
                disabled={trying || !sample.trim()}
              >
                {trying && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
                {t("setup.postProcessing.try.run")}
              </Button>
            </div>
            {result !== null && (
              <div className="flex flex-col gap-1">
                <span className="text-[13px] text-text-secondary">
                  {t("setup.postProcessing.try.result")}
                </span>
                <p className="text-sm whitespace-pre-wrap select-text rounded-lg border border-border bg-surface2 px-3 py-2">
                  {result}
                </p>
              </div>
            )}
            {tryError && (
              <p className="text-sm text-err-text">
                {t("setup.postProcessing.try.failed", { reason: tryError })}
              </p>
            )}
          </div>
        </SetupFrame>
      );
    case "done":
      return (
        <SetupFrame
          {...common}
          heading={t("setup.postProcessing.done.title")}
          body={t("setup.postProcessing.done.body", pages)}
          primary={t("setup.finish")}
          onPrimary={onClose}
        />
      );
  }
};
