import { listen } from "@tauri-apps/api/event";
import { Volume1 } from "lucide-react";
import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import i18n, { syncLanguageFromSettings } from "@/i18n";
import { commands } from "@/bindings";
import { getLanguageDirection } from "@/lib/utils/rtl";
import "./QuietHint.css";

/**
 * "Too quiet — speak up" in a small box of its own just under the recording
 * pill, so the pill keeps its sound bars and the text is never cut off.
 */
const QuietHint: React.FC = () => {
  const { t } = useTranslation();
  const [shown, setShown] = useState(false);

  useEffect(() => {
    const unlisten = listen<boolean>("quiet-hint", async (event) => {
      if (event.payload) {
        await syncLanguageFromSettings();
        // Overlay Size: the window is already that size; the page follows.
        const settings = await commands.getAppSettings();
        if (settings.status === "ok") {
          document.documentElement.style.zoom = String(
            (settings.data.pill_scale ?? 100) / 100,
          );
        }
      }
      setShown(event.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  return (
    <div className="quiet-hint-frame" dir={getLanguageDirection(i18n.language)}>
      <div className={`quiet-hint ${shown ? "shown" : ""}`} role="status">
        <Volume1 size={13} aria-hidden />
        {t("overlay.tooQuiet")}
      </div>
    </div>
  );
};

export default QuietHint;
