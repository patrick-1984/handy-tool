# Debug settings

Press `ctrl+shift+d` to enable debug mode and reveal `Debug`. Debug mode defaults to off; every control below requires it.

### Log Level

`More › Debug › Log Level` _{requires: Debug mode}_

Sets file-log verbosity. `Debug` and `Trace` logs can contain transcript fragments and prompt previews; `Info` does not write them. **Default:** `Info` in the released build (`Debug` in development builds).

Catalog: [Your dictation is not written into logs at the normal level](../../features.md#your-dictation-is-not-written-into-logs).

### Sound Theme

`More › Debug › Sound Theme` _{requires: Debug mode}_

Selects the cue-sound set. `Custom` appears only when both custom start and stop WAV files exist. **Default:** `Marimba`.

Catalog: [Hear when the microphone is hot](../../features.md#hear-when-the-microphone-is-hot).

### Word Correction Threshold

`More › Debug › Word Correction Threshold` _{requires: Debug mode}_

Sets fuzzy matching aggressiveness for [Custom Words](general.md#custom-words); higher values permit more substitutions and false positives. **Default:** `0.18`.

Catalog: [Names and jargon stop coming back mangled](../../features.md#names-and-jargon-stop-coming-back-mangled).

### Paste Delay

`More › Debug › Paste Delay` _{requires: Debug mode}_

Sets the wait between placing transcript text on the clipboard and sending the paste keystroke. This is separate from the jump and restore delays on `More › Output`. **Default:** `60 ms`.

Catalog: [Ctrl+V doesn't work in that app](../../features.md#ctrl-v-doesnt-work-in-that-app).

### Always-On Microphone

`More › Debug › Always-On Microphone` _{requires: Debug mode}_

Keeps the microphone stream open between takes, trading quicker capture for a continuously active microphone indicator. **Default:** Off.

Catalog: [The microphone light is off when you're not dictating](../../features.md#the-microphone-light-is-off-when-youre-not-dictating).

### Clamshell Microphone

`More › Debug › Clamshell Microphone` _{requires: Debug mode; planned}_

Chooses an alternate input for a closed-lid macOS laptop. It is not present in the shipped Windows build; macOS builds are planned. **Default:** none.
