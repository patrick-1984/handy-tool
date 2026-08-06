# MCP and CLI

## The moment

Your window manager owns the hotkey, or an agent needs to inspect and drive the same application state you use in the interface. Handy Tool accepts local orders through its command-line companion and a localhost MCP server, so those workflows do not need to imitate clicks.

## How it fits your day

Enable this surface only when another local tool needs it, and decide who you are trusting first: [Bound to localhost, behind a token — and what that does not cover](../features.md#bound-to-localhost-behind-a-token).

## What it can do

- [Let an agent drive the app](../features.md#let-an-agent-drive-the-app)
- [A handy command on your PATH](../features.md#a-handy-command-on-your-path)
- [Scriptable model tests that produce the same artifact as the interface](../features.md#scriptable-model-tests-that-match-the-interface)
- [Drive it from your window manager or a hotkey daemon](../features.md#drive-it-from-your-window-manager)
- [Bound to localhost, behind a token — and what that does not cover](../features.md#bound-to-localhost-behind-a-token)
- [An agent can set a key but never read one](../features.md#an-agent-can-set-a-key-but-never-read-one)

## Settings that matter

- [Advanced settings](../reference/settings/advanced.md)

## When it goes wrong

- [Your agent can read your history — know that before you enable it](../features.md#your-agent-can-read-your-history)
- [A hotkey another app already owns](../features.md#a-hotkey-another-app-already-owns)
- [Diagnose without guessing](../features.md#diagnose-without-guessing)

## Set it up

1. Turn on the local service at `Advanced › MCP & CLI › Enable MCP & CLI server = On`.
2. Review the listening value at `Advanced › MCP & CLI › Port`.
3. Copy or regenerate the credential at `Advanced › MCP & CLI › Token`.
4. Install the companion from `Advanced › MCP & CLI › Command-line companion`.
5. Give the token only to local clients you intend to trust, then verify the connection before exposing history or running provider-backed tools.
