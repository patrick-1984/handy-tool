import { listen } from "@tauri-apps/api/event";
import {
  CheckMenuItem,
  Menu,
  MenuItem,
  PredefinedMenuItem,
} from "@tauri-apps/api/menu";
import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertCircle, Lock, MicOff, X } from "lucide-react";
import "./RecordingOverlay.css";
import { PauseGlyph, PlayGlyph, TGlyph } from "./glyphs";
import { progressColorVars } from "./progressColor";
import {
  commands,
  type CancelBehavior,
  type LiveTextMode,
  type ProgressStyle,
} from "@/bindings";
import i18n, { syncLanguageFromSettings } from "@/i18n";
import { getLanguageDirection } from "@/lib/utils/rtl";
import { useOsType } from "@/hooks/useOsType";
import { takeShortcutsAvailable } from "@/lib/utils/keyboard";

type OverlayState =
  | "recording"
  | "paused"
  | "transcribing"
  | "processing"
  | "no-microphone"
  | "microphone-blocked"
  | "microphone-error";

// Why a take could not start; each is shown briefly, then the overlay hides.
const MICROPHONE_PROBLEMS: Partial<Record<OverlayState, string>> = {
  "no-microphone": "overlay.noMicrophone",
  "microphone-blocked": "overlay.microphoneBlocked",
  "microphone-error": "overlay.microphoneError",
};

// The sound bars: the voice range (400 Hz - 4 kHz) in 16 bands, low to high
// pitch, in the middle of the pill.
const BAR_BANDS = 16;
/** Below this level a band is drawn in the darker cyan. */
const QUIET_BAND = 0.12;

