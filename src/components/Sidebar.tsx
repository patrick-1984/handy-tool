import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Archive,
  AudioLines,
  Bot,
  Cog,
  Crosshair,
  FlaskConical,
  Hash,
  History,
  Info,
  Keyboard,
  Languages,
  Sparkles,
  Cpu,
} from "lucide-react";
import HandyTextLogo from "./icons/HandyTextLogo";
import HandyHand from "./icons/HandyHand";
import { UpdateBanner } from "./UpdateBanner";
import { useSettings } from "../hooks/useSettings";
import {
  GeneralSettings,
  AdvancedSettings,
  CurrentAudioView,
  HistorySettings,
  DebugSettings,
  AboutSettings,
  BackupSettings,
  PostProcessingSettings,
  ModelsSettings,
  TokenCountPage,
  KeyboardTyperPage,
  ModelTestingPage,
  JumperSettings,
  TranslatorSettings,
} from "./settings";

export type SidebarSection = keyof typeof SECTIONS_CONFIG;
export type SidebarGroup = "tools" | "config";

interface IconProps {
  width?: number | string;
  height?: number | string;
  size?: number | string;
  className?: string;
  [key: string]: any;
}

interface SectionConfig {
  labelKey: string;
  icon: React.ComponentType<IconProps>;
  component: React.ComponentType;
  group: SidebarGroup;
  enabled: (settings: any) => boolean;
}

// Order within this object controls the order within each sidebar group.
export const SECTIONS_CONFIG = {
  // --- Configuration ---
  general: {
    labelKey: "sidebar.general",
    icon: HandyHand,
    component: GeneralSettings,
    group: "config",
    enabled: () => true,
  },
  models: {
    labelKey: "sidebar.models",
    icon: Cpu,
    component: ModelsSettings,
    group: "config",
    enabled: () => true,
  },
  advanced: {
    labelKey: "sidebar.advanced",
    icon: Cog,
    component: AdvancedSettings,
    group: "config",
    enabled: () => true,
  },
  backup: {
    labelKey: "sidebar.backup",
    icon: Archive,
    component: BackupSettings,
    group: "config",
    enabled: () => true,
  },
  postprocessing: {
    labelKey: "sidebar.postProcessing",
    icon: Sparkles,
    component: PostProcessingSettings,
    group: "config",
    enabled: (settings) => settings?.post_process_enabled ?? false,
  },
  debug: {
    labelKey: "sidebar.debug",
    icon: FlaskConical,
    component: DebugSettings,
    group: "config",
    enabled: (settings) => settings?.debug_mode ?? false,
  },
  about: {
    labelKey: "sidebar.about",
    icon: Info,
    component: AboutSettings,
    group: "config",
    enabled: () => true,
  },
  // --- Tools --- (order here = order shown in the Tools group)
  history: {
    labelKey: "sidebar.history",
    icon: History,
    component: HistorySettings,
    group: "tools",
    enabled: () => true,
  },
  modelTesting: {
    labelKey: "sidebar.modelTesting",
    icon: Bot,
    component: ModelTestingPage,
    group: "tools",
    enabled: () => true,
  },
  keyboardTyper: {
    labelKey: "sidebar.keyboardTyper",
    icon: Keyboard,
    component: KeyboardTyperPage,
    group: "tools",
    enabled: () => true,
  },
  tokenCount: {
    labelKey: "sidebar.tokenCount",
    icon: Hash,
    component: TokenCountPage,
    group: "tools",
    enabled: () => true,
  },
  jumper: {
    labelKey: "sidebar.jumper",
    icon: Crosshair,
    component: JumperSettings,
    group: "tools",
    enabled: () => true,
  },
  translator: {
    labelKey: "sidebar.translator",
    icon: Languages,
    component: TranslatorSettings,
    group: "tools",
    enabled: () => true,
  },
  currentAudio: {
    labelKey: "sidebar.currentAudio",
    icon: AudioLines,
    component: CurrentAudioView,
    group: "tools",
    enabled: () => true,
  },
} as const satisfies Record<string, SectionConfig>;

