# History settings

Open `History`. The page has three tabs: Recordings (it opens on this one), Statistics and Settings. On Recordings, row actions repeat for every history entry and the search bar appears once entries exist. The tab bar stays in view while the list scrolls.

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

Writes a note from the entry's transcription with the skill, provider and model chosen on `Notes › Settings`, shows it under the entry, and marks the entry as saved. Disabled while that entry's note is being written. **Default:** not applicable; this is a per-row action.

Catalog: [Turn a recording into a note](../../features.md#turn-a-recording-into-a-note).

### Save transcription

`History › Recordings › Save transcription`

Marks an entry as saved; the same control becomes `Remove from saved` after use. **Default:** new entries are not saved.

Catalog: [Keep the ones that matter](../../features.md#keep-the-ones-that-matter).

### Delete entry

`History › Recordings › Delete entry`

Deletes the selected history row and its Handy-owned audio files. **Default:** not applicable; this is a per-row action.

Catalog: [Copy or delete a single entry](../../features.md#copy-or-delete-a-single-entry).

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