const RecordingOverlay: React.FC = () => {
  const { t } = useTranslation();
  const osType = useOsType();
  const [isVisible, setIsVisible] = useState(false);
  const [state, setState] = useState<OverlayState>("recording");
  // The overlay appears the moment the shortcut is pressed, but a microphone that
  // has been idle can take most of a second to deliver audio - and some then fade
  // in, too quiet for speech to be kept. Show "starting" rather than a waveform
  // the user talks over until a mic-level update says the microphone is live.
  const [micLive, setMicLive] = useState(false);
  // How far the transcription is after stop (percent); null until the backend
  // reports one (it waits half a second, and has no figure for remote engines).
  const [progress, setProgress] = useState<number | null>(null);
  // The figure on screen counts up to each new one instead of jumping: 1% per
  // step, bigger steps for a bigger gap, so even a jump rolls up smoothly.
  const [shownProgress, setShownProgress] = useState<number | null>(null);
  // The backend sends 100 the moment the transcript exists, and the figure shows
  // it at once. A pill hidden before its figure got to 100% (the text was ready
  // but none was sent) runs it up in a quick burst, then fades.
  const [finishing, setFinishing] = useState(false);
  const shownProgressRef = useRef<number | null>(null);
  shownProgressRef.current = shownProgress;
  useEffect(() => {
    const target = finishing ? 100 : progress;
    if (target === null) {
      setShownProgress(null);
      return;
    }
    if (shownProgress !== null && shownProgress >= target) {
      if (!finishing) return;
      const timer = setTimeout(() => setIsVisible(false), 120);
      return () => clearTimeout(timer);
    }
    // The backend's 100 means the text is ready and about to be pasted: show it
    // at once, since a count-up would still be running when the text lands.
    if (
      (target === 100 && !finishing) ||
      window.matchMedia("(prefers-reduced-motion: reduce)").matches
    ) {
      setShownProgress(target);
      return;
    }
    const timer = setTimeout(
      () => {
        const from = shownProgress ?? 0;
        const gap = target - from;
        const step = finishing
          ? Math.max(2, Math.round(gap / 3))
          : Math.max(1, Math.round(gap / 6));
        setShownProgress(Math.min(target, from + step));
      },
      finishing ? 16 : 30,
    );
    return () => clearTimeout(timer);
  }, [progress, shownProgress, finishing]);
  // Voice-like sound too quiet to be kept was heard just now.
  const [tooQuiet, setTooQuiet] = useState(false);
  // Progress Style: a line along the bottom, or the light round the edge.
  const [progressStyle, setProgressStyle] = useState<ProgressStyle>("line");
  // Glow strength in percent, and whether the line glows too.
  const [glow, setGlow] = useState(100);
  const [lineGlow, setLineGlow] = useState(false);
  // Progress Colour ("" = the default cyan).
  const [progressColor, setProgressColor] = useState("");
  // Wide sound bars (2.5 px bars and gaps).
  const [soundBarsWide, setSoundBarsWide] = useState(false);
  // Pause button (optional) and the live text box switch (the T button).
  const [pauseEnabled, setPauseEnabled] = useState(false);
  const [liveTextBox, setLiveTextBox] = useState(false);
  // What the button under the mouse does, shown in the middle of the pill.
  const [hoverLabel, setHoverLabel] = useState<string | null>(null);
  const [levels, setLevels] = useState<number[]>(Array(BAR_BANDS).fill(0));
  const smoothedLevelsRef = useRef<number[]>(Array(BAR_BANDS).fill(0));
  const direction = getLanguageDirection(i18n.language);

  useEffect(() => {
    const setupEventListeners = async () => {
      // Listen for show-overlay event from Rust
      const unlistenShow = await listen("show-overlay", async (event) => {
        // Sync language from settings each time overlay is shown
        await syncLanguageFromSettings();
        const overlayState = event.payload as OverlayState;
        setState(overlayState);
        setHoverLabel(null);
        setMicLive(false);
        setTooQuiet(false);
        setProgress(null);
        setFinishing(false);
        setIsVisible(true);
        const settings = await commands.getAppSettings();
        if (settings.status === "ok") {
          setPauseEnabled(settings.data.pause_button_enabled ?? false);
          setLiveTextBox(settings.data.live_text_box_enabled ?? false);
          // Reduce motion: the circling light becomes the line.
          const style = settings.data.progress_style ?? "line";
          setProgressStyle(
            style === "lap" &&
              window.matchMedia("(prefers-reduced-motion: reduce)").matches
              ? "line"
              : style,
          );
          setSoundBarsWide(settings.data.sound_bars_wide ?? false);
          setProgressColor(settings.data.progress_color ?? "");
          setGlow(settings.data.progress_glow ?? 100);
          setLineGlow(settings.data.progress_line_glow ?? false);
          // Overlay Size: the window is already that size; the page follows.
          document.documentElement.style.zoom = String(
            (settings.data.pill_scale ?? 100) / 100,
          );
        }
      });

      const unlistenLiveTextBox = await listen<boolean>(
        "live-text-box-changed",
        (event) => setLiveTextBox(event.payload),
      );

      // Transcription progress after stop, only meaningful in "transcribing"
      const unlistenProgress = await listen<number>(
        "transcription-progress",
        (event) => setProgress(event.payload),
      );

      // Listen for hide-overlay event from Rust
      const unlistenHide = await listen("hide-overlay", () => {
        const shown = shownProgressRef.current;
        if (shown !== null && shown < 100) {
          setFinishing(true); // hides once the figure reaches 100%
        } else {
          setIsVisible(false);
        }
      });

      // Listen for mic-level updates
      const unlistenLevel = await listen<{
        levels: number[];
        live: boolean;
        too_quiet: boolean;
      }>("mic-level", (event) => {
        const { levels: newLevels, live, too_quiet } = event.payload;
        setTooQuiet(too_quiet);

        // Apply smoothing to reduce jitter
        const smoothed = smoothedLevelsRef.current.map((prev, i) => {
          const target = newLevels[i] || 0;
          return prev * 0.7 + target * 0.3; // Smooth transition
        });

        smoothedLevelsRef.current = smoothed;
        setLevels(smoothed.slice(0, BAR_BANDS));
        if (live) setMicLive(true);
      });

      // Cleanup function
      return () => {
        unlistenShow();
        unlistenHide();
        unlistenProgress();
        unlistenLiveTextBox();
        unlistenLevel();
      };
    };

    setupEventListeners();
  }, []);

  useEffect(() => {
    // The WebView's own menu (Refresh, Save as, Print) has no place on the pill.
    const noMenu = (e: MouseEvent) => e.preventDefault();
    // A hover label must not outlive the pointer: no mouseleave reaches the
    // button when the pointer leaves through a menu or the window loses focus.
    const clearHover = () => setHoverLabel(null);
    document.addEventListener("contextmenu", noMenu);
    document.documentElement.addEventListener("mouseleave", clearHover);
    window.addEventListener("blur", clearHover);
    return () => {
      document.removeEventListener("contextmenu", noMenu);
      document.documentElement.removeEventListener("mouseleave", clearHover);
      window.removeEventListener("blur", clearHover);
    };
  }, []);

  // Hovering a button names it in the middle of the pill.
  const hoverProps = (label: string) => ({
    onMouseEnter: () => setHoverLabel(label),
    onMouseLeave: () => setHoverLabel(null),
  });

  // Right-click menus. Native, so the small pill window does not clip them.
  type MenuEntry = MenuItem | CheckMenuItem | PredefinedMenuItem;
  const popup = async (entries: Promise<MenuEntry>[]) => {
    setHoverLabel(null);
    const menu = await Menu.new({ items: await Promise.all(entries) });
    await menu.popup();
  };
  const openAt = (section: string, titleKey: string) => () => {
    commands.openSettingsAt(section, titleKey);
  };
  const changeShortcut = (shortcutId: string) =>
    MenuItem.new({
      text: t("overlay.menu.changeShortcut"),
      action: openAt(
        "shortcuts",
        `settings.general.shortcut.bindings.${shortcutId}.name`,
      ),
    });
  const separator = () => PredefinedMenuItem.new({ item: "Separator" });
  const currentSettings = async () => {
    const result = await commands.getAppSettings();
    return result.status === "ok" ? result.data : null;
  };

  const liveTextMenu = async () => {
    const mode = (await currentSettings())?.live_text_mode ?? "last_words";
    const style = (value: LiveTextMode, key: string) =>
      CheckMenuItem.new({
        text: t(key),
        checked: mode === value,
        action: () => {
          commands.changeLiveTextModeSetting(value);
        },
      });
    await popup([
      style("last_words", "settings.general.liveTextMode.lastWords"),
      style("full_text", "settings.general.liveTextMode.fullText"),
      separator(),
      MenuItem.new({
        text: t("overlay.menu.liveTextSettings"),
        action: openAt("general", "settings.general.liveTextBox.label"),
      }),
      changeShortcut("toggle_live_text_box"),
    ]);
  };

  const pauseMenu = () =>
    popup([
      // Take-only shortcuts are never registered on Linux, so not offered there.
      ...(takeShortcutsAvailable(osType) ? [changeShortcut("pause")] : []),
      // Not while paused: without the button a paused take could only be stopped.
      ...(state === "recording"
        ? [
            MenuItem.new({
              text: t("overlay.menu.hidePause"),
              action: () => {
                setHoverLabel(null);
                setPauseEnabled(false);
                commands.changePauseButtonSetting(false);
              },
            }),
          ]
        : []),
    ]);

  const cancelMenu = async () => {
    const behavior =
      (await currentSettings())?.cancel_behavior ?? "finish_silently";
    const option = (value: CancelBehavior, key: string) =>
      CheckMenuItem.new({
        text: t(key),
        checked: behavior === value,
        action: () => {
          commands.changeCancelBehaviorSetting(value);
        },
      });
    await popup([
      option(
        "finish_silently",
        "settings.general.cancelBehavior.options.finishSilently",
      ),
      option(
        "discard_recording",
        "settings.general.cancelBehavior.options.discardRecording",
      ),
      ...(takeShortcutsAvailable(osType)
        ? [separator(), changeShortcut("cancel")]
        : []),
    ]);
  };
  const onMenu = (open: () => void) => (e: React.MouseEvent) => {
    e.preventDefault();
    open();
  };

  return (
    // The pill sits in transparent room of its own (the window's margin), so
    // the edge light can run centred on its border.
    <div
      dir={direction}
      className={`pill-frame ${isVisible ? "fade-in" : ""}`}
      style={
        {
          "--glow": glow / 100,
          ...progressColorVars(progressColor),
        } as React.CSSProperties
      }
    >
      <div className="recording-overlay">
        {/* A microphone problem takes the whole pill: no buttons then. */}
        {!MICROPHONE_PROBLEMS[state] && (
          <div className="overlay-float-btn">
            <button
              type="button"
              className={`float-button ${liveTextBox ? "active" : ""}`}
              aria-label={t("overlay.liveTextBox")}
              aria-pressed={liveTextBox}
              {...hoverProps(t("overlay.liveTextBox"))}
              onContextMenu={onMenu(liveTextMenu)}
              onClick={async () => {
                const result = await commands.toggleLiveTextBox();
                if (result.status === "ok") setLiveTextBox(result.data);
              }}
            >
              <TGlyph />
            </button>
          </div>
        )}
        <div className="overlay-middle">
          {hoverLabel && (
            <div className="overlay-message hover-label">{hoverLabel}</div>
          )}
          {!hoverLabel && state === "recording" && !micLive && (
            <div className="transcribing-text">
              <span className="overlay-spinner" aria-hidden />
              <span className="text">{t("overlay.startingMic")}</span>
            </div>
          )}
          {!hoverLabel && state === "recording" && micLive && tooQuiet && (
            <div className="overlay-message too-quiet">
              {t("overlay.tooQuiet")}
            </div>
          )}
          {!hoverLabel && state === "recording" && micLive && !tooQuiet && (
            <div className={`bars-container ${soundBarsWide ? "wide" : ""}`}>
              {levels.map((v, i) => (
                <div
                  key={i}
                  className="bar"
                  style={{
                    height: `${Math.min(20, 4 + Math.pow(v, 0.7) * 16)}px`, // Cap at 20px max height
                    transition:
                      "height 60ms ease-out, background 120ms ease-out",
                    // Quiet bands in a darker cyan: the level reads without motion.
                    background:
                      v < QUIET_BAND ? "var(--overlay-bar-low)" : undefined,
                  }}
                />
              ))}
            </div>
          )}
          {!hoverLabel && state === "paused" && (
            <div className="overlay-message paused-message">
              {t("overlay.paused")}
            </div>
          )}
          {!hoverLabel && state === "transcribing" && (
            <div className="transcribing-text">
              {/* With a percentage the figure (and the progress light) show it
                  working; the spinner's room goes to the words. */}
              {shownProgress === null && (
                <span className="overlay-spinner" aria-hidden />
              )}
              <span className="text">
                {shownProgress === null
                  ? t("overlay.transcribing")
                  : t("overlay.transcribingProgress", {
                      percent: shownProgress,
                    })}
              </span>
            </div>
          )}
          {!hoverLabel && state === "processing" && (
            <div className="transcribing-text">
              <span className="overlay-spinner" aria-hidden />
              <span className="text">{t("overlay.processing")}</span>
            </div>
          )}
          {MICROPHONE_PROBLEMS[state] && (
            <div className="overlay-message problem">
              {state === "no-microphone" ? (
                <MicOff size={13} aria-hidden />
              ) : state === "microphone-blocked" ? (
                <Lock size={13} aria-hidden />
              ) : (
                <AlertCircle size={13} aria-hidden />
              )}
              {t(MICROPHONE_PROBLEMS[state]!)}
            </div>
          )}
        </div>

        <div className="overlay-right">
          {/* Not while the microphone starts (nothing to pause yet): its room
              goes to "Starting mic...". While transcribing it is gone too. */}
          {pauseEnabled &&
            ((state === "recording" && micLive) || state === "paused") && (
              <button
                type="button"
                className={`cancel-button ${state === "paused" ? "resume" : ""}`}
                aria-label={t(
                  state === "paused" ? "overlay.resume" : "overlay.pause",
                )}
                {...hoverProps(
                  t(state === "paused" ? "overlay.resume" : "overlay.pause"),
                )}
                onContextMenu={onMenu(pauseMenu)}
                onClick={() => commands.togglePauseRecording()}
              >
                {state === "paused" ? <PlayGlyph /> : <PauseGlyph />}
              </button>
            )}
          {/* Cancel, also while transcribing: the text is then not pasted. */}
          {(state === "recording" ||
            state === "paused" ||
            state === "transcribing" ||
            state === "processing") && (
            <button
              type="button"
              className="cancel-button"
              aria-label={t("common.cancel")}
              {...hoverProps(t("common.cancel"))}
              onContextMenu={onMenu(cancelMenu)}
              onClick={() => {
                commands.cancelOperation();
              }}
            >
              <X size={12} strokeWidth={2.5} aria-hidden />
            </button>
          )}
        </div>
      </div>
      {/* How far the transcription is (Progress Style: line), clipped to the
          pill's inside; outside the pill so its glow is not cut off. */}
      {progressStyle === "line" &&
        state === "transcribing" &&
        shownProgress !== null && (
          <div
            className={`progress-line ${lineGlow ? "glow" : ""}`}
            aria-hidden
          >
            <div className="progress-line-clip">
              <div
                className="overlay-progress"
                style={{ width: `${shownProgress}%` }}
              />
            </div>
          </div>
        )}
      {progressStyle === "ring" &&
        state === "transcribing" &&
        shownProgress !== null && (
          // Progress Style: ring - the light running round the pill, centred
          // on its border, clockwise from the bottom centre; its trail stays
          // lit, one even colour and glow up to the figure.
          <div className="progress-ring" aria-hidden>
            <div
              className="progress-ring-light"
              style={{ "--ring-p": shownProgress } as React.CSSProperties}
            />
          </div>
        )}
      {progressStyle === "lap" && state === "transcribing" && (
        // Progress Style: lap - a thin light with a fading tail circling the
        // edge, one lap every 1.6 s; the figure is in the words.
        <div className="progress-lap" aria-hidden />
      )}
    </div>
  );
};

export default RecordingOverlay;
