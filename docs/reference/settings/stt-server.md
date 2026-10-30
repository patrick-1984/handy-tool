# STT server settings

Open `Advanced settings › STT server`. The page connects a speech-to-text server of your own (OpenAI-compatible, on this PC or your network) as the Custom STT server model, with or without telling the speakers apart. Telling speakers apart on this PC instead, and which of the two History uses, are on [Speakers](speakers.md).

## Your STT server

### Server address

`Advanced settings › STT server › Server address`

The server's API address with its version, e.g. `http://192.168.1.10:8000/v1`. Requests go to `{address}/audio/transcriptions` as Ogg/Opus. With `History › Settings › Crash-Safe Recording` on (the default) each piece of a take is sent while you record; with it off, the whole take in one request when you stop ([Online models have the text ready when you stop](../../features.md#online-models-transcribe-while-you-record)). **Default:** empty (not set up).

### API key

`Advanced settings › STT server › API key`

Sent as a bearer token, only when filled in. **Default:** empty.

### Test the server

`Advanced settings › STT server › Test the server`

Asks `{address}/models` for the server's models; the result fills the two model lists below and shows what runs behind them when the server says. **Default:** not applicable; this is an action.

### Use it for dictation

`Advanced settings › STT server › Use it for dictation`

Makes the Custom STT server the dictation model. It can also be picked on the Models page or set as a fallback there (`Models › If your dictation model fails`). **Default:** not applicable; this is an action.

## Transcription

### Model for text

`Advanced settings › STT server › Model for text`

The server's model for plain text. **Default:** `transcribe`.

### Tell the speakers apart

`Advanced settings › STT server › Tell the speakers apart`

With Crash-Safe Recording on, the text still comes from the pieces sent while you record, and the whole take is sent once to the model with speakers after you stop (if that fails, the plain text is kept); with it off, the take goes to the model with speakers in one request. Saves the take with speakers in History, one `A: …` paragraph per speaker turn, next to the plain text. A model answering with OpenAI-style segments (speaker, start, end) or with `A: …` paragraphs both work; with one voice there is only the text. **Default:** Off.

### Model with speakers

`Advanced settings › STT server › Model with speakers`

**Default:** `transcribe-diarize-openai`.

### What to paste

`Advanced settings › STT server › What to paste`

`Text only` or `Text with speakers`. With `Text with speakers` the paste waits for the whole-take request after you stop. History keeps both either way; a row's `Text` / `Speakers` switch shows and copies either one. **Default:** `Text only`.

Custom words are sent to the server as vocabulary (the OpenAI `prompt` field) and corrected in its answer, in the speaker text too, as for every model.

## Around the plain text / Around the text with speakers

Each has its own prefix and suffix (with an optional new line), applied to the pasted text only and inside the general prefix and suffix. **Default:** Off.
