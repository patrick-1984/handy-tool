# Debug settings

Press `ctrl+shift+d` to enable debug mode and reveal `Debug`. Debug mode defaults to off; every control below requires it.

### Log Level

`Debug › Log Level` *{requires: Debug mode}*

Sets file-log verbosity. `Debug` and `Trace` logs can contain transcript fragments and prompt previews; `Info` does not write them. **Default:** `Info` in the released build (`Debug` in development builds).

Catalog: [Your dictation is not written into logs at the normal level](../../features.md#your-dictation-is-not-written-into-logs).

### Sound Theme

`Debug › Sound Theme` *{requires: Debug mode}*

Selects the cue-sound set. `Custom` appears only when both custom start and stop WAV files exist. **Default:** `Marimba`.

Catalog: [Hear when the microphone is hot](../../features.md#hear-when-the-microphone-is-hot).

### Word Correction Threshold

`Debug › Word Correction Threshold` *{requires: Debug mode}*

Sets fuzzy matching aggressiveness for [Custom Words](general.md#custom-words); higher values permit more substitutions and false positives. **Default:** `0.18`.

Catalog: [Names and jargon stop coming back mangled](../../features.md#names-and-jargon-stop-coming-back-mangled).

### Paste Delay

`Debug › Paste Delay` *{requires: Debug mode}*

Sets the wait between placing transcript text on the clipboard and sending the paste keystroke. This is separate from Advanced jump and restore delays. **Default:** `60 ms`.

Catalog: [Ctrl+V doesn't work in that app](../../features.md#ctrl-v-doesnt-work-in-that-app).

### Always-On Microphone

`Debug › Always-On Microphone` *{requires: Debug mode}*

Keeps the microphone stream open between takes, trading quicker capture for a continuously active microphone indicator. **Default:** Off.

Catalog: [The microphone light is off when you're not dictating](../../features.md#the-microphone-light-is-off-when-youre-not-dictating).

### Clamshell Microphone

`Debug › Clamshell Microphone` *{requires: Debug mode; planned}*

Chooses an alternate input for a closed-lid macOS laptop. It is not present in the shipped Windows build; macOS builds are planned. **Default:** none.

### Cancel Shortcut

`Debug › Cancel Shortcut` *{requires: Debug mode}*

Sets the shortcut that applies [Cancel behavior](general.md#cancel-behavior). **Default:** `escape`.

Catalog: [Cancel means the same thing however you trigger it](../../features.md#cancel-means-the-same-thing-however-you-trigger-it).
