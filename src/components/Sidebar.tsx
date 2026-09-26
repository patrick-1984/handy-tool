import React, { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Archive,
  AudioLines,
  Bot,
  ClipboardPaste,
  Cog,
  Command,
  Crosshair,
  Ellipsis,
  FlaskConical,
  Hash,
  History,
  Info,
  Keyboard,
  Languages,
  Plug,
  Sparkles,
  Terminal,
  Cpu,
} from "lucide-react";
import HandyTextLogo from "./icons/HandyTextLogo";
import HandyHand from "./icons/HandyHand";
import { UpdateBanner } from "./UpdateBanner";
import { SidebarSearch } from "./SidebarSearch";
import { useSettings } from "../hooks/useSettings";
import { useNavStore } from "../stores/navStore";
import {
  GeneralSettings,
  ShortcutsSettings,
  AppSection,
  OutputSection,
  ProvidersSection,
  McpSection,
  PostProcessingSection,
  CurrentAudioView,
  HistorySettings,
  DebugSettings,
  AboutSettings,
  BackupSettings,
  ModelsSettings,
  TokenCountPage,
  KeyboardTyperPage,
  ModelTestingPage,
  JumperSettings,
  TranslatorSettings,
} from "./settings";

export type SidebarSection = keyof typeof SECTIONS_CONFIG;
/** The sidebar itself, or one of the More page's two tab rows. */
export type SectionPlacement = "sidebar" | "more-settings" | "more-tools";

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
  placement: SectionPlacement;
  enabled: (settings: any) => boolean;
}

// Order within this object is the order in the sidebar and in each More row.
export const SECTIONS_CONFIG = {
  // --- Sidebar ---
  general: {
    labelKey: "sidebar.general",
    icon: HandyHand,
    component: GeneralSettings,
    placement: "sidebar",
    enabled: () => true,
  },
  shortcuts: {
    labelKey: "sidebar.shortcuts",
    icon: Command,
    component: ShortcutsSettings,
    placement: "sidebar",
    enabled: () => true,
  },
  models: {
    labelKey: "sidebar.models",
    icon: Cpu,
    component: ModelsSettings,
    placement: "sidebar",
    enabled: () => true,
  },
  history: {
    labelKey: "sidebar.history",
    icon: History,
    component: HistorySettings,
    placement: "sidebar",
    enabled: () => true,
  },
  jumper: {
    labelKey: "sidebar.jumper",
    icon: Crosshair,
    component: JumperSettings,
    placement: "sidebar",
    enabled: () => true,
  },
  keyboardTyper: {
    labelKey: "sidebar.keyboardTyper",
    icon: Keyboard,
    component: KeyboardTyperPage,
    placement: "sidebar",
    enabled: () => true,
  },
  // --- More › Settings ---
  app: {
    labelKey: "settings.advanced.tabs.app",
    icon: Cog,
    component: AppSection,
    placement: "more-settings",
    enabled: () => true,
  },
  output: {
    labelKey: "settings.advanced.tabs.output",
    icon: ClipboardPaste,
    component: OutputSection,
    placement: "more-settings",
    enabled: () => true,
  },
  providers: {
    labelKey: "settings.advanced.tabs.providers",
    icon: Plug,
    component: ProvidersSection,
    placement: "more-settings",
    enabled: () => true,
  },
  postprocessing: {
    labelKey: "settings.advanced.tabs.postProcessing",
    icon: Sparkles,
    component: PostProcessingSection,
    placement: "more-settings",
    enabled: () => true,
  },
  mcp: {
    labelKey: "settings.advanced.tabs.mcp",
    icon: Terminal,
    component: McpSection,
    placement: "more-settings",
    enabled: () => true,
  },
  backup: {
    labelKey: "sidebar.backup",
    icon: Archive,
    component: BackupSettings,
    placement: "more-settings",
    enabled: () => true,
  },
  debug: {
    labelKey: "sidebar.debug",
    icon: FlaskConical,
    component: DebugSettings,
    placement: "more-settings",
    enabled: (settings) => settings?.debug_mode ?? false,
  },
  about: {
    labelKey: "sidebar.about",
    icon: Info,
    component: AboutSettings,
    placement: "more-settings",
    enabled: () => true,
  },
  // --- More › Tools ---
  translator: {
    labelKey: "sidebar.translator",
    icon: Languages,
    component: TranslatorSettings,
    placement: "more-tools",
    enabled: () => true,
  },
  tokenCount: {
    labelKey: "sidebar.tokenCount",
    icon: Hash,
    component: TokenCountPage,
    placement: "more-tools",
    enabled: () => true,
  },
  modelTesting: {
    labelKey: "sidebar.modelTesting",
    icon: Bot,
    component: ModelTestingPage,
    placement: "more-tools",
    enabled: () => true,
  },
  currentAudio: {
    labelKey: "sidebar.currentAudio",
    icon: AudioLines,
    component: CurrentAudioView,
    placement: "more-tools",
    enabled: () => true,
  },
} as const satisfies Record<string, SectionConfig>;

