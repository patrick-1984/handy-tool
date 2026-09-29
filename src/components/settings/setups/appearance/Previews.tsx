import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Volume1, X } from "lucide-react";
import type { LiveTextMode, ProgressStyle, Theme } from "@/bindings";
import { PauseGlyph, TGlyph } from "../../../../overlay/glyphs";
import { progressColorVars } from "../../../../overlay/progressColor";
import { getLanguageDirection } from "@/lib/utils/rtl";
import "../../../../overlay/RecordingOverlay.css";

/**
 * Moving previews for the Appearance setup: the real pill (its own styles), the
 * live text box, and where the pill sits. Everything runs on one clock; with
 * reduced motion it holds still at a telling moment.
 */

const reducedMotion = () =>
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;

/** Milliseconds since the preview appeared, ticking every `every` ms. */
const useClock = (every = 90): number => {
  const [now, setNow] = useState(() => (reducedMotion() ? 2600 : 0));
  useEffect(() => {
    if (reducedMotion()) return;
    const start = performance.now();
    const id = setInterval(() => setNow(performance.now() - start), every);
    return () => clearInterval(id);
  }, [every]);
  return now;
};

/** 16 sound levels that move like speech. */
const levelsAt = (ms: number) =>
  Array.from({ length: 16 }, (_, i) => {
    const wave = Math.abs(Math.sin(ms * 0.0042 + i * 0.9));
    const swell = 0.55 + 0.45 * Math.sin(ms * 0.0013 + i * 0.35);
    return Math.max(0, Math.min(1, 0.05 + 0.9 * wave * swell));
  });

/** Stands in for a desktop behind the pill, dark so the glow shows. */
export const DESKTOP =
  "bg-[linear-gradient(135deg,#4a5563_0%,#2a313b_100%)] text-white";

export interface PillLook {
  wideBars?: boolean;
  progressStyle?: ProgressStyle;
  glow?: number;
  lineGlow?: boolean;
  /** The T lit: the live text box is on. */
  liveText?: boolean;
  pause?: boolean;
  /** "Too quiet": in a box under the pill, in the pill, or not shown. */
  quiet?: "box" | "pill" | null;
  /** Progress Colour ("" = the default cyan). */
  color?: string;
}

/** The recording pill, recording or transcribing (progress running 0-100%). */
export const PillPreview: React.FC<
  PillLook & { mode: "recording" | "transcribing"; scale?: number }
