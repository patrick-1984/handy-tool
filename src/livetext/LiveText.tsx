import { listen } from "@tauri-apps/api/event";
import React, { useEffect, useRef, useState } from "react";
import "./LiveText.css";

type LiveTextMode = "last_words" | "full_text";

interface LiveTextShow {
  mode: LiveTextMode;
  fade: boolean;
  below_pill: boolean;
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
/** With fading on: how long the text stays once it stops changing. */
const FADE_AFTER_MS = 3000;

const reducedMotion = () =>
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;

/**
 * The live text box next to the recording pill. Its window never changes size
 * (so nothing jumps): "last words" is one line whose newest words stay at the
 * edge nearest the text's end, "whole text" is a three-line box anchored at the
 * bottom whose older lines slide out of the top. New words are typed in rather
 * than appearing at once, so the eye can follow them; removed words (undo, or
 * the preview correcting itself) go at once.
 */
const LiveText: React.FC = () => {
  const [visible, setVisible] = useState(false);
  const [mode, setMode] = useState<LiveTextMode>("last_words");
  const [fade, setFade] = useState(false);
  const [belowPill, setBelowPill] = useState(false);
  // The latest text from the preview, and how much of it is on screen so far.
  const [target, setTarget] = useState("");
  const [shown, setShown] = useState("");
  const shownRef = useRef("");
  const [faded, setFaded] = useState(false);

  useEffect(() => {
    const unlisteners = Promise.all([
      listen<LiveTextShow>("live-text-show", (event) => {
        setMode(event.payload.mode);
        setFade(event.payload.fade);
        setBelowPill(event.payload.below_pill);
        setVisible(true);
      }),
      listen("live-text-hide", () => setVisible(false)),
      listen("live-transcription-reset", () => setTarget("")),
      listen<LiveTranscriptionChunk>("live-transcription-chunk", (event) =>
        setTarget(event.payload.text),
      ),
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
    if (reducedMotion()) {
      put(target.length);
      return;
    }
    put(typed);
    if (typed >= target.length) return;
    const perChar = Math.min(
      MAX_CHAR_MS,
      Math.max(MIN_CHAR_MS, TYPE_WITHIN_MS / (target.length - typed)),
    );
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
        className={`live-text ${mode === "last_words" ? "one-line" : "full"} ${
          visible && shown ? "shown" : ""
        } ${faded ? "faded" : ""}`}
      >
        <span>{shown}</span>
      </div>
    </div>
  );
};

export default LiveText;