// Tools first (the primary menu), Configuration below.
const GROUP_ORDER: { id: SidebarGroup; labelKey: string }[] = [
  { id: "tools", labelKey: "sidebar.groups.tools" },
  { id: "config", labelKey: "sidebar.groups.config" },
];

interface SidebarProps {
  activeSection: SidebarSection;
  onSectionChange: (section: SidebarSection) => void;
}

const MIN_SIDEBAR_WIDTH = 140;
const MAX_SIDEBAR_WIDTH = 420;
const DEFAULT_SIDEBAR_WIDTH = 176;
const SIDEBAR_WIDTH_KEY = "handy.sidebarWidth";

function loadSidebarWidth(): number {
  const saved = Number(localStorage.getItem(SIDEBAR_WIDTH_KEY));
  return Number.isFinite(saved) &&
    saved >= MIN_SIDEBAR_WIDTH &&
    saved <= MAX_SIDEBAR_WIDTH
    ? saved
    : DEFAULT_SIDEBAR_WIDTH;
}

export const Sidebar: React.FC<SidebarProps> = ({
  activeSection,
  onSectionChange,
}) => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const [width, setWidth] = useState<number>(loadSidebarWidth);

  const availableSections = Object.entries(SECTIONS_CONFIG)
    .filter(([_, config]) => config.enabled(settings))
    .map(([id, config]) => ({ id: id as SidebarSection, ...config }));

  // Drag the right edge to resize; persist the width to localStorage on release.
  const startResize = (e: React.MouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = width;
    const onMove = (ev: MouseEvent) => {
      const next = Math.min(
        MAX_SIDEBAR_WIDTH,
        Math.max(MIN_SIDEBAR_WIDTH, startW + (ev.clientX - startX)),
      );
      setWidth(next);
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      setWidth((w) => {
        localStorage.setItem(SIDEBAR_WIDTH_KEY, String(w));
        return w;
      });
    };
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  return (
    <div
      className="relative flex flex-col h-full shrink-0 border-e border-mid-gray/20 items-center px-2 overflow-y-auto"
      style={{ width }}
    >
      <HandyTextLogo width={120} className="m-4 shrink-0" />
      <div className="flex flex-col w-full gap-3 pt-2 border-t border-mid-gray/20">
        {GROUP_ORDER.map((group) => {
          const items = availableSections.filter((s) => s.group === group.id);
          if (items.length === 0) return null;

          return (
            <div key={group.id} className="flex flex-col w-full gap-1">
              <p className="text-[10px] font-semibold uppercase tracking-wider text-text/40 px-2 pt-1">
                {t(group.labelKey)}
              </p>
              {items.map((section) => {
                const Icon = section.icon;
                const isActive = activeSection === section.id;

                return (
                  <React.Fragment key={section.id}>
                  <div
                    className={`flex gap-2 items-center p-2 w-full rounded-lg cursor-pointer transition-colors ${
                      isActive
                        ? "bg-logo-primary/80"
                        : "hover:bg-mid-gray/20 hover:opacity-100 opacity-85"
                    }`}
                    onClick={() => onSectionChange(section.id)}
                  >
                    <Icon width={24} height={24} className="shrink-0" />
                    <p
                      className="text-sm font-medium truncate"
                      title={t(section.labelKey)}
                    >
                      {t(section.labelKey)}
                    </p>
                  </div>
                  {section.id === "about" && <UpdateBanner />}
                  </React.Fragment>
                );
              })}
            </div>
          );
        })}
      </div>
      {/* Drag handle: resize the sidebar; width persists across launches. */}
      <div
        onMouseDown={startResize}
        title={t("sidebar.resize")}
        className="absolute top-0 right-0 h-full w-1.5 cursor-col-resize hover:bg-logo-primary/40 active:bg-logo-primary/60 transition-colors"
      />
    </div>
  );
};
