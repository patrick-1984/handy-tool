import { create } from "zustand";
import { subscribeWithSelector } from "zustand/middleware";
import type {
  AppSettings as Settings,
  AudioDevice,
  LlmProvider,
  ModelTestLibrary,
} from "@/bindings";
import { commands } from "@/bindings";

interface SettingsStore {
  settings: Settings | null;
  defaultSettings: Settings | null;
  isLoading: boolean;
  isUpdating: Record<string, boolean>;
  audioDevices: AudioDevice[];
  outputDevices: AudioDevice[];
  customSounds: { start: boolean; stop: boolean };

  // Actions
  initialize: () => Promise<void>;
  loadDefaultSettings: () => Promise<void>;
  updateSetting: <K extends keyof Settings>(
    key: K,
    value: Settings[K],
  ) => Promise<void>;
  resetSetting: (key: keyof Settings) => Promise<void>;
  refreshSettings: () => Promise<void>;
  refreshAudioDevices: () => Promise<void>;
  refreshOutputDevices: () => Promise<void>;
  updateBinding: (id: string, binding: string) => Promise<void>;
  resetBinding: (id: string) => Promise<void>;
  getSetting: <K extends keyof Settings>(key: K) => Settings[K] | undefined;
  isUpdatingKey: (key: string) => boolean;
  playTestSound: (soundType: "start" | "stop") => Promise<void>;
  checkCustomSounds: () => Promise<void>;

  // Internal state setters
  setSettings: (settings: Settings | null) => void;
  setDefaultSettings: (defaultSettings: Settings | null) => void;
  setLoading: (loading: boolean) => void;
  setUpdating: (key: string, updating: boolean) => void;
  setAudioDevices: (devices: AudioDevice[]) => void;
  setOutputDevices: (devices: AudioDevice[]) => void;
  setCustomSounds: (sounds: { start: boolean; stop: boolean }) => void;
}

// Note: Default settings are now fetched from Rust via commands.getDefaultSettings()
// This ensures platform-specific defaults (like overlay_position, shortcuts, paste_method) work correctly

const DEFAULT_AUDIO_DEVICE: AudioDevice = {
  index: "default",
  name: "Default",
  is_default: true,
};

