import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Check, Download, Loader2, Search, X } from "lucide-react";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { Button } from "../../ui/Button";
import { TEXT_FIELD } from "../../ui/controlClasses";
import { useModelStore } from "../../../stores/modelStore";
import { commands, type HfModelSummary, type HfRepoCheck } from "@/bindings";
import { HF_SECTION_TITLE_KEY } from "./modelConfig";

const REPO_ID = /^[A-Za-z0-9._-]+\/[A-Za-z0-9._-]+$/;

/** Models → Parakeet from Hugging Face: newer or fine-tuned Parakeet weights
 *  (ONNX exports laid out like the one Handy ships), searched on Hugging
 *  Face or typed as owner/name, checked, then downloaded like any model. */
export const HuggingFaceParakeet: React.FC = () => {
  const { t } = useTranslation();
  const models = useModelStore((s) => s.models);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<HfModelSummary[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [checks, setChecks] = useState<Record<string, HfRepoCheck | "busy">>(
    {},
  );
  const [adding, setAdding] = useState<string | null>(null);

  const search = async () => {
    setSearching(true);
    const typed = query.trim();
    const result = await commands.searchHfParakeetModels(
      REPO_ID.test(typed) ? "" : typed,
    );
    setSearching(false);
    if (result.status === "error") {
      toast.error(
        t("settings.models.huggingFace.searchFailed", { error: result.error }),
      );
      return;
    }
    const list = result.data;
    // A repo typed as owner/name comes first, found or not.
    if (REPO_ID.test(typed) && !list.some((r) => r.repo === typed)) {
      list.unshift({ repo: typed, downloads: 0 });
    }
    setResults(list);
  };

  const check = async (repo: string) => {
    setChecks((c) => ({ ...c, [repo]: "busy" }));
    const result = await commands.checkHfParakeetModel(repo);
    if (result.status === "ok") {
      setChecks((c) => ({ ...c, [repo]: result.data }));
    } else {
      setChecks((c) => {
        const next = { ...c };
        delete next[repo];
        return next;
      });
      toast.error(
        t("settings.models.huggingFace.checkFailed", { error: result.error }),
      );
    }
  };

  const add = async (repo: string) => {
    setAdding(repo);
    const result = await commands.addHfParakeetModel(repo);
    if (result.status === "error") {
      setAdding(null);
      toast.error(result.error);
      return;
    }
    await useModelStore.getState().loadModels();
    setAdding(null);
    await useModelStore.getState().downloadModel(result.data.id);
  };

  const installedIds = new Set(models.map((m) => m.id));

  return (
    <SettingsGroup
      title={t(HF_SECTION_TITLE_KEY)}
      description={t("settings.models.huggingFace.description")}
    >
      <SettingContainer
        title={t(HF_SECTION_TITLE_KEY)}
        description={t("settings.models.huggingFace.searchDescription")}
        descriptionMode="tooltip"
        grouped={true}
      >
        <div className="flex items-center gap-2">
          <input
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void search();
            }}
            placeholder={t("settings.models.huggingFace.placeholder")}
            className={`w-64 ${TEXT_FIELD}`}
          />
          <Button
            variant="secondary"
            onClick={() => void search()}
            disabled={searching}
          >
            {searching ? (
              <Loader2 className="w-3.5 h-3.5 animate-spin" />
            ) : (
              <Search className="w-3.5 h-3.5" />
            )}
            {t("settings.models.huggingFace.search")}
          </Button>
        </div>
      </SettingContainer>
      {results && results.length === 0 && (
        <p className="px-1 text-xs text-text-secondary">
          {t("settings.models.huggingFace.noResults")}
        </p>
      )}
      {results && results.length > 0 && (
        <div className="card divide-y divide-border">
          {results.map(({ repo, downloads }) => {
            const state = checks[repo];
            const checked = state && state !== "busy" ? state : null;
            const installed = checked
              ? installedIds.has(checked.model_id)
              : false;
            return (
              <div
                key={repo}
                className="flex items-center justify-between gap-3 px-3 py-2"
              >
                <div className="min-w-0">
                  <a
                    href={`https://huggingface.co/${repo}`}
                    target="_blank"
                    rel="noreferrer"
                    className="text-sm font-medium text-text hover:underline break-all"
                  >
                    {repo}
                  </a>
                  <p className="text-xs text-text-secondary">
                    {downloads > 0 &&
                      t("settings.models.huggingFace.downloads", {
                        count: downloads,
                      })}
                    {checked?.compatible &&
                      ` · ${t("settings.models.huggingFace.size", { size: checked.total_mb })}`}
                    {checked && !checked.compatible && (
                      <span className="text-err-text">
                        {" "}
                        ·{" "}
                        {t("settings.models.huggingFace.incompatible", {
                          reason: checked.reason,
                        })}
                      </span>
                    )}
                  </p>
                </div>
                <div className="shrink-0">
                  {!state && (
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => void check(repo)}
                    >
                      {t("settings.models.huggingFace.check")}
                    </Button>
                  )}
                  {state === "busy" && (
                    <Loader2 className="w-4 h-4 animate-spin text-text-secondary" />
                  )}
                  {checked && !checked.compatible && (
                    <X className="w-4 h-4 text-err-text" />
                  )}
                  {checked?.compatible && installed && (
                    <span className="inline-flex items-center gap-1 text-xs text-text-secondary">
                      <Check className="w-3.5 h-3.5" />
                      {t("settings.models.huggingFace.added")}
                    </span>
                  )}
                  {checked?.compatible && !installed && (
                    <Button
                      variant="primary"
                      size="sm"
                      onClick={() => void add(repo)}
                      disabled={adding !== null}
                    >
                      {adding === repo ? (
                        <Loader2 className="w-3.5 h-3.5 animate-spin" />
                      ) : (
                        <Download className="w-3.5 h-3.5" />
                      )}
                      {t("settings.models.huggingFace.add")}
                    </Button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </SettingsGroup>
  );
};
