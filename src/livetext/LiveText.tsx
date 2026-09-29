import { listen } from "@tauri-apps/api/event";
import React, { useEffect, useLayoutEffect, useRef, useState } from "react";
import "./LiveText.css";

type LiveTextMode = "last_words" | "full_text";

interface LiveTextShow {
  mode: LiveTextMode;
  fade: boolean;
  below_pill: boolean;
  /** Text size in logical pixels. */
  font_size: number;
}

interface LiveTranscriptionChunk {
  index: number;
  text: string;
  is_final: boolean;
}

/** New text is typed in within about this long, before the next update lands. */
const TYPE_WITHIN_MS = 900;
const MIN_CHAR_MS = 8;
const MAX_CHAR_MS = 45;
/** After the take: how long the final text stays before the box fades. */
const FINAL_LINGER_MS = 1500;
/** Longest the box waits for text still being typed in before it fades. */
const MAX_TYPING_WAIT_MS = 12000;
/** With fading on: how long the text stays once it stops changing. */
const FADE_AFTER_MS = 3000;

const reducedMotion = () =>
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;

/**
 * The live text box next to the recording pill. "Last words" is one line whose
 * newest words stay at the edge nearest the text's end; "whole text" starts at
 * one line and grows away from the pill with the take, until it fills its window
 * and the oldest lines slide out. New words are typed in rather
 * than appearing at once, so the eye can follow them; removed words (undo, or
 * the preview correcting itself) go at once.
 */
const LiveText: React.FC = () => {
  const [visible, setVisible] = useState(false);
  const [mode, setMode] = useState<LiveTextMode>("last_words");
  const [fade, setFade] = useState(false);
  const [belowPill, setBelowPill] = useState(false);
  const [fontSize, setFontSize] = useState(15);
  // The latest text from the preview, and how much of it is on screen so far.
  const [target, setTarget] = useState("");
  const [shown, setShown] = useState("");
  const shownRef = useRef("");
  const [faded, setFaded] = useState(false);
  // Text longer than the box: fade the edge the oldest words leave by.
  const boxRef = useRef<HTMLDivElement>(null);
  const [overflowing, setOverflowing] = useState(false);
  // The take's final text arrives just before the box is told to hide; it is
  // shown whole at once and kept a moment, so the last words are seen too.
  const instantRef = useRef(false);
  const finalAtRef = useRef(0);
  // When the text being typed in will be complete ("Show the text as it's
  // transcribed" types a stopped take's text in; the box waits for it).
  const typedAtRef = useRef(0);
  const hideTimer = useRef<number | undefined>(undefined);
  useLayoutEffect(() => {
    const box = boxRef.current;
    const text = box?.firstElementChild as HTMLElement | null | undefined;
    setOverflowing(
      !!box &&
        (mode === "last_words"
          ? !!text && text.offsetWidth > box.clientWidth - 28 // minus padding
          : box.scrollHeight > box.clientHeight + 1),
    );
  }, [shown, mode]);

  useEffect(() => {
    const unlisteners = Promise.all([
      listen<LiveTextShow>("live-text-show", (event) => {
        window.clearTimeout(hideTimer.current);
        setMode(event.payload.mode);
        setFade(event.payload.fade);
        setBelowPill(event.payload.below_pill);
        setFontSize(event.payload.font_size ?? 15);
        setVisible(true);
      }),
      listen("live-text-hide", () => {
        const now = Date.now();
        const typing = Math.min(typedAtRef.current - now, MAX_TYPING_WAIT_MS);
        const left = Math.max(
          FINAL_LINGER_MS - (now - finalAtRef.current),
          typing > 0 ? typing + FINAL_LINGER_MS : 0,
        );
        window.clearTimeout(hideTimer.current);
        hideTimer.current = window.setTimeout(
          () => setVisible(false),
          Math.max(0, left),
        );
      }),
      listen("live-transcription-reset", () => {
        finalAtRef.current = 0;
        typedAtRef.current = 0;
        setTarget("");
      }),
      listen<LiveTranscriptionChunk>("live-transcription-chunk", (event) => {
        if (event.payload.is_final) {
          instantRef.current = true;
          finalAtRef.current = Date.now();
        } else {
          // Until the typing effect works out the real time: a hide arriving
          // right behind this text still lets it be typed in.
          typedAtRef.current = Math.max(
            typedAtRef.current,
            Date.now() + TYPE_WITHIN_MS,
          );
        }
        setTarget(event.payload.text);
      }),
    ]);
    return () => {
      unlisteners.then((fns) => fns.forEach((unlisten) => unlisten()));
    };
  }, []);

  // Keep what is on screen as far as it still matches the new text, then type
  // in the rest.
  useEffect(() => {
    const current = shownRef.current;
    let typed = 0;
    while (
      typed < current.length &&
      typed < target.length &&
      current[typed] === target[typed]
    ) {
      typed++;
    }
    const put = (length: number) => {
      shownRef.current = target.slice(0, length);
      setShown(shownRef.current);
    };
    if (reducedMotion() || instantRef.current) {
      instantRef.current = false;
      typedAtRef.current = 0;
      put(target.length);
      return;
    }
    put(typed);
    if (typed >= target.length) return;
    const perChar = Math.min(
      MAX_CHAR_MS,
      Math.max(MIN_CHAR_MS, TYPE_WITHIN_MS / (target.length - typed)),
    );
    typedAtRef.current = Date.now() + perChar * (target.length - typed);
    const timer = setInterval(() => {
      typed++;
      put(typed);
      if (typed >= target.length) clearInterval(timer);
    }, perChar);
    return () => clearInterval(timer);
  }, [target]);

  // Fading: once the text has stopped changing for a while, let it go; the
  // next words bring the box back.
  useEffect(() => {
    setFaded(false);
    if (!fade || !shown || shown !== target) return;
    const timer = setTimeout(() => setFaded(true), FADE_AFTER_MS);
    return () => clearTimeout(timer);
  }, [fade, shown, target]);

  return (
    <div className={`live-text-frame ${belowPill ? "below" : "above"}`}>
      <div
        ref={boxRef}
        className={`live-text ${mode === "last_words" ? "one-line" : "full"} ${
          visible && shown ? "shown" : ""
        } ${faded ? "faded" : ""} ${overflowing ? "overflowing" : ""}`}
        style={{ "--lt-size": `${fontSize}px` } as React.CSSProperties}
      >
        <span>
          {shown}
          <span className="live-text-caret" aria-hidden />
        </span>
      </div>
    </div>
  );
};

export default LiveText;
