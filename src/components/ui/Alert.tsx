import React from "react";
import { AlertCircle, AlertTriangle, Info, CheckCircle } from "lucide-react";

type AlertVariant = "error" | "warning" | "info" | "success";

interface AlertProps {
  variant?: AlertVariant;
  /** When true, removes rounded corners and borders for use inside containers */
  contained?: boolean;
  children: React.ReactNode;
  className?: string;
}

/** The redesign's notices: a tinted card with a matching border and icon. */
const variantStyles: Record<
  AlertVariant,
  { container: string; icon: string; text: string }
> = {
  error: {
    container: "bg-err-bg border-err-border",
    icon: "text-err-text",
    text: "text-text",
  },
  warning: {
    container: "bg-warn-bg border-warn-border",
    icon: "text-warn-text",
    text: "text-text",
  },
  info: {
    container: "bg-info-bg border-info-border",
    icon: "text-accent-text",
    text: "text-text",
  },
  success: {
    container: "bg-surface border-border",
    icon: "text-ok-text",
    text: "text-text",
  },
};

const variantIcons: Record<AlertVariant, React.ElementType> = {
  error: AlertCircle,
  warning: AlertTriangle,
  info: Info,
  success: CheckCircle,
};

export const Alert: React.FC<AlertProps> = ({
  variant = "error",
  contained = false,
  children,
  className = "",
}) => {
  const styles = variantStyles[variant];
  const Icon = variantIcons[variant];

  return (
    <div
      className={`flex items-start gap-3 px-4 py-3 ${styles.container} ${contained ? "" : "rounded-lg border"} ${className}`}
    >
      <Icon className={`w-4 h-4 shrink-0 mt-0.5 ${styles.icon}`} />
      <div className={`text-[13px] leading-[18px] ${styles.text}`}>
        {children}
      </div>
    </div>
  );
};
