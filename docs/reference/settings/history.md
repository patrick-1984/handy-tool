# History settings

Open `History`. The page has three tabs: Recordings (it opens on this one), Statistics and Settings. On Recordings, row actions repeat for every history entry and the search bar appears once entries exist. The tab bar stays in view while the list scrolls.

### Note skills

`History › Recordings › Note skills`

Turns skills on and off for new notes, right from the list: a compact button that names the skill that is on, says how many are on, or says No skills, and opens a small menu with a checkbox per imported skill. Shown only once a skill is imported. It is the same setting as `Notes › Settings › Skills › Active skills`. **Default:** no skills on.

Catalog: [Notes written your way, with a skill](../../features.md#notes-written-your-way-with-a-skill).

### Open Recordings Folder

`History › Recordings › Open Recordings Folder`

Opens the app's recordings directory in File Explorer. **Default:** not applicable; this is an action.

Catalog: [Open the folder the audio actually lives in](../../features.md#open-the-folder-the-audio-lives-in).

### Search history (text or regex)...

`History › Recordings › Search history (text or regex)...`

Filters the visible entries using text or a regular expression. **Default:** empty query.

Catalog: [What did I dictate last Tuesday?](../../features.md#what-did-i-dictate-last-tuesday).

### Copy transcription to clipboard

`History › Recordings › Copy transcription to clipboard`

Copies the selected row's transcription. **Default:** not applicable; this is a per-row action.

Catalog: [Copy or delete a single entry](../../features.md#copy-or-delete-a-single-entry).

### Make note

`History › Recordings › Make note`

Writes a note from the entry's transcription with your instructions, the skills that are on, the note language, provider and model chosen on `Notes › Settings`, shows it under the entry with its model, cost, tokens and generation time, and marks the entry as saved. Disabled while that entry's note is being written. **Default:** not applicable; this is a per-row action.

Catalog: [Turn a recording into a note](../../features.md#turn-a-recording-into-a-note).

### Make note with speakers

`History › Recordings › Make note with speakers`

Labels who is speaking in the entry's recording with the selected transcription model and the speaker models, then writes a note from the labelled text like Make note and marks the entry as saved. Shown only while the entry still has its recording. Offers the speaker model download first when it is missing. Disabled, like Make note and Delete, while that entry's note is being made. **Default:** not applicable; this is a per-row action.

Catalog: [A note that says who said what](../../features.md#a-note-that-says-who-said-what).

### Save transcription

`History › Recordings › Save transcription`

Marks an entry as saved; the same control becomes `Remove from saved` after use. **Default:** new entries are not saved.

Catalog: [Keep the ones that matter](../../features.md#keep-the-ones-that-matter).

### Delete entry

`History › Recordings › Delete entry`

Deletes the selected history row and its Handy-owned audio files. **Default:** not applicable; this is a per-row action.

Catalog: [Copy or delete a single entry](../../features.md#copy-or-delete-a-single-entry).

### Try another model

`History › Recordings › Try another model`

An icon button on each note under an entry. It opens a model picker with prices and Write note, which writes a new note from the same text with that model, using your current instructions, skills and note language (the picker says so, and shows both skill lists when they differ); the new note appears next to the original, side by side, so the two can be compared. Make note and Make note with speakers stay disabled while it runs, and it stays disabled while they run. **Default:** not applicable; this is a per-note action.

Catalog: [Compare models on the same text, with cost and time](../../features.md#compare-models-on-the-same-text).

## Statistics

### Recording statistics

`History › Statistics`

How many recordings you made and how long they were, per day (last 7), week (last 4), month (last 12) and year, with `Recalculate durations` and `Download CSV` actions. It is the Providers cost report without the cost. The all-time line also counts recordings that History Limit or Auto-Delete Recordings have since removed; entries you delete yourself are not counted. **Default:** no recordings on a fresh install.

Catalog: [How much have I dictated?](../../features.md#how-much-have-i-dictated).

## History settings

### Crash-Safe Recording

`History › Settings › Crash-Safe Recording`

Writes incremental Opus chunks that can be recovered after interruption. Turning it off produces uncompressed recording files that full backup does not include. **Default:** On.

Catalog: [A crash mid-dictation costs you nothing](../../features.md#a-crash-mid-dictation-costs-you-nothing).

### History Limit

`History › Settings › History Limit`

Sets how many newest unsaved history entries are retained; zero is allowed. [Auto-Delete Recordings](#auto-delete-recordings) can tie audio retention to it. **Default:** `5` entries.

Catalog: [Don't keep audio forever](../../features.md#dont-keep-audio-forever).

### Auto-Delete Recordings

`History › Settings › Auto-Delete Recordings`

Chooses the retention rule for unsaved recordings. The preserve-limit label includes the current [History Limit](#history-limit). **Default:** `Keep latest 5`.

Catalog: [Don't keep audio forever](../../features.md#dont-keep-audio-forever).
