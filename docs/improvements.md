# The engineering journey

Handy Tool's recent releases were less about adding checkboxes and more about re-engineering the parts users never see — until they fail. This is the honest changelog narrative: what was rebuilt, why, and what it bought.

## Crash-resilient recording (the chunked-Opus rework)

The original pipeline kept your entire recording in memory and wrote it once, at the end. A crash at minute 29 of a 30-minute dictation meant 30 minutes of silence in your history.

The rework changed the physics of the problem:

- Audio is encoded to **Opus on the fly** and flushed to disk in chunks while you speak. Any crash — app, driver, power — leaves recoverable audio; the next launch folds it into history automatically.
- **Transcription no longer waits for the end.** Chunks are transcribed in the background as they close, cut at *silence boundaries* (never mid-word), so stopping a long recording returns text almost immediately.
- Chunk transcriptions are strictly serialized against the engine, so a fast talker on a slow machine can't produce gaps in the final text.
- Storage dropped ~10× versus WAV, and a recording that fits one chunk produces one tidy file.

## A custom GPU-accelerated Whisper

Stock builds lagged upstream whisper.cpp. Handy Tool ships a **custom-built whisper.cpp (v1.8.2+) with Vulkan acceleration** on Windows/Linux — meaning modern integrated GPUs (not just NVIDIA dGPUs) transcribe at interactive speeds — and Metal on macOS. This is what makes Live mode feel real-time and makes push-to-talk largely optional: by the time you'd have released the key, the text is basically there.

## The last-word problem

Transducer/CTC engines (Parakeet, Moonshine, SenseVoice) drop the final word(s) when audio ends abruptly — recordings cut at the stop keystroke did exactly that, and Whisper's immunity disguised it as a model-quality difference. Every non-Whisper local engine now gets **automatic trailing-silence padding** at a single pipeline choke point. The end of your sentence survives, on every engine.

## Delivery that survives remote sessions

"Paste the transcription" sounds trivial until a remote session is involved. Citrix (and slow RDP) fetch clipboard data **on demand, after the paste keystroke arrives** — and the app used to restore your old clipboard ~50 ms after pasting, handing the remote session stale content. The clipboard lifecycle was rebuilt: restoration is asynchronous, generation-guarded (a pending restore can never clobber a newer paste), and its delay is configurable up to 5 seconds. Local pastes are unaffected; Citrix pastes become reliable.

The **Transcribe & Submit** shortcut grew from the same work: one keystroke that finishes the recording, pastes with its own method, presses Enter, and preserves your clipboard — the "dictate into chat and send" flow with no mouse.

## Push-to-talk, audited and hardened

Before opening the project up, the push-to-talk path — largely untouched through the pipeline rework — was put through a full multi-lens audit (architecture, wiring, press/release lifecycle, per-engine pipeline behavior, keyboard backend internals, including the upstream hotkey crates' source). The verdict: the core state machine was sound — release events verified on all platforms, key auto-repeat filtered at three layers, no orphaned recordings. The findings that did surface were fixed:

- **Hot-mic after release**: the recorder now stops the instant you release the key, instead of after in-flight transcription work — post-release chatter and the stop beep no longer leak into your text.
- **macOS default shortcut collision**: the default PTT chord aliased to the main transcribe chord (option *is* alt on macOS), so one of the two silently failed to register each launch. New default plus automatic migration of affected configs.
- **Pipeline decisions are locked at recording start**: changing settings or models mid-recording can no longer reroute (and silently discard) the take at stop time.
- **No silently dead hotkeys**: registration failures now surface as a visible notification.
- **Stuck states eliminated**: a failed recording start rolls back the recording UI; unregistering or rebinding a held shortcut synthesizes the release; a press while the pipeline is busy gives an audible cue instead of losing the utterance.
- Assorted hardening: per-shortcut debounce isolation, accidental-tap detection that ignores padding, live-swap of the cancel hotkey during a recording, external triggers can no longer half-start a hold-based recording.

## Install & environment hardening

- The CLI companion (`handy` on PATH) no longer bootstrap-installs itself; it appears only when you ask for it, and launching it bare forwards to the installed app instead of dying in a console window.
- Backup/restore extracts strictly whitelisted entries — no path traversal, ever.
- The app only deletes files it created; your own notes stored in the recordings folder are untouchable by design.

## Rebrand

With the internals rebuilt, the identity followed: **Handy Tool**, a technocratic hex-badge icon (a waveform becoming a checkmark — voice in, done-task out), and a Rajdhani wordmark. Your data location, settings, and the `handy` binary/CLI name are unchanged — the rebrand is skin-deep by design.
