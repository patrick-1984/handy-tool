# Notes settings

Open `Notes`. The page has three tabs: Settings, Saved notes and Manual note. Until you pick a tab it opens on Settings while notes cannot be written yet (no provider, key or model), and on Saved notes after that. The tab bar stays in view while the page scrolls.

## Settings

### Skill

`Notes › Settings › Skills › Skill`

The skill new notes are written with: Default instructions, or one of the imported skills. A trash button next to it removes the selected skill and its copied files. **Default:** Default instructions.

Catalog: [Notes written your way, with a skill](../../features.md#notes-written-your-way-with-a-skill).

### Import file…

`Notes › Settings › Skills › Import file…`

Imports a skill from a `.md`, `.markdown` or `.txt` file, or a `.zip` or `.skill` archive, and selects it. **Default:** not applicable; this is an action.

Catalog: [Notes written your way, with a skill](../../features.md#notes-written-your-way-with-a-skill).

### Import folder…

`Notes › Settings › Skills › Import folder…`

Imports a skill from a folder (its text files, `SKILL.md` first) and selects it. **Default:** not applicable; this is an action.

Catalog: [Notes written your way, with a skill](../../features.md#notes-written-your-way-with-a-skill).

### Provider

`Notes › Settings › Provider and model › Provider`

The registered LLM provider that writes notes, listed as on Post-processing. The local token counter is not listed. **Default:** the first enabled OpenRouter provider, else the first OpenRouter provider.

Catalog: [Choose which model writes your notes](../../features.md#choose-which-model-writes-your-notes).

### API key

`Notes › Settings › Provider and model › API key`

The selected provider's API key, shown once a provider is selected. It edits the same registry entry as `Advanced settings › LLM providers`. **Default:** empty.

Catalog: [Choose which model writes your notes](../../features.md#choose-which-model-writes-your-notes).

### Model

`Notes › Settings › Provider and model › Model`

The model that writes notes, picked from the provider's live model list or typed. Empty uses the provider's own model. **Default:** `google/gemini-2.5-flash`.

Catalog: [Choose which model writes your notes](../../features.md#choose-which-model-writes-your-notes).

### Manage LLM providers

`Notes › Settings › Provider and model › Manage LLM providers`

Opens `Advanced settings › LLM providers`. **Default:** not applicable; this is an action.

### Speaker models

`Notes › Settings › Speaker detection › Speaker models`

The two speaker models used only by `History › Recordings › Make note with speakers`: Not downloaded with a Download (32 MB) button, a progress bar while they download, and Ready with a Delete button once they are in place. Delete is refused while a note with speakers is being made. **Default:** not downloaded.

Catalog: [A note that says who said what](../../features.md#a-note-that-says-who-said-what).

## Saved notes

### Go to transcript

`Notes › Saved notes › Go to transcript`

Opens `History › Recordings` at the recording a note was made from. When that recording was deleted, the button is disabled and reads Source transcript was deleted. Notes from the Manual note tab have no button. **Default:** not applicable; this is a per-note action.

Catalog: [Every note in one place, one click from its transcript](../../features.md#every-note-in-one-place).

## Manual note

### Generate note

`Notes › Manual note › Generate note`

Writes a note from the pasted or typed text with the selected skill and model, and saves it without a source recording. Disabled until there is text and the note settings are complete. **Default:** not applicable; this is an action.

Catalog: [A note from any text](../../features.md#a-note-from-any-text).
