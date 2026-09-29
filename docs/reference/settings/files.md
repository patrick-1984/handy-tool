# Files settings

Open `Files`. The page has four groups: Transcribe a file, the list of transcribed files, and the watched folders (Watched folders and Folders). The folder scanner's store-only interval defaults to 15 seconds; it has no control on this page.

## Transcribe a file

### Model

`Files › Transcribe a file › Model`

The speech model that transcribes the file. The list shows your dictation model first, then the other downloaded models, then the ones not downloaded yet; picking one of those shows `Download`, and the file can be chosen once it is here. Online engines are listed only when they are your dictation model. Any other model loads into a second slot beside the dictation model, so dictation stays ready, and is unloaded after the file unless the watched folders use it. **Default:** your dictation model; the model you pick is remembered.

### Choose an audio file

`Files › Transcribe a file › Choose an audio file`

Opens a file picker for one WAV, MP3, M4A, MP4 (its audio track), AAC, FLAC or Ogg Opus (`.ogg`, `.opus`) file and transcribes it with the model chosen in [Model](#model) and the language settings. While [Save the text](#save-the-text) is `Ask every time`, a pop-up first asks where the text goes: `In Handy's folder`, `Next to the recording` or `Don't save`, with `Remember my choice`. A progress bar follows the file's length; `Stop` ends it after the piece in progress. When it finishes, the text is shown with `Copy text`. The original file is only read. **Default:** not applicable; this is an action.

Catalog: [One file, transcribed as accurately as the model can](../../features.md#one-file-transcribed-as-accurately-as-the-model-can).

### Save the text

`Files › Transcribe a file › Save the text`

Where each transcript is written as a `.txt`, named after the audio file; a number is added rather than overwriting anything. `In Handy's folder` uses [Folder](#folder); `Next to the recording` uses the audio file's own folder; `Don't save` keeps the text only on the page and in the list. `Remember my choice` in the pop-up sets this. **Default:** `Ask every time`.

### Keep a copy of the audio

`Files › Transcribe a file › Keep a copy of the audio`

Copies the audio file into [Folder](#folder) next to its text. The original is never moved or changed. **Default:** Off.

### Folder

`Files › Transcribe a file › Folder`

Shown while Keep a copy of the audio is on, or Save the text is `Ask every time` or `In Handy's folder`. `Change` picks another folder, the reset button goes back to the default, and `Open` shows it in Explorer. **Default:** `files` inside the app's data folder.

## Transcribed files

`Files › Transcribed files`

Lists every file transcribed on this page, newest first, with its date, length, model and text. Each row can copy the text, show the saved `.txt` in its folder, or be removed from the list (the saved files stay). **Default:** empty.

## Watched folders

### Watch folders

`Files › Watched folders › Watch folders`

Starts or stops batch watching for configured folders. **Default:** Off.

Catalog: [A folder of recordings, transcribed while you sleep](../../features.md#a-folder-of-recordings-transcribed-while-you-sleep).

### Priority

`Files › Watched folders › Priority`

Chooses how batch work shares the engine with live dictation. **Default:** `Live dictation first`.

Catalog: [Live dictation always wins](../../features.md#live-dictation-always-wins).

### Batch model

`Files › Watched folders › Batch model`

Selects the watched folders' engine; an empty selection follows the main dictation model. It combines with [Unload batch model after](#unload-batch-model-after). **Default:** `Same as dictation (default)`.

Catalog: [Batch on one accelerator, dictation on another](../../features.md#batch-on-one-accelerator-dictation-on-another).

### Unload batch model after

`Files › Watched folders › Unload batch model after`

Sets the idle-unload rule for a separate local batch-model slot. `Custom…` reveals a duration and unit. **Default:** `Never (keep loaded — fastest start)`; custom duration `300` seconds.

Catalog: [Batch on one accelerator, dictation on another](../../features.md#batch-on-one-accelerator-dictation-on-another).

### Status

`Files › Watched folders › Status`

Shows whether watching is off, idle, queued, or processing a particular file segment. **Default:** `Off`.

Catalog: [You can see what it is working on](../../features.md#you-can-see-what-it-is-working-on).

## Folders

### No watched folders

`Files › Folders › No watched folders`

Shows the empty state when the list contains no folders. **Default:** the stored list starts empty; the recordings folder may be seeded the first time watching starts.

### \<folder name\>

`Files › Folders › <folder name>`

Enables or pauses that dynamic watched-folder row; its title is the folder basename and its description is the full path. **Default:** On when a folder is added.

Catalog: [Your existing files are left alone](../../features.md#your-existing-files-are-left-alone).

### Add a folder

`Files › Folders › Add a folder`

Opens the folder picker and appends the chosen path to the watched list. **Default:** not applicable; this is an action.

Catalog: [A folder of recordings, transcribed while you sleep](../../features.md#a-folder-of-recordings-transcribed-while-you-sleep).
