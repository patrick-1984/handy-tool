import React from "react";

interface TextIconProps {
  width?: number;
  height?: number;
  color?: string;
  className?: string;
}

const TextIcon: React.FC<TextIconProps> = ({
  width = 16,
  height = 16,
  color = "currentColor",
  className = "",
}) => {
  return (
    <svg
      width={width}
      height={height}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
    >
      <path
        d="M4 7V4h16v3M9 20h6M12 4v16"
        stroke={color}
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
};

export default TextIcon;
