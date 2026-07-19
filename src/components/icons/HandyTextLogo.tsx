import React from "react";
import { useTranslation } from "react-i18next";

/**
 * "Handy TOOL" brand wordmark, set in Rajdhani (see App.css @font-face).
 * Keeps the old SVG component's API (width/height/className); font size is
 * derived from the requested width so existing call sites scale unchanged.
 */
const HandyTextLogo = ({
  width,
  height,
  className,
}: {
  width?: number;
  height?: number;
  className?: string;
}) => {
  const { t } = useTranslation();
  const fontSize = width ? Math.round(width / 4.6) : 28;
  return (
    <span
      className={className}
      style={{
        fontFamily: '"Rajdhani", var(--font-display)',
        fontWeight: 700,
        fontSize,
        height,
        lineHeight: 1,
        letterSpacing: "0.02em",
        display: "inline-flex",
        alignItems: "baseline",
        gap: "0.26em",
        userSelect: "none",
      }}
    >
      <span className="text-text">{t("brand.name")}</span>
      <span style={{ color: "var(--color-logo-primary)" }}>
        {t("brand.suffix")}
      </span>
    </span>
  );
};

export default HandyTextLogo;
