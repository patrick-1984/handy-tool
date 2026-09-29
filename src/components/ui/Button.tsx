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

/**
 * Buttons of the redesign: 32 px (md), 6 px corners. Primary is the cyan
 * button, secondary looks like the other controls (a darker bottom edge),
 * danger is filled (confirm dialogs) and danger-ghost is its outline form for
 * rows. Disabled ones are greyed, not faded.
 */
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
    "inline-flex items-center justify-center gap-1.5 font-medium rounded-md border transition-colors duration-150 cursor-pointer disabled:cursor-not-allowed disabled:bg-dis-bg disabled:text-dis-text disabled:border-transparent";

  const variantClasses = {
    primary:
      "text-on-btn bg-btn border-btn hover:bg-btn-hover hover:border-btn-hover active:bg-btn-press active:border-btn-press",
    "primary-soft":
      "text-accent-text bg-accent-soft border-transparent hover:bg-accent-soft/70",
    secondary:
      "text-text bg-control border-control-border border-b-control-bottom hover:bg-control-hover active:bg-surface2",
    danger: "text-on-err bg-err-solid border-err-solid hover:bg-err-solid/90",
    "danger-ghost":
      "text-err-text bg-transparent border-err-border hover:bg-err-bg",
    ghost:
      "text-current bg-transparent border-transparent hover:bg-hover active:bg-active",
  };

  const sizeClasses = {
    sm: "h-7 px-2.5 text-[13px]",
    md: "h-8 px-3 text-sm",
    lg: "h-9 px-4 text-sm",
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
