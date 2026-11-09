// Made-up data for the README captures: no real names, texts or keys.
import { readFileSync } from "fs";

const here = new URL(".", import.meta.url);
export const D = JSON.parse(
  readFileSync(new URL("defaults.json", here), "utf8"),
);
export const MOCK = readFileSync(new URL("mock.js", here), "utf8");

const b = (id, name, description, key) => ({
  id,
  name,
  description,
  default_binding: key,
  current_binding: key,
});
const bindings = Object.fromEntries(
  [
    b(
      "transcribe",
      "Transcribe",
      "Converts your speech into text.",
      "ctrl+space",
    ),
    b(
      "transcribe_ptt",
      "Push-to-Talk",
      "Hold to record, release to stop.",
      "ctrl+shift+space",
    ),
    b(
      "transcribe_with_post_process",
      "Transcribe with Post-Processing",
      "Converts your speech into text and applies AI post-processing.",
      "ctrl+shift+f8",
    ),
    b("cancel", "Cancel", "Cancels the current recording.", "escape"),
    b(
      "pause",
      "Pause / Resume",
      "Pauses the recording in progress, or resumes it.",
      "",
    ),
    b(
      "undo_word",
      "Undo Last Word",
      "Removes the last word of a live take.",
      "ctrl+backspace",
    ),
    b(
      "toggle_live_text_box",
      "Live Text Box On/Off",
      "Shows or hides the live text box.",
      "",
    ),
    b(
      "type_text",
      "Type Text",
      "Types the Keyboard Typer text into the focused window.",
      "ctrl+shift+f7",
    ),
    b(
      "anchor_set",
      "Set Anchor",
      "Anchors the focused text field as the delivery target.",
      "ctrl+shift+f1",
    ),
    b(
      "anchor_jump",
      "Jump to Anchor",
      "Brings the anchored window and field into focus.",
      "ctrl+shift+f2",
    ),
    b(
      "anchor_set_2",
      "Set Anchor 2",
      "Anchors the focused text field as the second delivery target.",
      "ctrl+shift+f3",
    ),
    b(
      "anchor_jump_2",
      "Jump to Anchor 2",
      "Brings the second anchored window and field into focus.",
      "ctrl+shift+f4",
    ),
    ...[1, 2, 3, 4, 5, 6, 7, 8, 9].flatMap((i) => [
      b(
        `jump_set_slot_${i}`,
        `Set Jump Slot ${i}`,
        `Memorizes the focused field as jump slot ${i}.`,
        `ctrl+alt+shift+${i}`,
      ),
      b(
        `jump_slot_${i}`,
        `Jump to Slot ${i}`,
        `Brings the window and field of jump slot ${i} into focus.`,
        `ctrl+alt+${i}`,
      ),
    ]),
    b(
      "transcribe_and_submit",
      "Transcribe & Submit",
      "Finish recording, paste the transcription, then press the submit key.",
      "ctrl+shift+f9",
    ),
    b(
      "paste_last",
      "Paste Last Transcription",
      "Pastes the most recent transcription again.",
      "ctrl+shift+f10",
    ),
  ].map((x) => [x.id, x]),
);

export const settings = (over = {}) => ({
  ...D.types.AppSettings,
  bindings,
  selected_model: "parakeet-tdt-0.6b-v3",
  selected_language: "auto",
  app_language: "en",
  pill_scale: 100,
  progress_style: "line",
  progress_glow: 100,
  sound_bars_wide: false,
  audio_feedback: true,
  audio_feedback_volume: 0.6,
  automatic_update_checks: false,
  history_limit: 500,
  ...over,
});

