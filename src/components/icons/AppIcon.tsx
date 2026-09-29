import React from "react";

/**
 * The app icon (1A "wave + caret"): three cyan sound bars and a text caret on
 * a dark rounded square - the same drawing as the installed icon's master.
 */
const AppIcon: React.FC<{ className?: string }> = ({ className }) => (
  <svg viewBox="64 64 896 896" className={className} aria-hidden="true">
    <rect x="64" y="64" width="896" height="896" rx="208" fill="#12181C" />
    <g transform="translate(24 0)">
      <rect
        x="190"
        y="402"
        width="84"
        height="220"
        rx="42"
        fill="#22D3EE"
        opacity="0.55"
      />
      <rect
        x="316"
        y="312"
        width="84"
        height="400"
        rx="42"
        fill="#22D3EE"
        opacity="0.8"
      />
      <rect x="442" y="382" width="84" height="260" rx="42" fill="#22D3EE" />
      <rect x="636" y="232" width="76" height="560" rx="38" fill="#FFFFFF" />
      <rect x="564" y="232" width="220" height="76" rx="38" fill="#FFFFFF" />
      <rect x="564" y="716" width="220" height="76" rx="38" fill="#FFFFFF" />
    </g>
  </svg>
);

export default AppIcon;
