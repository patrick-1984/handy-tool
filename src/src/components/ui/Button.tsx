import React from "react";

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?:
    | "primary"
    | "primary-soft"
    | "secondary"
    | "danger"
    | "danger-ghost"
    | "ghost";
  size?: "sm" | "md" | "lg";
}

export const Button: React.FC<ButtonProps> = ({
  children,
  className = "",
  variant = "primary",
  size = "md",
  ...props
}) => {
  // Keyboard focus comes from the app-wide :focus-visible outline (App.css) —
  // never re-add `focus:outline-none` here without a replacement.
  const baseClasses =
    "inline-flex items-center justify-center gap-1.5 font-medium rounded-lg border transition-colors disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer";

  const variantClasses = {
    primary:
      "text-white bg-accent-strong border-accent-strong hover:bg-accent-strong/85 hover:border-accent-strong/85",
    "primary-soft":
      "text-text bg-logo-primary/20 border-transparent hover:bg-logo-primary/30",
    secondary:
      "bg-mid-gray/10 border-mid-gray/20 hover:bg-logo-primary/10 hover:border-logo-primary",
    danger: "text-white bg-danger border-danger hover:bg-danger/90",
    "danger-ghost":
      "text-danger border-transparent hover:bg-danger/10 hover:border-danger/20",
    ghost:
      "text-current border-transparent hover:bg-mid-gray/10 hover:border-mid-gray/20",
  };

  const sizeClasses = {
    sm: "min-h-[26px] px-2 py-0.5 text-xs",
    md: "min-h-8 px-4 py-1 text-sm",
    lg: "min-h-[38px] px-4 py-1.5 text-base",
  };

  return (
    <button
      className={`${baseClasses} ${variantClasses[variant]} ${sizeClasses[size]} ${className}`}
      {...props}
    >
      {children}
    </button>
  );
};