const settingUpdaters: {
  [K in keyof Settings]?: (value: Settings[K]) => Promise<unknown>;
} = {
  always_on_microphone: (value) =>
    commands.updateMicrophoneMode(value as boolean),
  audio_feedback: (value) =>
    commands.changeAudioFeedbackSetting(value as boolean),
  audio_feedback_volume: (value) =>
    commands.changeAudioFeedbackVolumeSetting(value as number),
  sound_theme: (value) => commands.changeSoundThemeSetting(value as string),
  start_hidden: (value) => commands.changeStartHiddenSetting(value as boolean),
  autostart_enabled: (value) =>
    commands.changeAutostartSetting(value as boolean),
  push_to_talk: (value) => commands.changePttSetting(value as boolean),
  typing_start_delay_secs: (value) =>
    commands.changeTypingStartDelaySetting(value as number),
  typing_key_delay_ms: (value) =>
    commands.changeTypingKeyDelaySetting(value as number),
  llm_providers: (value) => commands.updateLlmProviders(value as LlmProvider[]),
  model_test_library: (value) =>
    commands.updateModelTestLibrary(value as ModelTestLibrary),
  post_process_provider_ref: (value) =>
    commands.setPostProcessProviderRef(value as string),
  post_process_temperature: (value) =>
    commands.changePostProcessTemperatureSetting(value as number),
  selected_microphone: (value) =>
    commands.setSelectedMicrophone(
      (value as string) === "Default" || value === null
        ? "default"
        : (value as string),
    ),
  clamshell_microphone: (value) =>
    commands.setClamshellMicrophone(
      (value as string) === "Default" ? "default" : (value as string),
    ),
  selected_output_device: (value) =>
    commands.setSelectedOutputDevice(
      (value as string) === "Default" || value === null
        ? "default"
        : (value as string),
    ),
  recording_retention_period: (value) =>
    commands.updateRecordingRetentionPeriod(value as string),
  translate_to_english: (value) =>
    commands.changeTranslateToEnglishSetting(value as boolean),
  selected_language: (value) =>
    commands.changeSelectedLanguageSetting(value as string),
  overlay_position: (value) =>
    commands.changeOverlayPositionSetting(value as string),
  app_theme: (value) => commands.changeAppThemeSetting(value as string),
  debug_mode: (value) => commands.changeDebugModeSetting(value as boolean),
  custom_words: (value) => commands.updateCustomWords(value as string[]),
  word_correction_threshold: (value) =>
    commands.changeWordCorrectionThresholdSetting(value as number),
  paste_method: (value) => commands.changePasteMethodSetting(value as string),
  paste_method_ptt: (value) =>
    commands.changePasteMethodPttSetting(value as string),
  typing_tool: (value) => commands.changeTypingToolSetting(value as string),
  external_script_path: (value) =>
    commands.changeExternalScriptPathSetting(value as string | null),
  clipboard_handling: (value) =>
    commands.changeClipboardHandlingSetting(value as string),
  auto_submit: (value) => commands.changeAutoSubmitSetting(value as boolean),
  auto_submit_key: (value) =>
    commands.changeAutoSubmitKeySetting(value as string),
  submit_paste_method: (value) =>
    commands.changeSubmitPasteMethodSetting(value as string),
  submit_key: (value) => commands.changeSubmitKeySetting(value as string),
  submit_idle_behavior: (value) =>
    commands.changeSubmitIdleBehaviorSetting(value as string),
  submit_clipboard_handling: (value) =>
    commands.changeSubmitClipboardHandlingSetting(value as string),
  clipboard_restore_delay: (value) =>
    commands.changeClipboardRestoreDelaySetting(value as string),
  submit_clipboard_restore_delay: (value) =>
    commands.changeSubmitClipboardRestoreDelaySetting(value as string),
  jumper_submit_delay: (value) =>
    commands.changeJumperSubmitDelaySetting(value as string),
  return_focus_output: (value) =>
    commands.changeReturnFocusSetting("output", value as boolean),
  return_focus_submit: (value) =>
    commands.changeReturnFocusSetting("submit", value as boolean),
  anchor_action_output_idle: (value) =>
    commands.changeAnchorActionSetting("output_idle", value as string),
  anchor_action_output_stop: (value) =>
    commands.changeAnchorActionSetting("output_stop", value as string),
  anchor_action_submit_idle: (value) =>
    commands.changeAnchorActionSetting("submit_idle", value as string),
  anchor_action_submit_stop: (value) =>
    commands.changeAnchorActionSetting("submit_stop", value as string),
  anchor_action_output_idle_slot: (value) =>
    commands.changeAnchorActionSlotSetting("output_idle", value as number),
  anchor_action_output_stop_slot: (value) =>
    commands.changeAnchorActionSlotSetting("output_stop", value as number),
  anchor_action_submit_idle_slot: (value) =>
    commands.changeAnchorActionSlotSetting("submit_idle", value as number),
  anchor_action_submit_stop_slot: (value) =>
    commands.changeAnchorActionSlotSetting("submit_stop", value as number),
  // jumper_track_enabled / jumper_track_slot are deprecated (superseded by
  // the per-flow fields below) and have no updater: the backend commands now
  // require a `flow` arg, so there's nothing left to route these to.
  jumper_track_output_enabled: (value) =>
    commands.changeJumperTrackSetting("output", value as boolean),
  jumper_track_output_slot: (value) =>
    commands.changeJumperTrackSlotSetting("output", value as number),
  jumper_track_submit_enabled: (value) =>
    commands.changeJumperTrackSetting("submit", value as boolean),
  jumper_track_submit_slot: (value) =>
    commands.changeJumperTrackSlotSetting("submit", value as number),
  // jumper_save_cursor_slots (Vec<bool>), jumper_cursor_mode_slots
  // (Vec<CursorMode>) and anchor_on_finish_require_same_flow are driven by
  // direct command calls from JumperSettings (commands.changeJumperSaveCursorSlot
  // / changeJumperCursorModeSlot / changeAnchorRequireSameFlow), so they have no
  // updater entries here.
  translator_model_unload_timeout: (value) =>
    commands.changeTranslatorModelUnloadTimeout(value as string),
  translator_model_unload_custom_seconds: (value) =>
    commands.changeTranslatorModelUnloadCustomSeconds(value as number),
  translator_enabled: (value) =>
    commands.changeTranslatorEnabled(value as boolean),
  translator_priority: (value) =>
    commands.changeTranslatorPriority(value as string),
  translator_model: (value) => commands.changeTranslatorModel(value as string),
  jumper_persist: (value) =>
    commands.changeJumperPersistSetting(value as boolean),
  model_unload_custom_seconds: (value) =>
    commands.setModelUnloadCustomSeconds(value as number),
  history_limit: (value) => commands.updateHistoryLimit(value as number),
  post_process_enabled: (value) =>
    commands.changePostProcessEnabledSetting(value as boolean),
  post_process_selected_prompt_id: (value) =>
    commands.setPostProcessSelectedPrompt(value as string),
  mute_while_recording: (value) =>
    commands.changeMuteWhileRecordingSetting(value as boolean),
  append_trailing_space: (value) =>
    commands.changeAppendTrailingSpaceSetting(value as boolean),
  crash_resilient_recording: (value) =>
    commands.changeCrashResilientRecordingSetting(value as boolean),
  log_level: (value) => commands.setLogLevel(value as any),
  app_language: (value) => commands.changeAppLanguageSetting(value as string),
  experimental_enabled: (value) =>
    commands.changeExperimentalEnabledSetting(value as boolean),
  transcription_mode: (value) =>
    commands.changeTranscriptionModeSetting(value as string),
  transcription_mode_ptt: (value) =>
    commands.changeTranscriptionModePttSetting(value as string),
  transcribe_gpu_device: (value) =>
    commands.changeTranscribeGpuDeviceSetting(value as number),
  api_transcription_url: (value) =>
    commands.changeApiTranscriptionUrlSetting(value as string),
  api_transcription_key: (value) =>
    commands.changeApiTranscriptionKeySetting(value as string),
  api_transcription_model: (value) =>
    commands.changeApiTranscriptionModelSetting(value as string),
  openrouter_transcription_provider_ref: (value) =>
    commands.setOpenrouterTranscriptionProviderRef(value as string),
  openrouter_transcription_model: (value) =>
    commands.changeOpenrouterTranscriptionModelSetting(value as string),
  openrouter_transcription_route: (value) =>
    commands.changeOpenrouterTranscriptionRouteSetting(value as string),
  openrouter_transcription_audio_format: (value) =>
    commands.changeOpenrouterTranscriptionAudioFormatSetting(value as string),
  show_tray_icon: (value) =>
    commands.changeShowTrayIconSetting(value as boolean),
  post_process_disable_thinking: (value) =>
    commands.changePostProcessDisableThinkingSetting(value as boolean),
};

