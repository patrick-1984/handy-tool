import React from "react";
import type { LucideIcon } from "lucide-react";
import { RichText } from "./RichText";

interface SettingsGroupProps {
  title?: string;
  /** Line icon before the title, in the sidebar's style, for visual memory. */
  icon?: LucideIcon;
  description?: string;
  children: React.ReactNode;
}

/** A section title: as strong as the settings under it, with its icon. */
export const SectionTitle: React.FC<{ title: string; icon?: LucideIcon }> = ({
  title,
  icon: Icon,
}) => (
  <h2 className="flex items-center gap-2 text-sm font-semibold text-text uppercase tracking-wide">
    {Icon && <Icon className="w-4 h-4 shrink-0" aria-hidden />}
    {title}
  </h2>
);

export const SettingsGroup: React.FC<SettingsGroupProps> = ({
  title,
  icon,
  description,
  children,
}) => {
  return (
    <div className="space-y-2">
      {title && (
        <div className="px-4">
          <SectionTitle title={title} icon={icon} />
          {description && (
            <RichText
              text={description}
              className="text-xs text-mid-gray mt-1"
            />
          )}
        </div>
      )}
      <div className="bg-background border border-mid-gray/20 rounded-lg overflow-visible">
        <div className="divide-y divide-mid-gray/20">{children}</div>
      </div>
    </div>
  );
};

/**
 * Settings that a switch (or choice) above them reveals: indented under it
 * with a connecting line, so they read as part of that setting.
 */
export const SubSettings: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => (
  <div className="ms-6 border-s-2 border-logo-primary/50 divide-y divide-mid-gray/20">
    {children}
  </div>
);
