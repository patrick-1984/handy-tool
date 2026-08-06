# Glossary

These are the words Handy Tool uses for recording, delivery, and window targeting.

## Anchor

A remembered Windows window and text field used by a hot slot. Handy checks the target again before anchored delivery. Anchors and the Jumper are Windows-only. See [Send it where you were](../features.md#send-it-where-you-were).

## Chunked recording

The roughly ten-minute audio files a long take is written into as you speak. Recording chunks are not VAD transcription segments. See [A crash mid-dictation costs you nothing](../features.md#a-crash-mid-dictation-costs-you-nothing).

## Engine

The implementation that turns audio into text. An engine can run a downloaded model locally, call a configured speech endpoint, or delegate to a separate local service. See [Pick the engine that fits the machine](../features.md#pick-the-engine-that-fits-the-machine).

## Hot slot

One of two Windows-only anchor destinations intended to be replaced and revisited frequently. "Hot slot" is the on-screen group name and the term these pages use; "hot destination" and "hot anchor" mean the same thing. Hot 1 uses Set Anchor and Jump to Anchor; Hot 2 uses their “2” counterparts. See [Two live destinations at once](../features.md#two-live-destinations-at-once).

## Jump slot

A numbered Windows-only saved destination. Nine static slots complement the two hot slots. Set remembers the focused field; Jump returns focus to it without pasting. See [Nine memorised destinations](../features.md#nine-memorised-destinations).

## Live transcription

Text is produced during the take, so much of a long recording is already processed when you stop. Compare **post-recording transcription**. See [Watch the text appear, or wait for the most accurate pass](../features.md#live-or-post-recording).

## Paste method

The delivery keystroke or mechanism used to put text into a destination: Ctrl+V, Ctrl+Shift+V, Shift+Insert, direct typing, an external script, or no paste. Applications and remote sessions do not accept every method. See [Ctrl+V doesn't work in that app](../features.md#ctrl-v-doesnt-work-in-that-app).

## Post-processing

An optional second pass in which a selected LLM provider receives the transcript and instruction prompt, then returns transformed text. Audio is not part of that request. See [A second key for "clean this up with AI"](../features.md#a-second-key-for-clean-this-up).

## Post-recording transcription

Audio is collected first and transcribed after the take stops. Compare **live transcription**. Remote API and OpenRouter transcription use the completed VAD-retained recording rather than live segments. See [Watch the text appear, or wait for the most accurate pass](../features.md#live-or-post-recording).

## Provider

A saved LLM connection: kind, base URL, model, credentials, cost fields, and scheduling settings. The registry is shared by post-processing, model testing, and provider-backed token counting. See [Configure a provider once, use it everywhere](../features.md#configure-a-provider-once-use-it-everywhere).

## Push-to-Talk

A hold gesture: recording begins on key-down and stops on key-up. A macro device must emit separate press and release events. See [Hold a key for a one-line thought](../features.md#hold-to-talk).

## Segment

A VAD-bounded portion of speech, and the unit the transcription pipeline works on. A segment is usually much shorter than a **chunked recording** file. See [Watch the text appear, or wait for the most accurate pass](../features.md#live-or-post-recording).

## Sidecar

The plain-text file the Translator writes next to each transcribed audio file. The MCP discovery file is not called a sidecar in these pages; it is named `handy-mcp.json`. See [A folder of recordings, transcribed while you sleep](../features.md#a-folder-of-recordings-transcribed-while-you-sleep).

## Take

One recording from start until stop or cancel, together with the transcript and history entry produced from it. See [Escape stops the delivery, not your words](../features.md#escape-stops-the-delivery-not-your-words).

## VAD

Voice Activity Detection: the bundled Silero model that decides which microphone frames contain speech. See [It records what you say, not the silence](../features.md#it-records-what-you-say-not-the-silence).

## VAD-retained recording

The audio that remains once VAD has dropped the silence — the only audio Handy stores or sends. See [It records what you say, not the silence](../features.md#it-records-what-you-say-not-the-silence).
