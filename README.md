# Handy Tool

**Press a key. Speak. Your words land exactly where you need them — typed, pasted, or submitted.**

Handy Tool is a cross-platform desktop speech-to-text app (Windows, macOS, Linux) built with Tauri 2 — a Rust core with a React interface. It runs your transcription **locally by default**: your voice never has to leave your machine, and the app works offline. When you want cloud quality instead, point it at any OpenAI-compatible endpoint or OpenRouter with two settings.

It began as a fork of [Handy](https://github.com/cjpais/Handy) **v0.8.1** (March 2026), created by **[CJ Pais](https://github.com/cjpais)** — the foundation this project stands on — and has since been developed independently by Patrick R, growing a substantially reworked recording pipeline, new productivity tools, and a hardened shortcut system.

> **Platform note:** development and testing happen on **Windows**. macOS and Linux code paths are inherited from upstream and kept compiling, but are currently untested — treat those builds as experimental.

## Why it exists

Dictation tools usually fail in one of three ways: they send your audio to someone else's server, they lose your words when something crashes, or they dump text wherever the cursor happens to be and leave the cleanup to you. Handy Tool is built around fixing all three — local processing, crash-resilient recording, and precise delivery of the result (paste method, auto-submit, clipboard preservation, remote-session support).

## Documentation

| Document                             | What it covers                                                                                                 |
| ------------------------------------ | -------------------------------------------------------------------------------------------------------------- |
| [Features](docs/features.md)         | Every feature, framed as the problem it solves — recording, transcription modes, engines, delivery, resilience |
| [Tools](docs/tools.md)               | The built-in toolbox: Model Testing & Judge, Keyboard Typer, Token Counter, MCP/CLI server                     |
| [Improvements](docs/improvements.md) | The engineering journey — what was rebuilt, why, and what it bought                                            |
| [Building](BUILD.md)                 | Build instructions for all platforms                                                                           |

## At a glance

- **One-keystroke dictation** anywhere in the OS, with configurable global shortcuts (toggle, push-to-talk, and finish-and-submit variants)
- **Local engines**: Whisper (GPU-accelerated via Vulkan/Metal), NVIDIA Parakeet, Moonshine, SenseVoice — plus NPU-accelerated FLM, any OpenAI-compatible API, and OpenRouter
- **Near-real-time transcription**: live mode transcribes while you speak
- **Crash-resilient recording**: audio is written to compact Opus chunks as you talk; a crash mid-dictation loses nothing
- **Precise delivery**: per-shortcut paste method (Ctrl+V / Shift+Insert / direct typing), optional auto-submit key, clipboard preservation with remote-session-aware restore delay
- **The Jumper**: anchor text fields anywhere on the desktop and deliver dictation into them — five slots, per-flow actions, verified focus before any keystroke
- **The Translator**: watch folders and batch-transcribe new recordings into `.txt` sidecars, sharing the engine with live dictation under a configurable priority policy
- **A toolbox for LLM work**: side-by-side model testing with a judge panel, token counting across providers, a secure keyboard typer, and an MCP server so agents can drive the app
- **17 interface languages**

## Installing

Grab the latest Windows installer from [Releases](https://github.com/patrick-1984/handy-tool/releases). To build from source, see [BUILD.md](BUILD.md).

## Privacy posture

Local models process audio entirely on your machine. History, settings, and recordings live in your user profile and never sync anywhere. The optional MCP/CLI server binds to `127.0.0.1` only and requires a bearer token. API keys you configure are write-only through the MCP surface — they can be set, never read back.

## License and acknowledgments

MIT — see [LICENSE](LICENSE). Vendored third-party code keeps its own licenses in place: whisper.cpp is MIT, whisper-rs is Unlicense/MIT, transcribe-rs is MIT, some ggml GPU-backend sources carry Apache-2.0 WITH LLVM-exception headers, and the bundled Rajdhani font is SIL OFL 1.1 (`src/assets/fonts/`).

Handy Tool is a fork of [Handy](https://github.com/cjpais/Handy) by [CJ Pais](https://github.com/cjpais) ([handy.computer](https://handy.computer)). Huge thanks to CJ and the Handy contributors for the foundation this project stands on. Speech recognition is powered by [whisper.cpp](https://github.com/ggerganov/whisper.cpp) (Georgi Gerganov and contributors) via a vendored [whisper-rs](https://codeberg.org/tazz4843/whisper-rs), plus [transcribe-rs](https://github.com/cjpais/transcribe-rs) for the Parakeet/Moonshine/SenseVoice engines.
