# Handy Tool

**Press a key. Speak. Your words land exactly where you need them — typed, pasted, or submitted.**

Handy Tool is a cross-platform desktop speech-to-text app (Windows, macOS, Linux) built with Tauri 2 — a Rust core with a React interface. It runs your transcription **locally by default**: your voice never has to leave your machine, and the app works offline. When you want cloud quality instead, point it at any OpenAI-compatible endpoint or OpenRouter with two settings.

It began as a fork of [Handy](https://github.com/cjpais/Handy) **v0.8.1** (March 2026), created by **[CJ Pais](https://github.com/cjpais)** — the foundation this project stands on — and has since been developed independently by Patrick R, growing a substantially reworked recording pipeline, new productivity tools, and a hardened shortcut system.

## Why it exists

Dictation tools usually fail in one of three ways: they send your audio to someone else's server, they lose your words when something crashes, or they dump text wherever the cursor happens to be and leave the cleanup to you. Handy Tool is built around fixing all three — local processing, crash-resilient recording, and precise delivery of the result (paste method, auto-submit, clipboard preservation, remote-session support).

## Documentation

| Document | What it covers |
|---|---|
| [Features](features.md) | Every feature, framed as the problem it solves — recording, transcription modes, engines, delivery, resilience |
| [Tools](tools.md) | The built-in toolbox: Model Testing & Judge, Keyboard Typer, Token Counter, MCP/CLI server |
| [Improvements](improvements.md) | The engineering journey — what was rebuilt, why, and what it bought |
| [Portable Distribution](portable.md) | Running Handy Tool from a folder/USB stick with no installer — current status and how to build the ZIP |

## At a glance

- **One-keystroke dictation** anywhere in the OS, with configurable global shortcuts (toggle, push-to-talk, and finish-and-submit variants)
- **Local engines**: Whisper (GPU-accelerated via Vulkan/Metal), NVIDIA Parakeet, Moonshine, SenseVoice — plus NPU-accelerated FLM, any OpenAI-compatible API, and OpenRouter
- **Near-real-time transcription**: live mode transcribes while you speak
- **Crash-resilient recording**: audio is written to compact Opus chunks as you talk; a crash mid-dictation loses nothing
- **Precise delivery**: per-shortcut paste method (Ctrl+V / Shift+Insert / direct typing), optional auto-submit key, clipboard preservation with remote-session-aware restore delay
- **A toolbox for LLM work**: side-by-side model testing with a judge panel, token counting across providers, a secure keyboard typer, and an MCP server so agents can drive the app
- **17 interface languages**

## Privacy posture

Local models process audio entirely on your machine. History, settings, and recordings live in your user profile and never sync anywhere. The optional MCP/CLI server binds to `127.0.0.1` only and requires a bearer token. API keys you configure are write-only through the MCP surface — they can be set, never read back.
