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
import Onboarding, { AccessibilityOnboarding } from "./components/onboarding";
import { Sidebar, SidebarSection, SECTIONS_CONFIG } from "./components/Sidebar";
import { useSettings } from "./hooks/useSettings";
import { useSettingsStore } from "./stores/settingsStore";
import { commands } from "@/bindings";
import { getLanguageDirection, initializeRTL } from "@/lib/utils/rtl";

type OnboardingStep = "accessibility" | "model" | "done";

const renderSettingsContent = (section: SidebarSection) => {
  const ActiveComponent =
    SECTIONS_CONFIG[section]?.component || SECTIONS_CONFIG.general.component;
  return <ActiveComponent />;
};

function App() {
  const { t, i18n } = useTranslation();
  const [onboardingStep, setOnboardingStep] = useState<OnboardingStep | null>(
    null,
  );
  // Track if this is a returning user who just needs to grant permissions
  // (vs a new user who needs full onboarding including model selection)
  const [isReturningUser, setIsReturningUser] = useState(false);
  const [currentSection, setCurrentSection] =
    useState<SidebarSection>("general");
  const { settings, updateSetting } = useSettings();
  const direction = getLanguageDirection(i18n.language);
  // Resolved (never "system") light/dark value driving this window's
  // data-theme attribute AND the Sonner toaster — a single source of truth
  // per T-204's "consistent resolved theme" requirement instead of letting
  // the toaster do its own independent system detection.
  const [resolvedTheme, setResolvedTheme] = useState<"light" | "dark">(
    "light",
  );
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

  // Anchored-delivery failures park the text on the clipboard — the toast must
  // fire no matter which settings page is open, so the listener lives here.
  useEffect(() => {
    const unlisten = listen<string>("anchor-delivery-failed", (e) =>
      toast.error(
        t("settings.general.anchor.deliveryFailed", { reason: e.payload }),
        { duration: 8000 },
      ),
    );
    // Ordinary paste failures likewise: the text is parked on the clipboard
    // (and always in History) — say so instead of failing silently.
    const unlistenPaste = listen<{ error: string; parked: boolean }>(
      "paste-failed",
      (e) =>
        toast.error(
          t(
            e.payload.parked
              ? "toasts.pasteFailedParked"
              : "toasts.pasteFailed",
            { reason: e.payload.error },
          ),
          { duration: 8000 },
        ),
    );
    return () => {
      unlisten.then((f) => f());
      unlistenPaste.then((f) => f());
    };
  }, [t]);

  // Initialize Enigo, shortcuts, and refresh audio devices when main app loads
  useEffect(() => {
    if (onboardingStep === "done" && !hasCompletedPostOnboardingInit.current) {
      hasCompletedPostOnboardingInit.current = true;
      Promise.all([
        commands.initializeEnigo(),
        commands.initializeShortcuts(),
      ])
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
    // Transition to main app - user has started a download
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
    return <Onboarding onModelSelected={handleModelSelected} />;
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
              "bg-background border border-mid-gray/20 rounded-lg shadow-lg px-4 py-3 flex items-center gap-3 text-sm",
            title: "font-medium",
            description: "text-mid-gray",
          },
        }}
      />
      {/* Main content area that takes remaining space */}
      <div className="flex-1 flex overflow-hidden">
        <Sidebar
          activeSection={currentSection}
          onSectionChange={setCurrentSection}
        />
        {/* Scrollable content area */}
        <div className="flex-1 flex flex-col overflow-hidden">
          <div className="flex-1 overflow-y-auto">
            <div className="flex flex-col items-center p-4 gap-4">
              <AccessibilityPermissions />
              {renderSettingsContent(currentSection)}
            </div>
          </div>
        </div>
      </div>
      {/* Fixed footer at bottom */}
      <Footer />
    </div>
  );
}

export default App;
