import React from "react";
import { HelpModeButton } from "./HelpModeButton";

/**
 * A page's title with its sidebar icon: 26px, the display face. The help
 * mode button sits at the other end of the same line.
 */
export const PageTitle: React.FC<{
  icon: React.ComponentType<{
    width?: number | string;
    height?: number | string;
    className?: string;
  }>;
  label: string;
}> = ({ icon: Icon, label }) => (
  <div className="w-full flex items-center gap-3">
    <Icon width={22} height={22} className="shrink-0" />
    <h1 className="font-display text-[26px] leading-8 font-semibold">
      {label}
    </h1>
    <HelpModeButton />
  </div>
);
