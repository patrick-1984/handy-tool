# Features — and the problems they solve

Every feature below exists because dictation broke down somewhere in a real workflow. Each section starts with the problem.

## Global shortcuts: dictate without leaving your work

**Problem:** switching to a dictation window destroys the flow you were dictating about.

Handy Tool lives behind global hotkeys. Press once to record, press again to stop — the transcription is delivered into whatever app you were using. Every shortcut is remappable, and registration failures (another app already owns the combo) are reported instead of silently ignored.

There are three recording shortcuts, because delivery needs differ:

- **Transcribe** — the everyday toggle. Record, stop, paste at the cursor.
- **Push-to-Talk** — hold to record, release to transcribe. A radio-style flow for quick snippets. With live transcription being near-real-time, most users find the toggle covers everything — PTT remains for those who prefer hold semantics. The mic goes cold the instant you release.
- **Transcribe & Submit** — the power shortcut. It finishes any active recording (even one started by another shortcut), pastes with **its own** paste method, presses **its own** submit key (Enter / Ctrl+Enter / Cmd+Enter), and applies **its own** clipboard policy. One keystroke takes you from "still speaking" to "message sent" in a chat box or a command executed in a terminal.

A **Cancel** shortcut (Esc while recording) abandons the take, and pressing a recording key while the previous take is still processing gives an audible "busy" cue instead of silently eating your words.

## Anchor & Deliver (Windows)

**Problem:** while you dictate, you browse. When the transcription lands, your cursor is three windows away from where the text belongs — so you paste into the wrong app, or trek back with the mouse.

Click into the target field once, press **Set Anchor** (`Ctrl+Alt+K`) — the exact window _and control_ are remembered. Keep working anywhere. When your transcription finishes, Handy Tool activates the anchored window, focuses the anchored field, verifies both actually happened (it never pastes blind), delivers the text — including your submit key if the flow has one — and returns you to where you were. The anchor is **one-shot**: consumed by a verified delivery (a "keep anchor" toggle covers repeated dictation into one document).

**Jump to Anchor** (`Ctrl+Alt+J`) is the same bookmark used manually: it teleports you to the anchored field without pasting anything — handy on its own as a "back to my draft" key.

Safety rails: password fields are refused at anchor time; a destroyed target clears the anchor; any failed or unverifiable delivery keeps your text safely on the clipboard (and keeps the anchor) instead of typing into a surprise location; focus is returned only if you didn't switch windows yourself mid-delivery.

## Live vs Post-Recording transcription

**Problem:** long dictations used to be a black box — you spoke for minutes and only found out at the end whether it worked.

- **Live mode** transcribes progressively while you speak, showing text within seconds — effectively real-time on GPU-accelerated Whisper. On stop, the complete audio is re-transcribed once for a final, tail-accurate result (the live text is only a preview and a fallback).
- **Post-Recording mode** (default) records silently and transcribes in the background **as the recording progresses**, chunk by chunk — so even a 30-minute dictation is mostly transcribed by the time you stop.

Both modes are configured independently for the toggle and push-to-talk shortcuts.

## Transcription engines: pick your trade-off

**Problem:** no single engine wins on privacy, speed, quality, and hardware at once.

| Engine                         | Runs                                                             | Why you'd pick it                                                                                        |
| ------------------------------ | ---------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| **Whisper** (small → large-v3) | Local, GPU-accelerated (Vulkan on Windows/Linux, Metal on macOS) | Best all-round quality, fully offline. Ships with a custom-built whisper.cpp for modern GPU acceleration |
| **NVIDIA Parakeet** V2/V3      | Local, CPU-only (~5× realtime)                                   | Excellent accuracy without a GPU; automatic language detection                                           |
| **Moonshine**                  | Local                                                            | Lightweight and fast for short clips                                                                     |
| **SenseVoice**                 | Local                                                            | Strong multilingual coverage                                                                             |
| **FLM**                        | Local, NPU-accelerated                                           | Uses AI accelerator silicon (e.g. AMD Ryzen AI) so your CPU/GPU stay free                                |
| **OpenAI-compatible API**      | Remote                                                           | Bring any `/v1/audio/transcriptions` server — Groq, faster-whisper-server, your own                      |
| **OpenRouter**                 | Remote                                                           | One key, many STT-capable models; per-request cost is tracked into history                               |

Custom Whisper GGML models dropped into the models folder are auto-discovered. Engines that clip trailing words on abrupt audio endings get automatic trailing-silence padding — the last word of your sentence survives every engine.

## Delivery: text lands the way the target app needs it

**Problem:** apps disagree about how text should arrive. Terminals want Shift+Insert or Ctrl+Shift+V, chat boxes want Enter afterwards, and remote desktops fetch the clipboard late.

- **Paste method** per flow: clipboard paste via Ctrl+V, Ctrl+Shift+V, or Shift+Insert; direct typing; an external script hook (Linux); or none.
- **Auto-submit**: optionally press Enter / Ctrl+Enter / Cmd+Enter after pasting — globally, or per-shortcut on Transcribe & Submit.
- **Clipboard preservation**: by default your clipboard is restored after the paste; alternatively keep the transcription on it.
- **Clipboard restore delay** (None → 5 s): remote sessions such as **Citrix and RDP fetch clipboard content on demand, _after_ the paste keystroke arrives**. Restoring your old clipboard too early hands the remote session stale content. The configurable delay closes that race — the fix for "my Citrix session pastes the wrong thing".

## Crash-resilient recording

**Problem:** losing a half-hour dictation to a crash, a dead battery, or an update is unforgivable.

While you speak, audio is continuously encoded to compact **Opus** chunks on disk (~10× smaller than WAV). If the app or the machine dies mid-recording, the next launch recovers the chunks into your history. Recordings are cut at natural silence boundaries — never mid-word. Your own files stored alongside recordings are never touched by retention cleanup: the app only ever deletes files it created.

## History, retention, and backups

**Problem:** "what did I dictate last Tuesday?" — and "how do I move to a new machine?"

Every transcription is stored with its audio, duration, engine, and (for metered engines) cost. Retention is configurable from "keep nothing" to months. One-click backup exports settings + history (optionally with compressed recordings) as a `.tar.gz`; restore is whitelist-only — a crafted archive cannot write outside the app's data.

## Post-processing with your LLM

**Problem:** raw speech has filler words, missing punctuation, and meandering phrasing.

A dedicated shortcut runs the transcription through an LLM you configure (any registered provider — including on-device Apple Intelligence on macOS ARM) with a prompt you control: clean-up, translation, summarization, reformatting. The raw transcription is always preserved in history alongside the processed version.

## Custom vocabulary

**Problem:** engines mangle names, brands, and jargon ("Charge B" for "ChargeBee").

A user dictionary with fuzzy matching (edit-distance + phonetic) corrects transcriptions after the fact — without retraining anything.

## Internationalization

The interface ships in **17 languages** (English source; Arabic, Czech, German, Spanish, French, Italian, Japanese, Korean, Polish, Portuguese, Russian, Turkish, Ukrainian, Vietnamese, Chinese Simplified & Traditional), with RTL support.
