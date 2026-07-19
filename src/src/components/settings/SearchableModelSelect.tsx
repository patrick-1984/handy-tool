import React, { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { RefreshCw, ChevronDown } from "lucide-react";
import { commands } from "@/bindings";

interface Props {
  value: string;
  /** Registry provider id used to fetch the live model list (null = no fetch). */
  providerId: string | null;
  onCommit: (value: string) => void;
  placeholder?: string;
  className?: string;
  disabled?: boolean;
  /**
   * Optional custom model-list fetcher. When provided it REPLACES the default
   * providerId-based fetch (used e.g. for OpenRouter STT models, which aren't in
   * the normal /models list). Returns the model ids to offer.
   */
  fetchOverride?: () => Promise<string[]>;
}

const fieldClass =
  "rounded-md border border-zinc-700 bg-zinc-800 px-2 py-1 text-sm text-zinc-100 placeholder-zinc-500 focus:border-blue-500 focus:outline-none disabled:opacity-50";

const MAX_VISIBLE = 60;

/**
 * Searchable, free-text model selector. Fetches the provider's live model list
 * (e.g. OpenRouter's hundreds of models) and filters as you type; you can also
 * type any model id directly. Commits on selection or on blur.
 */
export const SearchableModelSelect: React.FC<Props> = ({
  value,
  providerId,
  onCommit,
  placeholder,
  className,
  disabled = false,
  fetchOverride,
}) => {
  const { t } = useTranslation();
  const [query, setQuery] = useState(value);
  const [open, setOpen] = useState(false);
  const [models, setModels] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(false);
  const [fetched, setFetched] = useState(false);
  // Whether the user has typed since focusing. While false (just focused, field
  // still holds the committed value) we show the FULL list rather than filtering
  // by the existing full model id — which otherwise matches nothing and forces
  // the user to clear the field before the picker is usable.
  const [touched, setTouched] = useState(false);
  const blurTimer = useRef<number | null>(null);

  useEffect(() => setQuery(value), [value]);

  // Invalidate the cached list when the fetch source changes (e.g. the user
  // switches provider), so the next focus re-fetches instead of showing a stale
  // list keyed to the previous provider/key.
  useEffect(() => {
    setFetched(false);
    setModels([]);
  }, [providerId]);

  const fetchModels = async () => {
    if (!providerId && !fetchOverride) return;
    setLoading(true);
    setError(false);
    try {
      if (fetchOverride) {
        setModels(await fetchOverride());
        setFetched(true);
      } else {
        const result = await commands.listProviderModels(providerId!);
        if (result.status === "ok") {
          setModels(result.data);
          setFetched(true);
        } else {
          setError(true);
        }
      }
    } catch {
      setError(true);
    } finally {
      setLoading(false);
    }
  };

  const filtered = useMemo(() => {
    // Only filter once the user has actually typed; on a fresh focus show all.
    const q = touched ? query.trim().toLowerCase() : "";
    const list = q ? models.filter((m) => m.toLowerCase().includes(q)) : models;
    return list.slice(0, MAX_VISIBLE);
  }, [models, query, touched]);

  const commit = (next: string) => {
    if (next !== value) onCommit(next);
  };

  const select = (m: string) => {
    setQuery(m);
    setTouched(false);
    setOpen(false);
    commit(m);
  };

  const handleFocus = (e: React.FocusEvent<HTMLInputElement>) => {
    if (blurTimer.current) window.clearTimeout(blurTimer.current);
    setTouched(false);
    setOpen(true);
    // Highlight the existing value so the first keystroke replaces it.
    e.target.select();
    // Lazily fetch the list the first time the field is opened.
    if (!fetched && !loading && (providerId || fetchOverride))
      void fetchModels();
  };

  const handleBlur = () => {
    // Delay so an option's mousedown can register before we close + commit.
    blurTimer.current = window.setTimeout(() => {
      setOpen(false);
      setTouched(false);
      commit(query.trim());
    }, 150);
  };

  return (
    <div className={`relative ${className ?? ""}`}>
      <div className="flex items-center gap-1">
        <div className="relative flex-1">
          <input
            type="text"
            value={query}
            disabled={disabled}
            onChange={(e) => {
              setQuery(e.target.value);
              setTouched(true);
              setOpen(true);
            }}
            onFocus={handleFocus}
            onBlur={handleBlur}
            placeholder={placeholder}
            className={`${fieldClass} w-full pr-6`}
          />
          <ChevronDown className="pointer-events-none absolute right-1.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-text/40" />
        </div>
        <button
          type="button"
          onClick={fetchModels}
          disabled={loading || disabled || (!providerId && !fetchOverride)}
          className="p-1.5 rounded-md border border-zinc-700 text-text/60 hover:text-text hover:border-blue-500 disabled:opacity-50 transition-colors cursor-pointer shrink-0"
          title={t("settings.modelPicker.refresh")}
        >
          <RefreshCw className={`w-4 h-4 ${loading ? "animate-spin" : ""}`} />
        </button>
      </div>

      {open && (loading || error || models.length > 0) && (
        <div className="absolute z-20 mt-1 w-full max-h-64 overflow-y-auto rounded-md border border-zinc-700 bg-zinc-900 shadow-lg">
          {loading && (
            <div className="px-2 py-1.5 text-xs text-text/50">
              {t("settings.modelPicker.loading")}
            </div>
          )}
          {error && !loading && (
            <div className="px-2 py-1.5 text-xs text-red-400">
              {t("settings.modelPicker.error")}
            </div>
          )}
          {!loading &&
            !error &&
            filtered.map((m) => (
              <button
                key={m}
                type="button"
                // mousedown (not click) so it fires before the input blur closes the list
                onMouseDown={(e) => {
                  e.preventDefault();
                  select(m);
                }}
                className={`block w-full text-left px-2 py-1.5 text-sm hover:bg-mid-gray/20 ${
                  m === value ? "text-logo-primary" : "text-zinc-100"
                }`}
              >
                {m}
              </button>
            ))}
          {!loading && !error && fetched && filtered.length === 0 && (
            <div className="px-2 py-1.5 text-xs text-text/50">
              {t("settings.modelPicker.noMatches")}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
