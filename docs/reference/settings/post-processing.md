# Post Process settings

Enable `Advanced › Post-processing › Post Processing = On` to reveal this page, then open `Post Process`. Every breadcrumb below is gated by that switch.

## Hotkey

### Post-Processing Hotkey

`Post Process › Hotkey › Post-Processing Hotkey` *{requires: Post-processing enabled}*

Sets the shortcut that transcribes and then sends the result through the configured provider and prompt. **Default:** `ctrl+shift+space`.

Catalog: [A second key for "clean this up with AI"](../../features.md#a-second-key-for-clean-this-up).

## API (OpenAI Compatible)

### Provider

`Post Process › API (OpenAI Compatible) › Provider` *{requires: Post-processing enabled}*

Selects a chat-capable entry from Registered LLM Providers. Its key, model, and base URL remain configured on [Advanced](advanced.md#registered-llm-providers). **Default:** no provider selected.

Catalog: [Configure a provider once, use it everywhere](../../features.md#configure-a-provider-once-use-it-everywhere).

### Temperature

`Post Process › API (OpenAI Compatible) › Temperature` *{requires: Post-processing enabled}*

Sets sampling temperature from 0 through 1 for post-processing. **Default:** `0.3`.

Catalog: [Dial how creative the cleanup is allowed to be](../../features.md#dial-how-creative-the-cleanup-is).

### Disable Thinking

`Post Process › API (OpenAI Compatible) › Disable Thinking` *{requires: Post-processing enabled}*

Requests suppression of reasoning output when the selected model supports the provider-specific option. **Default:** Off.

Catalog: [Dial how creative the cleanup is allowed to be](../../features.md#dial-how-creative-the-cleanup-is).

## Prompt

### Selected Prompt

`Post Process › Prompt › Selected Prompt` *{requires: Post-processing enabled}*

Selects and edits the active prompt; its expanded editor creates, updates, or deletes saved prompts. **Default:** `Structure & Clean` (`default_structure`).

Catalog: [Your own post-processing prompts](../../features.md#your-own-post-processing-prompts) and [A default prompt that respects your words](../../features.md#a-default-prompt-that-respects-your-words).
