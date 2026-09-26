import React from "react";

export interface Tab<T extends string> {
  id: T;
  label: string;
  icon?: React.ComponentType<{
    width?: number | string;
    height?: number | string;
    className?: string;
  }>;
}

/**
 * A row of page tabs (More, History). Wraps onto a second line rather than
 * scrolling when the window is narrow.
 */
export const TabBar = <T extends string>({
  tabs,
  active,
  onSelect,
}: {
  tabs: Tab<T>[];
  active: T;
  onSelect: (id: T) => void;
}) => (
  <div className="flex flex-wrap gap-1">
    {tabs.map(({ id, label, icon: Icon }) => (
      <button
        key={id}
        type="button"
        onClick={() => onSelect(id)}
        className={`flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium whitespace-nowrap rounded-t-md border-b-2 transition-colors cursor-pointer ${
          active === id
            ? "border-logo-primary text-text"
            : "border-transparent text-text/50 hover:text-text/80"
        }`}
      >
        {Icon && <Icon width={14} height={14} className="shrink-0" />}
        {label}
      </button>
    ))}
  </div>
);

/** Classes that keep a page's tab bar in view while the page scrolls under it. */
export const STICKY_TABS =
  "sticky top-0 z-20 bg-background border-b border-mid-gray/20 pb-px";
