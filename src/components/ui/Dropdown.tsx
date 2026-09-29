import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

export interface DropdownOption {
  value: string;
  label: string;
  disabled?: boolean;
  /** A heading shown above the first option of each group. */
  group?: string;
  /** Quiet text at the end, e.g. a size. */
  hint?: string;
  /** A small icon at the very end, e.g. a download glyph. */
  trailing?: React.ReactNode;
  /** What the closed dropdown shows when this option is chosen. */
  selectedLabel?: string;
}

interface DropdownProps {
  options: DropdownOption[];
  className?: string;
  selectedValue: string | null;
  onSelect: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
  onRefresh?: () => void;
}

export const Dropdown: React.FC<DropdownProps> = ({
  options,
  selectedValue,
  onSelect,
  className = "",
  placeholder,
  disabled = false,
  onRefresh,
}) => {
  const { t } = useTranslation();
  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const resolvedPlaceholder = placeholder ?? t("common.selectOption");

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (
        dropdownRef.current &&
        !dropdownRef.current.contains(event.target as Node)
      ) {
        setIsOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  const selectedOption = options.find(
    (option) => option.value === selectedValue,
  );

  const handleSelect = (value: string) => {
    onSelect(value);
    setIsOpen(false);
  };

  const handleToggle = () => {
    if (disabled) return;
    if (!isOpen && onRefresh) onRefresh();
    setIsOpen(!isOpen);
  };

  return (
    <div
      className={`relative ${className}`}
      ref={dropdownRef}
      onKeyDown={(e) => {
        if (e.key === "Escape" && isOpen) {
          e.stopPropagation();
          setIsOpen(false);
        }
      }}
    >
      <button
        type="button"
        aria-haspopup="listbox"
        aria-expanded={isOpen}
        className={`h-8 ps-2.5 pe-2 text-sm text-text bg-control border border-control-border border-b-control-bottom rounded-md min-w-[190px] text-start flex items-center justify-between transition-colors duration-150 ${
          disabled
            ? "cursor-not-allowed bg-dis-bg text-dis-text border-transparent"
            : "hover:bg-control-hover cursor-pointer"
        } ${isOpen ? "shadow-[inset_0_-2px_0_var(--color-accent)]" : ""}`}
        onClick={handleToggle}
        disabled={disabled}
      >
        <span className="truncate">
          {selectedOption?.selectedLabel ||
            selectedOption?.label ||
            resolvedPlaceholder}
        </span>
        <svg
          className={`w-3.5 h-3.5 ms-2 shrink-0 text-text-secondary transition-transform duration-150 ${isOpen ? "rotate-180" : ""}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M19 9l-7 7-7-7"
          />
        </svg>
      </button>
      {isOpen && !disabled && (
        <div
          role="listbox"
          className="absolute top-full start-0 end-0 mt-1 p-1 bg-surface border border-border rounded-lg shadow-float z-50 max-h-60 overflow-y-auto"
        >
          {options.length === 0 ? (
            <div className="px-2.5 py-1.5 text-sm text-text-secondary">
              {t("common.noOptionsFound")}
            </div>
          ) : (
            options.map((option, i) => {
              const selected = selectedValue === option.value;
              const heading =
                option.group && option.group !== options[i - 1]?.group
                  ? option.group
                  : null;
              return (
                <React.Fragment key={option.value}>
                  {heading && (
                    <div
                      className="px-2.5 pt-2 pb-1 text-xs font-semibold uppercase tracking-[0.06em] text-text-secondary"
                      role="presentation"
                    >
                      {heading}
                    </div>
                  )}
                  <button
                    type="button"
                    role="option"
                    aria-selected={selected}
                    className={`relative w-full min-h-8 flex items-center gap-2 ps-3 pe-2 text-sm text-start rounded-md transition-colors duration-150 ${
                      option.disabled
                        ? "text-dis-text cursor-not-allowed"
                        : "hover:bg-hover cursor-pointer"
                    } ${selected ? "bg-active" : ""}`}
                    onClick={() => handleSelect(option.value)}
                    disabled={option.disabled}
                  >
                    {/* Selected: a cyan bar at the start and a tick at the end. */}
                    {selected && (
                      <span className="absolute start-0 top-2 bottom-2 w-[3px] rounded-full bg-accent" />
                    )}
                    <span className="truncate flex-1">{option.label}</span>
                    {option.hint && (
                      <span className="shrink-0 text-xs text-text-secondary tabular-nums">
                        {option.hint}
                      </span>
                    )}
                    {option.trailing}
                    {selected && !option.trailing && (
                      <svg
                        className="w-3.5 h-3.5 shrink-0 text-accent-text"
                        fill="none"
                        stroke="currentColor"
                        viewBox="0 0 24 24"
                      >
                        <path
                          strokeLinecap="round"
                          strokeLinejoin="round"
                          strokeWidth={2.5}
                          d="M5 13l4 4L19 7"
                        />
                      </svg>
                    )}
                  </button>
                </React.Fragment>
              );
            })
          )}
        </div>
      )}
    </div>
  );
};
