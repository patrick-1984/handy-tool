import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { type LlmProvider } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { SearchableModelSelect } from "../SearchableModelSelect";
import { resolveModelPrice } from "@/lib/openrouterPrices";

const fieldClass =
  "rounded-md border border-zinc-700 bg-zinc-800 px-2 py-1 text-sm text-zinc-100 placeholder-zinc-500 focus:border-blue-500 focus:outline-none disabled:opacity-50";

const labelClass = "text-xs text-text/60 w-24 shrink-0";

interface SlotProps {
  provider: LlmProvider;
  index: number;
  onCommit: (updated: LlmProvider) => void;
  onPriceResolved: (id: string, input: number, output: number) => void;
}

const ProviderSlot: React.FC<SlotProps> = ({
  provider,
  index,
  onCommit,
  onPriceResolved,
}) => {
  const { t } = useTranslation();
  const [name, setName] = useState(provider.name);
  const [baseUrl, setBaseUrl] = useState(provider.base_url ?? "");
  const [apiKey, setApiKey] = useState(provider.api_key ?? "");
  const [model, setModel] = useState(provider.model);
  const [group, setGroup] = useState(provider.concurrency_group ?? "");
  const [costIn, setCostIn] = useState(
    String(provider.cost_input_per_million ?? 0),
  );
  const [costOut, setCostOut] = useState(
    String(provider.cost_output_per_million ?? 0),
  );

  useEffect(() => setName(provider.name), [provider.name]);
  useEffect(() => setBaseUrl(provider.base_url ?? ""), [provider.base_url]);
  useEffect(() => setApiKey(provider.api_key ?? ""), [provider.api_key]);
  useEffect(() => setModel(provider.model), [provider.model]);
  useEffect(
    () => setGroup(provider.concurrency_group ?? ""),
    [provider.concurrency_group],
  );
  useEffect(
    () => setCostIn(String(provider.cost_input_per_million ?? 0)),
    [provider.cost_input_per_million],
  );
  useEffect(
    () => setCostOut(String(provider.cost_output_per_million ?? 0)),
    [provider.cost_output_per_million],
  );

  const isLocalTokenizer = provider.kind === "openai_local";
  const showBaseUrl = !isLocalTokenizer && provider.allow_base_url_edit;
  const showApiKey = !isLocalTokenizer;

  const commitField = (field: keyof LlmProvider, value: string) => {
    if (provider[field] !== value) {
      onCommit({ ...provider, [field]: value });
    }
  };

  const commitCost = (field: keyof LlmProvider, value: string) => {
    const parsed = Number.parseFloat(value);
    const next = Number.isFinite(parsed) && parsed >= 0 ? parsed : 0;
    if (provider[field] !== next) {
      onCommit({ ...provider, [field]: next });
    }
  };

  // Map the selected model to OpenRouter's pass-through pricing and auto-fill the
  // cost fields on change (unless the user has locked the price). Gemini/Anthropic
  // don't publish prices via their own API; OpenRouter reports the real per-request
  // cost at run time, but we still pre-fill the fields so the price is visible.
  // Best-effort + offline-safe.
  const autoPriceKind =
    provider.kind === "gemini" ||
    provider.kind === "anthropic" ||
    provider.kind === "openrouter";
  const onModelCommit = (v: string) => {
    commitField("model", v);
    if (!autoPriceKind || provider.persist_price) return;
    // resolveModelPrice may take seconds on a cache-miss network fetch. Patch the
    // cost fields by provider id against the *live* settings (see onPriceResolved)
    // rather than spreading this stale `provider` snapshot, which would otherwise
    // revert any edits the user made to this or sibling providers in the meantime.
    resolveModelPrice(provider.kind, v)
      .then((price) => {
        if (price) {
          onPriceResolved(provider.id, price.input, price.output);
        }
      })
      .catch(() => {});
  };

  return (
    <div className="border border-mid-gray/20 rounded-lg p-3 space-y-2 bg-mid-gray/5">
      <div className="flex items-center gap-2">
        <span
          className="text-xs font-mono font-semibold text-text/50 shrink-0 w-7"
          title={provider.id}
        >
          #{index + 1}
        </span>
        <input
          type="checkbox"
          checked={provider.enabled ?? false}
          onChange={(e) => onCommit({ ...provider, enabled: e.target.checked })}
          className="w-4 h-4 accent-blue-600 cursor-pointer"
          title={t("settings.advanced.llmProviders.enable")}
        />
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={() => commitField("name", name)}
          className={`${fieldClass} flex-1 font-medium`}
          placeholder={t("settings.advanced.llmProviders.namePlaceholder")}
        />
        <span className="text-[10px] text-text/40 uppercase tracking-wide shrink-0">
          {t(`settings.advanced.llmProviders.kinds.${provider.kind}`)}
        </span>
      </div>

      {showBaseUrl && (
        <div className="flex items-center gap-2">
          <span className={labelClass}>
            {t("settings.advanced.llmProviders.baseUrl")}
          </span>
          <input
            type="text"
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.target.value)}
            onBlur={() => commitField("base_url", baseUrl)}
            className={`${fieldClass} flex-1`}
          />
        </div>
      )}

      {showApiKey && (
        <div className="flex items-center gap-2">
          <span className={labelClass}>
            {t("settings.advanced.llmProviders.apiKey")}
          </span>
          <input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            onBlur={() => commitField("api_key", apiKey)}
            className={`${fieldClass} flex-1`}
            placeholder={t("settings.advanced.llmProviders.apiKeyPlaceholder")}
          />
        </div>
      )}

      <div className="flex items-center gap-2">
        <span className={labelClass}>
          {t("settings.advanced.llmProviders.model")}
        </span>
        {isLocalTokenizer ? (
          <select
            value={model}
            onChange={(e) => {
              setModel(e.target.value);
              commitField("model", e.target.value);
            }}
            className={`${fieldClass} flex-1 cursor-pointer`}
          >
            {["o200k_base", "cl100k_base"].map((id) => (
              <option key={id} value={id}>
                {id}
              </option>
            ))}
          </select>
        ) : (
          <SearchableModelSelect
            value={model}
            providerId={provider.id}
            onCommit={onModelCommit}
            placeholder={t("settings.advanced.llmProviders.modelPlaceholder")}
            className="flex-1"
          />
        )}
      </div>

      {!isLocalTokenizer && (
        <div className="flex items-center gap-2 flex-wrap">
          <span className={labelClass}>
            {t("settings.advanced.llmProviders.cost")}
          </span>
          <div className="flex items-center gap-2 flex-wrap">
            <label className="flex items-center gap-1 text-xs text-text/50">
              {t("settings.advanced.llmProviders.costInput")}
              <input
                type="number"
                min="0"
                step="0.01"
                value={costIn}
                onChange={(e) => setCostIn(e.target.value)}
                onBlur={() => commitCost("cost_input_per_million", costIn)}
                className={`${fieldClass} w-20`}
              />
            </label>
            <label className="flex items-center gap-1 text-xs text-text/50">
              {t("settings.advanced.llmProviders.costOutput")}
              <input
                type="number"
                min="0"
                step="0.01"
                value={costOut}
                onChange={(e) => setCostOut(e.target.value)}
                onBlur={() => commitCost("cost_output_per_million", costOut)}
                className={`${fieldClass} w-20`}
              />
            </label>
            {autoPriceKind && (
              <label
                className="flex items-center gap-1.5 text-xs text-text/50 cursor-pointer"
                title={t("settings.advanced.llmProviders.persistPriceHint")}
              >
                <input
                  type="checkbox"
                  checked={provider.persist_price ?? false}
                  onChange={(e) =>
                    onCommit({ ...provider, persist_price: e.target.checked })
                  }
                  className="w-3.5 h-3.5 accent-blue-600 cursor-pointer"
                />
                {t("settings.advanced.llmProviders.persistPrice")}
              </label>
            )}
          </div>
        </div>
      )}

      {!isLocalTokenizer && (
        <div className="flex items-center gap-2 flex-wrap">
          <span className={labelClass}>
            {t("settings.advanced.llmProviders.concurrency")}
          </span>
          <label className="flex items-center gap-1.5 text-xs text-text/70 cursor-pointer">
            <input
              type="checkbox"
              checked={provider.sequential ?? false}
              onChange={(e) =>
                onCommit({ ...provider, sequential: e.target.checked })
              }
              className="w-3.5 h-3.5 accent-blue-600 cursor-pointer"
            />
            {t("settings.advanced.llmProviders.sequential")}
          </label>
          <span className="text-xs text-text/40">
            {t("settings.advanced.llmProviders.family")}
          </span>
          <input
            type="text"
            value={group}
            onChange={(e) => setGroup(e.target.value)}
            onBlur={() => commitField("concurrency_group", group)}
            className={`${fieldClass} w-28`}
            placeholder={t("settings.advanced.llmProviders.familyPlaceholder")}
          />
        </div>
      )}
    </div>
  );
};

export const RegisteredLlmProviders: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();

  const providers =
    (getSetting("llm_providers") as LlmProvider[] | undefined) ?? [];

  const commitProvider = (updated: LlmProvider) => {
    const next = providers.map((p) => (p.id === updated.id ? updated : p));
    updateSetting("llm_providers", next);
  };

  // Apply an async price result as an isolated cost-only patch keyed by id,
  // reading the live providers array at resolution time so it never clobbers
  // intervening edits made during the (possibly slow) price look-up.
  const patchProviderCost = (id: string, input: number, output: number) => {
    const current =
      (getSetting("llm_providers") as LlmProvider[] | undefined) ?? [];
    const next = current.map((p) =>
      p.id === id
        ? {
            ...p,
            cost_input_per_million: input,
            cost_output_per_million: output,
          }
        : p,
    );
    updateSetting("llm_providers", next);
  };

  return (
    <div className="space-y-2">
      <p className="text-xs text-text/60 px-1">
        {t("settings.advanced.llmProviders.hint")}
      </p>
      {providers.map((provider, index) => (
        <ProviderSlot
          key={provider.id}
          provider={provider}
          index={index}
          onCommit={commitProvider}
          onPriceResolved={patchProviderCost}
        />
      ))}
    </div>
  );
};
