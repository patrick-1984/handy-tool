# Privacy and data flow

This page describes Handy Tool 1.0.0’s current source behavior, audited on 6 August 2026. It does not describe third-party model providers, Windows, destination applications, accessibility software, clipboard managers, security products, cloud-backup tools, or external executables.

With a downloaded local Whisper or Parakeet model selected, normal dictation sends no microphone audio or transcript over the network. VAD and transcription run locally. Data is still stored: recordings, transcripts, settings, logs, and credentials are local files that Handy does not encrypt.

See [The app makes no calls you didn't ask for](features.md#the-app-makes-no-calls-you-didnt-ask-for) and [Where your data lives on disk](features.md#where-your-data-lives-on-disk).

## What stays on the machine by default

After you explicitly download and select a local model:

- Bundled Silero VAD runs in process and filters microphone frames before retention. Smoothing can retain some silence around speech.
- Local Whisper and Parakeet consume in-memory audio directly and make no HTTP request.
- Crash-resilient recording is on. VAD-retained audio is written incrementally as mono 16 kHz Ogg/Opus.
- The default post-recording pipeline transcribes closed chunks locally and assembles text in process.
- LLM post-processing is off, no post-processing provider is selected, and MCP/CLI is off.
- Keyboard Typer keeps text in process memory, not settings or a file, and does not log its content. It does not clear or zeroize that memory after typing; the value can remain until replaced or the process exits. Windows input facilities, the destination, pagefile, and crash dumps are outside that promise. See [The text never touches your disk](features.md#the-text-never-touches-your-disk).
- Unsaved model-test results normally remain in frontend/process state. They become files or settings data when you save a Markdown report, use an MCP `save_path`, or save prompts/images into the model-test library.

A fresh installation has no selected model and cannot transcribe. Choosing a model makes an explicit HTTPS download request. It reveals ordinary IP/TLS/HTTP metadata and the chosen download to `blob.handy.computer`, but contains no microphone audio or transcript.

Handy has no telemetry, analytics, crash reporting, account, or identifier. Update checks make one plain HTTPS GET for the static public `latest.json` file on the project's GitHub releases. The request sends no information about the user, machine, configuration, or usage. GitHub serves that request like any other download and therefore receives ordinary web-request metadata such as the IP address. Downloading an available update fetches the release asset from the same public GitHub release. The updater's locally generated random value only chooses the nightly timing offset; it is never transmitted and is not an identifier.

## What can leave the machine

Nothing below happens during normal dictation with a downloaded local model. Each row requires the listed choice.

| Choice | Data transmitted | Limits |
| --- | --- | --- |
| Select API Transcription and configure its URL | The complete VAD-retained recording at stop, as 16 kHz/16-bit WAV multipart data, plus model and global language. A configured bearer key is included. | It sends audio, not a locally generated transcript. Translation can first use the translations route. |
| Select OpenRouter Transcription and configure URL/key | The complete VAD-retained recording at stop, base64 JSON, plus model, route, language or instructions. STT uses WAV; Chat can use WAV or Ogg/Opus. Headers identify Handy. | Stale comments say per-segment; the current planner sends one assembled recording at stop. |
| Enable post-processing, select a provider/prompt, and use its shortcut | Full transcript, system prompt, model parameters, temperature/reasoning values, and credentials. | Audio is not sent. The selected provider reference runs even when its registry `enabled` flag is off. |
| Run Model Testing | Prompts, provider/model parameters, and optional image. Judges also receive instructions, the original prompt, and candidate outputs. | History and dictation audio are not added automatically. |
| Count tokens with a configured provider or Count All | Complete submitted text. OpenAI-compatible calibration sends another request containing `a`. | Built-in tiktoken/estimate counting is offline. MCP and `handy token-count` use the offline command. |
| Open or refresh a provider model picker | A GET to the configured provider, possibly with its API key. | No prompt, transcript, or audio. |
| Commit certain remote model names without a manual price | A public GET to OpenRouter’s catalogue, cached in WebView local storage for 24 hours. | No API key or user content. |
| Explicitly download a model | HTTPS GET and a byte range when resuming. | No speech, transcript, prompt, history, setting, or key. |

Provider base URLs are editable and not restricted to HTTPS. A remote `http://` URL sends content and credentials without transport encryption.

The optional FLM Whisper engine starts a separate third-party process and sends WAV audio only to it over `127.0.0.1:52625`. Handy sends no FLM transcription audio off the machine, and it does not bundle, install, or download FLM. See [Transcribe without tying up the CPU or GPU](features.md#use-the-npu-in-your-laptop).

Dedicated per-engine API/OpenRouter language fields exist but are unused by the current request path. Requests read the global selected language and translate-to-English values. This is current 1.0.0 behavior.

## Where data is stored

Normal Windows state is under `File › %APPDATA%\pr.handy`. Use `About › App Data Directory` to open the active location. With a usable `portable.marker` beside the executable, state goes to an adjacent `data` directory; if it cannot be created or written, Handy falls back to the profile location.

| Data | Content | Retention |
| --- | --- | --- |
| `settings_store.json` | Settings, shortcuts, provider URLs/models, API/LLM keys, MCP token, prompts, model-test library and embedded images, retention choices, and UI state. | No TTL; remains until changed, reset, deleted, or restored. |
| `history.db` | Raw/post-processed text, post-processing prompt, title, timestamp, saved state, filename, cost, duration, and model. | Automatic cleanup applies only to unsaved rows. |
| `recordings\` | Default Ogg/Opus; WAV when crash resilience is off; temporary Opus chunks in progress. | Follows history cleanup when deletion succeeds. Saved entries remain. |
| `models\` | Downloaded models and resumable partial archives. | Until explicitly deleted. |
| File logs | Rotated logs that can contain sensitive previews or complete text. | Rotate at 10 MB; `KeepAll` retains rotated files. History cleanup does not remove them. |
| WebView local storage | UI state, last model-test save path, sidebar width, and cached public prices. | Until WebView storage is cleared. |

Default cleanup preserves the five newest unsaved history entries and recordings. It runs after saving an entry and when retention/count changes, not continuously. It deletes the database row before the audio file, so a failed file deletion can leave an orphan. It refuses filenames outside Handy’s recording pattern.

Long takes split around ten minutes. After a clean finish, non-temporary chunks and a glued recording can coexist until cleanup, temporarily duplicating encoded audio.

## Logs can contain your text

Release builds default to **Info**, which does not record transcript content. This changed in 1.0.0; before that, release builds logged at Debug.

**An upgraded install may still be logging at Debug.** The new default only applies when no level has been saved, and a profile created by 0.63.0 or earlier has `log_level` written into its settings from back when Debug was the default. A saved value always wins, so upgrading does not lower the level for you — check it once if your profile predates 1.0.0.

At Debug, logs can record transcript fragments, complete API-transcription responses, complete final transcriptions in some flows, and LLM prompt/transcript previews.

Use `About › Log Directory` to inspect files. Press `ctrl+shift+d` to reveal the page, then change `Debug › Log Level` *{requires: Debug mode}*. Lowering the level reduces future detail but does not erase existing logs.

See [The logs still exist when you finally need them](features.md#the-logs-still-exist-when-you-need-them).

## Backups contain secrets

Backups are gzip-compressed tar archives, not encrypted archives.

- Configuration backup contains `settings_store.json` and `history.db`.
- Full backup adds `.opus` and `.ogg` files from `recordings`.
- Both exclude downloaded models.
- Full backup still excludes WAV, FLAC, and `-temp.opus`. When crash resilience was off, WAV audio can be absent while metadata remains in `history.db`.
- Non-temporary chunks and the glued Opus file from one long take can both be included.
- Stored API keys and the MCP bearer token are in settings, so either backup profile includes them.

Treat every backup as sensitive. Restore uses a filename whitelist and can restore configuration/history separately from recordings, but replacing settings/history requires a restart. See [What a backup deliberately leaves out](features.md#what-a-backup-deliberately-leaves-out).

## Localhost MCP/CLI server

The server is off by default and defaults to port 8765. Enable it at `Advanced › MCP & CLI › Enable MCP & CLI server = On` only when an agent or CLI client needs it.

It binds to `127.0.0.1`, not a LAN/wildcard address, and exposes:

- authenticated `POST /mcp` for MCP JSON-RPC;
- authenticated `POST /cli/<tool>` for CLI calls;
- unauthenticated `GET /health` for liveness and version.

Every POST requires an exact bearer token. The generated UUID is plaintext in `settings_store.json` and `handy-mcp.json`, and is displayed for client setup. The sidecar also contains port and process ID. Orderly shutdown removes it; a crash can leave it behind.

Loopback blocks direct LAN access but does not isolate Handy from other processes running as your Windows user. A process that reads the token can invoke tools that expose transcript snippets, full history and local audio paths; change provider settings; run remote model tests; save a report to a caller path; clear Jumper slots; or enable Translator. The server has no direct start/stop-recording tools in 1.0.0.

Traffic is plain HTTP over loopback. Provider API keys are write-only in MCP responses—clients can set one but later receive only `has_api_key`—yet remain plaintext at rest. The MCP token is not write-only.

MCP/CLI `keyboard_type` uses ordinary clipboard paste delivery. It is separate from in-memory Keyboard Typer and can expose text through clipboard and Debug logs. See [Your agent can read your history — know that before you enable it](features.md#your-agent-can-read-your-history).

## Clipboard behaviour

The Windows default is Ctrl+V with restoration of the previous clipboard text. “Do not modify” controls the final state; it does not mean the transcript never enters the clipboard.

A default paste:

1. reads current clipboard text; if it recognizes one of Handy's own transcripts, it keeps the real original instead, while a failed read becomes an empty string;
2. writes the transcript as text;
3. waits 60 ms;
4. sends Ctrl+V;
5. waits another 50 ms in the background, then restores captured text only if Handy still owns the clipboard; something you copy during the delay is left alone, and a failed restore is retried before a warn-level log entry.

That puts the transcript on the real system clipboard for roughly 100–300 ms, depending on the restore delay. Clipboard history/sync, managers, RDP/Citrix clipboard redirection, and any other watcher can still capture it in that window. With a clipboard paste method, RDP or Citrix redirection can copy the transcript to the remote machine's separate clipboard and clipboard history. Restoring the local clipboard cannot remove that remote copy because Handy has no access to the remote operating system's clipboard. Copy-to-clipboard mode skips restoration.

Typing is the only delivery path that never reads or writes a clipboard on either machine. `Direct` uses batched simulated keystrokes and must be selected explicitly as your paste method. There is no automatic version: 1.2.0 briefly substituted typing for recognized remote-desktop targets by default, and 1.3.0 withdrew that because batched injection delivered text unreliably over RDP. Typing is not a general upgrade even when you choose it: it can trigger autocomplete, bracket auto-closing and IME behavior that a paste does not, it creates many undo steps instead of one, it can drop or reorder characters in remote sessions and VM consoles, and every line break is sent as an Enter key press, which submits in a form and sends in a chat box. The practical consequence for privacy is that a remote-desktop delivery normally uses a clipboard paste method, so the transcript does cross to the remote machine's clipboard and its clipboard history, and nothing on this side can retract it.

The limits are material:

- Only text is preserved. Images, file lists, HTML, rich formatting, and other formats are not restored. A non-text clipboard or failed read can become an empty string.
- A crash, forced exit, or restore failure after retries can leave the transcript on the clipboard.
- A newer Handy paste or a user copy suppresses an older pending restore; Handy leaves the newer clipboard content alone.
- A failed normal or anchored delivery under `Don't Modify Clipboard` does not park the transcript. The take was saved to History before delivery, and the failure toast names the bound Paste Last Transcription shortcut when available or points to the History page. A clipboard-modifying policy can still leave the transcript available for manual recovery.
- RDP/Citrix can fetch data after the paste keystroke. Restoring too early can make the target receive old clipboard text. If it was a secret, it can land in the wrong remote field. This applies to every remote-desktop delivery, because a remote target uses your configured paste method. Only selecting `Direct` explicitly avoids the clipboard, with the reliability costs listed above.

With sensitive clipboard data, worst cases are loss of the old clipboard, capture of the transcript, or—especially over a slow remote session—delivery of the old secret instead. See [The honest limits of clipboard safety](features.md#the-honest-limits-of-clipboard-safety).

## Password-field refusal has a narrow boundary

Password protection belongs only to Windows Jumper anchor capture and anchored delivery. Handy refuses its own window, classic Win32 password edits, and browser/Electron/WinUI fields when UI Automation definitively reports a password field. It checks an anchored target again and parks the transcript if the foreground window or control changed.

It is defence in depth, not a promise that Handy never enters a password field:

- Ordinary dictation into the focused field has no password check.
- Keyboard Typer deliberately supports password prompts.
- UI Automation/COM failures are fail-open: “could not determine” is not treated as a password field.
- Custom controls may expose neither the Win32 password style nor UI Automation property.
- The last per-keystroke guard repeats Win32 checks but not UI Automation. A non-Win32 field can change after the earlier check.
- RDP/Citrix can expose only a window canvas, so Handy cannot inspect a password field inside the remote session.
- Refusing an anchored password target protects the field, but parking the transcript does not protect the transcript or old clipboard.
- The synchronous UI Automation call has no timeout; a slow provider can delay capture or delivery checks.

See [It refuses to dictate into a password box](features.md#it-refuses-to-dictate-into-a-password-box).

## Practical boundary

Local-model dictation avoids an app-level network request, but still writes sensitive local data. Clipboard-based normal delivery briefly uses the system clipboard; `Direct` does not. Remote engines, post-processing, provider-backed tools, MCP clients, Windows, and destination apps widen that boundary. Choose them as carefully as you would choose where to upload the original audio or text.