const m = (id, name, size, engine, acc, speed, extra = {}) => ({
  id,
  name,
  description: "",
  filename: id,
  url: null,
  size_mb: size,
  is_downloaded: false,
  is_downloading: false,
  partial_size: 0,
  is_directory: engine !== "Whisper",
  engine_type: engine,
  accuracy_score: acc,
  speed_score: speed,
  supports_translation: engine === "Whisper",
  is_recommended: false,
  supported_languages: ["en", "de", "fr", "es", "pl", "it", "pt", "nl"],
  is_custom: false,
  ...extra,
});
export const models = (over = {}) =>
  [
    m("parakeet-tdt-0.6b-v3", "Parakeet V3", 478, "Parakeet", 0.96, 0.82, {
      is_downloaded: true,
      is_recommended: true,
    }),
    m("parakeet-tdt-0.6b-v2", "Parakeet V2", 473, "Parakeet", 0.98, 0.82, {
      supported_languages: ["en"],
    }),
    m("turbo", "Whisper Large V3 Turbo", 1600, "Whisper", 0.84, 0.44, {
      is_downloaded: true,
    }),
    m("large", "Whisper Large V3", 1100, "Whisper", 0.94, 0.22),
    m("small", "Whisper Small", 487, "Whisper", 0.62, 0.6),
    m(
      "moonshine-small-streaming-en",
      "Moonshine V2 Small",
      100,
      "MoonshineStreaming",
      0.84,
      0.84,
      { supported_languages: ["en"] },
    ),
    m("sense-voice-int8", "SenseVoice", 160, "SenseVoice", 0.64, 0.96, {
      supported_languages: ["zh", "en", "ja", "ko"],
    }),
    m(
      "openrouter-transcription",
      "OpenRouter Transcription",
      0,
      "OpenRouterWhisper",
      0.92,
      0.75,
      { is_downloaded: true, is_directory: false },
    ),
    m(
      "api-whisper",
      "API Transcription (OpenAI-compatible)",
      0,
      "ApiWhisper",
      0.9,
      0.8,
      { is_downloaded: true, is_directory: false },
    ),
    m("stt-server", "Custom STT server", 0, "SttServer", 0, 0, {
      is_downloaded: true,
      is_directory: false,
    }),
  ].map((x) => ({ ...x, ...(over[x.id] || {}) }));

const now = Math.floor(Date.now() / 1000);
let nextId = 100;
const h = (minsAgo, secs, model, text, extra = {}) => ({
  id: nextId++,
  file_name: `handy-${now - minsAgo * 60}.opus`,
  timestamp: now - minsAgo * 60,
  saved: false,
  title: "",
  transcription_text: text,
  post_processed_text: null,
  post_process_prompt: null,
  cost_usd: null,
  duration_seconds: secs,
  model_used: model,
  audio_purged_at: null,
  diarized_text: null,
  name: null,
  ...extra,
});
export const history = () => [
  h(
    3,
    14,
    "Parakeet V3",
    "Can we move the design review to Thursday afternoon? The new onboarding screens need one more pass, and the budget for them is open.",
  ),
  h(
    26,
    41,
    "Parakeet V3",
    "Quick summary of the budget call: the cloud bill went down eleven percent after we switched the nightly jobs to spot instances. Marketing wants the same budget as last quarter, and the budget for the offsite is still open. Next step is a short budget note for Friday.",
    { name: "Budget call", saved: true },
  ),
  h(
    95,
    9,
    "Whisper Large V3 Turbo",
    "Remind me to renew the domain before the end of the month.",
  ),
  h(
    240,
    73,
    "OpenRouter · openai/whisper-large-v3",
    "Notes for the release: History search now shows the whole text, counts every hit and sorts by the most hits. The budget for the docs work is approved, so the screenshots can be redone this week.",
    { cost_usd: 0.0021 },
  ),
  h(
    1440,
    128,
    "Custom STT server",
    "Weekly sync. We agreed to ship on Tuesday and to keep the budget for testing.",
    {
      diarized_text:
        "A: Weekly sync. Are we still on track for Tuesday?\nB: Yes, we ship on Tuesday.\nA: Then we keep the budget for testing.",
      name: "Weekly sync",
    },
  ),
  h(
    2900,
    22,
    "Parakeet V3",
    "Draft reply: thanks for the quick turnaround, the numbers look right to me.",
  ),
  h(
    5000,
    56,
    "Whisper Large V3 Turbo",
    "Idea for the blog: how dictation changes the way you write long emails. Start with the first draft, then the edit.",
  ),
];

/** The init script for a page: data + the IPC mock. */
export const initScript = (handlers, label = "main", fnSource = "") =>
  `window.__RM__ = ${JSON.stringify({ commands: D.commands, handlers, label })};\n` +
  (fnSource ? `Object.assign(window.__RM__.handlers, (${fnSource}));\n` : "") +
  MOCK;

export const baseHandlers = (over = {}) => {
  const s = settings(over.settings);
  return {
    get_app_settings: s,
    get_default_settings: s,
    has_any_models_available: true,
    get_available_models: models(over.models),
    get_current_model: s.selected_model,
    get_history_entries: history(),
    get_model_load_status: { is_loaded: true, current_model: s.selected_model },
    get_transcription_model_status: s.selected_model,
    get_updater_status: { ...D.commands.get_updater_status, state: "idle" },
    is_recording: false,
    ...(over.handlers || {}),
  };
};
