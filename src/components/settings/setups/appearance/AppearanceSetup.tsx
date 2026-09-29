import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Mic, X } from "lucide-react";
import type {
  LiveTextMode,
  OverlayPosition,
  ProgressStyle,
  Theme,
} from "@/bindings";
import { SetupFrame } from "../SetupFrame";
import { useSettings } from "../../../../hooks/useSettings";
import { useModelStore } from "../../../../stores/modelStore";
import { isRemote } from "../../TakeControls";
import { PauseGlyph, TGlyph } from "../../../../overlay/glyphs";
import { DEFAULT_PROGRESS_COLOR } from "../../../../overlay/progressColor";
import {
  BoxOverPill,
  DESKTOP,
  LiveTextPreview,
  PillPreview,
  ScreenPreview,
  ThemePreview,
  type PillLook,
} from "./Previews";

type Step =
  | "intro"
  | "theme"
  | "position"
  | "size"
  | "bars"
  | "progress"
  | "glow"
  | "color"
  | "liveText"
  | "width"
  | "textSize"
  | "quiet"
  | "done";
const ORDER: Step[] = [
  "intro",
  "theme",
  "position",
  "size",
  "bars",
  "progress",
  "glow",
  "color",
  "liveText",
  "width",
  "textSize",
  "quiet",
  "done",
];

const WIDTHS = [
  { px: 360, key: "narrow" },
  { px: 460, key: "medium" },
  { px: 640, key: "wide" },
  { px: 860, key: "extraWide" },
];
const SIZES = [
  { px: 13, key: "small" },
  { px: 15, key: "normal" },
  { px: 18, key: "large" },
  { px: 22, key: "extraLarge" },
];
/** Ready colours for the pill ("default" = cyan); any other in General › App. */
const COLORS = [
  { value: "default", key: "cyan" },
  { value: "#3b82f6", key: "blue" },
  { value: "#a855f7", key: "violet" },
  { value: "#ec4899", key: "pink" },
  { value: "#f59e0b", key: "amber" },
  { value: "#22c55e", key: "green" },
];

const GLOWS_RING = [
  { value: "50", key: "soft" },
  { value: "100", key: "normal" },
  { value: "150", key: "strong" },
  { value: "200", key: "brightest" },
];
const GLOWS_LINE = [
  { value: "0", key: "none" },
  { value: "100", key: "normal" },
  { value: "150", key: "strong" },
  { value: "200", key: "brightest" },
];

/** The option closest to a stored number (a slider may sit between them). */
const nearest = (values: number[], v: number) =>
  String(values.reduce((a, b) => (Math.abs(b - v) < Math.abs(a - v) ? b : a)));

interface Option {
  value: string;
  title: string;
  detail?: string;
  preview: React.ReactNode;
  /** A stage for the pill (a dark desktop) or a plain one. */
  plain?: boolean;
  disabled?: boolean;
}

/**
 * One choice with its own moving preview; all of a step's choices are shown
 * side by side, so they can be compared at a glance. The cards share their
 * grid's rows (subgrid), so previews and titles line up along a row. Picked = the radio's dot,
 * an accent edge and a soft tint, as the other choice cards.
 */
const OptionCard: React.FC<{
  option: Option;
  selected: boolean;
  onPick: () => void;
}> = ({ option, selected, onPick }) => (
  <button
    type="button"
    role="radio"
    aria-checked={selected}
    disabled={option.disabled}
    onClick={onPick}
    className={`grid grid-rows-subgrid row-span-2 gap-0 rounded-lg border text-start overflow-hidden transition-colors cursor-pointer disabled:cursor-not-allowed disabled:opacity-60 ${
      selected
        ? "border-accent shadow-[inset_0_0_0_1px_var(--color-accent)] bg-accent-soft"
        : "border-border bg-surface hover:border-control-bottom hover:bg-control-hover"
    }`}
  >
    <div
      className={`flex items-center justify-center min-h-[124px] p-3 m-1 mb-0 rounded-md overflow-hidden ${
        option.plain ? "bg-surface2" : DESKTOP
      }`}
      aria-hidden
    >
      {option.preview}
    </div>
    <div className="flex items-start gap-2.5 px-3 py-2.5">
      <span
        className={`flex-none mt-0.5 flex items-center justify-center w-4 h-4 rounded-full border-[1.5px] ${selected ? "border-accent" : "border-control-bottom"}`}
        aria-hidden
      >
        {selected && <span className="w-1.5 h-1.5 rounded-full bg-accent" />}
      </span>
      <span className="flex flex-col gap-0.5 min-w-0">
        <span className="text-sm font-semibold">{option.title}</span>
        {option.detail && (
          <span className="text-[13px] leading-[1.45] text-text-secondary">
            {option.detail}
          </span>
        )}
      </span>
    </div>
  </button>
);

