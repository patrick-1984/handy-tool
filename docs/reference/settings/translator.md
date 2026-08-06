# Translator settings

Open `Translator`. The folder scanner's store-only interval defaults to 15 seconds; it has no control on this page.

## Translator

### Watch folders

`Translator › Watch folders`

Starts or stops batch watching for configured folders. **Default:** Off.

Catalog: [A folder of recordings, transcribed while you sleep](../../features.md#a-folder-of-recordings-transcribed-while-you-sleep).

### Priority

`Translator › Priority`

Chooses how batch work shares the engine with live dictation. **Default:** `Live dictation first`.

Catalog: [Live dictation always wins](../../features.md#live-dictation-always-wins).

### Batch model

`Translator › Batch model`

Selects the Translator engine; an empty selection follows the main dictation model. It combines with [Unload batch model after](#unload-batch-model-after). **Default:** `Same as dictation (default)`.

Catalog: [Batch on one accelerator, dictation on another](../../features.md#batch-on-one-accelerator-dictation-on-another).

### Unload batch model after

`Translator › Unload batch model after`

Sets the idle-unload rule for a separate local batch-model slot. `Custom…` reveals a duration and unit. **Default:** `Never (keep loaded — fastest start)`; custom duration `300` seconds.

Catalog: [Batch on one accelerator, dictation on another](../../features.md#batch-on-one-accelerator-dictation-on-another).

### Status

`Translator › Status`

Shows whether watching is off, idle, queued, or processing a particular file segment. **Default:** `Off`.

Catalog: [You can see what it is working on](../../features.md#you-can-see-what-it-is-working-on).

## Watched folders

### No watched folders

`Translator › Watched folders › No watched folders`

Shows the empty state when the list contains no folders. **Default:** the stored list starts empty; the recordings folder may be seeded on the first Translator startup.

### \<folder name\>

`Translator › Watched folders › <folder name>`

Enables or pauses that dynamic watched-folder row; its title is the folder basename and its description is the full path. **Default:** On when a folder is added.

Catalog: [Your existing files are left alone](../../features.md#your-existing-files-are-left-alone).

### Add a folder

`Translator › Watched folders › Add a folder`

Opens the folder picker and appends the chosen path to the watched list. **Default:** not applicable; this is an action.

Catalog: [A folder of recordings, transcribed while you sleep](../../features.md#a-folder-of-recordings-transcribed-while-you-sleep).
