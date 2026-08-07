# Portable Distribution

**Windows only.** Portable mode exists in the Windows x64 build. It will not arrive with the
planned macOS and Linux builds — see
[What runs today, and what is planned](features.md#what-runs-today-and-what-is-planned).

**Status: shipped in 1.0.0.**

Handy Tool can be packaged as a portable ZIP: extract it anywhere — a USB stick, a network
share, a folder on the PC — and run `handy.exe` from there, with no installer and no admin
rights. What a portable launch does with your data, what it deliberately does not write, and
what happens when the folder beside the executable cannot be written, is described in
[Run it from a USB stick and leave no trace](features.md#run-it-from-a-usb-stick).

To move a normal installation between machines instead of running from a stick, see
[Backup and portable mode](tools/backup-and-portable.md).

## What's in the ZIP

```
Handy Tool/
├── handy.exe
├── portable.marker           (empty; its presence is what selects portable mode)
├── README.txt                (first-run and data-location notes for the end user)
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
│       └── silero_vad_v4.onnx   (bundled voice-activity model; the speech-to-text
│                                 model you select downloads on first use)
└── data/                     (portable state: settings, history, downloaded models,
    ├── models/                recordings, logs, and web-view storage)
    └── recordings/
```

The `resources/` list mirrors the layout the installer produces on disk. It excludes
`resources\icon.ico`, which is present in the raw build output but which the installer does not
place there either.

## First-run model downloads

Only the small voice-activity-detection model (`resources/models/silero_vad_v4.onnx`) ships
inside the package. Handy-managed speech-to-text models — Whisper, Parakeet, Moonshine and
SenseVoice — download from `https://blob.handy.computer/` the first time you select one on the
`Models` page. That needs an internet connection once; afterwards the model is cached in
`data\models\` and dictation works offline. For the external FastFlowLM (FLM) NPU engine, see
[Transcribe without tying up the CPU or GPU](features.md#use-the-npu-in-your-laptop).

## Building the package

The portable ZIP is assembled by `portable.cmd` at the repository root from a completed release
build; it does not build the application itself. See [BUILD.md](../BUILD.md).

## Uninstalling

Delete the folder. A portable launch keeps its settings, history, models, recordings and logs
inside `data\` beside the executable, so nothing else remains.