/** A part of the pill and what it is for (the first step). */
const Part: React.FC<{ icon: React.ReactNode; text: string }> = ({
  icon,
  text,
}) => (
  <li className="flex items-start gap-3">
    <span className="flex-none flex items-center justify-center w-7 h-7 rounded-full bg-[rgba(10,9,11,0.88)] border border-white/10 text-[#fbfbfb]">
      {icon}
    </span>
    <span className="text-sm leading-[1.5] pt-1">{text}</span>
  </li>
);

/**
 * The Appearance setup: how the recording pill and the live text box look,
 * one thing per step. Every choice is shown moving (the real pill, the box with
 * words appearing), so nothing has to be tried by recording. A step is saved
 * with Save & next; Skip and Close setup keep what was there.
 */
export const AppearanceSetup: React.FC<{ onClose: () => void }> = ({
  onClose,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const [step, setStep] = useState<Step>("intro");
  const [choice, setChoice] = useState("");

  const models = useModelStore((s) => s.models);
  const model = models.find((m) => m.id === getSetting("selected_model"));
  const remote = isRemote(model);

  const style = (getSetting("progress_style") ?? "line") as ProgressStyle;
  const glow = getSetting("progress_glow") ?? 100;
  const lineGlow = getSetting("progress_line_glow") ?? false;
  const liveOn = getSetting("live_text_box_enabled") ?? false;
  const liveMode = (getSetting("live_text_mode") ??
    "last_words") as LiveTextMode;
  const boxWidth = getSetting("live_text_width") ?? 460;
  const fontSize = getSetting("live_text_font_size") ?? 15;
  const lines = getSetting("live_text_lines") ?? 6;
  const fade = getSetting("live_text_fade") ?? false;
  const quietOn = getSetting("too_quiet_hint") ?? true;
  const quietBox = getSetting("too_quiet_hint_box") ?? true;
  // The pill as it is now; each step changes one thing in its previews.
  const pill: PillLook = {
    wideBars: getSetting("sound_bars_wide") ?? false,
    progressStyle: style,
    glow,
    lineGlow,
    liveText: liveOn,
    pause: getSetting("pause_button_enabled") ?? false,
    color: getSetting("progress_color") ?? "",
  };
  const box = {
    mode: liveMode,
    width: boxWidth,
    fontSize,
    lines,
    fade,
  };

  // Steps that apply: with no pill (position None) neither the pill's nor the
  // box's steps (neither is shown then); no glow for the circling light; the
  // box's own steps (width, text size) only while it is on and can work (not
  // with an online model). Lines and Fade are left to the settings.
  const PILL_STEPS: Step[] = ORDER.slice(
    ORDER.indexOf("size"),
    ORDER.indexOf("done"),
  );
  const now = {
    pos: (getSetting("overlay_position") ?? "bottom") as OverlayPosition,
    st: style,
    on: liveOn && !remote,
  };
  const applies = (s: Step, c = now) =>
    c.pos === "none" && PILL_STEPS.includes(s)
      ? false
      : s === "glow"
        ? c.st !== "lap"
        : s === "width" || s === "textSize"
          ? c.on
          : true;
  const steps = ORDER.filter((s) => applies(s));
  const index = steps.indexOf(step);

  const current = (s: Step): string => {
    switch (s) {
      case "theme":
        return getSetting("app_theme") ?? "system";
      case "position":
        return getSetting("overlay_position") ?? "bottom";
      case "size":
        return String(getSetting("pill_scale") ?? 100);
      case "bars":
        return pill.wideBars ? "wide" : "narrow";
      case "progress":
        return style;
      case "glow":
        return style === "line" && !lineGlow
          ? "0"
          : nearest(
              style === "line" ? [100, 150, 200] : [50, 100, 150, 200],
              glow,
            );
      case "color": {
        const c = getSetting("progress_color") ?? "";
        return !c
          ? "default"
          : COLORS.some((o) => o.value === c)
            ? c
            : "custom";
      }
      case "liveText":
        // An online model cannot use it: open on Off.
        return liveOn && !remote ? liveMode : "off";
      case "width":
        return nearest(
          WIDTHS.map((w) => w.px),
          boxWidth,
        );
      case "textSize":
        return nearest(
          SIZES.map((w) => w.px),
          fontSize,
        );
      case "quiet":
        return !quietOn ? "off" : quietBox ? "box" : "pill";
      default:
        return "";
    }
  };
  // Each step opens on what is set now.
  useEffect(() => {
    setChoice(current(step));
  }, [step]);

  const save = async (s: Step, v: string) => {
    switch (s) {
      case "theme":
        return updateSetting("app_theme", v as Theme);
      case "position":
        return updateSetting("overlay_position", v as OverlayPosition);
      case "size":
        return updateSetting("pill_scale", Number(v));
      case "bars":
        return updateSetting("sound_bars_wide", v === "wide");
      case "progress":
        return updateSetting("progress_style", v as ProgressStyle);
      case "glow":
        if (style === "line") {
          await updateSetting("progress_line_glow", v !== "0");
          if (v === "0") return;
        }
        return updateSetting("progress_glow", Number(v));
      case "color":
        // "custom" keeps the colour set with the sliders.
        if (v === "custom") return;
        return updateSetting("progress_color", v === "default" ? "" : v);
      case "liveText":
        if (v === "off") return updateSetting("live_text_box_enabled", false);
        await updateSetting("live_text_mode", v as LiveTextMode);
        return updateSetting("live_text_box_enabled", true);
      case "width":
        return updateSetting("live_text_width", Number(v));
      case "textSize":
        return updateSetting("live_text_font_size", Number(v));
      case "quiet":
        if (v === "off") return updateSetting("too_quiet_hint", false);
        await updateSetting("too_quiet_hint", true);
        return updateSetting("too_quiet_hint_box", v === "box");
    }
  };

  // A choice can add or drop later steps (position None, the circling light,
  // the box on or off), so the next one follows it.
  const goNext = (from: Step, picked?: string) => {
    const c = { ...now };
    if (picked && from === "position") c.pos = picked as OverlayPosition;
    if (picked && from === "progress") c.st = picked as ProgressStyle;
    if (picked && from === "liveText") c.on = picked !== "off";
    const later = ORDER.slice(ORDER.indexOf(from) + 1);
    setStep(later.find((s) => applies(s, c)) ?? "done");
  };
  // While a step saves, Back, Skip and a second click wait.
  const [saving, setSaving] = useState(false);
  const saveNext = async () => {
    if (saving) return;
    setSaving(true);
    try {
      await save(step, choice);
    } finally {
      setSaving(false);
    }
    goNext(step, choice);
  };

  const k = (key: string, opts?: Record<string, unknown>) =>
    t(`setup.appearance.${step}.${key}`, opts);

  const options = ((): Option[] => {
    switch (step) {
      case "theme":
        return (["system", "light", "dark"] as Theme[]).map((v) => ({
          value: v,
          title: t(`settings.advanced.appearance.options.${v}`),
          detail: k(v),
          preview: <ThemePreview theme={v} />,
          plain: true,
        }));
      case "position":
        return (["bottom", "top", "none"] as const).map((v) => ({
          value: v,
          title: t(`settings.advanced.overlay.options.${v}`),
          detail: k(v),
          preview: <ScreenPreview position={v} />,
          plain: true,
        }));
      case "size":
        return [
          { scale: 100, key: "normal" },
          { scale: 125, key: "large" },
          { scale: 150, key: "extraLarge" },
        ].map(({ scale, key }) => ({
          value: String(scale),
          title: t(`settings.advanced.overlaySize.${key}`),
          detail: `${scale}%`,
          preview: (
            <PillPreview
              mode="recording"
              {...pill}
              scale={(0.7 * scale) / 100}
            />
          ),
        }));
      case "bars":
        return (["narrow", "wide"] as const).map((v) => ({
          value: v,
          title: k(`${v}.title`),
          detail: k(`${v}.detail`),
          preview: (
            <PillPreview mode="recording" {...pill} wideBars={v === "wide"} />
          ),
        }));
      case "progress":
        return (["line", "ring", "lap"] as ProgressStyle[]).map((v) => ({
          value: v,
          title: t(`settings.advanced.progressStyle.${v}`),
          detail: k(v),
          preview: (
            <PillPreview mode="transcribing" {...pill} progressStyle={v} />
          ),
        }));
      case "glow":
        return (style === "line" ? GLOWS_LINE : GLOWS_RING).map(
          ({ value, key }) => ({
            value,
            title: k(`${key}.title`),
            detail: value === "0" ? k(`${key}.detail`) : `${value}%`,
            preview: (
              <PillPreview
                mode="transcribing"
                {...pill}
                glow={Number(value) || 100}
                lineGlow={value !== "0"}
              />
            ),
          }),
        );
      case "color": {
        const own = getSetting("progress_color") ?? "";
        const swatch = (color: string) => (
          // The recording pill (T and bars) over the transcribing one.
          <div className="flex flex-col items-center">
            <PillPreview
              mode="recording"
              {...pill}
              liveText
              color={color}
              scale={0.78}
            />
            <PillPreview
              mode="transcribing"
              {...pill}
              progressStyle={style}
              color={color}
              scale={0.78}
            />
          </div>
        );
        const presets: Option[] = COLORS.map(({ value, key }) => ({
          value,
          title: k(key),
          preview: swatch(value === "default" ? "" : value),
        }));
        return own && !COLORS.some((o) => o.value === own)
          ? [
              ...presets,
              {
                value: "custom",
                title: k("custom"),
                detail: own,
                preview: swatch(own),
              },
            ]
          : presets;
      }
      case "liveText":
        return (["off", "last_words", "full_text"] as const).map((v) => ({
          value: v,
          title: k(`${v}.title`),
          detail: k(`${v}.detail`),
          // It needs a model that runs on this PC.
          disabled: v !== "off" && remote,
          preview:
            v === "off" ? (
              <PillPreview mode="recording" {...pill} liveText={false} />
            ) : (
              <BoxOverPill
                box={{ ...box, mode: v, width: Math.min(boxWidth, 460) }}
                pill={pill}
                scale={0.45}
              />
            ),
        }));
      case "width":
        return WIDTHS.map(({ px, key }) => ({
          value: String(px),
          title: t(`settings.general.liveTextWidth.${key}`),
          detail: `${px} px`,
          preview: (
            <LiveTextPreview {...box} width={px} lines={2} full scale={0.42} />
          ),
        }));
      case "textSize":
        return SIZES.map(({ px, key }) => ({
          value: String(px),
          title: t(`settings.general.liveTextSize.${key}`),
          detail: `${px} px`,
          preview: (
            <LiveTextPreview {...box} fontSize={px} width={460} scale={0.62} />
          ),
        }));
      case "quiet":
        return (["box", "pill", "off"] as const).map((v) => ({
          value: v,
          title: k(`${v}.title`),
          detail: k(`${v}.detail`),
          preview: (
            <PillPreview
              mode="recording"
              {...pill}
              quiet={v === "off" ? null : v}
            />
          ),
        }));
      default:
        return [];
    }
  })();

  const common = {
    name: t("setup.appearance.name"),
    step: step === "done" ? 0 : index + 1,
    total: steps.length - 1,
    onClose,
    onBack:
      index > 0 && step !== "done"
        ? () => !saving && setStep(steps[index - 1])
        : undefined,
  };

  if (step === "intro")
    return (
      <SetupFrame
        {...common}
        heading={k("title")}
        body={k("body")}
        primary={t("setup.next")}
        onPrimary={() => goNext("intro")}
      >
        <div className="flex flex-col sm:flex-row items-center gap-8 rounded-lg border border-border bg-surface p-6">
          <div
            className={`flex-none flex items-center justify-center w-[280px] h-[120px] rounded-md ${DESKTOP}`}
            aria-hidden
          >
            <PillPreview mode="recording" {...pill} pause scale={1.25} />
          </div>
          <ul className="flex flex-col gap-3">
            <Part
              icon={
                <span
                  // The glowing T's light centre, in the pill's colour.
                  style={{
                    color: `color-mix(in srgb, ${pill.color || DEFAULT_PROGRESS_COLOR} 45%, white)`,
                  }}
                >
                  <TGlyph />
                </span>
              }
              text={k("t")}
            />
            <Part icon={<Mic className="w-3.5 h-3.5" />} text={k("bars")} />
            <Part icon={<PauseGlyph />} text={k("pause")} />
            <Part icon={<X className="w-3.5 h-3.5" />} text={k("cancel")} />
          </ul>
        </div>
      </SetupFrame>
    );

  if (step === "done")
    return (
      <SetupFrame
        {...common}
        heading={k("title")}
        body={k("body")}
        primary={t("setup.finish")}
        onPrimary={onClose}
      />
    );

  const cols =
    options.length === 3 || step === "color"
      ? "grid-cols-3"
      : "grid-cols-1 sm:grid-cols-2";
  return (
    <SetupFrame
      {...common}
      heading={k("title")}
      body={k("body")}
      primary={t("setup.saveNext")}
      onPrimary={() => void saveNext()}
      onSkip={() => !saving && goNext(step)}
      primaryDisabled={saving}
    >
      <div
        role="radiogroup"
        aria-label={k("title")}
        className={`grid ${cols} gap-3`}
      >
        {options.map((o) => (
          <OptionCard
            key={o.value}
            option={o}
            selected={choice === o.value}
            onPick={() => setChoice(o.value)}
          />
        ))}
      </div>
      {step === "liveText" && remote && (
        <p className="text-sm text-warn-text">
          {k("remote", { model: model?.name })}
        </p>
      )}
    </SetupFrame>
  );
};
