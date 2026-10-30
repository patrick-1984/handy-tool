# History settings

Open `History`. The page has four tabs: Recordings (it opens on this one), Favorites, Statistics and Settings. On Recordings and Favorites, row actions repeat for every history entry and the search bar and filters appear once entries exist. The tab bar stays in view while the list scrolls; `Show me how` at its end starts the History tour ([A guided tour of History](../../features.md#a-guided-tour-of-history), also `Setups › History`).

### Open Recordings Folder

`History › Recordings › Open Recordings Folder`

Opens the app's recordings directory in File Explorer. **Default:** not applicable; this is an action.

Catalog: [Open the folder the audio actually lives in](../../features.md#open-the-folder-the-audio-lives-in).

### Search history (text or regex)...

`History › Recordings › Search history (text or regex)...`

Filters the visible entries using text or a regular expression. **Default:** empty query.

Catalog: [What did I dictate last Tuesday?](../../features.md#what-did-i-dictate-last-tuesday).

### Favorites only

`History › Recordings › Favorites only`

Shows only starred recordings. Not shown on the Favorites tab, which lists only those anyway. **Default:** Off.

Catalog: [Narrow History to favorites, a time and a length](../../features.md#narrow-history-by-favorites-date-and-length).

### Time filter

`History › Recordings` — the first list next to the search box, without a label.

`Any time`, `Last 3 hours`, `Last 12 hours`, `Last 24 hours`, `Last 7 days`, `Last 30 days` or `Custom dates…`, which adds a From and a To date. **Default:** `Any time`.

Catalog: [Narrow History to favorites, a time and a length](../../features.md#narrow-history-by-favorites-date-and-length).

### Length filter

`History › Recordings` — the second list next to the search box, without a label.

`Any length`, `Under 10 s`, `10–30 s`, `30 s–1 min`, `1–2 min`, `2–5 min`, `5–10 min` or `Over 10 min`. **Default:** `Any length`.

Catalog: [Narrow History to favorites, a time and a length](../../features.md#narrow-history-by-favorites-date-and-length).

### Select to combine

`History › Recordings › Select to combine`

The box at the start of each row. A ticked row shows its place in the order you ticked them; a bar then shows how many are selected, with `Clear` and `Combine…`, which opens the list to order and copy as one text. How they are joined is set in [Combine recordings](combine.md). **Default:** nothing selected.

Catalog: [Several takes, copied as one text](../../features.md#combine-recordings).

### Copy transcription to clipboard

`History › Recordings › Copy transcription to clipboard`

Copies the selected row's transcription. **Default:** not applicable; this is a per-row action.

Catalog: [Copy or delete a single entry](../../features.md#copy-or-delete-a-single-entry).

### Save transcription

`History › Recordings › Save transcription`

The star on each row. Marks an entry as saved and a favorite (a colored edge in Recordings, listed on the Favorites tab); the same control becomes `Remove from saved` after use. **Default:** new entries are not saved.

Catalog: [Keep the ones that matter](../../features.md#keep-the-ones-that-matter).

### More actions

`History › Recordings › More actions`

The ⋯ on each row opens its menu; each item has a line on what it does. **Default:** not applicable; these are per-row actions.

Catalog: [Fewer icons on each recording, every action explained](../../features.md#fewer-icons-every-action-explained).

#### Transcribe again

`History › Recordings › More actions › Transcribe again`

Reads the recording again with your dictation model and replaces the text; asks first when the row has text. Only for a recording that still has its audio.

#### Transcribe again with speakers

`History › Recordings › More actions › Transcribe again with speakers`

Sends the whole recording to the model with speakers of your STT server ([STT server](stt-server.md)). Only for a recording that still has its audio.

Catalog: [Find out who said what in a recording](../../features.md#transcribe-again-with-speakers).

#### Name…

`History › Recordings › More actions › Name…`

Shows a `Name this recording` field next to the date and time; ✓ or Enter keeps it, Escape cancels, an empty name removes it. Reads `Rename…` once the recording has a name. **Default:** no name.

Catalog: [Give a recording a name you will recognize](../../features.md#name-a-recording).

#### Delete

`History › Recordings › More actions › Delete`

Asks "Delete this recording and its text?", then deletes the row and its Handy-owned audio files.

Catalog: [Copy or delete a single entry](../../features.md#copy-or-delete-a-single-entry).

## Favorites

### Favorites

`History › Favorites`

The starred recordings only, with the same search, time and length filters, row actions and selection as Recordings. Empty, it says "No favorites yet. The star on a recording adds it here." **Default:** no favorites on a fresh install.

Catalog: [Narrow History to favorites, a time and a length](../../features.md#narrow-history-by-favorites-date-and-length).

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

### Save a text file next to each recording

`History › Settings › Save a text file next to each recording`

Writes each take's transcript to `handy-….txt` beside its recording in the recordings folder, from the text the take already has - the recording is not transcribed a second time. It never writes over an existing file. `Transcribe again` updates the file, and deleting the entry (or retention removing the row) removes it - only while it still holds the entry's text, so a `.txt` of your own is never replaced or deleted. **Default:** On.

### History Limit

`History › Settings › History Limit`

Sets how many newest unsaved history entries are retained; zero is allowed. [Auto-Delete Recordings](#auto-delete-recordings) can tie audio retention to it. **Default:** `5` entries.

Catalog: [Don't keep audio forever](../../features.md#dont-keep-audio-forever).

### Auto-Delete Recordings

`History › Settings › Auto-Delete Recordings`

Chooses the retention rule for unsaved recordings. The preserve-limit label includes the current [History Limit](#history-limit). **Default:** `Keep latest 5`.

Catalog: [Don't keep audio forever](../../features.md#dont-keep-audio-forever).
