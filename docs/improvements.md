# The engineering journey

Handy Tool 1.0.0 is the result of re-engineering the parts of a dictation tool nobody notices
until they fail. This page explains how each guarantee was built. What each one _does_ for you
is stated once, in the catalog, and every item below opens with a link to it.

**Platform scope.** Windows x64 is the only build produced, tested and released. macOS and
Linux are planned and in the queue. Where the work below was done in source for those targets,
it says so — none of it has been built or run. See
[What runs today, and what is planned](features.md#what-runs-today-and-what-is-planned).

## Crash-resilient recording (the chunked-Opus rework)

The original pipeline held the whole take in memory and wrote it once, at the end. A crash at
minute 29 of a 30-minute dictation left nothing on disk.

- [A crash mid-dictation costs you nothing](features.md#a-crash-mid-dictation-costs-you-nothing)
  — audio is Opus-encoded and flushed to disk while you speak, on the consumer thread rather
  than in the audio callback, which must stay trivial. The Ogg muxing, repair and glue code is
  hand-written against the byte format, which is what makes a half-written file repairable
  instead of discardable, and testable without an audio device.
- [A thirty-minute dictation is already transcribed when you stop](features.md#a-thirty-minute-dictation-is-already-transcribed-when-you-stop)
  — the hard lesson here was to decouple transcription granularity from file granularity. Tying
  transcription to the ten-minute storage chunks meant a short recording never transcribed until
  it ended. The two cadences are now independent: storage chunks close on size, transcription
  segments close on detected silence, and the two never have to agree.
- [Recordings that don't eat your disk](features.md#recordings-that-dont-eat-your-disk) — Opus
  at speech bitrates replaced 16-bit WAV for roughly a tenth of the bytes, and a take that fits
  in a single chunk is renamed rather than glued, so the common case leaves one file.

## A custom GPU-accelerated Whisper

- [GPU acceleration, including integrated graphics](features.md#gpu-acceleration-including-integrated-graphics)
  — stock builds lagged upstream whisper.cpp, so Handy Tool builds whisper.cpp v1.8.2+ itself
  with Vulkan enabled. An ordinary integrated GPU then transcribes fast enough to read the words
  as you speak them, on a laptop with no discrete card. The planned macOS build would use Metal
  instead; that build has never been produced.

## The last-word problem

- [Every engine keeps your last word](features.md#every-engine-keeps-your-last-word) —
  transducer and CTC engines drop the final word when audio ends abruptly, and Whisper's
  immunity to the problem disguised it as a model-quality difference. The fix is applied at one
  choke point in the transcription manager rather than in each caller, so a future call path
  cannot miss it, and Whisper's audio stays byte-identical.

## Delivery that survives remote sessions

Pasting works until a remote session is involved. Citrix and slow RDP fetch clipboard data on
demand, _after_ the paste keystroke arrives, over a slower channel — and the app used to restore
your previous clipboard 50 ms after pasting, handing the remote session the wrong content.

- [Your remote session pastes the right thing](features.md#your-remote-session-pastes-the-right-thing)
  — the clipboard restore moved off the main thread and behind a generation counter, so a
  pending restore can never overwrite a newer paste and a slow restore can never block the
  interface.
- [Dictate and send in one keystroke](features.md#dictate-and-send-in-one-keystroke) — the same
  work produced the second dictation key, built on the same delivery path so that its own paste
  method, submit key and clipboard policy are configuration rather than a parallel code path.

## Push-to-talk, audited and hardened

Before the project opened up, the push-to-talk path — untouched by the pipeline rework — went
through a multi-lens audit: architecture, wiring, the press-and-release lifecycle, per-engine
behavior, and the source of the hotkey crates underneath. The state machine held up: auto-repeat
is filtered at three layers and no recording is orphaned. Release-event handling was verified on
the shipped Windows build and reviewed in the source paths prepared for the planned macOS and
Linux targets.

- [Push-to-talk you can trust](features.md#push-to-talk-you-can-trust) — the findings that did
  surface were fixed together: the recorder is stopped before in-flight transcription work is
  awaited, the live-versus-chunked decision is snapshotted at recording start and consumed at
  stop rather than re-derived, chunk transcriptions serialize against the engine behind one
  lock, and unregistering a held shortcut synthesizes its release.
- [A hotkey another app already owns](features.md#a-hotkey-another-app-already-owns) —
  registration results are recorded at startup instead of being dropped, so the interface can
  report a shortcut that failed to register rather than leaving you to guess.
- **macOS default shortcut collision — prepared in source, never built.** On macOS, option _is_
  alt, so the default push-to-talk chord would alias to the default transcribe chord and one of
  the two would fail to register on every launch. The default was changed in source and a
  migration for affected configurations was written ahead of the planned macOS build. None of it
  has run on a Mac.

## Install and environment hardening

- [A handy command on your PATH](features.md#a-handy-command-on-your-path) — the companion no
  longer bootstrap-installs itself at startup; installation became an explicit action, and
  launching the command bare forwards to the installed app instead of dying in a console window.
- [A crafted archive can't write outside the app](features.md#a-crafted-archive-cant-write-outside-the-app)
  — restore extracts a whitelist of expected entry names rather than filtering a blocklist, so
  anything unanticipated is skipped by default instead of needing to have been predicted.
- [Your own files in the recordings folder are never touched](features.md#your-own-files-in-the-recordings-folder-are-never-touched)
  — every deletion and rename path goes through one predicate that recognizes Handy Tool's own
  filenames, so retention cleanup, manual deletion and crash recovery cannot drift apart.

## Rebrand

With the internals rebuilt, the identity followed: **Handy Tool**, a hex-badge icon of a
waveform becoming a checkmark, and a Rajdhani wordmark. The data location, the settings and the
`handy` binary and command name are unchanged, so the rebrand moves nothing on your disk.
