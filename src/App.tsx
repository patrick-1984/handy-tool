import { useEffect, useState, useRef } from "react";
import { Toaster, toast } from "sonner";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { platform } from "@tauri-apps/plugin-os";
import {
  checkAccessibilityPermission,
  checkMicrophonePermission,
} from "tauri-plugin-macos-permissions-api";
import "./App.css";
import AccessibilityPermissions from "./components/AccessibilityPermissions";
import Footer from "./components/footer";
import SetupWizard, { AccessibilityOnboarding } from "./components/onboarding";
import {
  Sidebar,
  type SidebarSection,
  SECTIONS_CONFIG,
  isMoreSection,
} from "./components/Sidebar";
import { MorePage } from "./components/settings/more/MorePage";
import { useSettings } from "./hooks/useSettings";
import { lastPage, useNavStore } from "./stores/navStore";
import { useSettingsStore } from "./stores/settingsStore";
import { useModelStore } from "./stores/modelStore";
import { RUN_SETUP_EVENT } from "./lib/runSetup";
import { commands } from "@/bindings";
import { getLanguageDirection, initializeRTL } from "@/lib/utils/rtl";
import { isFlmBlockedByWindowsApplicationControl } from "@/lib/flm";
import { highlightSetting } from "@/lib/settingsSearch";
import { PageTitle } from "./components/ui/PageTitle";

type OnboardingStep = "accessibility" | "model" | "done";

/** A sidebar page's title, with its sidebar icon. */
function PageHeader({ section }: { section: SidebarSection }) {
  const { t } = useTranslation();
  const { icon, labelKey } = SECTIONS_CONFIG[section];
  return <PageTitle icon={icon} label={t(labelKey)} />;
}

const renderSettingsContent = (section: SidebarSection) => {
  if (SECTIONS_CONFIG[section] && isMoreSection(section)) {
    return <MorePage section={section} />;
  }
  const shown = SECTIONS_CONFIG[section] ? section : "general";
  const ActiveComponent = SECTIONS_CONFIG[shown].component;
  return (
    <>
      <PageHeader section={shown} />
      <ActiveComponent />
    </>
  );
};

