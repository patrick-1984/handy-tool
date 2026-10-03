import React from "react";
import type { LucideIcon } from "lucide-react";
import { RichText } from "./RichText";

interface SettingsGroupProps {
  title?: string;
  /** Line icon before the title, in the sidebar's style, for visual memory. */
  icon?: LucideIcon;
  description?: string;
  /** A small tag after the title ("Windows only"). */
  tag?: string;
  children: React.ReactNode;
}

/** A section header: small caps in the secondary colour, with its icon. */
export const SectionTitle: React.FC<{
  title: string;
  icon?: LucideIcon;
  tag?: string;
}> = ({ title, icon: Icon, tag }) => (
  <h2 className="flex items-center gap-2 text-xs leading-4 font-semibold text-text-secondary uppercase tracking-[0.06em]">
    {Icon && <Icon className="w-3.5 h-3.5 shrink-0" aria-hidden />}
    {title}
    {tag && (
      <span className="inline-flex items-center h-5 px-1.5 rounded bg-surface2 border border-border text-xs font-normal normal-case tracking-normal">
        {tag}
      </span>
    )}
  </h2>
);

/**
 * Rows in a group read as cards: every row paints its own part of the card
 * (surface, side borders, a divider above), and a run of rows gets rounded
 * corners where it starts and ends. Anything marked `card-break` - a rail of
 * revealed settings, a notice - sits between cards: the card above it closes
 * and the rows after it start a new one. (The rows are separate components,
 * so the split cannot be made when rendering.)
 */
const ROWS_AS_CARDS = [
  "flex flex-col",
  "[&>:not(.card-break)]:bg-surface [&>:not(.card-break)]:border-x [&>:not(.card-break)]:border-t [&>:not(.card-break)]:border-border",
  "[&>:not(.card-break):first-child]:rounded-t-lg [&>.card-break+:not(.card-break)]:rounded-t-lg",
  "[&>:not(.card-break):last-child]:rounded-b-lg [&>:not(.card-break):last-child]:border-b [&>:not(.card-break):last-child]:shadow-card",
  "[&>:not(.card-break):has(+.card-break)]:rounded-b-lg [&>:not(.card-break):has(+.card-break)]:border-b [&>:not(.card-break):has(+.card-break)]:shadow-card",
  "[&>.card-break+:not(.card-break)]:mt-4",
].join(" ");

export const SettingsGroup: React.FC<SettingsGroupProps> = ({
  title,
  icon,
  description,
  tag,
  children,
}) => {
  return (
    <div className="space-y-2">
      {title && (
        <div className="px-0.5">
          <SectionTitle title={title} icon={icon} tag={tag} />
          {description && (
            <RichText
              text={description}
              className="text-[13px] leading-[18px] text-text-secondary mt-1"
            />
          )}
        </div>
      )}
      <div className={ROWS_AS_CARDS}>{children}</div>
    </div>
  );
};

/**
 * Settings that a switch (or choice) above them reveals — the designer's
 * "connected rail": the parent's card closes, and a 2px cyan rail starts right
 * at its bottom edge, 22px in, with the revealed settings in a card of their
 * own 16px to the right of it.
 */
export const SubSettings: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => (
  <div className="card-break rail-block reveal ms-[22px] border-s-2 border-accent ps-4 pt-3">
    <div className="card divide-y divide-border [&>:first-child]:rounded-t-lg [&>:last-child]:rounded-b-lg">
      {children}
    </div>
  </div>
);

/** The same rail around whole sections (Post-processing's Hotkey, API, Prompt). */
export const SubSections: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => (
  <div className="card-break reveal ms-[22px] border-s-2 border-accent ps-4 pt-4 flex flex-col gap-6">
    {children}
  </div>
);
