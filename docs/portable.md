# Portable Distribution

Handy Tool can be packaged as a portable ZIP — extract it anywhere (a USB
stick, a network share, a folder on the PC) and run `handy.exe` directly, no
installer and no admin rights.

> **Registry note:** enabling **Autostart** in Settings writes a Windows
> `Run`-key entry pointing at the exe's current location (independent of
> packaging). Even with autostart _off_, startup calls the autostart
> library's `disable()`, which _attempts to delete_ a pre-existing `Run`
> value if one is present — so a launch may touch the registry to remove a
> stale entry (this delete is best-effort and its result is not checked).
> Autostart is **disabled in portable mode** (it would write a machine-level
> registry Run entry pointing at a removable path), so a portable launch does
> not add a registry entry. See T-114 for the isolation details.

> **Status: fully wired.** When a `portable.marker` file sits beside
> `handy.exe`, the app reads and writes ALL of its data — settings, history,
> downloaded models, recordings, logs, and WebView storage — inside a `data\`
> folder next to the exe. Nothing lands in `%APPDATA%\pr.handy` or
> `%LOCALAPPDATA%`, and a portable launch does not mutate machine-level state
> (autostart and CLI self-install are suppressed). If `data\` can't be written
> (e.g. a read-only drive), Handy falls back to the normal per-user profile and
> logs a warning. See
> [T-114](../tickets/T-114-portable-distribution.md) for the full spec.

## Building the portable ZIP

Prerequisites: a completed release build (`build.cmd`, run manually per the
project's own convention — never from an automated agent). That produces
`C:\tmp\hb\release\handy.exe` and its `resources\` folder.

```bash
portable.cmd
```

This reads the version out of `src-tauri/tauri.conf.json`, assembles a
staging folder mirroring the installed app's file layout, and produces:

```
C:\tmp\hb\release\Handy-Tool-{version}-portable.zip
```

next to the NSIS/MSI installers the same build already produced.

## What's in the ZIP

```
Handy Tool/
├── handy.exe
├── portable.marker          (empty; see "How detection will work" below)
├── README.txt                (first-run + data-location notes for the end user)
├── resources/
│   ├── default_settings.json
│   ├── handy.png
│   ├── marimba_start.wav, marimba_stop.wav
│   ├── pop_start.wav, pop_stop.wav
│   ├── recording.png, transcribing.png
│   ├── tray_idle.png, tray_idle_dark.png
│   ├── tray_recording.png, tray_recording_dark.png
│   ├── tray_transcribing.png, tray_transcribing_dark.png
│   └── models/
│       └── silero_vad_v4.onnx   (bundled VAD model; the speech-to-text
│                                  model you select downloads on first use)
└── data/                     (currently inert placeholder — see status note)
    ├── models/
    └── recordings/
```

The file list under `resources/` is not a guess — it was read directly off
an installed copy of the app (`%LOCALAPPDATA%\Handy Tool\resources\` on this
machine) so the portable package matches what the installer actually ships.
Notably this **excludes** `resources\icon.ico`, which is present in the raw
Cargo build output but is _not_ copied there by the NSIS installer either —
`portable.cmd` mirrors the installed reality, not the raw build folder.

## First-run model downloads

Only the small voice-activity-detection model
(`resources/models/silero_vad_v4.onnx`) ships in the package. The actual
transcription model you choose in **Settings → Models** (Whisper, Parakeet,
Moonshine, SenseVoice, …) downloads from `https://blob.handy.computer/` the
first time you select/use it — this needs an internet connection once, after
which the model is cached and the app works fully offline.

## Where data lives today (important caveat)

Tauri resolves `AppHandle::path().app_data_dir()` from the app's
`identifier` (`pr.handy`) plus the OS's per-user profile convention — there
is no supported way to override that at runtime from outside the Rust code.
Every persistence call site in Handy (settings, history DB, downloaded
models, recordings, the translator queue) goes through that resolver today,
so a portable-launched `handy.exe` currently behaves exactly like an
installed one: everything lands under `%APPDATA%\pr.handy\`.

The ZIP's `data\` folder is a placeholder for the day the Rust-side change
lands (tracked in T-114), not a working feature yet. `README.txt` inside the
package spells this out for anyone who extracts it, so nobody is surprised
to find their settings in the Windows profile instead of the folder they
extracted.

## How detection will work (once wired)

The chosen mechanism is presence-of-file, not a build flag: a zero-byte
`portable.marker` dropped next to `handy.exe` (this script already writes
one). One `handy.exe` binary serves both distributions — the installer never
creates the marker, so an installed copy is unaffected; a portable copy
checks for it at startup and, if found, redirects all app-data resolution to
a `data\` folder beside the exe instead of the OS profile dir. The exact
call sites that need to change are listed in
[T-114](../tickets/T-114-portable-distribution.md), and a ready-to-wire
helper module already exists at `src-tauri/src/portable.rs` (not yet
referenced from `lib.rs`).

## Uninstalling

Delete the folder. If you also want to remove settings/history/downloaded
models from a portable run made before T-114's Rust change ships, also
delete `%APPDATA%\pr.handy\`.
