import React, { useEffect, useMemo, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { useTranslation } from "react-i18next";
import {
  Archive,
  AudioLines,
  Bot,
  BrainCircuit,
  Captions,
  CircleArrowUp,
  FileAudio,
  Gift,
  FlaskConical,
  Hash,
  History,
  Info,
  Keyboard,
  ListChecks,
  MoveUpRight,
  Plug,
  Settings as Gear,
  SlidersHorizontal,
  Sparkles,
  Terminal,
  ToolCase,
  Type,
  Cpu,
} from "lucide-react";
import AppIcon from "./icons/AppIcon";
import { SetupsPage } from "./settings/setups/SetupsPage";
import {
  WhatsNewPage,
  WHATS_NEW_SEEN_EVENT,
  WHATS_NEW_SEEN_KEY,
} from "./settings/whatsnew/WhatsNewPage";
import { UpdateBanner } from "./UpdateBanner";
import { UpdatePanel } from "./UpdatePanel";
import { useUpdaterStatus } from "../hooks/useUpdaterStatus";
import { SidebarSearch } from "./SidebarSearch";
import { useSettings } from "../hooks/useSettings";
import { useNavStore } from "../stores/navStore";
import {
  GeneralSettings,
  ShortcutsSettings,
  OutputSection,
  ProvidersSection,
  LlmProvidersSection,
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
  FilesPage,
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
  // First, with a divider under it: the takes you made.
  history: {
    labelKey: "sidebar.history",
    icon: History,
    component: HistorySettings,
    placement: "sidebar",
    enabled: () => true,
  },
  general: {
    labelKey: "sidebar.general",
    icon: SlidersHorizontal,
    component: GeneralSettings,
    placement: "sidebar",
    enabled: () => true,
  },
  setups: {
    labelKey: "sidebar.setups",
    icon: ListChecks,
    component: SetupsPage,
    placement: "sidebar",
    enabled: () => true,
  },
  // How the transcription is delivered (paste, clipboard, submit, jumps). It was
  // More › Output before 2.0.1; the id stays "output" so saved pages, search and
  // What's new keep pointing at it.
  output: {
    labelKey: "sidebar.transcription",
    icon: Captions,
    component: OutputSection,
    placement: "sidebar",
    enabled: () => true,
  },
  shortcuts: {
    labelKey: "sidebar.shortcuts",
    icon: Keyboard,
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
  files: {
    labelKey: "sidebar.files",
    icon: FileAudio,
    component: FilesPage,
    placement: "sidebar",
    enabled: () => true,
  },
  jumper: {
    labelKey: "sidebar.jumper",
    icon: MoveUpRight,
    component: JumperSettings,
    placement: "sidebar",
    enabled: () => true,
  },
  whatsNew: {
    labelKey: "sidebar.whatsNew",
    icon: Gift,
    component: WhatsNewPage,
    placement: "sidebar",
    enabled: () => true,
  },
  // Version, the update controls and credits: a sidebar page since 2.0.2 (it was
  // a tab of Advanced settings).
  about: {
    labelKey: "sidebar.about",
    icon: Info,
    component: AboutSettings,
    placement: "sidebar",
    enabled: () => true,
  },
  // --- Advanced settings (the sidebar's More entry before 2.0.1) ---
  // Transcription providers (the id stays "providers"), then the LLM providers
  // that were a group on the same page before 2.0.1.
  providers: {
    labelKey: "sidebar.transcriptionProviders",
    icon: Plug,
    component: ProvidersSection,
    placement: "more-settings",
    enabled: () => true,
  },
  llmProviders: {
    labelKey: "sidebar.llmProviders",
    icon: BrainCircuit,
    component: LlmProvidersSection,
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
  // --- More Tools (its own sidebar entry, after Jumper) ---
  keyboardTyper: {
    labelKey: "sidebar.keyboardTyper",
    icon: Type,
    component: KeyboardTyperPage,
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

/** The sidebar entries that hold several pages as tabs (see MorePage). */
export const MORE_GROUPS = {
  "more-settings": { labelKey: "sidebar.advancedSettings", icon: Gear },
  "more-tools": { labelKey: "sidebar.moreTools", icon: ToolCase },
} as const;
export type MoreGroup = keyof typeof MORE_GROUPS;

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
  const lastToolsSection = useNavStore((state) => state.lastToolsSection);
  const [width, setWidth] = useState<number>(loadSidebarWidth);
  const [searchQuery, setSearchQuery] = useState("");
  // What's new carries a dot until this version's news were opened.
  const [newsUnseen, setNewsUnseen] = useState(false);
  // Shown at the bottom-left of the sidebar.
  const [appVersion, setAppVersion] = useState("");
  useEffect(() => {
    const check = () =>
      getVersion().then((version) => {
        setAppVersion(version);
        let seen: string | null = null;
        try {
          seen = localStorage.getItem(WHATS_NEW_SEEN_KEY);
        } catch {
          // No storage: no dot.
          seen = version;
        }
        setNewsUnseen(seen !== version);
      });
    void check();
    window.addEventListener(WHATS_NEW_SEEN_EVENT, check);
    return () => window.removeEventListener(WHATS_NEW_SEEN_EVENT, check);
  }, []);
  // The version opens a panel with the update status; an arrow beside it shows
  // a newer version is known (only while update checks are on).
  const [updaterStatus, refreshUpdaterStatus] = useUpdaterStatus();
  const [updatePanelOpen, setUpdatePanelOpen] = useState(false);
  const versionButton = useRef<HTMLButtonElement>(null);
  const closeUpdatePanel = () => {
    setUpdatePanelOpen(false);
    versionButton.current?.focus();
  };
  // Escape closes the panel even when the focused button has just gone (an
  // update check replaces it), and gives focus back to the version.
  useEffect(() => {
    if (!updatePanelOpen) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setUpdatePanelOpen(false);
        versionButton.current?.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [updatePanelOpen]);
  const updateIndicator =
    (settings?.automatic_update_checks ?? true) &&
    ["available", "downloading", "ready_to_restart"].includes(
      updaterStatus?.state ?? "",
    );

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
        group: placement === "sidebar" ? null : MORE_GROUPS[placement].labelKey,
      })),
    [sectionIds],
  );

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
    dot = false,
  ) => (
    <div
      key={key}
      className={`relative flex gap-3 items-center h-8 px-2.5 w-full rounded-md cursor-pointer transition-colors duration-150 ${
        isActive
          ? "bg-active font-semibold before:content-[''] before:absolute before:start-0 before:top-2 before:bottom-2 before:w-[3px] before:rounded-full before:bg-accent"
          : "hover:bg-hover"
      }`}
      onClick={onClick}
    >
      <Icon width={16} height={16} className="shrink-0" />
      <p className="text-sm truncate" title={label}>
        {label}
      </p>
      {dot && (
        <>
          <span
            className="ms-auto h-2 w-2 shrink-0 rounded-full bg-accent"
            aria-hidden
          />
          {/* Read out as "What's new, unread". */}
          <span className="sr-only">{t("sidebar.unread")}</span>
        </>
      )}
    </div>
  );

  // Advanced settings and More Tools: each opens its pages as tabs (MorePage)
  // and reopens the tab last used there, unless that one is hidden now.
  const renderGroup = (group: MoreGroup) => {
    const pages = availableSections.filter((s) => s.placement === group);
    const last = group === "more-tools" ? lastToolsSection : lastMoreSection;
    const target = pages.some((s) => s.id === last) ? last : pages[0]?.id;
    if (!target) return null;
    const { icon, labelKey } = MORE_GROUPS[group];
    return renderItem(
      group,
      icon,
      t(labelKey),
      SECTIONS_CONFIG[activeSection].placement === group,
      () => onSectionChange(target),
    );
  };

  return (
    <div
      className="relative flex flex-col h-full shrink-0 bg-sidebar border-e border-border items-center px-2 overflow-y-auto overflow-x-hidden"
      style={{
        width: searchQuery.trim()
          ? Math.max(width, SEARCH_SIDEBAR_WIDTH)
          : width,
      }}
    >
      <div className="flex items-center gap-2 w-full px-2 pt-4 pb-3 shrink-0">
        <AppIcon className="w-5 h-5 shrink-0" />
        {/* The product name is not translated. */}
        {/* eslint-disable-next-line i18next/no-literal-string */}
        <span className="text-sm font-semibold truncate">Handy Tool</span>
      </div>
      <div className="flex flex-col w-full gap-3">
        <SidebarSearch
          query={searchQuery}
          onQueryChange={setSearchQuery}
          sections={searchSections}
        />
        {/* Hidden, not unmounted, while searching: the update banner keeps its
            "Remind me later" (it lives in the banner's own state). */}
        <div
          className={`${searchQuery.trim() ? "hidden" : "flex"} flex-col w-full gap-1`}
        >
          {sidebarSections.map((section) => (
            <React.Fragment key={section.id}>
              {renderItem(
                section.id,
                section.icon,
                t(section.labelKey),
                activeSection === section.id,
                () => onSectionChange(section.id),
                section.id === "whatsNew" && newsUnseen,
              )}
              {section.id === "history" && (
                <div className="mx-2 my-1 border-t border-border" />
              )}
              {section.id === "jumper" && renderGroup("more-tools")}
            </React.Fragment>
          ))}
          <div className="mx-2 my-1 border-t border-border" />
          {renderGroup("more-settings")}
          <UpdateBanner suppressStatus={updatePanelOpen} />
        </div>
      </div>
      {appVersion && (
        <div className="mt-auto w-full shrink-0 px-2.5 pt-3 pb-2 text-[11px] leading-4 text-text-secondary tabular-nums">
          {updatePanelOpen && (
            <UpdatePanel
              id="sidebar-update-panel"
              status={updaterStatus}
              onClose={closeUpdatePanel}
            />
          )}
          <button
            ref={versionButton}
            type="button"
            aria-expanded={updatePanelOpen}
            aria-controls="sidebar-update-panel"
            onClick={() => {
              if (updatePanelOpen) {
                setUpdatePanelOpen(false);
              } else {
                refreshUpdaterStatus();
                setUpdatePanelOpen(true);
              }
            }}
            className="inline-flex items-center gap-1 rounded hover:text-text cursor-pointer"
          >
            {/* eslint-disable-next-line i18next/no-literal-string */}
            <span>v{appVersion}</span>
            {updateIndicator && (
              <>
                <CircleArrowUp
                  className="h-3 w-3 text-accent-text"
                  aria-hidden
                />
                <span className="sr-only">{t("sidebar.update.indicator")}</span>
              </>
            )}
          </button>
        </div>
      )}
      {/* Drag handle: resize the sidebar; width persists across launches. */}
      <div
        onMouseDown={startResize}
        title={t("sidebar.resize")}
        className="absolute top-0 end-0 h-full w-1.5 cursor-col-resize hover:bg-accent/40 active:bg-accent/60 transition-colors"
      />
    </div>
  );
};
