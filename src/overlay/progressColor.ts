import type React from "react";

/** The progress light's colour when none is chosen (Progress Colour). */
export const DEFAULT_PROGRESS_COLOR = "#22d3ee";

/**
 * The pill's style variables for a chosen colour ("#rrggbb"): the progress
 * light and its lighter centre, which the glowing T uses too, and the sound
 * bars (quiet ones darker). Empty = the stylesheet's default cyan.
 */
export const progressColorVars = (color: string): React.CSSProperties =>
  /^#[0-9a-f]{6}$/i.test(color)
    ? ({
        "--progress-color": color,
        "--progress-core": `color-mix(in srgb, ${color} 45%, white)`,
        "--overlay-bar": color,
        "--overlay-bar-low": `color-mix(in srgb, ${color} 55%, #0a090b)`,
      } as React.CSSProperties)
    : {};

/** Hue 0-360, saturation and lightness 0-100. */
export interface Hsl {
  h: number;
  s: number;
  l: number;
}

export const hexToHsl = (hex: string): Hsl => {
  const n = parseInt(hex.slice(1), 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map(
    (v) => v / 255,
  );
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  const d = max - min;
  const s = d === 0 ? 0 : d / (1 - Math.abs(2 * l - 1));
  const h =
    d === 0
      ? 0
      : max === r
        ? 60 * (((g - b) / d + 6) % 6)
        : max === g
          ? 60 * ((b - r) / d + 2)
          : 60 * ((r - g) / d + 4);
  return { h: Math.round(h), s: Math.round(s * 100), l: Math.round(l * 100) };
};

export const hslToHex = ({ h, s, l }: Hsl): string => {
  const sat = s / 100;
  const lig = l / 100;
  const k = (n: number) => (n + h / 30) % 12;
  const a = sat * Math.min(lig, 1 - lig);
  const f = (n: number) =>
    lig - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)));
  return (
    "#" +
    [f(0), f(8), f(4)]
      .map((v) =>
        Math.round(v * 255)
          .toString(16)
          .padStart(2, "0"),
      )
      .join("")
  );
};
