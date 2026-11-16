import React from "react";
import { HelpModeButton } from "./HelpModeButton";
import { PinButton } from "../sidebar/PinButton";

/**
 * A page's title with its sidebar icon: 26px, the display face. The pin
 * (2.7.0, for `pin`'s page) and the help mode button sit at the other end of
 * the same line.
 */
export const PageTitle: React.FC<{
  icon: React.ComponentType<{
    width?: number | string;
    height?: number | string;
    className?: string;
  }>;
  label: string;
  /** The page (and History tab) the pin button pins. */
  pin?: { section: string; historyTab?: string | null };
}> = ({ icon: Icon, label, pin }) => (
  <div className="w-full flex items-center gap-3">
    <Icon width={22} height={22} className="shrink-0" />
    <h1 className="font-display text-[26px] leading-8 font-semibold">
      {label}
    </h1>
    <div className="ms-auto flex items-center gap-1">
      {pin && <PinButton section={pin.section} historyTab={pin.historyTab} />}
      <HelpModeButton />
    </div>
  </div>
);
