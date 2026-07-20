# The built-in toolbox

Handy Tool ships several tools that have nothing to do with dictation per se — they exist because once an app can type, count tokens, and talk to LLM providers, a set of daily problems becomes trivially solvable in one place.

## Model Testing & the Judge

**Problem:** "which model should I use?" is usually answered by vibes. Providers publish benchmarks; your prompts are not benchmarks.

The Model Testing page runs **one prompt across every provider/model you select, side by side**, capturing for each: the full response, token usage, **real cost** (actual metered cost for OpenRouter; computed from your configured rates elsewhere), and round-trip time.

Then the **judge panel** takes over: you nominate one or more models as arbiters, and they score the collected answers against each other. Instead of you eyeballing six walls of text, you get a ranked verdict — _from models judging models_ — plus the raw responses to check the judges' work. Results render as a Markdown report you can save or share.

Local providers that can only load one model at a time are run sequentially and never in parallel — the tool understands provider families, so a single-loader local server doesn't get trampled.

## Keyboard Typer

**Problem:** some places simply do not accept paste. Remote consoles, VM login screens, BIOS-era admin panels, RDP gateways with clipboard redirection disabled, password prompts. Retyping a 32-character generated password by hand, glancing between windows, is both painful and error-prone.

The Keyboard Typer takes a text you place in it and **types it, keystroke by keystroke, into whatever window has focus** when you press its dedicated shortcut. A short configurable countdown gives you time to focus the target; per-keystroke delay is tunable for slow remote links; pressing the shortcut again cancels mid-typing.

Security is the point, not an afterthought: **the text lives only in memory**. It is never written to settings, disk, or history — so using it for a password on a remote host doesn't leave a trace on either machine. It also waits for you to release the shortcut's modifier keys before typing, so a held Ctrl doesn't turn your password into a hotkey barrage.

## Token Counter

**Problem:** nobody has intuition for tokens. Is this contract 8k tokens or 80k? Will it fit the context window? What will this cost across providers — whose tokenizers all differ?

Paste any text and get its token count **per provider**: Anthropic and Gemini via their native counting endpoints, OpenAI via a bundled tokenizer (offline), and OpenAI-compatible local servers via a calibrated probe. Comparing the same content across providers side by side turns "I think it's big" into a number — before you spend money or blow a context window.

## MCP server & `handy` CLI

**Problem:** a desktop app is a silo. Your agents and scripts can't click buttons.

Handy Tool can expose a **localhost-only MCP server** (Streamable HTTP, bearer-token protected) that re-exposes the app's own capabilities to AI agents — Claude Desktop, Claude Code, or anything MCP-capable — plus a `handy` **CLI** for scripts. Through it, an agent can run model tests, count tokens, type text via the Keyboard Typer, read transcription history, and manage provider configuration.

Safety rails: the server binds to `127.0.0.1` only, every call requires the token, and **API keys are write-only** — an agent can configure a provider but can never read a key back out.

## Remote control without the server

For window managers and automation that just need the basics: CLI flags (`--toggle-transcription`, `--toggle-post-process`, `--cancel`) drive a running instance, and on macOS/Linux the same is available via Unix signals (`SIGUSR2` / `SIGUSR1`) — handy for Wayland setups where the compositor owns the hotkeys.
