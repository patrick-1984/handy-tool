# Post-processing settings

Open `More › Post-processing` and set `More › Post-processing › Post Processing = On`; the controls below then appear under the switch on the same tab. Every breadcrumb below is gated by that switch.

## Hotkey

### Post-Processing Hotkey

`More › Post-processing › Hotkey › Post-Processing Hotkey` _{requires: Post-processing enabled}_

Sets the shortcut that transcribes and then sends the result through the configured provider and prompt. **Default:** `ctrl+shift+f12`.

Catalog: [A second key for "clean this up with AI"](../../features.md#a-second-key-for-clean-this-up).

## API (OpenAI Compatible)

### Provider

`More › Post-processing › API (OpenAI Compatible) › Provider` _{requires: Post-processing enabled}_

Selects a chat-capable entry from Registered LLM Providers. Its key, model, and base URL remain configured on [More › Providers](advanced.md#registered-llm-providers). **Default:** no provider selected.

Catalog: [Configure a provider once, use it everywhere](../../features.md#configure-a-provider-once-use-it-everywhere).

### Temperature

`More › Post-processing › API (OpenAI Compatible) › Temperature` _{requires: Post-processing enabled}_

Sets sampling temperature from 0 through 1 for post-processing. **Default:** `0.3`.

Catalog: [Dial how creative the cleanup is allowed to be](../../features.md#dial-how-creative-the-cleanup-is).

### Disable Thinking

`More › Post-processing › API (OpenAI Compatible) › Disable Thinking` _{requires: Post-processing enabled}_

Requests suppression of reasoning output when the selected model supports the provider-specific option. **Default:** Off.

Catalog: [Dial how creative the cleanup is allowed to be](../../features.md#dial-how-creative-the-cleanup-is).

## Prompt

### Selected Prompt

`More › Post-processing › Prompt › Selected Prompt` _{requires: Post-processing enabled}_

Selects and edits the active prompt; its expanded editor creates, updates, or deletes saved prompts. **Default:** `Structure & Clean` (`default_structure`).

Catalog: [Your own post-processing prompts](../../features.md#your-own-post-processing-prompts) and [A default prompt that respects your words](../../features.md#a-default-prompt-that-respects-your-words).
