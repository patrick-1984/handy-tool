import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { commands } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { ShortcutInput } from "../ShortcutInput";

type TypingStatus =
  | { state: "countdown"; seconds_left: number }
  | { state: "typing"; typed: number; total: number }
  | { state: "done"; total: number }
  | { state: "cancelled" }
  | { state: "error"; message: string };

const START_DELAY_PRESETS = [1, 3, 5];
const KEY_DELAY_PRESETS = [5, 15, 50, 500];

const primaryButtonClass =
  "px-4 py-1.5 rounded-md bg-blue-600 text-white text-sm font-medium hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors";
const presetButtonClass =
  "px-2.5 py-1.5 rounded-md border border-zinc-700 bg-zinc-800 text-zinc-100 text-sm hover:border-blue-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors cursor-pointer";
const numberInputClass =
  "rounded-md border border-zinc-700 bg-zinc-800 px-2 py-1.5 text-sm text-zinc-100 focus:border-blue-500 focus:outline-none";

export const KeyboardTyperPage: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();

  const savedStartDelay =
    (getSetting("typing_start_delay_secs") as number) ?? 10;
  const savedKeyDelay = (getSetting("typing_key_delay_ms") as number) ?? 15;

  const [text, setText] = useState("");
  const [startDelay, setStartDelay] = useState(String(savedStartDelay));
  const [keyDelay, setKeyDelay] = useState(String(savedKeyDelay));
  const [status, setStatus] = useState<TypingStatus | null>(null);

  // Re-sync committed values when the store changes externally
  useEffect(() => setStartDelay(String(savedStartDelay)), [savedStartDelay]);
  useEffect(() => setKeyDelay(String(savedKeyDelay)), [savedKeyDelay]);

  // Push the text to the backend (in-memory only, never persisted) so the
  // global shortcut can type it while another window has focus.
  const pushTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const handleTextChange = (value: string) => {
    setText(value);
    if (pushTimer.current) clearTimeout(pushTimer.current);
    pushTimer.current = setTimeout(() => {
      commands.setTypingText(value).catch(console.error);
    }, 250);
  };

  useEffect(() => {
    const unlistenPromise = listen<TypingStatus>("typing-status", (event) => {
      setStatus(event.payload);
    });
    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, []);

  const active = status?.state === "countdown" || status?.state === "typing";

  const commitStartDelay = () => {
    const parsed = parseInt(startDelay, 10);
    if (!isNaN(parsed) && parsed >= 0 && parsed <= 9999) {
      if (parsed !== savedStartDelay) {
        updateSetting("typing_start_delay_secs", parsed);
      }
      setStartDelay(String(parsed));
      return parsed;
    }
    setStartDelay(String(savedStartDelay));
    return savedStartDelay;
  };

  const commitKeyDelay = () => {
    const parsed = parseInt(keyDelay, 10);
    if (!isNaN(parsed) && parsed >= 0 && parsed <= 99999) {
      if (parsed !== savedKeyDelay) {
        updateSetting("typing_key_delay_ms", parsed);
      }
      setKeyDelay(String(parsed));
    } else {
      setKeyDelay(String(savedKeyDelay));
    }
  };

  const startWithDelay = async (delaySecs: number) => {
    // Flush any pending text push so the session types the latest content
    if (pushTimer.current) clearTimeout(pushTimer.current);
    try {
      await commands.setTypingText(text);
      const result = await commands.startTyping(delaySecs);
      if (result.status === "error") {
        setStatus({ state: "error", message: result.error });
      }
    } catch (e) {
      console.error("Failed to start typing:", e);
    }
  };

  const handleGo = () => {
    const delay = commitStartDelay();
    startWithDelay(delay);
  };

  const handleCancel = () => {
    commands.cancelTyping().catch(console.error);
  };

  const statusText = (() => {
    if (!status) return t("keyboardTyper.status.idle");
    switch (status.state) {
      case "countdown":
        return t("keyboardTyper.status.countdown", {
          seconds: status.seconds_left,
        });
      case "typing":
        return t("keyboardTyper.status.typing", {
          typed: status.typed,
          total: status.total,
        });
      case "done":
        return t("keyboardTyper.status.done", { total: status.total });
      case "cancelled":
        return t("keyboardTyper.status.cancelled");
      case "error":
        return t("keyboardTyper.status.error", { message: status.message });
    }
  })();

  return (
    <div className="w-full flex flex-col gap-4 h-full">
      <h2 className="text-lg font-semibold text-text">
        {t("keyboardTyper.title")}
      </h2>
      <p className="text-sm text-text/60">{t("keyboardTyper.description")}</p>

      <textarea
        className="flex-1 min-h-[200px] w-full rounded-md border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm text-zinc-100 placeholder-zinc-500 focus:border-blue-500 focus:outline-none resize-none"
        placeholder={t("keyboardTyper.placeholder")}
        value={text}
        onChange={(e) => handleTextChange(e.target.value)}
        autoComplete="off"
        autoCorrect="off"
        spellCheck={false}
      />

      <div className="flex items-center gap-2 flex-wrap">
        <span className="text-sm text-text/80 w-28 shrink-0">
          {t("keyboardTyper.startDelay.label")}
        </span>
        <input
          type="number"
          min="0"
          max="9999"
          value={startDelay}
          onChange={(e) => setStartDelay(e.target.value)}
          onBlur={commitStartDelay}
          className={`${numberInputClass} w-20`}
          disabled={active}
        />
        <span className="text-sm text-text/60">
          {t("keyboardTyper.startDelay.unit")}
        </span>
        <button
          onClick={handleGo}
          disabled={active || !text}
          className={primaryButtonClass}
        >
          {t("keyboardTyper.go")}
        </button>
        {START_DELAY_PRESETS.map((seconds) => (
          <button
            key={seconds}
            onClick={() => startWithDelay(seconds)}
            disabled={active || !text}
            className={presetButtonClass}
            title={t("keyboardTyper.startDelay.presetTitle", { seconds })}
          >
            {t("keyboardTyper.startDelay.preset", { seconds })}
          </button>
        ))}
      </div>

      <div className="flex items-center gap-2 flex-wrap">
        <span className="text-sm text-text/80 w-28 shrink-0">
          {t("keyboardTyper.keyDelay.label")}
        </span>
        <input
          type="number"
          min="0"
          max="99999"
          value={keyDelay}
          onChange={(e) => setKeyDelay(e.target.value)}
          onBlur={commitKeyDelay}
          onKeyDown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
          }}
          className={`${numberInputClass} w-24`}
        />
        <span className="text-sm text-text/60">
          {t("keyboardTyper.keyDelay.unit")}
        </span>
        {KEY_DELAY_PRESETS.map((ms) => (
          <button
            key={ms}
            onClick={() => {
              setKeyDelay(String(ms));
              if (ms !== savedKeyDelay) {
                updateSetting("typing_key_delay_ms", ms);
              }
            }}
            className={`${presetButtonClass} ${
              savedKeyDelay === ms ? "border-blue-500" : ""
            }`}
          >
            {ms}
          </button>
        ))}
      </div>

      <ShortcutInput shortcutId="type_text" grouped={false} />

      <div className="flex items-center gap-3 min-h-8">
        <span
          className={`text-sm ${
            status?.state === "error" ? "text-red-400" : "text-text/80"
          }`}
        >
          {statusText}
        </span>
        {active && (
          <button onClick={handleCancel} className={presetButtonClass}>
            {t("keyboardTyper.cancel")}
          </button>
        )}
      </div>
    </div>
  );
};
