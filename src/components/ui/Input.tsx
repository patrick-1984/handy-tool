import React from "react";

interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  variant?: "default" | "compact";
}

/**
 * A text field of the redesign: 32 px like every control, with a darker
 * bottom edge that turns into a 2 px cyan line while focused.
 */
export const Input: React.FC<InputProps> = ({
  className = "",
  variant = "default",
  disabled,
  ...props
}) => {
  const baseClasses =
    "text-sm text-text bg-control border border-control-border border-b-control-bottom rounded-md text-start transition-colors duration-150 placeholder:text-text-secondary";

  const interactiveClasses = disabled
    ? "cursor-not-allowed bg-dis-bg text-dis-text border-transparent"
    : "hover:bg-control-hover focus:outline-none focus:bg-control focus:shadow-[inset_0_-2px_0_var(--color-accent)]";

  const variantClasses = {
    default: "h-8 px-2.5",
    compact: "h-7 px-2",
  } as const;

  return (
    <input
      className={`${baseClasses} ${variantClasses[variant]} ${interactiveClasses} ${className}`}
      disabled={disabled}
      {...props}
    />
  );
};