function App() {
  const { t, i18n } = useTranslation();
  const [onboardingStep, setOnboardingStep] = useState<OnboardingStep | null>(
    null,
  );
  // Track if this is a returning user who just needs to grant permissions
  // (vs a new user who needs full onboarding including model selection)
  const [isReturningUser, setIsReturningUser] = useState(false);
  const currentSection = useNavStore((state) => state.currentSection);
  const setCurrentSection = useNavStore((state) => state.setCurrentSection);
  const { settings, updateSetting } = useSettings();
  const direction = getLanguageDirection(i18n.language);
  // Resolved (never "system") light/dark value driving this window's
  // data-theme attribute AND the Sonner toaster — a single source of truth
  // per T-204's "consistent resolved theme" requirement instead of letting
  // the toaster do its own independent system detection.
  const [resolvedTheme, setResolvedTheme] = useState<"light" | "dark">("light");
  const refreshAudioDevices = useSettingsStore(
    (state) => state.refreshAudioDevices,
  );
  const refreshOutputDevices = useSettingsStore(
    (state) => state.refreshOutputDevices,
  );
  const hasCompletedPostOnboardingInit = useRef(false);

  useEffect(() => {
    checkOnboardingStatus();
  }, []);

  // Initialize RTL direction when language changes
  useEffect(() => {
    initializeRTL(i18n.language);
  }, [i18n.language]);

  // Resolve appearance (system/light/dark) and stamp it on this window's
  // document root. "system" tracks the OS live via a matchMedia change
  // listener; an explicit light/dark choice ignores subsequent OS changes
  // because the listener always recomputes from `settings.app_theme` first.
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const applyResolvedTheme = () => {
      const appTheme = settings?.app_theme ?? "system";
      const resolved =
        appTheme === "light"
          ? "light"
          : appTheme === "dark"
            ? "dark"
            : media.matches
              ? "dark"
              : "light";
      setResolvedTheme(resolved);
      document.documentElement.setAttribute("data-theme", resolved);
    };
    applyResolvedTheme();
    media.addEventListener("change", applyResolvedTheme);
    return () => media.removeEventListener("change", applyResolvedTheme);
  }, [settings?.app_theme]);

  // A failed delivery never loses the take: it is written to History BEFORE
  // delivery is attempted, and re-pastable with the Paste Last shortcut. What
  // the toast must say depends on whether the text was parked on the clipboard
  // or deliberately withheld (because the user chose "Don't Modify Clipboard"),
  // so the backend reports which happened. When the recovery shortcut is
  // actually bound we name it; when it is not, we point at History instead of
  // printing a key the user does not have.
  const pasteLastKey = settings?.bindings?.paste_last?.current_binding?.trim();
  useEffect(() => {
    const recoveryHint = (): string =>
      pasteLastKey
        ? t("toasts.recoverWithPasteLast", { shortcut: pasteLastKey })
        : t("toasts.recoverFromHistory");

    const unlisten = listen<{ reason: string; clipboard: string }>(
      "anchor-delivery-failed",
      (e) =>
        toast.error(
          e.payload.clipboard === "parked"
            ? t("settings.general.anchor.deliveryFailed", {
                reason: e.payload.reason,
              })
            : t("settings.general.anchor.deliveryFailedHistoryOnly", {
                reason: e.payload.reason,
                recovery: recoveryHint(),
              }),
          { duration: 8000 },
        ),
    );
    // Ordinary paste failures likewise.
    const unlistenPaste = listen<{
      error: string;
      parked: boolean;
      clipboard?: string;
    }>("paste-failed", (e) =>
      toast.error(
        e.payload.parked
          ? t("toasts.pasteFailedParked", { reason: e.payload.error })
          : t("toasts.pasteFailedHistoryOnly", {
              reason: e.payload.error,
              recovery: recoveryHint(),
            }),
        { duration: 8000 },
      ),
    );
    // An engine error that leaves the take with no text (e.g. FLM's ASR model
    // failed to load) — otherwise the recording is saved textless and reads as
    // "recording works but produces nothing".
    const unlistenTranscribe = listen<string>("transcription-failed", (e) =>
      toast.error(
        isFlmBlockedByWindowsApplicationControl(e.payload)
          ? t("flm.windowsApplicationControlBlocked")
          : t("toasts.transcriptionFailed", { reason: e.payload }),
        { duration: 10000 },
      ),
    );
    return () => {
      unlisten.then((f) => f());
      unlistenPaste.then((f) => f());
      unlistenTranscribe.then((f) => f());
    };
  }, [t, pasteLastKey]);

  // The pill's right-click menus open a page here at a given setting.
  const navigateTo = useNavStore((state) => state.navigateTo);

  // Reopen Last Page: once the settings are in, go back to the page that was
  // open when the app was closed (if it still exists and is on).
  const reopenedRef = useRef(false);
  useEffect(() => {
    if (reopenedRef.current || !settings) return;
    reopenedRef.current = true;
    if (settings.reopen_last_page === false) return;
    const page = lastPage();
    if (
      page &&
      page in SECTIONS_CONFIG &&
      SECTIONS_CONFIG[page as SidebarSection].enabled(settings)
    ) {
      navigateTo(page as SidebarSection);
    }
  }, [settings, navigateTo]);
  useEffect(() => {
    const unlisten = listen<{ section: SidebarSection; title_key: string }>(
      "open-setting",
      (e) => {
        navigateTo(e.payload.section);
        highlightSetting(t(e.payload.title_key));
      },
    );
    return () => {
      unlisten.then((f) => f());
    };
  }, [t, navigateTo]);

  // The setup's model, picked while it still downloads: select it once it has
  // landed (and been unpacked), whether or not the setup is still open.
  const {
    models,
    downloadingModels,
    extractingModels,
    pendingSelection,
    setPendingSelection,
    selectModel,
  } = useModelStore();
  useEffect(() => {
    if (!pendingSelection) return;
    const model = models.find((m) => m.id === pendingSelection);
    if (
      model?.is_downloaded &&
      !(pendingSelection in downloadingModels) &&
      !(pendingSelection in extractingModels)
    ) {
      setPendingSelection(null);
      void selectModel(pendingSelection);
    }
  }, [
    pendingSelection,
    models,
    downloadingModels,
    extractingModels,
    setPendingSelection,
    selectModel,
  ]);

  // More › App › Setup guide: run the setup again.
  useEffect(() => {
    const runSetup = () => setOnboardingStep("model");
    window.addEventListener(RUN_SETUP_EVENT, runSetup);
    return () => window.removeEventListener(RUN_SETUP_EVENT, runSetup);
  }, []);

  // Initialize Enigo, shortcuts, and refresh audio devices when main app loads
  useEffect(() => {
    if (onboardingStep === "done" && !hasCompletedPostOnboardingInit.current) {
      hasCompletedPostOnboardingInit.current = true;
      Promise.all([commands.initializeEnigo(), commands.initializeShortcuts()])
        .then(async () => {
          // Surface hotkeys that failed to register (e.g. taken by another
          // app) — otherwise they are silently dead while the UI shows them
          // as active.
          const failures = await commands.getShortcutRegistrationFailures();
          if (failures.length > 0) {
            const names = failures
              .map((f) => `${f.id}: ${f.binding}`)
              .join(", ");
            toast.error(
              t("settings.general.shortcut.errors.registrationFailed", {
                bindings: names,
              }),
              { duration: 10000 },
            );
          }
        })
        .catch((e) => {
          console.warn("Failed to initialize:", e);
        });
      refreshAudioDevices();
      refreshOutputDevices();
    }
  }, [onboardingStep, refreshAudioDevices, refreshOutputDevices, t]);

  // Handle keyboard shortcuts for debug mode toggle
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      // Check for Ctrl+Shift+D (Windows/Linux) or Cmd+Shift+D (macOS)
      const isDebugShortcut =
        event.shiftKey &&
        event.key.toLowerCase() === "d" &&
        (event.ctrlKey || event.metaKey);

      if (isDebugShortcut) {
        event.preventDefault();
        const currentDebugMode = settings?.debug_mode ?? false;
        updateSetting("debug_mode", !currentDebugMode);
      }
    };

    // Add event listener when component mounts
    document.addEventListener("keydown", handleKeyDown);

    // Cleanup event listener when component unmounts
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [settings?.debug_mode, updateSetting]);

  const checkOnboardingStatus = async () => {
    try {
      // Check if they have any models available
      const result = await commands.hasAnyModelsAvailable();
      const hasModels = result.status === "ok" && result.data;

      if (hasModels) {
        // Returning user - but check if they need to grant permissions on macOS
        setIsReturningUser(true);
        if (platform() === "macos") {
          try {
            const [hasAccessibility, hasMicrophone] = await Promise.all([
              checkAccessibilityPermission(),
              checkMicrophonePermission(),
            ]);
            if (!hasAccessibility || !hasMicrophone) {
              // Missing permissions - show accessibility onboarding
              setOnboardingStep("accessibility");
              return;
            }
          } catch (e) {
            console.warn("Failed to check permissions:", e);
            // If we can't check, proceed to main app and let them fix it there
          }
        }
        setOnboardingStep("done");
      } else {
        // New user - start full onboarding
        setIsReturningUser(false);
        setOnboardingStep("accessibility");
      }
    } catch (error) {
      console.error("Failed to check onboarding status:", error);
      setOnboardingStep("accessibility");
    }
  };

  const handleAccessibilityComplete = () => {
    // Returning users already have models, skip to main app
    // New users need to select a model
    setOnboardingStep(isReturningUser ? "done" : "model");
  };

  const handleModelSelected = () => {
    // The setup is finished or skipped (its model may still be downloading).
    setOnboardingStep("done");
  };

  // Still checking onboarding status
  if (onboardingStep === null) {
    return null;
  }

  if (onboardingStep === "accessibility") {
    return <AccessibilityOnboarding onComplete={handleAccessibilityComplete} />;
  }

  if (onboardingStep === "model") {
    return <SetupWizard onDone={handleModelSelected} />;
  }

  return (
    <div
      dir={direction}
      className="h-screen flex flex-col select-none cursor-default"
    >
      <Toaster
        theme={resolvedTheme}
        toastOptions={{
          unstyled: true,
          classNames: {
            toast:
              "bg-surface text-text border border-border rounded-lg shadow-float px-4 py-3 flex items-center gap-3 text-[13px]",
            title: "font-medium",
            description: "text-text-secondary",
          },
        }}
      />
      {/* The sidebar runs the full height; the footer sits under the pages. */}
      <div className="flex-1 flex overflow-hidden">
        <Sidebar
          activeSection={currentSection}
          onSectionChange={setCurrentSection}
        />
        <div className="flex-1 flex flex-col overflow-hidden bg-background">
          {/* Scrollable content area */}
          <div className="flex-1 overflow-y-auto">
            <div className="flex flex-col items-center px-8 pt-6 pb-8">
              {/* Wide windows: keep lines at a readable length. */}
              <div className="w-full max-w-4xl flex flex-col items-center gap-6">
                <AccessibilityPermissions />
                {renderSettingsContent(currentSection)}
              </div>
            </div>
          </div>
          <Footer />
        </div>
      </div>
    </div>
  );
}

export default App;
