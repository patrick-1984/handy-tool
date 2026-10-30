# Speakers settings

Open `Advanced settings › Speakers`. The tab holds the speakers model that tells who said what on this PC, and chooses where History's `Transcribe again with speakers` does it: on this PC or on your [STT server](stt-server.md).

Catalog: [Who said what, worked out on this PC](../../features.md#speakers-on-this-pc); [Find out who said what in a recording](../../features.md#transcribe-again-with-speakers).

## Speakers

### Speakers model (NVIDIA Sortformer v2.1)

`Advanced settings › Speakers › Speakers model (NVIDIA Sortformer v2.1)`

The model's card. `Download (141 MB)` fetches it once from Hugging Face, with a progress bar and `Cancel`; afterwards the card shows `Downloaded` and `Delete`, which asks first and removes it from this PC (it can be downloaded again any time). The `NVIDIA Open Model License` link on the card opens the license of the model's weights. **Default:** not downloaded.

### Find the speakers after each take

`Advanced settings › Speakers › Find the speakers after each take`

After a take's text is pasted, reads the take again on this PC and adds the text with speakers to its History row. It works with dictation models on this PC only, not online ones, and can be switched on once the speakers model is downloaded. **Default:** Off.

Catalog: [Who said what, worked out on this PC](../../features.md#speakers-on-this-pc).

### "Transcribe again with speakers" uses

`Advanced settings › Speakers › "Transcribe again with speakers" uses`

Where History's `Transcribe again with speakers` tells the speakers apart: `Automatic` (this PC when the speakers model is downloaded and the dictation model is not an online one, otherwise the STT server), `This PC` or `STT server`. `This PC` needs a dictation model on this PC; `STT server` needs `Server address` and `Model with speakers` on [STT server](stt-server.md). **Default:** `Automatic`.

Catalog: [Find out who said what in a recording](../../features.md#transcribe-again-with-speakers).
