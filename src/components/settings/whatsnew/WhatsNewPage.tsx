import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { getVersion } from "@tauri-apps/api/app";
import { ArrowRight } from "lucide-react";
import { SettingsGroup, SectionTitle } from "../../ui/SettingsGroup";
import { Button } from "../../ui/Button";
import { useNavStore } from "../../../stores/navStore";
import { useOsType } from "../../../hooks/useOsType";
import { highlightSetting } from "../../../lib/settingsSearch";
import { useModelStore } from "../../../stores/modelStore";
import { getTranslatedModelName } from "../../../lib/utils/modelTranslation";
import type { SidebarSection } from "../../Sidebar";
import type { HistoryTab } from "../../../stores/navStore";

/** One new thing: its text (whatsNew.items.<key>) and where it lives. */
interface Item {
  key: string;
  section?: SidebarSection;
  /** The settings to outline there (their titles' i18n keys), as search does. */
  titleKeys?: string[];
  /** Outlined instead when the setting is hidden (its parent switch is off). */
  fallbackKey?: string;
  /** Outline the card of the model used for dictation. */
  dictationModel?: boolean;
  /** For History: the tab to open. */
  historyTab?: HistoryTab;
  /** Its setting exists on Windows only: no Show me elsewhere. */
  windowsOnly?: boolean;
}

// A live text box setting is only shown while the box is on: then its switch.
const LIVE_TEXT_BOX = "settings.general.liveTextBox.label";

/**
 * The last three GitHub releases, newest first: what each brought to someone
 * updating from the one before. The coming release carries everything since
 * 1.6.2 (1.6.3-1.13.0 were only local builds). Add the next release on top and
 * drop the oldest.
 */
const RELEASES: { version: string; items: Item[] }[] = [
  {
    version: "2.0.0", // everything since 1.6.2 (1.6.3-1.13.0 were local builds)
    items: [
      {
        key: "files",
        section: "files",
        titleKeys: ["settings.files.transcribe.choose.title"],
      },
      {
        key: "liveTextBox",
        section: "general",
        titleKeys: ["settings.general.liveTextBox.label"],
      },
      {
        key: "liveTranscript",
        section: "general",
        titleKeys: ["settings.general.liveTextBox.label"],
      },
      {
        key: "firstSetup",
        section: "setups",
        titleKeys: ["setup.catalog.basic.title"],
      },
      {
        key: "setups",
        section: "setups",
        titleKeys: [
          "setup.catalog.postProcessing.title",
          "setup.catalog.jumper.title",
        ],
      },
      {
        key: "appearanceSetup",
        section: "setups",
        titleKeys: ["setup.catalog.appearance.title"],
      },
      { key: "newLook" },
      { key: "newLayout" },
      { key: "newIcon" },
      { key: "helpMode", section: "general", titleKeys: ["helpMode.button"] },
      { key: "shortcutsPage", section: "shortcuts" },
      {
        key: "shortcutNone",
        section: "shortcuts",
        titleKeys: ["settings.general.shortcut.bindings.cancel.name"],
      },
      {
        key: "historyTabs",
        section: "history",
        titleKeys: ["settings.history.stats.title"],
        historyTab: "statistics",
      },
      {
        key: "pauseButton",
        section: "general",
        titleKeys: ["settings.general.pauseButton.label"],
      },
      {
        key: "holdUndo",
        section: "general",
        titleKeys: ["settings.general.undoWord.label"],
      },
      { key: "progressPercent" },
      { key: "micStatus" },
      {
        key: "micWarmup",
        section: "general",
        titleKeys: ["settings.sound.micWarmup.title"],
      },
      {
        key: "micKeepWarm",
        section: "general",
        titleKeys: ["settings.sound.micKeepWarm.title"],
      },
      {
        key: "tooQuiet",
        section: "general",
        titleKeys: ["settings.sound.tooQuiet.title"],
      },
      { key: "modelRatings", section: "models", dictationModel: true },
      {
        key: "liveTextAfterStop",
        section: "general",
        titleKeys: ["settings.general.liveTextAfterStop.label"],
      },
      {
        key: "liveTextSize",
        section: "general",
        titleKeys: ["settings.general.liveTextSize.title"],
        fallbackKey: LIVE_TEXT_BOX,
      },
      {
        key: "liveTextFade",
        section: "general",
        titleKeys: ["settings.general.liveTextFade.label"],
        fallbackKey: LIVE_TEXT_BOX,
      },
      {
        key: "progressStyle",
        section: "general",
        titleKeys: ["settings.advanced.progressStyle.title"],
      },
      {
        key: "progressColor",
        section: "general",
        titleKeys: ["settings.advanced.progressColor.title"],
      },
      {
        key: "overlaySize",
        section: "general",
        titleKeys: ["settings.advanced.overlaySize.title"],
      },
      { key: "pillMenus" },
      { key: "trayClick" },
      {
        key: "reopenLastPage",
        section: "general",
        titleKeys: ["settings.advanced.reopenLastPage.title"],
      },
    ],
  },
  {
    version: "1.6.2",
    items: [{ key: "blockedUpdate" }, { key: "macBuilds" }],
  },
  {
    version: "1.6.1",
    items: [
      {
        key: "soundSource",
        windowsOnly: true,
        section: "general",
        titleKeys: ["settings.advanced.captureSource.title"],
      },
      {
        key: "systemAudioControls",
        windowsOnly: true,
        section: "general",
        titleKeys: [
          "settings.advanced.systemAudioDelay.title",
          "settings.advanced.systemAudioGain.title",
        ],
        fallbackKey: "settings.advanced.captureSource.title",
      },
      { key: "linuxBuilds" },
    ],
  },
];