export const useSettingsStore = create<SettingsStore>()(
  subscribeWithSelector((set, get) => ({
    settings: null,
    defaultSettings: null,
    isLoading: true,
    isUpdating: {},
    audioDevices: [],
    outputDevices: [],
    customSounds: { start: false, stop: false },

    // Internal setters
    setSettings: (settings) => set({ settings }),
    setDefaultSettings: (defaultSettings) => set({ defaultSettings }),
    setLoading: (isLoading) => set({ isLoading }),
    setUpdating: (key, updating) =>
      set((state) => ({
        isUpdating: { ...state.isUpdating, [key]: updating },
      })),
    setAudioDevices: (audioDevices) => set({ audioDevices }),
    setOutputDevices: (outputDevices) => set({ outputDevices }),
    setCustomSounds: (customSounds) => set({ customSounds }),

    // Getters
    getSetting: (key) => get().settings?.[key],
    isUpdatingKey: (key) => get().isUpdating[key] || false,

    // Load settings from store
    refreshSettings: async () => {
      try {
        const result = await commands.getAppSettings();
        if (result.status === "ok") {
          const settings = result.data;
          const normalizedSettings: Settings = {
            ...settings,
            always_on_microphone: settings.always_on_microphone ?? false,
            selected_microphone: settings.selected_microphone ?? "Default",
            clamshell_microphone: settings.clamshell_microphone ?? "Default",
            selected_output_device:
              settings.selected_output_device ?? "Default",
          };
          set({ settings: normalizedSettings, isLoading: false });
        } else {
          console.error("Failed to load settings:", result.error);
          set({ isLoading: false });
        }
      } catch (error) {
        console.error("Failed to load settings:", error);
        set({ isLoading: false });
      }
    },

    // Load audio devices
    refreshAudioDevices: async () => {
      try {
        const result = await commands.getAvailableMicrophones();
        if (result.status === "ok") {
          const devicesWithDefault = [
            DEFAULT_AUDIO_DEVICE,
            ...result.data.filter(
              (d) => d.name !== "Default" && d.name !== "default",
            ),
          ];
          set({ audioDevices: devicesWithDefault });
        } else {
          set({ audioDevices: [DEFAULT_AUDIO_DEVICE] });
        }
      } catch (error) {
        console.error("Failed to load audio devices:", error);
        set({ audioDevices: [DEFAULT_AUDIO_DEVICE] });
      }
    },

    // Load output devices
    refreshOutputDevices: async () => {
      try {
        const result = await commands.getAvailableOutputDevices();
        if (result.status === "ok") {
          const devicesWithDefault = [
            DEFAULT_AUDIO_DEVICE,
            ...result.data.filter(
              (d) => d.name !== "Default" && d.name !== "default",
            ),
          ];
          set({ outputDevices: devicesWithDefault });
        } else {
          set({ outputDevices: [DEFAULT_AUDIO_DEVICE] });
        }
      } catch (error) {
        console.error("Failed to load output devices:", error);
        set({ outputDevices: [DEFAULT_AUDIO_DEVICE] });
      }
    },

    // Play a test sound
    playTestSound: async (soundType: "start" | "stop") => {
      try {
        await commands.playTestSound(soundType);
      } catch (error) {
        console.error(`Failed to play test sound (${soundType}):`, error);
      }
    },

    checkCustomSounds: async () => {
      try {
        const sounds = await commands.checkCustomSounds();
        get().setCustomSounds(sounds);
      } catch (error) {
        console.error("Failed to check custom sounds:", error);
      }
    },

    // Update a specific setting
    updateSetting: async <K extends keyof Settings>(
      key: K,
      value: Settings[K],
    ) => {
      const { settings, setUpdating } = get();
      const updateKey = String(key);
      const originalValue = settings?.[key];

      setUpdating(updateKey, true);

      try {
        set((state) => ({
          settings: state.settings ? { ...state.settings, [key]: value } : null,
        }));

        const updater = settingUpdaters[key];
        if (updater) {
          await updater(value);
        } else if (key !== "bindings" && key !== "selected_model") {
          console.warn(`No handler for setting: ${String(key)}`);
        }
      } catch (error) {
        console.error(`Failed to update setting ${String(key)}:`, error);
        if (settings) {
          set({ settings: { ...settings, [key]: originalValue } });
        }
      } finally {
        setUpdating(updateKey, false);
      }
    },

    // Reset a setting to its default value
    resetSetting: async (key) => {
      const { defaultSettings } = get();
      if (defaultSettings) {
        const defaultValue = defaultSettings[key];
        if (defaultValue !== undefined) {
          await get().updateSetting(key, defaultValue as any);
        }
      }
    },

    // Update a specific binding
    updateBinding: async (id, binding) => {
      const { settings, setUpdating } = get();
      const updateKey = `binding_${id}`;
      const originalBinding = settings?.bindings?.[id]?.current_binding;

      setUpdating(updateKey, true);

      try {
        // Optimistic update
        set((state) => ({
          settings: state.settings
            ? {
                ...state.settings,
                bindings: {
                  ...state.settings.bindings,
                  [id]: {
                    ...state.settings.bindings[id]!,
                    current_binding: binding,
                  },
                },
              }
            : null,
        }));

        const result = await commands.changeBinding(id, binding);

        // Check if the command executed successfully
        if (result.status === "error") {
          throw new Error(result.error);
        }

        // Check if the binding change was successful
        if (!result.data.success) {
          throw new Error(result.data.error || "Failed to update binding");
        }
      } catch (error) {
        console.error(`Failed to update binding ${id}:`, error);

        // Rollback on error
        if (originalBinding && get().settings) {
          set((state) => ({
            settings: state.settings
              ? {
                  ...state.settings,
                  bindings: {
                    ...state.settings.bindings,
                    [id]: {
                      ...state.settings.bindings[id]!,
                      current_binding: originalBinding,
                    },
                  },
                }
              : null,
          }));
        }

        // Re-throw to let the caller know it failed
        throw error;
      } finally {
        setUpdating(updateKey, false);
      }
    },

    // Reset a specific binding
    resetBinding: async (id) => {
      const { setUpdating, refreshSettings } = get();
      const updateKey = `binding_${id}`;

      setUpdating(updateKey, true);

      try {
        await commands.resetBinding(id);
        await refreshSettings();
      } catch (error) {
        console.error(`Failed to reset binding ${id}:`, error);
      } finally {
        setUpdating(updateKey, false);
      }
    },

    // Load default settings from Rust
    loadDefaultSettings: async () => {
      try {
        const result = await commands.getDefaultSettings();
        if (result.status === "ok") {
          set({ defaultSettings: result.data });
        } else {
          console.error("Failed to load default settings:", result.error);
        }
      } catch (error) {
        console.error("Failed to load default settings:", error);
      }
    },

    // Initialize everything
    initialize: async () => {
      const { refreshSettings, checkCustomSounds, loadDefaultSettings } = get();

      // Note: Audio devices are NOT refreshed here. The frontend (App.tsx)
      // is responsible for calling refreshAudioDevices/refreshOutputDevices
      // after onboarding completes. This avoids triggering permission dialogs
      // on macOS before the user is ready.
      await Promise.all([
        loadDefaultSettings(),
        refreshSettings(),
        checkCustomSounds(),
      ]);
    },
  })),
);
