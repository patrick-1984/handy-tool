import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../../ui/Dropdown";
import { TEXT_FIELD } from "../../ui/controlClasses";
import {
  DATE_RANGES,
  LENGTH_BUCKETS,
  type DateRange,
  type HistoryFilter,
  type LengthBucket,
} from "./historyFilters";

/** Next to History's search: favorites only, when, and how long. */
export const HistoryFiltersBar: React.FC<{
  filter: HistoryFilter;
  onChange: (next: HistoryFilter) => void;
  /** The Favorites tab shows favorites anyway: no checkbox there. */
  showFavorites: boolean;
}> = ({ filter, onChange, showFavorites }) => {
  const { t } = useTranslation();
  return (
    <div className="flex flex-wrap items-center gap-2">
      {showFavorites && (
        <label className="inline-flex items-center gap-1.5 h-8 px-2 text-sm text-text cursor-pointer rounded-md hover:bg-hover">
          <input
            type="checkbox"
            checked={filter.favoritesOnly}
            onChange={(e) =>
              onChange({ ...filter, favoritesOnly: e.target.checked })
            }
            className="accent-[var(--color-accent)]"
          />
          {t("settings.history.filters.favoritesOnly")}
        </label>
      )}
      <Dropdown
        options={DATE_RANGES.map((value) => ({
          value,
          label: t(`settings.history.filters.date.${value}`),
        }))}
        selectedValue={filter.date}
        onSelect={(v) => onChange({ ...filter, date: v as DateRange })}
        className="w-36 [&>button]:w-full [&>button]:min-w-0"
      />
      {filter.date === "custom" && (
        <span className="inline-flex items-center gap-1 text-xs text-text-secondary">
          <input
            type="date"
            value={filter.customFrom}
            onChange={(e) =>
              onChange({ ...filter, customFrom: e.target.value })
            }
            className={`h-8 ${TEXT_FIELD}`}
            aria-label={t("settings.history.filters.from")}
          />
          –
          <input
            type="date"
            value={filter.customTo}
            onChange={(e) => onChange({ ...filter, customTo: e.target.value })}
            className={`h-8 ${TEXT_FIELD}`}
            aria-label={t("settings.history.filters.to")}
          />
        </span>
      )}
      <Dropdown
        options={LENGTH_BUCKETS.map((value) => ({
          value,
          label: t(`settings.history.filters.length.${value}`),
        }))}
        selectedValue={filter.length}
        onSelect={(v) => onChange({ ...filter, length: v as LengthBucket })}
        className="w-32 [&>button]:w-full [&>button]:min-w-0"
      />
    </div>
  );
};
