# Settings reference

This shelf mirrors the Windows 1.0.0 sidebar. Open a page below when you need the exact control label, location, shipped default, or interaction with another control.

## Tools

- [History](history.md) — controls on saved transcription rows and the recordings folder.
- [Model Testing](model-testing.md) — run, judge, prompt, image, and report controls.
- [Keyboard Typer](keyboard-typer.md) — the in-memory text buffer and typing timing.
- [Token Count](token-count.md) — local and provider-backed counting actions.
- [Jumper](jumper.md) — Windows-only anchors, slots, cursor options, and remote matching.
- [Translator](translator.md) — folder watching, batch priority, and batch model controls.
- [Current Audio](current-audio.md) — the live transcript and floating window.

## Configuration

- [General](general.md) — shortcuts, model-specific choices, transcription, re-paste, and sound.
- [Models](models.md) — downloads, selection, filtering, and idle unloading.
- [Advanced](advanced.md) — App, Transcription, Providers, MCP & CLI, History, and Post-processing.
- [Backup](backup.md) — export and selective restore.
- [Post Process](post-processing.md) — provider, prompt, and generation controls.
- [Debug](debug.md) — logging and low-level timing or device controls.
- [About](about.md) — language, version, source, releases, and data locations.

## Hidden pages

Enable `Advanced › Post-processing › Post Processing = On` to reveal the `Post Process` page. Its controls carry *{requires: Post-processing enabled}*.

Press `ctrl+shift+d` to enable debug mode and reveal the `Debug` page. Its controls carry *{requires: Debug mode}*. Press the chord again to hide the page; debug mode defaults to off.

## Controls with no working interface

Four fields in the settings file look like language options for the API and OpenRouter speech engines, but nothing reads them: those engines use the `Language` and `Translate to English` controls on [General](general.md#language). Editing the four fields changes nothing.

The Translator scan interval is store-only in a different way — it works, but it has no control. `translator_poll_secs` defaults to `15` seconds and is changed by editing `File › %APPDATA%\pr.handy\settings_store.json` and restarting the app.

Every other field in that file is written by a control on one of the pages above. Leave the rest alone.
