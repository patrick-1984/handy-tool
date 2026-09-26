import React, { useEffect, useRef, useState } from "react";
import { Tooltip } from "./Tooltip";
import { RichText } from "./RichText";

/** A small line icon before a setting's title, for the ones worth spotting at a glance. */
export type SettingIcon = React.ComponentType<{ className?: string }>;

interface SettingContainerProps {
  title: string;
  description: string;
  children: React.ReactNode;
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  layout?: "horizontal" | "stacked";
  disabled?: boolean;
  tooltipPosition?: "top" | "bottom";
  icon?: SettingIcon;
}

export const SettingContainer: React.FC<SettingContainerProps> = ({
  title,
  description,
  children,
  descriptionMode = "tooltip",
  grouped = false,
  layout = "horizontal",
  disabled = false,
  tooltipPosition = "top",
  icon: Icon,
}) => {
  const [showTooltip, setShowTooltip] = useState(false);
  const heading = (
    <>
      {Icon && <Icon className="w-4 h-4 shrink-0" aria-hidden />}
      {title}
    </>
  );
  const tooltipRef = useRef<HTMLDivElement>(null);

  // Handle click outside to close tooltip
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (
        tooltipRef.current &&
        !tooltipRef.current.contains(event.target as Node)
      ) {
        setShowTooltip(false);
      }
    };

    if (showTooltip) {
      document.addEventListener("mousedown", handleClickOutside);
      return () =>
        document.removeEventListener("mousedown", handleClickOutside);
    }
  }, [showTooltip]);

  const toggleTooltip = () => {
    setShowTooltip(!showTooltip);
  };

  const containerClasses = grouped
    ? "px-4 p-2"
    : "px-4 p-2 rounded-lg border border-mid-gray/20";

  if (layout === "stacked") {
    if (descriptionMode === "tooltip") {
      return (
        <div className={containerClasses} data-setting-title={title}>
          <div className="flex items-center gap-2 mb-2">
            <h3
              className={`flex items-center gap-1.5 text-sm font-medium ${disabled ? "opacity-50" : ""}`}
            >
              {heading}
            </h3>
            <div
              ref={tooltipRef}
              className="relative"
              onMouseEnter={() => setShowTooltip(true)}
              onMouseLeave={() => setShowTooltip(false)}
              onClick={toggleTooltip}
            >
              <svg
                className="w-4 h-4 text-mid-gray cursor-help hover:text-logo-primary transition-colors duration-200 select-none"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
                aria-label="More information"
                role="button"
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    toggleTooltip();
                  }
                }}
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                />
              </svg>
              {showTooltip && (
                <Tooltip targetRef={tooltipRef} position="top">
                  <RichText
                    text={description}
                    className="text-sm leading-relaxed"
                  />
                </Tooltip>
              )}
            </div>
          </div>
          <div className="w-full">{children}</div>
        </div>
      );
    }

    return (
      <div className={containerClasses} data-setting-title={title}>
        <div className="mb-2">
          <h3
            className={`flex items-center gap-1.5 text-sm font-medium ${disabled ? "opacity-50" : ""}`}
          >
            {heading}
          </h3>
          <RichText
            text={description}
            className={`text-sm ${disabled ? "opacity-50" : ""}`}
          />
        </div>
        <div className="w-full">{children}</div>
      </div>
    );
  }

  // Horizontal layout (default)
  const horizontalContainerClasses = grouped
    ? "flex items-center justify-between px-4 p-2"
    : "flex items-center justify-between px-4 p-2 rounded-lg border border-mid-gray/20";

  if (descriptionMode === "tooltip") {
    return (
      <div className={horizontalContainerClasses} data-setting-title={title}>
        <div className="max-w-2/3">
          <div className="flex items-center gap-2">
            <h3
              className={`flex items-center gap-1.5 text-sm font-medium ${disabled ? "opacity-50" : ""}`}
            >
              {heading}
            </h3>
            <div
              ref={tooltipRef}
              className="relative"
              onMouseEnter={() => setShowTooltip(true)}
              onMouseLeave={() => setShowTooltip(false)}
              onClick={toggleTooltip}
            >
              <svg
                className="w-4 h-4 text-mid-gray cursor-help hover:text-logo-primary transition-colors duration-200 select-none"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
                aria-label="More information"
                role="button"
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    toggleTooltip();
                  }
                }}
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                />
              </svg>
              {showTooltip && (
                <Tooltip targetRef={tooltipRef} position={tooltipPosition}>
                  <RichText
                    text={description}
                    className="text-sm leading-relaxed"
                  />
                </Tooltip>
              )}
            </div>
          </div>
        </div>
        <div className="relative">{children}</div>
      </div>
    );
  }

  return (
    <div className={horizontalContainerClasses} data-setting-title={title}>
      <div className="max-w-2/3">
        <h3
          className={`flex items-center gap-1.5 text-sm font-medium ${disabled ? "opacity-50" : ""}`}
        >
          {heading}
        </h3>
        <RichText
          text={description}
          className={`text-sm ${disabled ? "opacity-50" : ""}`}
        />
      </div>
      <div className="relative">{children}</div>
    </div>
  );
};
