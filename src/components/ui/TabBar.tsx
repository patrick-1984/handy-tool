import React from "react";

export interface Tab<T extends string> {
  id: T;
  label: string;
}

/**
 * A row of page tabs (More, History). The selected one has a cyan underline
 * and heavier text - the same selection language as the sidebar's bar. Wraps
 * onto a second line rather than scrolling when the window is narrow.
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
    {tabs.map(({ id, label }) => (
      <button
        key={id}
        type="button"
        onClick={() => onSelect(id)}
        className={`relative h-9 px-2.5 text-sm whitespace-nowrap rounded-md transition-colors duration-150 cursor-pointer ${
          active === id
            ? "font-semibold text-text after:content-[''] after:absolute after:start-2.5 after:end-2.5 after:bottom-0 after:h-[3px] after:rounded-full after:bg-accent"
            : "text-text-secondary hover:text-text hover:bg-hover"
        }`}
      >
        {label}
      </button>
    ))}
  </div>
);

/** Classes that keep a page's tab bar in view while the page scrolls under it. */
export const STICKY_TABS =
  "sticky top-0 z-20 bg-background border-b border-border";