export const isMoreSection = (section: SidebarSection) =>
  SECTIONS_CONFIG[section].placement !== "sidebar";

interface SidebarProps {
  activeSection: SidebarSection;
  onSectionChange: (section: SidebarSection) => void;
}

const MIN_SIDEBAR_WIDTH = 140;
const MAX_SIDEBAR_WIDTH = 420;
const DEFAULT_SIDEBAR_WIDTH = 176;
// While searching the sidebar widens so result names and their locations are not
// cut off; it returns to the saved width once a result is picked or the box cleared.
const SEARCH_SIDEBAR_WIDTH = 320;
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
  const lastMoreSection = useNavStore((state) => state.lastMoreSection);
  const [width, setWidth] = useState<number>(loadSidebarWidth);
  const [searchQuery, setSearchQuery] = useState("");

  const availableSections = Object.entries(SECTIONS_CONFIG)
    .filter(([_, config]) => config.enabled(settings))
    .map(([id, config]) => ({ id: id as SidebarSection, ...config }));
  const sidebarSections = availableSections.filter(
    (s) => s.placement === "sidebar",
  );
  // Stable identity for the search index: rebuild only when the set of shown
  // sections changes, not on every render.
  const sectionIds = availableSections.map((s) => s.id).join(",");
  const searchSections = useMemo(
    () =>
      availableSections.map(({ id, labelKey, placement }) => ({
        id,
        labelKey,
        more: placement !== "sidebar",
      })),
    [sectionIds],
  );
  // More reopens the tab last used there, unless it has been hidden since.
  const moreTarget = availableSections.some((s) => s.id === lastMoreSection)
    ? lastMoreSection
    : "app";

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

  const renderItem = (
    key: string,
    Icon: React.ComponentType<IconProps>,
    label: string,
    isActive: boolean,
    onClick: () => void,
  ) => (
    <div
      key={key}
      className={`flex gap-2 items-center p-2 w-full rounded-lg cursor-pointer transition-colors ${
        isActive
          ? "bg-logo-primary/80"
          : "hover:bg-mid-gray/20 hover:opacity-100 opacity-85"
      }`}
      onClick={onClick}
    >
      <Icon width={24} height={24} className="shrink-0" />
      <p className="text-sm font-medium truncate" title={label}>
        {label}
      </p>
    </div>
  );

  return (
    <div
      className="relative flex flex-col h-full shrink-0 border-e border-mid-gray/20 items-center px-2 overflow-y-auto"
      style={{
        width: searchQuery.trim()
          ? Math.max(width, SEARCH_SIDEBAR_WIDTH)
          : width,
      }}
    >
      <HandyTextLogo width={120} className="m-4 shrink-0" />
      <div className="flex flex-col w-full gap-3 pt-2 border-t border-mid-gray/20">
        <SidebarSearch
          query={searchQuery}
          onQueryChange={setSearchQuery}
          sections={searchSections}
        />
        {!searchQuery.trim() && (
          <div className="flex flex-col w-full gap-1">
            {sidebarSections.map((section) =>
              renderItem(
                section.id,
                section.icon,
                t(section.labelKey),
                activeSection === section.id,
                () => onSectionChange(section.id),
              ),
            )}
            {renderItem(
              "more",
              Ellipsis,
              t("sidebar.more"),
              isMoreSection(activeSection),
              () => onSectionChange(moreTarget),
            )}
            <UpdateBanner />
          </div>
        )}
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