/** localStorage: the app version whose news were last opened (the sidebar's dot). */
export const WHATS_NEW_SEEN_KEY = "handy.whatsNewSeen";
export const WHATS_NEW_SEEN_EVENT = "handy:whats-new-seen";

/**
 * What's new (sidebar): the new and changed things of the last releases, each
 * with a "Show me" that opens its page and outlines the setting.
 */
export const WhatsNewPage: React.FC = () => {
  const { t } = useTranslation();
  const osType = useOsType();
  const navigateTo = useNavStore((s) => s.navigateTo);
  const models = useModelStore((s) => s.models);
  const currentModel = useModelStore((s) => s.currentModel);
  const [installed, setInstalled] = useState("");

  // Opened: the sidebar's dot goes until the next version.
  useEffect(() => {
    getVersion().then((version) => {
      setInstalled(version);
      try {
        localStorage.setItem(WHATS_NEW_SEEN_KEY, version);
      } catch {
        // No storage: the dot simply stays.
      }
      window.dispatchEvent(new Event(WHATS_NEW_SEEN_EVENT));
    });
  }, []);

  const show = (item: Item) => {
    if (!item.section) return;
    navigateTo(item.section, item.historyTab);
    const fallback = item.fallbackKey ? t(item.fallbackKey) : undefined;
    for (const key of item.titleKeys ?? [])
      highlightSetting(t(key), 0, fallback);
    if (item.dictationModel) {
      const model = models.find((m) => m.id === currentModel);
      if (model) highlightSetting(getTranslatedModelName(model, t));
    }
  };

  return (
    <div className="w-full space-y-6">
      <p className="text-sm text-text-secondary">{t("whatsNew.intro")}</p>
      {RELEASES.map((release) => (
        <div key={release.version} className="space-y-2">
          <div className="px-0.5 flex items-center gap-2">
            <SectionTitle
              title={t("whatsNew.release", { version: release.version })}
            />
            {release.version === installed && (
              <span className="inline-flex items-center h-5 px-1.5 rounded bg-accent-soft text-xs font-semibold text-accent-text">
                {t("whatsNew.installed")}
              </span>
            )}
          </div>
          <SettingsGroup>
            {release.items.map((item) => (
              <div
                key={item.key}
                className="flex items-center justify-between gap-4 px-4 py-3"
              >
                <div className="min-w-0">
                  <p className="text-sm font-semibold">
                    {t(`whatsNew.items.${item.key}.title`)}
                  </p>
                  <p className="text-[13px] leading-[18px] text-text-secondary">
                    {t(`whatsNew.items.${item.key}.description`)}
                  </p>
                </div>
                {item.section &&
                  (!item.windowsOnly || osType === "windows") && (
                    <Button
                      variant="secondary"
                      size="sm"
                      className="shrink-0"
                      onClick={() => show(item)}
                    >
                      {t("whatsNew.show")}
                      <ArrowRight
                        className="w-3.5 h-3.5 rtl:-scale-x-100"
                        aria-hidden
                      />
                    </Button>
                  )}
              </div>
            ))}
          </SettingsGroup>
        </div>
      ))}
    </div>
  );
};
