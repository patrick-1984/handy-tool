/**
 * The redesign's control styles as class strings, for pages that style plain
 * elements instead of using Button / Input: 32px, 6px corners, and a darker
 * bottom edge (a 2px cyan line while a field is focused).
 */
const DISABLED =
  "disabled:bg-dis-bg disabled:text-dis-text disabled:border-transparent disabled:cursor-not-allowed";

export const PRIMARY_BUTTON = `inline-flex items-center justify-center gap-1.5 h-8 px-3 rounded-md border border-btn bg-btn text-on-btn text-sm font-medium hover:bg-btn-hover hover:border-btn-hover active:bg-btn-press transition-colors cursor-pointer ${DISABLED}`;

export const SECONDARY_BUTTON = `inline-flex items-center justify-center gap-1.5 h-8 px-3 rounded-md border border-control-border border-b-control-bottom bg-control text-text text-sm hover:bg-control-hover transition-colors cursor-pointer ${DISABLED}`;

const FIELD =
  "rounded-md border border-control-border border-b-control-bottom bg-control text-sm text-text placeholder:text-text-secondary hover:bg-control-hover focus:outline-none focus:bg-control focus:shadow-[inset_0_-2px_0_var(--color-accent)]";

export const TEXT_FIELD = `h-8 px-2.5 ${FIELD} ${DISABLED}`;

export const TEXT_AREA = `px-3 py-2 ${FIELD} ${DISABLED}`;

/** Small icon buttons without a frame (History's Copy, Star, Delete; notes). */
export const ICON_BUTTON =
  "inline-flex items-center justify-center h-7 w-7 rounded-md text-text-secondary hover:bg-hover hover:text-text transition-colors cursor-pointer disabled:text-dis-text disabled:cursor-not-allowed disabled:hover:bg-transparent";
