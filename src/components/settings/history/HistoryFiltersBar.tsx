import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../../ui/Dropdown";
import { TEXT_FIELD } from "../../ui/controlClasses";
import {
  DATE_RANGES,
  LENGTH_BUCKETS,
  SORT_ORDERS,
  type DateRange,
  type HistoryFilter,
  type LengthBucket,
  type SortOrder,
} from "./historyFilters";

const CHECKBOX_LABEL =
  "inline-flex items-center gap-1.5 h-8 px-2 text-[13px] text-text cursor-pointer rounded-md hover:bg-hover whitespace-nowrap";

/** Under History's search: favorites only, when, how long, the order, and
 *  whether each recording shows in full or as a few lines. */
export const HistoryFiltersBar: React.FC<{
  filter: HistoryFilter;
  onChange: (next: HistoryFilter) => void;
  /** The Favorites tab shows favorites anyway: no checkbox there. */
  showFavorites: boolean;
  sort: SortOrder;
  onSortChange: (next: SortOrder) => void;
  /** "Most hits" needs a search. */
  searching: boolean;
  shortPreviews: boolean;
  onShortPreviewsChange: (next: boolean) => void;
}> = ({
  filter,
  onChange,
  showFavorites,
  sort,
  onSortChange,
  searching,
  shortPreviews,
  onShortPreviewsChange,
}) => {
  const { t } = useTranslation();
  return (
    <div className="flex flex-wrap items-center gap-x-2 gap-y-1.5">
      {showFavorites && (
        <label className={CHECKBOX_LABEL}>
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
      <Dropdown
        options={SORT_ORDERS.map((value) => ({
          value,
          label: t(`settings.history.filters.sort.${value}`),
          disabled: value === "hits" && !searching,
        }))}
        selectedValue={sort === "hits" && !searching ? "newest" : sort}
        onSelect={(v) => onSortChange(v as SortOrder)}
        className="w-32 [&>button]:w-full [&>button]:min-w-0"
      />
      <label
        className={`${CHECKBOX_LABEL} ms-auto`}
        title={t("settings.history.filters.shortPreviewsHint")}
      >
        <input
          type="checkbox"
          checked={shortPreviews}
          onChange={(e) => onShortPreviewsChange(e.target.checked)}
          className="accent-[var(--color-accent)]"
        />
        {t("settings.history.filters.shortPreviews")}
      </label>
    </div>
  );
};
