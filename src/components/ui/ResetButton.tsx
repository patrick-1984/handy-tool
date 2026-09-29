import React from "react";
import ResetIcon from "../icons/ResetIcon";

interface ResetButtonProps {
  onClick: () => void;
  disabled?: boolean;
  className?: string;
  ariaLabel?: string;
  children?: React.ReactNode;
}

/** A small icon-only button (reset, clear): no frame until hovered. */
export const ResetButton: React.FC<ResetButtonProps> = React.memo(
  ({ onClick, disabled = false, className = "", ariaLabel, children }) => (
    <button
      type="button"
      aria-label={ariaLabel}
      title={ariaLabel}
      className={`inline-flex items-center justify-center h-7 w-7 rounded-md transition-colors duration-150 ${
        disabled
          ? "cursor-not-allowed text-dis-text"
          : "cursor-pointer text-text-secondary hover:bg-hover hover:text-text active:bg-active"
      } ${className}`}
      onClick={onClick}
      disabled={disabled}
    >
      {children ?? <ResetIcon />}
    </button>
  ),
);
