import { listen } from "@tauri-apps/api/event";
import React, { useEffect, useState } from "react";
import "./LiveText.css";

type LiveTextMode = "last_words" | "full_text";

interface LiveTextShow {
  mode: LiveTextMode;
  below_pill: boolean;
}

interface LiveTranscriptionChunk {
  index: number;
  text: string;
  is_final: boolean;
}

/**
 * The live text box next to the recording pill. Its window never changes size
 * (so nothing jumps): "last words" is one line whose newest words stay at the
 * edge nearest the text's end, "whole text" is a three-line box anchored at the
 * bottom whose older lines slide out of the top.
 */
const LiveText: React.FC = () => {
  const [visible, setVisible] = useState(false);
  const [mode, setMode] = useState<LiveTextMode>("last_words");
  const [belowPill, setBelowPill] = useState(false);
  const [text, setText] = useState("");

  useEffect(() => {
    const unlisteners = Promise.all([
      listen<LiveTextShow>("live-text-show", (event) => {
        setMode(event.payload.mode);
        setBelowPill(event.payload.below_pill);
        setVisible(true);
      }),
      listen("live-text-hide", () => setVisible(false)),
      listen("live-transcription-reset", () => setText("")),
      listen<LiveTranscriptionChunk>("live-transcription-chunk", (event) =>
        setText(event.payload.text),
      ),
    ]);
    return () => {
      unlisteners.then((fns) => fns.forEach((unlisten) => unlisten()));
    };
  }, []);

  return (
    <div className={`live-text-frame ${belowPill ? "below" : "above"}`}>
      <div
        className={`live-text ${mode === "last_words" ? "one-line" : "full"} ${
          visible && text ? "shown" : ""
        }`}
      >
        <span>{text}</span>
      </div>
    </div>
  );
};

export default LiveText;
