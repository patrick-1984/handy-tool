# Models settings

Open `Models`. Model cards and language-filter values are generated from the installed model registry, so their visible names vary.

### Unload Model

`Models › Unload Model`

Chooses how long the active transcription model stays in RAM or VRAM after use. Choosing `Custom…` reveals a duration and unit; debug mode adds a five-second test option. **Default:** `Never (keep loaded — fastest start)`; custom duration defaults to `300` seconds.

Catalog: [Free the memory when you stop dictating](../../features.md#free-the-memory-when-you-stop-dictating).

## Downloaded Models

### All Languages

`Models › Downloaded Models › All Languages`

Filters both model lists by supported language; its label changes to the selected language. **Default:** `All Languages`.

Catalog: [Pick the engine that fits the machine](../../features.md#pick-the-engine-that-fits-the-machine).

### \<model name\>

`Models › Downloaded Models › <model name>`

Selects or deletes a downloaded model; the title comes from the model registry or a custom model filename. **Default:** no model on a fresh install; onboarding sets the first downloaded selection.

Catalog: [Selecting a remote engine no longer reverts](../../features.md#selecting-a-remote-engine-no-longer-reverts).

## Available to Download

### \<model name\>

`Models › Available to Download › <model name>`

Starts or cancels that model's download. The section contains registry models not already installed and matching [All Languages](#all-languages). **Default:** no download in progress.

Catalog: [Cancel a stuck download and it stops now](../../features.md#cancel-a-stuck-download-and-it-stops-now).

## If your dictation model fails

### Second model / Third model

`Models › If your dictation model fails › Second model`

Up to two more models, tried in this order when the dictation model cannot transcribe a take (its server is down, the network dropped, a request failed) - the whole take or each 10-minute piece. An online dictation model is checked when the take starts, so a server that does not answer is skipped at once, and after one failure the rest of the take goes straight to the next model. Any downloaded or online model can be picked. History shows the model that transcribed. **Default:** None.

## Parakeet from Hugging Face

`Models › Parakeet from Hugging Face`

Searches Hugging Face for Parakeet ONNX models (or takes a repo typed as owner/name). `Check` tells whether Handy's Parakeet engine can read it (it needs `encoder-model`, `decoder_joint-model`, `nemo128.onnx` and `vocab.txt`, int8 preferred) and how big it is; `Add and download` adds it to Downloaded Models and downloads it file by file, resumably. It then runs on this PC like the Parakeet Handy ships. **Default:** nothing added.

A model card with settings of its own has a gear (`Settings of <model name>`), and a click on the card of the model in use opens those settings.
