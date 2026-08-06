# Providers

## The moment

Your travel laptop cannot run a useful local transcription model at the speed you need. An external provider gives that machine another option for little cost, while your shortcuts and working rhythm stay familiar. The same provider surface also helps when a local model produces a broken translation or a file is too large for the machine at hand.

## How it fits your day

Stay local when the machine can carry the work. Configure a remote speech endpoint for the situations where compute, language support, or file size makes that trade-off worthwhile, and remember that the selected service receives the recording.

## What it can do

- [Pick the engine that fits the machine](../features.md#pick-the-engine-that-fits-the-machine)
- [Point it at any OpenAI-compatible speech endpoint](../features.md#point-it-at-any-openai-compatible-speech-endpoint)
- [One OpenRouter key, many speech models](../features.md#one-openrouter-key-many-speech-models)
- [Configure a provider once, use it everywhere](../features.md#configure-a-provider-once-use-it-everywhere)
- [Whisper-style, or an audio-capable chat model](../features.md#whisper-style-or-an-audio-capable-chat-model)
- [Ten times less audio over the wire](../features.md#ten-times-less-audio-over-the-wire)
- [Know what your dictation costs](../features.md#know-what-your-dictation-costs)
- [Several slots, one local loader](../features.md#several-slots-one-local-loader)

## Settings that matter

- [Advanced settings](../reference/settings/advanced.md)
- [Models settings](../reference/settings/models.md)

## When it goes wrong

- [A network blip can't shred your take](../features.md#a-network-blip-cant-shred-your-take)
- [A hung provider can't stall the app](../features.md#a-hung-provider-cant-stall-the-app)
- [Selecting a remote engine no longer reverts](../features.md#selecting-a-remote-engine-no-longer-reverts)
- [The model list actually contains speech models](../features.md#the-model-list-actually-contains-speech-models)

## Set it up

1. For an OpenAI-compatible speech service, enter its endpoint at `Advanced › Providers › API Transcription (OpenAI-compatible) › API URL`.
2. Enter the credential at `Advanced › Providers › API Transcription (OpenAI-compatible) › API Key`.
3. Enter the service's model identifier at `Advanced › Providers › API Transcription (OpenAI-compatible) › Model`.
4. For OpenRouter instead, choose the request shape at `Advanced › Providers › OpenRouter Transcription › Endpoint = Transcription (Whisper-style)`.
5. Prefer the smaller supported upload at `Advanced › Providers › OpenRouter Transcription › Audio format = Opus — smaller (recommended)`.
6. Use harmless test audio first. Confirm the provider accepts the selected route and format before sending private or irreplaceable material.
