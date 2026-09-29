# Settings reference

This shelf mirrors the Windows 1.10.0 sidebar and its More page. Open a page below when you need the exact control label, location, shipped default, or interaction with another control.

## Sidebar

- [General](general.md) — shortcuts, model-specific choices, transcription, re-paste, sound, the app itself (appearance, the overlay's look), and updates.
- [Setups](setups.md) — the guided setups, starting with the first-start one.
- [Shortcuts](shortcuts.md) — every shortcut on one page, with duplicates flagged.
- [Models](models.md) — downloads, selection, filtering, and idle unloading.
- [History](history.md) — saved transcription rows, the recordings folder, and retention.
- [Files](files.md) — transcribe a file you pick, and the watched folders.
- [Jumper](jumper.md) — Windows-only anchors, slots, cursor options, and remote matching.

## More › Settings

- [Output, Providers, Post-processing, MCP & CLI](advanced.md) — how text is delivered, speech and LLM providers, and the local server.
- [Post-processing](post-processing.md) — provider, prompt, and generation controls under the Post-processing switch.
- [Backup](backup.md) — export and selective restore.
- [Debug](debug.md) — logging and low-level timing or device controls.
- [About](about.md) — language, version, source, and data locations.

## More › Tools

- [Keyboard Typer](keyboard-typer.md) — the in-memory text buffer and typing timing.
- [Token Count](token-count.md) — local and provider-backed counting actions.
- [Model Testing](model-testing.md) — run, judge, prompt, image, and report controls.
- [Current Audio](current-audio.md) — the live transcript and floating window.

## Hidden controls

Set `More › Post-processing › Post Processing = On` to reveal post-processing's hotkey, provider, and prompt controls on that tab. They carry _{requires: Post-processing enabled}_.

Press `ctrl+shift+d` to enable debug mode and reveal the `Debug` tab on More. Its controls carry _{requires: Debug mode}_. Press the chord again to hide the tab; debug mode defaults to off.

## Controls with no working interface

Four fields in the settings file look like language options for the API and OpenRouter speech engines, but nothing reads them: those engines use the `Language` and `Translate to English` controls on [General](general.md#language). Editing the four fields changes nothing.

The watched-folder scan interval is store-only in a different way — it works, but it has no control. `translator_poll_secs` defaults to `15` seconds and is changed by editing `File › %APPDATA%\pr.handy\settings_store.json` and restarting the app.

Every other field in that file is written by a control on one of the pages above. Leave the rest alone.
