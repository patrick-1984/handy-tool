# Command-line reference

Handy Tool 1.0.0 ships a Windows x64 `handy` command. Top-level flags control or launch the desktop app. Companion subcommands call the running app through its token-guarded loopback server.

Install or refresh the command on your user PATH:

```powershell
handy install-cli
```

Companion commands other than `install-cli` require `Advanced › MCP & CLI › Enable MCP & CLI server = On`. The server is off by default. A command uses the discovery file under `%APPDATA%\pr.handy`, starts Handy hidden if necessary, then calls `127.0.0.1`. If persisted settings say the server is disabled, it returns an error instead of starting an orphaned process.

See [A handy command on your PATH](../features.md#a-handy-command-on-your-path) and [Bound to localhost, behind a token — and what that does not cover](../features.md#bound-to-localhost-behind-a-token).

## Six desktop flags

| Flag | Effect | Example |
| --- | --- | --- |
| `CLI › handy --start-hidden` | Starts without showing the main window. | `handy --start-hidden` |
| `CLI › handy --no-tray` | Starts without the tray icon. | `handy --no-tray` |
| `CLI › handy --toggle-transcription` | Sends a Transcribe press to the running instance; invoke it again to stop. | `handy --toggle-transcription` |
| `CLI › handy --toggle-post-process` | Sends a Transcribe with Post-Processing press. | `handy --toggle-post-process` |
| `CLI › handy --cancel` | Sends Cancel. Current Cancel Behavior decides whether the take is saved silently or discarded. | `handy --cancel` |
| `CLI › handy --debug` | Starts with debug mode and verbose logging. | `handy --debug` |

Launch flags can be combined:

```powershell
handy --start-hidden --no-tray
```

The three action flags use the same coordinator as app shortcuts. They do not provide the hold/release pair needed for Push-to-Talk. See [Drive it from your window manager or a hotkey daemon](../features.md#drive-it-from-your-window-manager).

## Ten companion subcommands

### `model-test`

Runs one prompt across provider IDs or names, optionally asks a judge panel to assess the responses, and prints a Markdown report.

```powershell
handy model-test --run "anthropic,openrouter1" --judge "gemini" --prompt "Propose three failure cases for a retry queue." --out model-test-report.md
```

| Argument | Required | Meaning |
| --- | --- | --- |
| `--run <ids-or-names>` | Yes | Comma-separated runner provider IDs or names. |
| `--judge <ids-or-names>` | No | Comma-separated judge provider IDs or names. |
| `--prompt <text>` | No | Inline main prompt; takes precedence over `--prompt-file`. |
| `--prompt-file <path>` | No | Reads the main prompt from a file. |
| `--judge-prompt <text>` | No | Inline judge instructions; takes precedence over `--judge-prompt-file`. |
| `--judge-prompt-file <path>` | No | Reads judge instructions from a file. |
| `--preset <name-or-id>` | No | Uses a saved prompt preset. |
| `--model-temp <number>` | No | Runner temperature; default `0.3`. |
| `--model-thinking <auto|on|off>` | No | Runner thinking mode; default `auto`. |
| `--judge-temp <number>` | No | Judge temperature; default `0.3`. |
| `--judge-thinking <auto|on|off>` | No | Judge thinking mode; default `auto`. |
| `--image <path>` | No | Attaches an image as a data URL for vision-capable runners. |
| `--out <path>` | No | Writes Markdown relative to the shell directory; otherwise prints it. |
| `--json` | No | Prints full JSON instead of Markdown. |

At least `--run` is required. A preset can supply prompts; otherwise provide a prompt for a useful run. See [Scriptable model tests that produce the same artifact as the interface](../features.md#scriptable-model-tests-that-match-the-interface).

### `token-count`

Counts with Handy’s built-in offline tokenizer command; this CLI subcommand does not call configured providers.

```powershell
handy token-count --file .\prompt.txt --tokenizer o200k_base
```

| Argument | Required | Meaning |
| --- | --- | --- |
| `<text>` | One input form | Positional inline text. |
| `--file <path>` | One input form | Reads a file. Inline text wins if both are present. |
| `--tokenizer <name>` | No | Tokenizer name; default `cl100k_base`. |

Inline example: `handy token-count "Summarize the incident without assigning blame."`

See [Counts without a network call](../features.md#counts-without-a-network-call).

### `type`

Delivers text to the focused window through Handy’s ordinary clipboard paste path. Despite the name, this is not the in-memory Keyboard Typer and can put text on the Windows clipboard.

```powershell
handy type "Deploy after the test suite passes."
handy type --file .\message.txt
```

Supply positional `<text>` or `--file <path>`; inline text wins if both are present. Place focus before running it. The released build logs at `Info` and records no preview of the text, but raising `Debug › Log Level` to `Debug` does record one; review the [privacy limits](../privacy.md) before using sensitive text.

### `history-list`

Prints recent history as formatted JSON, including transcript snippets. `--limit <number>` is optional.

```powershell
handy history-list --limit 20
```

### `history-get`

Prints one full history entry as formatted JSON. `--id <number>` is required and comes from `history-list`. The result can include a local recording path.

```powershell
handy history-get --id 42
```

See [Your agent can read your history — know that before you enable it](../features.md#your-agent-can-read-your-history).

### `providers-list`

Prints registered providers as formatted JSON. API-key values are redacted; the response reports only whether a key exists.

```powershell
handy providers-list
```

### `providers-set`

Updates one registered provider and prints the redacted record.

```powershell
handy providers-set --id openrouter1 --model "openai/gpt-4.1-mini" --enabled true
```

| Argument | Required | Meaning |
| --- | --- | --- |
| `--id <provider-id>` | Yes | Existing provider ID. |
| `--model <model-id>` | No | Model ID; changing it can auto-fill known costs. |
| `--api-key <key>` | No | Persists a key. It is not returned later, but the literal can remain in shell history and process inspection. |
| `--name <name>` | No | Display name. |
| `--base-url <url>` | No | Base URL. Handy permits HTTP; use HTTPS for remote providers. |
| `--enabled <true|false>` | No | Provider enabled state. |
| `--sequential <true|false>` | No | Serializes the provider with its concurrency family. |
| `--concurrency-group <name>` | No | Concurrency-family name. |
| `--persist-price <true|false>` | No | Retains manual prices instead of replacing them from provider data. |
| `--cost-input <number>` | No | Input USD per million tokens. |
| `--cost-output <number>` | No | Output USD per million tokens. |

An API key is write-only over MCP/CLI, but remains plaintext in settings and backups. See [An agent can set a key but never read one](../features.md#an-agent-can-set-a-key-but-never-read-one).

### `providers-models`

Fetches a provider’s live model list as formatted JSON. `--id <provider-id>` is required. The request can include the configured key.

```powershell
handy providers-models --id openrouter1
```

### `mcp`

Runs a newline-delimited JSON-RPC bridge on standard input/output. `--stdio` is the working form; `handy mcp` without it returns an error.

```powershell
handy mcp --stdio
```

An MCP client launches that command. The bridge discovers the app’s port and token, posts each input line to `/mcp`, and writes each response line. Notifications produce no output. See [Let an agent drive the app](../features.md#let-an-agent-drive-the-app).

### `install-cli`

Installs the current binary to the per-user command location and reports the path. It does not require the server.

```powershell
handy install-cli
```

On Windows, the destination is `%LOCALAPPDATA%\Microsoft\WindowsApps`.

## Discovery and security boundary

The server writes `handy-mcp.json` at `File › %APPDATA%\pr.handy\handy-mcp.json`. It contains the loopback port, process ID, and bearer token in plaintext. Orderly shutdown removes it; a crash can leave it behind.

POST requests require the token and bind to `127.0.0.1`, but another process running as you can read token-bearing files if account permissions allow it. Transport is plain HTTP over loopback. The unauthenticated `/health` exposes liveness and version only. Read the complete [privacy and data-flow limits](../privacy.md#localhost-mcpcli-server).

## Signals in planned builds

macOS and Linux builds are planned and unavailable in 1.0.0. The source reserves these Unix signals for future builds:

| Signal | Planned action | Future Unix example |
| --- | --- | --- |
| `SIGUSR2` | Toggle ordinary transcription. | `kill -USR2 $(pidof handy)` |
| `SIGUSR1` | Toggle transcription with post-processing. | `kill -USR1 $(pidof handy)` |

They are one-shot toggles, not Push-to-Talk. They cannot be used with today’s Windows-only release.
