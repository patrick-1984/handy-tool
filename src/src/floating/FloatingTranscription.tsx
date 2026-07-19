import { listen } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import "./FloatingTranscription.css";

interface LiveTranscriptionChunk {
  index: number;
  text: string;
  is_final: boolean;
}

const FloatingTranscription: React.FC = () => {
  const { t } = useTranslation();
  const [chunks, setChunks] = useState<string[]>([]);
  const [isRecording, setIsRecording] = useState(false);
  const [copied, setCopied] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  const scrollToBottom = useCallback(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, []);

  const handleCopy = useCallback(async () => {
    const text = chunks.join(" ");
    if (text) {
      await writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  }, [chunks]);

  useEffect(() => {
    scrollToBottom();
  }, [chunks, scrollToBottom]);

  useEffect(() => {
    const setupListeners = async () => {
      const unlistenReset = await listen("live-transcription-reset", () => {
        setChunks([]);
        setIsRecording(true);
      });

      const unlistenChunk = await listen<LiveTranscriptionChunk>(
        "live-transcription-chunk",
        (event) => {
          const payload = event.payload;
          if (payload.is_final) {
            setChunks([payload.text]);
            setIsRecording(false);
          } else {
            // Replace with latest full-context transcription (not append)
            setChunks([payload.text]);
          }
        },
      );

      return () => {
        unlistenReset();
        unlistenChunk();
      };
    };

    let cleanup: (() => void) | undefined;
    setupListeners().then((fn) => {
      cleanup = fn;
    });

    return () => {
      cleanup?.();
    };
  }, []);

  const hasText = chunks.length > 0;

  return (
    <div className="floating-root" ref={scrollRef}>
      {!hasText && !isRecording && (
        <p className="floating-idle">{t("floating.waiting")}</p>
      )}
      {isRecording && !hasText && (
        <p className="floating-listening">
          <span className="recording-dot" />
          {t("floating.listening")}
        </p>
      )}
      {hasText && <p className="floating-text">{chunks.join(" ")}</p>}
      {hasText && (
        <button
          type="button"
          className="floating-copy-button"
          onClick={handleCopy}
        >
          {copied ? t("floating.copied") : t("floating.copy")}
        </button>
      )}
    </div>
  );
};

export default FloatingTranscription;
