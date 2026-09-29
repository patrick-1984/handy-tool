# Post-processing settings

Open `Advanced settings › Post-processing` and set `Advanced settings › Post-processing › Post Processing = On`; the controls below then appear under the switch on the same tab. Every breadcrumb below is gated by that switch.

### Post-processing setup

`Advanced settings › Post-processing › Post-processing setup`

A tinted row at the top: `Start setup` opens `Setups` and starts the Post-processing setup (what the AI should do, which AI, the shortcut, a try). See [Setups › Post-processing](setups.md#post-processing). While the Post-processing switch is off, a line under it says that switching it on shows more settings.

## Hotkey

### Post-Processing Hotkey

`Advanced settings › Post-processing › Hotkey › Post-Processing Hotkey` _{requires: Post-processing enabled}_

Sets the shortcut that transcribes and then sends the result through the configured provider and prompt. **Default:** `ctrl+shift+f12`.

Catalog: [A second key for "clean this up with AI"](../../features.md#a-second-key-for-clean-this-up).

## API (OpenAI Compatible)

### Provider

`Advanced settings › Post-processing › API (OpenAI Compatible) › Provider` _{requires: Post-processing enabled}_

Selects a chat-capable entry from Registered LLM Providers. Its key, model, and base URL remain configured on [Advanced settings › Transcription providers](advanced.md#registered-llm-providers). **Default:** no provider selected.

Catalog: [Configure a provider once, use it everywhere](../../features.md#configure-a-provider-once-use-it-everywhere).

### Temperature

`Advanced settings › Post-processing › API (OpenAI Compatible) › Temperature` _{requires: Post-processing enabled}_

Sets sampling temperature from 0 through 1 for post-processing. **Default:** `0.3`.

Catalog: [Dial how creative the cleanup is allowed to be](../../features.md#dial-how-creative-the-cleanup-is).

### Disable Thinking

`Advanced settings › Post-processing › API (OpenAI Compatible) › Disable Thinking` _{requires: Post-processing enabled}_

Requests suppression of reasoning output when the selected model supports the provider-specific option. **Default:** Off.

Catalog: [Dial how creative the cleanup is allowed to be](../../features.md#dial-how-creative-the-cleanup-is).

## Prompt

### Selected Prompt

`Advanced settings › Post-processing › Prompt › Selected Prompt` _{requires: Post-processing enabled}_

Selects and edits the active prompt; its expanded editor creates, updates, or deletes saved prompts. **Default:** `Structure & Clean` (`default_structure`).

Catalog: [Your own post-processing prompts](../../features.md#your-own-post-processing-prompts) and [A default prompt that respects your words](../../features.md#a-default-prompt-that-respects-your-words).
