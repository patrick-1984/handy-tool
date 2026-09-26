# History settings

Open `History`. Row actions repeat for every history entry; the search bar appears once entries exist. The History settings group at the bottom of the page holds the recording and retention settings.

### Open Recordings Folder

`History › Open Recordings Folder`

Opens the app's recordings directory in File Explorer. **Default:** not applicable; this is an action.

Catalog: [Open the folder the audio actually lives in](../../features.md#open-the-folder-the-audio-lives-in).

### Search history (text or regex)...

`History › Search history (text or regex)...`

Filters the visible entries using text or a regular expression. **Default:** empty query.

Catalog: [What did I dictate last Tuesday?](../../features.md#what-did-i-dictate-last-tuesday).

### Copy transcription to clipboard

`History › Copy transcription to clipboard`

Copies the selected row's transcription. **Default:** not applicable; this is a per-row action.

Catalog: [Copy or delete a single entry](../../features.md#copy-or-delete-a-single-entry).

### Save transcription

`History › Save transcription`

Marks an entry as saved; the same control becomes `Remove from saved` after use. **Default:** new entries are not saved.

Catalog: [Keep the ones that matter](../../features.md#keep-the-ones-that-matter).

### Delete entry

`History › Delete entry`

Deletes the selected history row and its Handy-owned audio files. **Default:** not applicable; this is a per-row action.

Catalog: [Copy or delete a single entry](../../features.md#copy-or-delete-a-single-entry).

## History settings

### Crash-Safe Recording

`History › Crash-Safe Recording`

Writes incremental Opus chunks that can be recovered after interruption. Turning it off produces uncompressed recording files that full backup does not include. **Default:** On.

Catalog: [A crash mid-dictation costs you nothing](../../features.md#a-crash-mid-dictation-costs-you-nothing).

### History Limit

`History › History Limit`

Sets how many newest unsaved history entries are retained; zero is allowed. [Auto-Delete Recordings](#auto-delete-recordings) can tie audio retention to it. **Default:** `5` entries.

Catalog: [Don't keep audio forever](../../features.md#dont-keep-audio-forever).

### Auto-Delete Recordings

`History › Auto-Delete Recordings`

Chooses the retention rule for unsaved recordings. The preserve-limit label includes the current [History Limit](#history-limit). **Default:** `Keep latest 5`.

Catalog: [Don't keep audio forever](../../features.md#dont-keep-audio-forever).
