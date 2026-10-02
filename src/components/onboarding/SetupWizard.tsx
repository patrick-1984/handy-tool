import React, { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import {
  AlertCircle,
  ArrowRight,
  Check,
  Loader2,
  MoveUpRight,
  Palette,
  Search,
  Sparkles,
} from "lucide-react";
import { commands, type ModelInfo } from "@/bindings";
import ModelCard, { type ModelCardStatus } from "./ModelCard";
import AppIcon from "../icons/AppIcon";
import { StepSegments } from "../settings/setups/SetupFrame";
import { Button } from "../ui/Button";
import { SettingsGroup } from "../ui/SettingsGroup";
import { TEXT_FIELD } from "../ui/controlClasses";
import { ShortcutInput } from "../settings/ShortcutInput";
import { MicrophoneSelector } from "../settings/MicrophoneSelector";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import { useModelStore } from "../../stores/modelStore";
import { useNavStore, type FeatureSetup } from "../../stores/navStore";
import { LANGUAGES } from "../../lib/constants/languages";
import { formatKeyCombination } from "../../lib/utils/keyboard";
import {
  offeredLanguages,
  suggestModels,
  understandsAll,
  type ModelSuggestions,
} from "./setupModels";

const STEPS = [
  "welcome",
  "languages",
  "model",
  "shortcuts",
  "microphone",
  "try",
  "done",
] as const;
type Step = (typeof STEPS)[number];
/** The steps counted in "Step N of 5" (not the welcome and the end). */
const COUNTED = STEPS.length - 2;

/** Offered first; the search box finds the rest. */
const POPULAR = [
  "en",
  "pl",
  "de",
  "es",
  "fr",
  "it",
  "pt",
  "nl",
  "ru",
  "uk",
  "cs",
  "sv",
  "tr",
  "zh-Hans",
  "ja",
  "ko",
  "ar",
  "hi",
];

/** The shortcuts a beginner needs; the Shortcuts page has the rest. */
const BASIC_SHORTCUTS = [
  { id: "transcribe", hintKey: "setup.shortcuts.transcribe" },
  { id: "transcribe_ptt", hintKey: "setup.shortcuts.ptt" },
  { id: "cancel", hintKey: "setup.shortcuts.cancel" },
];

const SUGGESTION_ROLES: (keyof ModelSuggestions)[] = [
  "accurate",
  "balanced",
  "fast",
];

interface SetupWizardProps {
  /** Finished or skipped: on to the app. */
  onDone: () => void;
}

/**
 * The first-start setup, for beginners: languages, a speech model suggested for
 * them, the basic shortcuts with their defaults, and the microphone. Every step
 * can be skipped, and so can all of it; More › App › Setup guide runs it again.
 */
/** Microphone problems, as the pill names them. */
const MIC_PROBLEMS: Record<string, string> = {
  "no-microphone": "overlay.noMicrophone",
  "microphone-blocked": "overlay.microphoneBlocked",
  "microphone-error": "overlay.microphoneError",
};

/** Offered on the last screen, all optional; the look first, as recommended. */
const SUGGESTED: {
  key: FeatureSetup;
  icon: React.ComponentType<{ className?: string }>;
  recommended?: boolean;
}[] = [
  { key: "appearance", icon: Palette, recommended: true },
  { key: "postProcessing", icon: Sparkles },
  { key: "jumper", icon: MoveUpRight },
];

const SetupWizard: React.FC<SetupWizardProps> = ({ onDone }) => {
  const startSetup = useNavStore((s) => s.startSetup);
  const { t, i18n } = useTranslation();
  const { getSetting, updateSetting, refreshAudioDevices } = useSettings();
  const osType = useOsType();
  const {
    models,
    currentModel,
    downloadingModels,
    extractingModels,
    downloadProgress,
    downloadStats,
    downloadModel,
    cancelDownload,
    selectModel,
    pendingSelection,
    setPendingSelection,
  } = useModelStore();

  const [step, setStep] = useState<Step>("welcome");
  const [languages, setLanguages] = useState<string[]>([]);
  const [languageQuery, setLanguageQuery] = useState("");
  const [showAllModels, setShowAllModels] = useState(false);
  const [modelQuery, setModelQuery] = useState("");
  const [failedModel, setFailedModel] = useState<string | null>(null);

  const index = STEPS.indexOf(step);

  useEffect(() => {
    if (step === "microphone") void refreshAudioDevices();
  }, [step, refreshAudioDevices]);

  // Try it: the shortcuts work from here on (on a first start they are
  // otherwise registered only after the setup), and the take's text is shown
  // in the box whatever the paste method is.
  const [tryText, setTryText] = useState("");
  // What the take is doing, as the pill shows it: recording, paused,
  // transcribing (with its percentage), or idle.
  const [takeState, setTakeState] = useState("idle");
  const [takeProgress, setTakeProgress] = useState<number | null>(null);
  useEffect(() => {
    if (step !== "try") return;
    // A take that ended while another step was open sent its "idle" to no one.
    setTakeState("idle");
    setTakeProgress(null);
    void commands.initializeEnigo().catch(() => {});
    void commands.initializeShortcuts().catch(() => {});
    const unlisteners = Promise.all([
      // A microphone problem stays in view until the next take (the pill
      // shows it only for a moment).
      listen<string>("take-state", (event) =>
        setTakeState((prev) =>
          event.payload === "idle" && MIC_PROBLEMS[prev] ? prev : event.payload,
        ),
      ),
      // Sent once per new take (not on resuming a pause): it replaces the
      // text of the last one.
      listen("live-transcription-reset", () => {
        setTakeProgress(null);
        setTryText("");
      }),
      listen<number>("take-progress", (event) =>
        setTakeProgress(event.payload),
      ),
      listen<{ text: string; is_final: boolean }>(
        "live-transcription-chunk",
        (event) => setTryText(event.payload.text),
      ),
    ]);
    return () => {
      unlisteners.then((fns) => fns.forEach((unlisten) => unlisten()));
    };
  }, [step]);

  // Language names in the app's own language (the list only has English ones).
  const displayNames = useMemo(() => {
    try {
      return new Intl.DisplayNames([i18n.language], { type: "language" });
    } catch {
      return null;
    }
  }, [i18n.language]);
  const languageName = (code: string) => {
    const english = LANGUAGES.find((l) => l.value === code)?.label ?? code;
    try {
      const name = displayNames?.of(code);
      if (name && name !== code) {
        return name.charAt(0).toLocaleUpperCase(i18n.language) + name.slice(1);
      }
    } catch {
      // Not a code the browser knows: the English name will do.
    }
    return english;
  };

  // Only languages some speech model can take.
  const offered = useMemo(() => {
    const codes = offeredLanguages(models);
    return LANGUAGES.map((l) => l.value).filter(
      (code) => code !== "auto" && codes.has(code),
    );
  }, [models]);

  const shownLanguages = useMemo(() => {
    const query = languageQuery.trim().toLocaleLowerCase();
    if (!query) {
      const popular = POPULAR.filter((code) => offered.includes(code));
      return [...popular, ...languages.filter((c) => !popular.includes(c))];
    }
    return offered.filter((code) => {
      const english = LANGUAGES.find((l) => l.value === code)?.label ?? "";
      return [languageName(code), english, code].some((s) =>
        s.toLocaleLowerCase().includes(query),
      );
    });
  }, [languageQuery, offered, languages, displayNames]);

  const toggleLanguage = (code: string) =>
    setLanguages((picked) =>
      picked.includes(code)
        ? picked.filter((c) => c !== code)
        : [...picked, code],
    );

  // One language: transcribe in it. Several: let the model tell them apart.
  const saveLanguages = () => {
    if (languages.length === 1) {
      void updateSetting("selected_language", languages[0]);
    } else if (languages.length > 1) {
      void updateSetting("selected_language", "auto");
    }
  };

  const next = () => {
    if (step === "languages") saveLanguages();
    setStep(STEPS[index + 1]);
  };
  const back = () => setStep(STEPS[index - 1]);
  // Skip: on to the next step without saving this one (no languages set, no
  // model downloaded). Shortcuts and the microphone save as they change.
  const skippable =
    step === "languages" ||
    step === "model" ||
    step === "shortcuts" ||
    step === "microphone";
  const skip = () => setStep(STEPS[index + 1]);

  /* ─── model ─── */
  const suggestions = useMemo(
    () => suggestModels(models, languages),
    [models, languages],
  );
  const statusOf = (m: ModelInfo): ModelCardStatus =>
    m.id in extractingModels
      ? "extracting"
      : m.id in downloadingModels
        ? "downloading"
        : m.id === currentModel
          ? "active"
          : m.is_downloaded
            ? "available"
            : "downloadable";

  // Picking a model starts its download at once, so it is ready by the end of
  // the setup; the app selects it when it lands (see App.tsx).
  const choose = async (id: string) => {
    setFailedModel(null);
    const model = models.find((m) => m.id === id);
    if (model?.is_downloaded) {
      setPendingSelection(null);
      await selectModel(id);
      return;
    }
    const previous = useModelStore.getState().pendingSelection;
    setPendingSelection(id);
    if (previous && previous !== id && previous in downloadingModels) {
      void cancelDownload(previous);
    }
    const ok = await downloadModel(id);
    if (!ok && useModelStore.getState().pendingSelection === id) {
      setFailedModel(id);
      setPendingSelection(null);
    }
  };
  const cancel = (id: string) => {
    if (useModelStore.getState().pendingSelection === id) {
      setPendingSelection(null);
    }
    void cancelDownload(id);
  };
  const chosenModel = pendingSelection ?? currentModel;

  // The model picked on this step: one always is - the one in use if it was
  // downloaded, else the balanced suggestion for the languages picked.
  const [picked, setPicked] = useState<string | null>(null);
  const inUse = models.find((m) => m.id === currentModel && m.is_downloaded);
  const selected = picked ?? inUse?.id ?? suggestions.balanced?.id ?? null;
  const selectedModel = models.find((m) => m.id === selected);
  const selectedReady =
    !selectedModel ||
    selectedModel.is_downloaded ||
    selectedModel.id in downloadingModels ||
    selectedModel.id in extractingModels;
  // Next on the model step: download the picked model (it goes on in the
  // background) or switch to it if it is already here.
  const modelNext = async () => {
    if (selectedModel && !selectedReady) {
      void choose(selectedModel.id);
    } else if (
      selectedModel?.is_downloaded &&
      selectedModel.id !== currentModel
    ) {
      void choose(selectedModel.id);
    }
    next();
  };

  const card = (m: ModelInfo, label?: React.ReactNode) => (
    <div key={m.id} className="flex flex-col gap-1.5">
      {label}
      <ModelCard
        model={m}
        status={statusOf(m)}
        showRecommended={false}
        hideActions
        picked={m.id === selected}
        onSelect={setPicked}
        onDownload={setPicked}
        onCancel={cancel}
        downloadProgress={downloadProgress[m.id]?.percentage}
        downloadSpeed={downloadStats[m.id]?.speed}
      />
    </div>
  );

  const allModels = useMemo(() => {
    const query = modelQuery.trim().toLocaleLowerCase();
    return models
      .filter((m) => !query || m.name.toLocaleLowerCase().includes(query))
      .sort(
        (a, b) =>
          Number(understandsAll(b, languages)) -
            Number(understandsAll(a, languages)) ||
          Number(a.size_mb) - Number(b.size_mb),
      );
  }, [models, modelQuery, languages]);

  const languageList = languages.map(languageName).join(", ");

  /* ─── screens ─── */
  const heading = (title: string, body?: string) => (
    <div className="flex flex-col gap-2">
      <h1 className="font-display text-[28px] leading-tight font-semibold text-text">
        {title}
      </h1>
      {body && (
        <p className="max-w-[620px] text-sm leading-relaxed text-text-secondary">
          {body}
        </p>
      )}
    </div>
  );

  const screen = (() => {
    switch (step) {
      case "welcome":
        return (
          <div className="flex flex-col items-center gap-4 text-center pt-10">
            <AppIcon className="w-16 h-16" />
            <h1 className="font-display text-[28px] leading-tight font-semibold text-text">
              {t("setup.welcome.title")}
            </h1>
            <p className="max-w-[520px] text-sm leading-relaxed text-text-secondary">
              {t("setup.welcome.body")}
            </p>
            <p className="max-w-[520px] text-sm leading-relaxed text-text-secondary">
              {t("setup.welcome.steps")}
            </p>
          </div>
        );

      case "languages":
        return (
          <div className="flex flex-col gap-5">
            {heading(t("setup.languages.title"), t("setup.languages.body"))}
            <div className="relative max-w-[320px]">
              <Search
                className="absolute start-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-text-secondary"
                aria-hidden
              />
              <input
                type="text"
                value={languageQuery}
                onChange={(e) => setLanguageQuery(e.target.value)}
                placeholder={t("setup.languages.search")}
                aria-label={t("setup.languages.search")}
                className={`${TEXT_FIELD} w-full ps-8`}
              />
            </div>
            <div className="flex flex-wrap gap-2">
              {shownLanguages.map((code) => {
                const picked = languages.includes(code);
                return (
                  <button
                    key={code}
                    type="button"
                    onClick={() => toggleLanguage(code)}
                    aria-pressed={picked}
                    // Square corners like the model filters; the weight never
                    // changes, so the row does not shift when one is picked.
                    className={`inline-flex items-center gap-1.5 h-8 px-3 rounded-md border text-sm transition-colors cursor-pointer ${
                      picked
                        ? "border-accent bg-accent-soft text-accent-text"
                        : "border-control-border bg-control text-text hover:border-control-bottom hover:bg-control-hover"
                    }`}
                  >
                    {picked && <Check className="w-3.5 h-3.5" aria-hidden />}
                    {languageName(code)}
                  </button>
                );
              })}
              {shownLanguages.length === 0 && (
                <p className="text-sm text-text-secondary">
                  {t("setup.languages.noMatch")}
                </p>
              )}
            </div>
            {languages.length > 0 && (
              <p className="text-[13px] text-text-secondary">
                {t("setup.languages.selected", { languages: languageList })}
              </p>
            )}
          </div>
        );

      case "model":
        return (
          <div className="flex flex-col gap-5">
            {heading(
              t("onboarding.title"),
              languages.length
                ? t("setup.model.body", { languages: languageList })
                : t("setup.model.bodyAny"),
            )}
            {failedModel && (
              <p className="text-sm text-err-text">
                {t("onboarding.downloadFailed")}
              </p>
            )}
            {showAllModels ? (
              <>
                <div className="relative max-w-[320px]">
                  <Search
                    className="absolute start-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-text-secondary"
                    aria-hidden
                  />
                  <input
                    type="text"
                    value={modelQuery}
                    onChange={(e) => setModelQuery(e.target.value)}
                    placeholder={t("setup.model.search")}
                    aria-label={t("setup.model.search")}
                    className={`${TEXT_FIELD} w-full ps-8`}
                  />
                </div>
                <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3">
                  {allModels.map((m) =>
                    card(
                      m,
                      !understandsAll(m, languages) && (
                        <span className="text-xs text-warn-text">
                          {t("setup.model.notForLanguages")}
                        </span>
                      ),
                    ),
                  )}
                </div>
              </>
            ) : (
              <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3">
                {SUGGESTION_ROLES.map((role) => {
                  const m = suggestions[role];
                  return (
                    m &&
                    card(
                      m,
                      <span className="text-xs font-semibold uppercase tracking-[0.06em] text-text-secondary">
                        {t(`setup.model.${role}`)}
                      </span>,
                    )
                  );
                })}
              </div>
            )}
            <div>
              <Button
                variant="secondary"
                onClick={() => setShowAllModels((all) => !all)}
              >
                {showAllModels
                  ? t("setup.model.showSuggestions")
                  : t("setup.model.seeAll")}
              </Button>
            </div>
          </div>
        );

      case "shortcuts": {
        const bindings = getSetting("bindings") ?? {};
        return (
          <div className="flex flex-col gap-5">
            {heading(t("setup.shortcuts.title"), t("setup.shortcuts.body"))}
            <SettingsGroup>
              {BASIC_SHORTCUTS.filter(({ id }) => bindings[id]).map(
                ({ id, hintKey }) => (
                  <div key={id}>
                    <ShortcutInput shortcutId={id} grouped />
                    <p className="px-4 pb-3 -mt-1 text-[13px] leading-[18px] text-text-secondary">
                      {t(hintKey)}{" "}
                      {bindings[id]?.default_binding &&
                        t("setup.shortcuts.default", {
                          keys: formatKeyCombination(
                            bindings[id]!.default_binding,
                            osType,
                          ),
                        })}
                    </p>
                  </div>
                ),
              )}
            </SettingsGroup>
            <p className="text-[13px] text-text-secondary">
              {t("setup.shortcuts.more")}
            </p>
            {osType === "windows" && (
              <p className="text-[13px] text-text-secondary">
                {t("setup.shortcuts.remote")}
              </p>
            )}
          </div>
        );
      }

      case "microphone":
        return (
          <div className="flex flex-col gap-5">
            {heading(t("setup.microphone.title"), t("setup.microphone.body"))}
            <SettingsGroup>
              <MicrophoneSelector grouped />
            </SettingsGroup>
          </div>
        );

      case "try": {
        const chosen = models.find((m) => m.id === chosenModel);
        const keys = formatKeyCombination(
          getSetting("bindings")?.transcribe?.current_binding ?? "",
          osType,
        );
        const downloading =
          chosen &&
          (chosen.id in downloadingModels || chosen.id in extractingModels);
        return (
          <div className="flex flex-col gap-5">
            {heading(t("setup.try.title"), t("setup.try.body", { keys }))}
            {!chosen ? (
              <p className="text-sm text-warn-text">{t("setup.try.noModel")}</p>
            ) : (
              downloading && (
                <p className="text-sm text-text-secondary">
                  {t("setup.try.downloading", {
                    model: chosen.name,
                    percent: Math.round(
                      downloadProgress[chosen.id]?.percentage ?? 0,
                    ),
                  })}
                </p>
              )
            )}
            {/* What is happening right now, so a blank box never leaves you
                guessing. */}
            {takeState !== "idle" && (
              <div className="flex flex-col gap-2" aria-live="polite">
                <p className="flex items-center gap-2 text-sm font-semibold">
                  {MIC_PROBLEMS[takeState] ? (
                    <AlertCircle
                      className="w-4 h-4 text-warn-text"
                      aria-hidden
                    />
                  ) : takeState === "recording" || takeState === "paused" ? (
                    <span
                      className={`w-2.5 h-2.5 rounded-full bg-recording ${takeState === "recording" ? "animate-pulse" : "opacity-50"}`}
                      aria-hidden
                    />
                  ) : (
                    <Loader2
                      className="w-4 h-4 animate-spin text-accent"
                      aria-hidden
                    />
                  )}
                  {MIC_PROBLEMS[takeState]
                    ? t(MIC_PROBLEMS[takeState])
                    : takeState === "recording"
                      ? t("setup.try.listening", { keys })
                      : takeState === "paused"
                        ? t("overlay.paused")
                        : takeState === "processing"
                          ? t("overlay.processing")
                          : takeProgress === null
                            ? t("overlay.transcribing")
                            : t("overlay.transcribingProgress", {
                                percent: takeProgress,
                              })}
                </p>
                {takeProgress !== null && takeState === "transcribing" && (
                  <div className="w-full h-1 bg-control-bottom rounded-full overflow-hidden">
                    <div
                      className="h-full bg-accent rounded-full transition-[width] duration-300"
                      style={{ width: `${takeProgress}%` }}
                    />
                  </div>
                )}
              </div>
            )}
            <div
              className="card min-h-[140px] px-4 py-3 text-[15px] leading-relaxed"
              aria-live="polite"
            >
              {tryText ? (
                <span className="text-text">{tryText}</span>
              ) : (
                <span className="text-text-secondary">
                  {t("setup.try.placeholder")}
                </span>
              )}
            </div>
          </div>
        );
      }

      case "done": {
        const chosen = models.find((m) => m.id === chosenModel);
        const percent = chosen && downloadProgress[chosen.id]?.percentage;
        const keys = formatKeyCombination(
          getSetting("bindings")?.transcribe?.current_binding ?? "",
          osType,
        );
        return (
          <div className="flex flex-col items-center gap-4 text-center pt-10">
            <AppIcon className="w-16 h-16" />
            <h1 className="font-display text-[28px] leading-tight font-semibold text-text">
              {t("setup.done.title")}
            </h1>
            {keys && (
              <p className="max-w-[520px] text-sm leading-relaxed text-text">
                {t("setup.done.howTo", { keys })}
              </p>
            )}
            <p className="max-w-[520px] text-sm leading-relaxed text-text-secondary">
              {!chosen
                ? t("setup.done.noModel")
                : chosen.id in downloadingModels ||
                    chosen.id in extractingModels
                  ? t("setup.done.downloading", {
                      model: chosen.name,
                      percent: Math.round(percent ?? 0),
                    })
                  : t("setup.done.ready", { model: chosen.name })}
            </p>
            {/* More setups, offered but not needed: the defaults already work. */}
            <div className="w-full max-w-[620px] mt-4 flex flex-col gap-3 text-start">
              <div>
                <h2 className="text-[15px] font-semibold">
                  {t("setup.done.suggested.title")}
                </h2>
                <p className="text-[13px] leading-[1.5] text-text-secondary">
                  {t("setup.done.suggested.body")}
                </p>
              </div>
              {SUGGESTED.filter(
                (s) => s.key !== "jumper" || osType === "windows",
              ).map(({ key, icon: Icon, recommended }) => (
                <div
                  key={key}
                  className="flex items-center gap-3 rounded-lg border border-border bg-surface px-3 py-2.5"
                >
                  <span className="flex-none flex items-center justify-center w-8 h-8 rounded-lg bg-accent-soft text-accent-text">
                    <Icon className="w-4 h-4" aria-hidden />
                  </span>
                  <div className="flex-1 min-w-0">
                    <p className="flex items-center gap-2 text-sm font-semibold">
                      {t(`setup.catalog.${key}.title`)}
                      {recommended && (
                        <span className="inline-flex items-center h-5 px-1.5 rounded bg-accent-soft text-xs font-semibold text-accent-text">
                          {t("onboarding.recommended")}
                        </span>
                      )}
                    </p>
                    <p className="text-[13px] leading-[1.45] text-text-secondary line-clamp-2">
                      {t(`setup.catalog.${key}.description`)}
                    </p>
                  </div>
                  <Button
                    variant="secondary"
                    className="flex-none"
                    onClick={() => {
                      startSetup(key);
                      onDone();
                    }}
                  >
                    {t("setup.catalog.start")}
                    <ArrowRight
                      className="w-3.5 h-3.5 rtl:-scale-x-100"
                      aria-hidden
                    />
                  </Button>
                </div>
              ))}
            </div>
          </div>
        );
      }
    }
  })();

  return (
    <div className="h-screen w-screen overflow-y-auto flex justify-center px-10 pt-8 pb-8">
      <div className="w-full max-w-[820px] min-h-full flex flex-col gap-6">
        <header className="flex items-center gap-3 min-h-8">
          {step !== "welcome" && step !== "done" && (
            <>
              <AppIcon className="w-6 h-6" />
              <span className="text-[13px] text-text-secondary tabular-nums">
                {t("setup.step", { current: index, total: COUNTED })}
              </span>
              <StepSegments step={index} total={COUNTED} />
            </>
          )}
          {step !== "done" && (
            <button
              type="button"
              onClick={onDone}
              className="ms-auto inline-flex items-center h-8 px-2.5 rounded-md text-[13px] text-text-secondary hover:bg-hover hover:text-text transition-colors cursor-pointer"
            >
              {t("setup.skipSetup")}
            </button>
          )}
        </header>

        <main className="flex-1">{screen}</main>

        {/* Under a divider, always at the bottom, so the buttons stay put. */}
        <footer className="flex items-center gap-2 pt-4 border-t border-border">
          {index > 0 && step !== "done" && (
            <Button variant="secondary" onClick={back}>
              {t("setup.back")}
            </Button>
          )}
          <span className="flex-1" />
          {skippable && (
            <Button variant="ghost" onClick={skip}>
              {t("setup.skip")}
            </Button>
          )}
          <Button
            variant="primary"
            onClick={
              step === "done" ? onDone : step === "model" ? modelNext : next
            }
          >
            {step === "welcome"
              ? t("setup.start")
              : step === "done"
                ? t("setup.finish")
                : step === "model" && !selectedReady
                  ? t("setup.model.downloadContinue")
                  : skippable
                    ? t("setup.saveNext")
                    : t("setup.next")}
          </Button>
        </footer>
      </div>
    </div>
  );
};

export default SetupWizard;
