import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { Slider } from "../ui/Slider";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { ResetButton } from "../ui/ResetButton";
import { DESKTOP, PillPreview } from "./setups/appearance/Previews";
import {
  DEFAULT_PROGRESS_COLOR,
  hexToHsl,
  hslToHex,
  type Hsl,
} from "../../overlay/progressColor";
import { useSettings } from "../../hooks/useSettings";

interface Props {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

const SIZES = [
  { scale: 100, key: "normal" },
  { scale: 125, key: "large" },
  { scale: 150, key: "extraLarge" },
];

/** How the recording overlay shows the transcription's progress. */
export const ProgressStyleSetting: React.FC<Props> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  return (
    <SettingContainer
      title={t("settings.advanced.progressStyle.title")}
      description={t("settings.advanced.progressStyle.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    >
      <Dropdown
        options={(["line", "ring", "lap"] as const).map((style) => ({
          value: style,
          label: t(`settings.advanced.progressStyle.${style}`),
        }))}
        selectedValue={getSetting("progress_style") ?? "line"}
        onSelect={(value) =>
          updateSetting("progress_style", value as "line" | "ring" | "lap")
        }
        disabled={isUpdating("progress_style")}
      />
    </SettingContainer>
  );
};

/**
 * The pill as the settings around it make it, moving: recording (the T lit
 * while the live text box is on, the sound bars) and transcribing (the
 * progress running 0-100 over and over, with its glow and colour), at the
 * Overlay Size chosen.
 */
export const PillLookPreview: React.FC<Props> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const look = {
    wideBars: getSetting("sound_bars_wide") ?? false,
    progressStyle: getSetting("progress_style") ?? "line",
    glow: getSetting("progress_glow") ?? 100,
    lineGlow: getSetting("progress_line_glow") ?? false,
    liveText: getSetting("live_text_box_enabled") ?? false,
    pause: getSetting("pause_button_enabled") ?? false,
    color: getSetting("progress_color") ?? "",
  };
  const scale = (getSetting("pill_scale") ?? 100) / 100;
  return (
    <SettingContainer
      title={t("settings.advanced.pillPreview.title")}
      description={t("settings.advanced.pillPreview.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      layout="stacked"
    >
      <div
        className={`flex flex-wrap items-center justify-center gap-x-8 gap-y-2 rounded-md py-3 ${DESKTOP}`}
        aria-hidden
      >
        <PillPreview mode="recording" {...look} scale={scale} />
        <PillPreview mode="transcribing" {...look} scale={scale} />
      </div>
    </SettingContainer>
  );
};

/** Progress Style: line - the line glows like the light around the edge. */
export const ProgressLineGlowSetting: React.FC<Props> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  return (
    <ToggleSwitch
      checked={getSetting("progress_line_glow") ?? false}
      onChange={(enabled) => updateSetting("progress_line_glow", enabled)}
      isUpdating={isUpdating("progress_line_glow")}
      disabled={(getSetting("progress_style") ?? "line") !== "line"}
      label={t("settings.advanced.progressLineGlow.title")}
      description={t("settings.advanced.progressLineGlow.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    />
  );
};

/** How strong the progress glow is (edge light, glowing line). */
export const ProgressGlowSetting: React.FC<Props> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  // Nothing glows with a plain line or the circling light.
  const style = getSetting("progress_style") ?? "line";
  const glows =
    style === "ring" ||
    (style === "line" && (getSetting("progress_line_glow") ?? false));
  return (
    <Slider
      value={getSetting("progress_glow") ?? 100}
      onChange={(value) => updateSetting("progress_glow", value)}
      min={0}
      max={200}
      step={10}
      label={t("settings.advanced.progressGlow.title")}
      description={t("settings.advanced.progressGlow.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      formatValue={(value) => `${value}%`}
      disabled={!glows}
    />
  );
};

/**
 * The progress light's colour and glow: hue, saturation and lightness sliders
 * (each track painted with what it picks), a swatch, and back to the default.
 */
export const ProgressColorSetting: React.FC<Props> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const saved = getSetting("progress_color") || DEFAULT_PROGRESS_COLOR;
  const [hsl, setHsl] = useState<Hsl>(() => hexToHsl(saved));
  // Follow a change made elsewhere (the reset button, the setup).
  useEffect(() => {
    if (hslToHex(hsl) !== hslToHex(hexToHsl(saved))) setHsl(hexToHsl(saved));
  }, [saved]);

  // Back on the default's values = the default itself (the exact cyan look,
  // no reset button): the hex round trip is a shade off.
  const byDefault = hexToHsl(DEFAULT_PROGRESS_COLOR);
  const change = (next: Hsl) => {
    setHsl(next);
    const isDefault =
      next.h === byDefault.h &&
      next.s === byDefault.s &&
      next.l === byDefault.l;
    updateSetting("progress_color", isDefault ? "" : hslToHex(next));
  };
  const { h, s, l } = hsl;
  const color = hslToHex(hsl);
  const rows: {
    key: "hue" | "saturation" | "lightness";
    value: number;
    min: number;
    max: number;
    unit: string;
    track: string;
    set: (v: number) => Hsl;
  }[] = [
    {
      key: "hue",
      value: h,
      min: 0,
      max: 360,
      unit: "°",
      track: `linear-gradient(to right, ${[0, 60, 120, 180, 240, 300, 360]
        .map((x) => `hsl(${x} ${s}% ${l}%)`)
        .join(", ")})`,
      set: (v) => ({ ...hsl, h: v }),
    },
    {
      key: "saturation",
      value: s,
      min: 0,
      max: 100,
      unit: "%",
      track: `linear-gradient(to right, hsl(${h} 0% ${l}%), hsl(${h} 100% ${l}%))`,
      set: (v) => ({ ...hsl, s: v }),
    },
    {
      key: "lightness",
      value: l,
      min: 20,
      max: 85,
      unit: "%",
      track: `linear-gradient(to right, hsl(${h} ${s}% 20%), hsl(${h} ${s}% 52%), hsl(${h} ${s}% 85%))`,
      set: (v) => ({ ...hsl, l: v }),
    },
  ];
  return (
    <SettingContainer
      title={t("settings.advanced.progressColor.title")}
      description={t("settings.advanced.progressColor.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      layout="stacked"
    >
      <div className="flex items-center gap-4">
        {/* The Preview row above shows the colour on the pill. */}
        <div className="flex-1 grid grid-cols-[auto_1fr_auto] items-center gap-x-3 gap-y-2">
          {rows.map((r) => (
            <React.Fragment key={r.key}>
              <label
                htmlFor={`progress-color-${r.key}`}
                className="text-[13px] text-text-secondary"
              >
                {t(`settings.advanced.progressColor.${r.key}`)}
              </label>
              <input
                id={`progress-color-${r.key}`}
                type="range"
                min={r.min}
                max={r.max}
                value={r.value}
                onChange={(e) => change(r.set(Number(e.target.value)))}
                className="range-slider color-track w-full"
                style={
                  {
                    background: r.track,
                    "--thumb": color,
                  } as React.CSSProperties
                }
              />
              <span className="w-10 text-end text-xs text-text-secondary tabular-nums">
                {r.value}
                {r.unit}
              </span>
            </React.Fragment>
          ))}
        </div>
        {getSetting("progress_color") && (
          <ResetButton
            onClick={() => updateSetting("progress_color", "")}
            ariaLabel={t("settings.advanced.progressColor.reset")}
          />
        )}
      </div>
    </SettingContainer>
  );
};

/** Wider sound bars on the pill (the designer's 77.5 px instead of 62). */
export const SoundBarsWideSetting: React.FC<Props> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  return (
    <ToggleSwitch
      checked={getSetting("sound_bars_wide") ?? false}
      onChange={(enabled) => updateSetting("sound_bars_wide", enabled)}
      isUpdating={isUpdating("sound_bars_wide")}
      label={t("settings.advanced.soundBarsWide.title")}
      description={t("settings.advanced.soundBarsWide.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    />
  );
};

/** How big the recording overlay (and its "Too quiet" box) is. */
export const OverlaySizeSetting: React.FC<Props> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  return (
    <SettingContainer
      title={t("settings.advanced.overlaySize.title")}
      description={t("settings.advanced.overlaySize.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    >
      <Dropdown
        options={SIZES.map(({ scale, key }) => ({
          value: String(scale),
          label: t(`settings.advanced.overlaySize.${key}`),
        }))}
        selectedValue={String(getSetting("pill_scale") ?? 100)}
        onSelect={(value) => updateSetting("pill_scale", Number(value))}
        disabled={isUpdating("pill_scale")}
      />
    </SettingContainer>
  );
};
