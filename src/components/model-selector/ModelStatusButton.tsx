import React from "react";
import { Cpu } from "lucide-react";

type ModelStatus =
  | "ready"
  | "loading"
  | "downloading"
  | "extracting"
  | "error"
  | "unloaded"
  | "none";

interface ModelStatusButtonProps {
  status: ModelStatus;
  displayText: string;
  isDropdownOpen: boolean;
  onClick: () => void;
  className?: string;
}

const ModelStatusButton: React.FC<ModelStatusButtonProps> = ({
  status,
  displayText,
  isDropdownOpen,
  onClick,
  className = "",
}) => {
  // The status itself is shown at the footer's other end (see ModelSelector);
  // a loading or failing model still colours the chip icon.
  const busy = ["loading", "downloading", "extracting"].includes(status);
  return (
    <button
      onClick={onClick}
      className={`flex items-center gap-2 h-7 px-2 -ms-2 rounded-md text-text hover:bg-hover transition-colors cursor-pointer ${className}`}
      title={displayText}
    >
      <Cpu
        className={`w-3.5 h-3.5 shrink-0 ${
          status === "error" || status === "none"
            ? "text-err-text"
            : busy
              ? "text-accent-text animate-pulse"
              : "text-text-secondary"
        }`}
      />
      <span className="max-w-48 truncate font-semibold">{displayText}</span>
      {/* The list opens upwards: the caret points up. */}
      <svg
        className={`w-3 h-3 text-text-secondary transition-transform ${isDropdownOpen ? "" : "rotate-180"}`}
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
  );
};

export default ModelStatusButton;
