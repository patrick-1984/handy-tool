import React, { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Search, X } from "lucide-react";
import { useSettings } from "../hooks/useSettings";
import { useOsType } from "../hooks/useOsType";
import { useNavStore } from "../stores/navStore";
import type { SidebarSection } from "./Sidebar";
import type { TabId as AdvancedTabId } from "./settings/advanced/AdvancedSettings";
import {
  buildSearchIndex,
  highlightSetting,
  searchSettings,
  type SearchEntry,
} from "../lib/settingsSearch";

interface SidebarSearchProps {
  query: string;
  onQueryChange: (query: string) => void;
  /** Sections currently shown in the sidebar; results only point at these. */
  sections: { id: SidebarSection; labelKey: string }[];
}

/**
 * Search box at the top of the sidebar. While it holds a query the sidebar
 * shows the matching settings and shortcuts instead of the page list; picking
 * one opens its page (and Advanced tab) and outlines the control.
 */
export const SidebarSearch: React.FC<SidebarSearchProps> = ({
  query,
  onQueryChange,
  sections,
}) => {
  const { t, i18n } = useTranslation();
  const { settings } = useSettings();
  const osType = useOsType();
  const navigateTo = useNavStore((state) => state.navigateTo);
  const [active, setActive] = useState(0);

  const index = useMemo(
    () =>
      buildSearchIndex(
        t,
        sections,
        osType,
        settings?.bindings ?? {},
        settings?.post_process_enabled ?? false,
      ),
    // i18n.language: rebuild the translated titles when the language changes
    [t, i18n.language, sections, osType, settings],
  );
  const results = useMemo(() => searchSettings(index, query), [index, query]);

  const pick = (entry: SearchEntry) => {
    navigateTo(
      entry.section as SidebarSection,
      entry.advancedTab as AdvancedTabId | undefined,
    );
    onQueryChange("");
    highlightSetting(entry.title);
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((i) => Math.min(i + 1, results.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter" && results[active]) {
      pick(results[active]);
    } else if (e.key === "Escape") {
      onQueryChange("");
    }
  };

  return (
    <div className="w-full flex flex-col gap-1">
      <div className="relative flex items-center">
        <Search className="absolute start-2 h-3.5 w-3.5 text-text/40 pointer-events-none" />
        <input
          type="text"
          value={query}
          onChange={(e) => {
            onQueryChange(e.target.value);
            setActive(0);
          }}
          onKeyDown={onKeyDown}
          placeholder={t("sidebar.search.placeholder")}
          aria-label={t("sidebar.search.placeholder")}
          className="w-full ps-7 pe-6 py-1.5 text-sm rounded-md bg-mid-gray/10 border border-mid-gray/20 focus:border-logo-primary focus:outline-none"
        />
        {query && (
          <button
            type="button"
            onClick={() => onQueryChange("")}
            aria-label={t("common.cancel")}
            className="absolute end-1.5 text-text/40 hover:text-text"
          >
            <X className="h-3.5 w-3.5" />
          </button>
        )}
      </div>
      {query.trim() && (
        <div className="flex flex-col gap-0.5">
          {results.length === 0 ? (
            <p className="px-2 py-1 text-xs text-text/50">
              {t("sidebar.search.noResults")}
            </p>
          ) : (
            results.map((entry, i) => (
              <button
                key={`${entry.section}-${entry.advancedTab ?? ""}-${entry.title}-${i}`}
                type="button"
                onClick={() => pick(entry)}
                onMouseEnter={() => setActive(i)}
                className={`w-full text-start px-2 py-1 rounded-md ${
                  i === active ? "bg-logo-primary/30" : "hover:bg-mid-gray/20"
                }`}
              >
                <p className="text-sm truncate" title={entry.title}>
                  {entry.title}
                </p>
                <p
                  className="text-[11px] text-text/50 truncate"
                  title={entry.where}
                >
                  {entry.where}
                </p>
              </button>
            ))
          )}
        </div>
      )}
    </div>
  );
};
