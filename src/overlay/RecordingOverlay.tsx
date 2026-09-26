import { listen } from "@tauri-apps/api/event";
import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Monitor, Pause, Play } from "lucide-react";
import {
  MicrophoneIcon,
  TranscriptionIcon,
  CancelIcon,
  TextIcon,
} from "../components/icons";
import "./RecordingOverlay.css";
import { commands } from "@/bindings";
import i18n, { syncLanguageFromSettings } from "@/i18n";
import { getLanguageDirection } from "@/lib/utils/rtl";

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

const RecordingOverlay: React.FC = () => {
  const { t } = useTranslation();
  const [isVisible, setIsVisible] = useState(false);
  const [state, setState] = useState<OverlayState>("recording");
  // The overlay appears the moment the shortcut is pressed, but a microphone that
  // has been idle can take most of a second to deliver audio. Nothing is captured
  // until then, so show "starting" rather than a flat waveform the user talks over.
  // The first mic-level update is the signal that audio is actually flowing.
  const [micLive, setMicLive] = useState(false);
  // How far the transcription is after stop (percent); null until the backend
  // reports one (it waits half a second, and has no figure for remote engines).
  const [progress, setProgress] = useState<number | null>(null);
  // Pause button (optional) and the live text box switch (the T button).
  const [pauseEnabled, setPauseEnabled] = useState(false);
  const [liveTextBox, setLiveTextBox] = useState(false);
  // How fast the PC transcribes against its own normal (percent), only while it
  // is clearly slower than usual; null otherwise. Starts empty on every take.
  const [speed, setSpeed] = useState<number | null>(null);
  const stateRef = useRef<OverlayState | null>(null);
  const [levels, setLevels] = useState<number[]>(Array(16).fill(0));
  const smoothedLevelsRef = useRef<number[]>(Array(16).fill(0));
  const direction = getLanguageDirection(i18n.language);

  useEffect(() => {
    const setupEventListeners = async () => {
      // Listen for show-overlay event from Rust
      const unlistenShow = await listen("show-overlay", async (event) => {
        // Sync language from settings each time overlay is shown
        await syncLanguageFromSettings();
        const overlayState = event.payload as OverlayState;
        // A new take (not a resume from pause) starts without a speed verdict.
        if (
          overlayState === "recording" &&
          stateRef.current !== "recording" &&
          stateRef.current !== "paused"
        ) {
          setSpeed(null);
        }
        stateRef.current = overlayState;
        setState(overlayState);
        setMicLive(false);
        setProgress(null);
        setIsVisible(true);
        const settings = await commands.getAppSettings();
        if (settings.status === "ok") {
          setPauseEnabled(settings.data.pause_button_enabled ?? false);
          setLiveTextBox(settings.data.live_text_box_enabled ?? false);
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

      const unlistenSpeed = await listen<number | null>(
        "transcription-speed",
        (event) => setSpeed(event.payload),
      );

      // Listen for hide-overlay event from Rust
      const unlistenHide = await listen("hide-overlay", () => {
        stateRef.current = null;
        setIsVisible(false);
      });

      // Listen for mic-level updates
      const unlistenLevel = await listen<number[]>("mic-level", (event) => {
        const newLevels = event.payload as number[];

        // Apply smoothing to reduce jitter
        const smoothed = smoothedLevelsRef.current.map((prev, i) => {
          const target = newLevels[i] || 0;
          return prev * 0.7 + target * 0.3; // Smooth transition
        });

        smoothedLevelsRef.current = smoothed;
        setLevels(smoothed.slice(0, 9));
        setMicLive(true);
      });

      // Cleanup function
      return () => {
        unlistenShow();
        unlistenHide();
        unlistenProgress();
        unlistenSpeed();
        unlistenLiveTextBox();
        unlistenLevel();
      };
    };

    setupEventListeners();
  }, []);

  const getIcon = () => {
    if (
      state === "recording" ||
      state === "paused" ||
      MICROPHONE_PROBLEMS[state]
    ) {
      return <MicrophoneIcon />;
    } else {
      return <TranscriptionIcon />;
    }
  };

  return (
    <div
      dir={direction}
      className={`recording-overlay ${isVisible ? "fade-in" : ""}`}
    >
      <div className="overlay-float-btn">
        <button
          type="button"
          className={`float-button ${liveTextBox ? "active" : ""}`}
          aria-label={t("overlay.liveTextBox")}
          aria-pressed={liveTextBox}
          title={t("overlay.liveTextBox")}
          onClick={async () => {
            const result = await commands.toggleLiveTextBox();
            if (result.status === "ok") setLiveTextBox(result.data);
          }}
        >
          <TextIcon />
        </button>
      </div>
      <div className="overlay-left">{getIcon()}</div>

      <div className="overlay-middle">
        {speed !== null && (state === "recording" || state === "paused") && (
          <div
            className={`speed-warning ${speed < 50 ? "very-slow" : ""}`}
            title={t("overlay.slowPc", { percent: speed })}
          >
            <Monitor size={12} aria-hidden />
            <span>{speed}%</span>
          </div>
        )}
        {state === "recording" && !micLive && (
          <div className="transcribing-text overlay-message">
            {t("overlay.startingMic")}
          </div>
        )}
        {state === "recording" && micLive && (
          <div className="bars-container">
            {levels.map((v, i) => (
              <div
                key={i}
                className="bar"
                style={{
                  height: `${Math.min(20, 4 + Math.pow(v, 0.7) * 16)}px`, // Cap at 20px max height
                  transition: "height 60ms ease-out, opacity 120ms ease-out",
                  opacity: Math.max(0.2, v * 1.7), // Minimum opacity for visibility
                }}
              />
            ))}
          </div>
        )}
        {state === "paused" && (
          <div className="overlay-message">{t("overlay.paused")}</div>
        )}
        {state === "transcribing" && (
          <div className="transcribing-text">
            {progress === null
              ? t("overlay.transcribing")
              : t("overlay.transcribingProgress", { percent: progress })}
          </div>
        )}
        {state === "processing" && (
          <div className="transcribing-text">{t("overlay.processing")}</div>
        )}
        {MICROPHONE_PROBLEMS[state] && (
          <div className="overlay-message">
            {t(MICROPHONE_PROBLEMS[state]!)}
          </div>
        )}
      </div>

      <div className="overlay-right">
        {pauseEnabled && (state === "recording" || state === "paused") && (
          <button
            type="button"
            className="cancel-button"
            aria-label={t(
              state === "paused" ? "overlay.resume" : "overlay.pause",
            )}
            title={t(state === "paused" ? "overlay.resume" : "overlay.pause")}
            onClick={() => commands.togglePauseRecording()}
          >
            {state === "paused" ? <Play size={14} /> : <Pause size={14} />}
          </button>
        )}
        {(state === "recording" || state === "paused") && (
          <button
            type="button"
            className="cancel-button"
            aria-label={t("common.cancel")}
            onClick={() => {
              commands.cancelOperation();
            }}
          >
            <CancelIcon />
          </button>
        )}
      </div>
    </div>
  );
};

export default RecordingOverlay;