> = ({
  mode,
  scale = 1,
  wideBars = false,
  progressStyle = "line",
  glow = 100,
  lineGlow = false,
  liveText = false,
  pause = false,
  quiet = null,
  color = "",
}) => {
  const { t, i18n } = useTranslation();
  const now = useClock();
  // As the overlay: with reduced motion the circling light becomes the line.
  const style =
    progressStyle === "lap" && reducedMotion() ? "line" : progressStyle;
  const levels = levelsAt(now);
  const progress = Math.min(100, Math.floor(((now % 7000) / 6000) * 100));
  // "Too quiet" shows first, then the bars come back for a moment.
  const quietNow = quiet !== null && (now % 5000 < 3200 || reducedMotion());
  return (
    <div className="flex flex-col items-center" style={{ zoom: scale }}>
      <div
        dir={getLanguageDirection(i18n.language)}
        className="pill-frame fade-in"
        style={
          {
            "--glow": glow / 100,
            ...progressColorVars(color),
          } as React.CSSProperties
        }
      >
        <div className="recording-overlay">
          <div className="overlay-float-btn">
            <span className={`float-button ${liveText ? "active" : ""}`}>
              <TGlyph />
            </span>
          </div>
          <div className="overlay-middle">
            {mode === "recording" && quietNow && quiet === "pill" ? (
              <div className="overlay-message too-quiet">
                {t("overlay.tooQuiet")}
              </div>
            ) : mode === "recording" ? (
              <div className={`bars-container ${wideBars ? "wide" : ""}`}>
                {levels.map((v, i) => (
                  <div
                    key={i}
                    className="bar"
                    style={{
                      height: `${Math.min(20, 4 + Math.pow(v, 0.7) * 16)}px`,
                      background:
                        v < 0.12 ? "var(--overlay-bar-low)" : undefined,
                    }}
                  />
                ))}
              </div>
            ) : (
              <div className="transcribing-text">
                <span className="text">
                  {t("overlay.transcribingProgress", { percent: progress })}
                </span>
              </div>
            )}
          </div>
          <div className="overlay-right">
            {mode === "recording" && pause && (
              <span className="cancel-button">
                <PauseGlyph />
              </span>
            )}
            <span className="cancel-button">
              <X size={12} strokeWidth={2.5} aria-hidden />
            </span>
          </div>
        </div>
        {mode === "transcribing" && style === "line" && (
          <div className={`progress-line ${lineGlow ? "glow" : ""}`}>
            <div className="progress-line-clip">
              <div
                className="overlay-progress"
                style={{ width: `${progress}%` }}
              />
            </div>
          </div>
        )}
        {mode === "transcribing" && style === "ring" && (
          <div className="progress-ring">
            <div
              className="progress-ring-light"
              style={{ "--ring-p": progress } as React.CSSProperties}
            />
          </div>
        )}
        {mode === "transcribing" && style === "lap" && (
          <div className="progress-lap" />
        )}
      </div>
      {/* The "Too quiet" box of its own: 6px under the pill. */}
      {mode === "recording" && quiet === "box" && (
        <div
          className="-mt-1 h-[22px] px-2.5 rounded-[11px] flex items-center gap-1.5 bg-[rgba(10,9,11,0.88)] border border-white/10 text-[#fbfbfb] text-xs font-semibold whitespace-nowrap transition-opacity duration-200"
          style={{ opacity: quietNow ? 1 : 0 }}
        >
          <Volume1 size={13} className="text-[#f5c563]" aria-hidden />
          {t("overlay.tooQuiet")}
        </div>
      )}
    </div>
  );
};

export interface BoxLook {
  mode: LiveTextMode;
  width?: number;
  fontSize?: number;
  lines?: number;
  fade?: boolean;
  /** Already full of text (no typing): shows the box at its full size. */
  full?: boolean;
}

/**
 * The live text box as you talk: your words appear one by one (the sample
 * sentence of this language), then it starts again. With Fade it fades out a
 * moment after the last word.
 */
export const LiveTextPreview: React.FC<BoxLook & { scale?: number }> = ({
  mode,
  width = 460,
  fontSize = 15,
  lines = 6,
  fade = false,
  full = false,
  scale = 0.5,
}) => {
  const { t } = useTranslation();
  const now = useClock(120);
  const sample = t("setup.appearance.sample");
  const all = full ? Array(12).fill(sample).join(" ") : sample;
  // Japanese and Chinese have no spaces: they appear a character at a time.
  const spaced = sample.includes(" ");
  const words = spaced ? all.split(" ") : Array.from(all);
  const cycle = words.length * 260 + 3600;
  const at = now % cycle;
  const shown =
    full || reducedMotion()
      ? words.length
      : Math.min(words.length, Math.floor(at / 260) + 1);
  const faded =
    fade && !full && !reducedMotion() && at > words.length * 260 + 1400;
  const text = words.slice(0, shown).join(spaced ? " " : "");
  const oneLine = mode === "last_words";
  return (
    // Left to right, as the real live text window.
    <div
      dir="ltr"
      style={{ zoom: scale, width: oneLine ? undefined : width }}
      className="flex justify-center"
    >
      <div
        className={`box-border overflow-hidden bg-[rgba(10,9,11,0.88)] border border-white/10 text-[#fbfbfb] leading-[1.55] transition-opacity ${
          faded ? "opacity-0 duration-[1200ms]" : "opacity-100 duration-200"
        } ${
          oneLine
            ? "flex items-center justify-end whitespace-nowrap rounded-[10px] px-3.5"
            : "flex flex-col justify-end rounded-xl px-4 py-3 w-full"
        }`}
        style={{
          fontFamily: "var(--overlay-font)",
          fontSize,
          maxWidth: width,
          height: oneLine ? (fontSize * 8) / 3 : undefined,
          maxHeight: oneLine
            ? undefined
            : Math.round(lines * fontSize * 1.55 + 26),
          maskImage:
            oneLine && text.length * fontSize * 0.5 > width - 28
              ? "linear-gradient(to right, transparent 0, black 48px)"
              : !oneLine && full
                ? "linear-gradient(to bottom, transparent 0, black 14px)"
                : undefined,
        }}
      >
        <span className={oneLine ? "" : "block"}>
          {text}
          <span
            className="inline-block w-[2px] ms-[3px] bg-[#22d3ee] align-[-3px]"
            style={{ height: fontSize * 1.1 }}
          />
        </span>
      </div>
    </div>
  );
};

