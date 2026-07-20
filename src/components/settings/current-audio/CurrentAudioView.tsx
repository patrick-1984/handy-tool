import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import { commands } from "@/bindings";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { ExternalLink, Copy, Check } from "lucide-react";

interface LiveTranscriptionChunk {
  index: number;
  text: string;
  is_final: boolean;
}

export const CurrentAudioView: React.FC = () => {
  const { t } = useTranslation();
  const [chunks, setChunks] = useState<string[]>([]);
  const [isRecording, setIsRecording] = useState(false);
  const [copied, setCopied] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  const handleCopy = useCallback(async () => {
    const text = chunks.join(" ").trim();
    if (!text) return;
    try {
      await writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (e) {
      console.warn("Failed to copy transcript:", e);
    }
  }, [chunks]);

  const scrollToBottom = useCallback(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, []);

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
    <div className="w-full space-y-6">
      <SettingsGroup title={t("settings.currentAudio.title")}>
        <div className="p-4">
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-2">
              {isRecording && (
                <span className="relative flex h-2 w-2">
                  <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-red-400 opacity-75" />
                  <span className="relative inline-flex rounded-full h-2 w-2 bg-red-500" />
                </span>
              )}
              <span className="text-xs text-mid-gray uppercase tracking-wide">
                {isRecording
                  ? t("settings.currentAudio.recording")
                  : hasText
                    ? ""
                    : ""}
              </span>
            </div>
            <div className="flex items-center gap-3">
              {hasText && (
                <button
                  onClick={handleCopy}
                  className="flex items-center gap-1 text-xs text-mid-gray hover:text-white transition-colors cursor-pointer"
                  title={
                    copied
                      ? t("settings.currentAudio.copied")
                      : t("settings.currentAudio.copy")
                  }
                >
                  {copied ? <Check size={14} /> : <Copy size={14} />}
                  {copied
                    ? t("settings.currentAudio.copied")
                    : t("settings.currentAudio.copy")}
                </button>
              )}
              <button
                onClick={() => commands.openFloatingTranscription()}
                className="flex items-center gap-1 text-xs text-mid-gray hover:text-white transition-colors cursor-pointer"
                title={t("settings.currentAudio.openFloating")}
              >
                <ExternalLink size={14} />
                {t("settings.currentAudio.openFloating")}
              </button>
            </div>
          </div>

          <div
            ref={scrollRef}
            className="min-h-[200px] max-h-[400px] overflow-y-auto rounded-lg bg-black/20 p-4"
          >
            {!hasText && !isRecording && (
              <p className="text-sm text-mid-gray italic">
                {t("settings.currentAudio.idle")}
              </p>
            )}
            {isRecording && !hasText && (
              <p className="text-sm text-mid-gray italic">
                {t("settings.currentAudio.recording")}
              </p>
            )}
            {hasText && (
              <p className="text-sm leading-relaxed whitespace-pre-wrap">
                {chunks.join(" ")}
              </p>
            )}
          </div>
        </div>
      </SettingsGroup>
    </div>
  );
};
