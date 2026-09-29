import React, { useRef, useState } from "react";
import { ChevronsDown } from "lucide-react";
import { Tooltip } from "./Tooltip";
import { RichText } from "./RichText";
import { useHelpMode } from "../../stores/helpModeStore";

/** A small line icon before a setting's title, for the ones worth spotting at a glance. */
export type SettingIcon = React.ComponentType<{ className?: string }>;

/**
 * A description's lead: its bold opening phrase, or else its first sentence —
 * the one line a parent setting shows under its title.
 */
const leadOf = (description: string): string => {
  const bold = description.match(/^\*\*(.+?)\*\*/);
  if (bold) return bold[1];
  const plain = description.replace(/\*\*/g, "").split("\n")[0];
  const sentence = plain.match(/^.+?[.!?。](\s|$)/);
  return (sentence ? sentence[0] : plain).trim();
};

interface SettingContainerProps {
  title: string;
  description: string;
  children: React.ReactNode;
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  layout?: "horizontal" | "stacked";
  disabled?: boolean;
  icon?: SettingIcon;
  /** A setting that reveals others: bold title, with its description's lead under it. */
  lead?: boolean;
  /** Under the lead: that switching it on shows more settings. */
  revealHint?: string;
}

export const SettingContainer: React.FC<SettingContainerProps> = ({
  title,
  description,
  children,
  descriptionMode = "tooltip",
  grouped = false,
  layout = "horizontal",
  disabled = false,
  icon: Icon,
  lead = false,
  revealHint,
}) => {
  const heading = (
    <>
      {Icon && <Icon className="w-4 h-4 shrink-0" aria-hidden />}
      {title}
    </>
  );
  // Help mode (the ? at the top of the page): pointing anywhere on the row
  // outlines it and shows its description by its title.
  const helpOn = useHelpMode((st) => st.on);
  const [hovered, setHovered] = useState(false);
  const titleRef = useRef<HTMLHeadingElement>(null);
  const helping = helpOn && hovered && description.trim() !== "";
  // The pointed row is marked, so the others dim (App.css).
  const helpProps = {
    onMouseEnter: () => setHovered(true),
    onMouseLeave: () => setHovered(false),
    "data-help-row": "",
    "data-help-pointed": helping ? "" : undefined,
  };
  const helpOutline = helping
    ? " outline-2 -outline-offset-2 outline-accent rounded-md"
    : "";
  // It hangs from the setting's name (not the pointer), so it holds still.
  const helpTooltip = helping && (
    <Tooltip
      targetRef={titleRef}
      position="bottom"
      align="start"
      width={410}
      passThrough
    >
      <RichText
        text={description}
        className="px-1 py-0.5 text-[13px] leading-relaxed"
      />
    </Tooltip>
  );

  const containerClasses = grouped ? "px-4 py-3" : "px-4 py-3 card";

  if (layout === "stacked") {
    if (descriptionMode === "tooltip") {
      return (
        <div
          className={containerClasses + helpOutline}
          data-setting-title={title}
          {...helpProps}
        >
          <div className="flex items-center gap-2 mb-2">
            <h3
              ref={titleRef}
              className={`flex items-center gap-1.5 text-sm ${disabled ? "text-dis-text" : ""}`}
            >
              {heading}
            </h3>
            {helpTooltip}
          </div>
          <div className="w-full">{children}</div>
        </div>
      );
    }

    return (
      <div className={containerClasses} data-setting-title={title}>
        <div className="mb-2">
          <h3
            className={`flex items-center gap-1.5 text-sm ${disabled ? "text-dis-text" : ""}`}
          >
            {heading}
          </h3>
          <RichText
            text={description}
            className={`text-[13px] leading-[18px] text-text-secondary ${disabled ? "text-dis-text" : ""}`}
          />
        </div>
        <div className="w-full">{children}</div>
      </div>
    );
  }

  // Horizontal layout (default)
  const horizontalContainerClasses = grouped
    ? "flex items-center justify-between gap-4 px-4 py-2.5 min-h-[52px]"
    : "flex items-center justify-between gap-4 px-4 py-2.5 min-h-[52px] card";

  if (descriptionMode === "tooltip") {
    return (
      <div
        className={horizontalContainerClasses + helpOutline}
        data-setting-title={title}
        {...helpProps}
      >
        <div className="max-w-2/3">
          <div className="flex items-center gap-2">
            <h3
              ref={titleRef}
              className={`flex items-center gap-1.5 text-sm ${lead ? "font-semibold" : ""} ${disabled ? "text-dis-text" : ""}`}
            >
              {heading}
            </h3>
            {helpTooltip}
          </div>
          {lead && (
            <p className="mt-0.5 text-[13px] leading-[18px] text-text-secondary line-clamp-2">
              {leadOf(description)}
            </p>
          )}
          {revealHint && (
            <p className="mt-1 flex items-center gap-1 text-xs text-accent-text">
              <ChevronsDown className="w-3.5 h-3.5 shrink-0" aria-hidden />
              {revealHint}
            </p>
          )}
        </div>
        <div className="relative">{children}</div>
      </div>
    );
  }

  return (
    <div className={horizontalContainerClasses} data-setting-title={title}>
      <div className="max-w-2/3">
        <h3
          className={`flex items-center gap-1.5 text-sm ${disabled ? "text-dis-text" : ""}`}
        >
          {heading}
        </h3>
        <RichText
          text={description}
          className={`text-[13px] leading-[18px] text-text-secondary ${disabled ? "text-dis-text" : ""}`}
        />
      </div>
      <div className="relative">{children}</div>
    </div>
  );
};