/** The live text box above the pill, as on screen (smaller). */
export const BoxOverPill: React.FC<{
  box: BoxLook;
  pill: PillLook;
  scale?: number;
}> = ({ box, pill, scale = 0.5 }) => (
  <div className="flex flex-col items-center gap-1">
    <LiveTextPreview {...box} scale={scale} />
    <PillPreview mode="recording" {...pill} liveText scale={scale * 1.3} />
  </div>
);

/** A small screen with the pill where it would be (or nowhere). */
export const ScreenPreview: React.FC<{
  position: "top" | "bottom" | "none";
}> = ({ position }) => (
  <div
    className={`relative w-[220px] h-[124px] rounded-md overflow-hidden border border-black/20 ${DESKTOP}`}
  >
    <div className="absolute bottom-0 inset-x-0 h-3 bg-black/45" />
    {position !== "none" && (
      <div
        className={`absolute inset-x-0 flex justify-center ${
          position === "top" ? "top-1" : "bottom-3.5"
        }`}
      >
        <PillPreview mode="recording" scale={0.42} />
      </div>
    )}
  </div>
);

const PALETTES = {
  light: {
    bg: "#fbfbfb",
    side: "#f3f2f0",
    card: "#ffffff",
    line: "#e6e3df",
    text: "#5f5c58",
    accent: "#0891b2",
  },
  dark: {
    bg: "#2c2b29",
    side: "#262523",
    card: "#343230",
    line: "#45423e",
    text: "#aeaaa4",
    accent: "#22d3ee",
  },
};

const MiniWindow: React.FC<{ scheme: "light" | "dark" }> = ({ scheme }) => {
  const p = PALETTES[scheme];
  return (
    <div className="absolute inset-0 flex" style={{ background: p.bg }}>
      <div
        className="w-12 p-1.5 flex flex-col gap-1.5"
        style={{ background: p.side }}
      >
        {[0, 1, 2, 3].map((i) => (
          <div
            key={i}
            className="h-1.5 rounded-full"
            style={{ background: i === 0 ? p.accent : p.line }}
          />
        ))}
      </div>
      <div className="flex-1 p-2 flex flex-col gap-1.5">
        {[0, 1].map((i) => (
          <div
            key={i}
            className="rounded p-1.5 flex items-center justify-between"
            style={{ background: p.card, border: `1px solid ${p.line}` }}
          >
            <div
              className="h-1.5 w-14 rounded-full"
              style={{ background: p.text }}
            />
            <div
              className="h-2.5 w-5 rounded-full"
              style={{ background: i === 0 ? p.accent : p.line }}
            />
          </div>
        ))}
      </div>
    </div>
  );
};

/** A small app window in that theme; System is half light, half dark. */
export const ThemePreview: React.FC<{ theme: Theme }> = ({ theme }) => (
  <div className="relative w-[200px] h-[112px] rounded-md overflow-hidden border border-black/15">
    {theme === "system" ? (
      <>
        <MiniWindow scheme="light" />
        <div className="absolute inset-0 [clip-path:polygon(100%_0,100%_100%,0_100%)]">
          <MiniWindow scheme="dark" />
        </div>
      </>
    ) : (
      <MiniWindow scheme={theme} />
    )}
  </div>
);
