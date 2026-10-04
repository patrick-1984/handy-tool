# Notes settings

Open `Notes`. The page has three tabs: Settings, Saved notes and Manual note. Until you pick a tab it opens on Settings while notes cannot be written yet (no provider, key or model), and on Saved notes after that. The tab bar stays in view while the page scrolls.

## Settings

### Your instructions

`Notes › Settings › Instructions › Your instructions`

A text box for your own instructions, sent with every note before the skills that are on; where a skill says otherwise, these win. With no skill on they are used alone, and when the box is empty too, notes follow the default instructions. Saved when you leave the box. **Default:** empty.

Catalog: [Notes written your way, with a skill](../../features.md#notes-written-your-way-with-a-skill).

### Active skills

`Notes › Settings › Skills › Active skills`

The imported skills, each with a checkbox and a trash button; every skill that is checked is used, in the list's order, each under its own name after Your instructions. The trash button removes a skill and its copied files. A line under the list says how many skills are on, or what notes follow when none is. **Default:** no skills on.

Catalog: [Notes written your way, with a skill](../../features.md#notes-written-your-way-with-a-skill).

### Import file…

`Notes › Settings › Skills › Import file…`

Imports a skill from a `.md`, `.markdown` or `.txt` file, or a `.zip` or `.skill` archive, and turns it on alongside the skills already on. **Default:** not applicable; this is an action.

Catalog: [Notes written your way, with a skill](../../features.md#notes-written-your-way-with-a-skill).

### Import folder…

`Notes › Settings › Skills › Import folder…`

Imports a skill from a folder (its text files, `SKILL.md` first) and turns it on alongside the skills already on. **Default:** not applicable; this is an action.

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

The model that writes notes, picked from every model the provider lists or typed. For an OpenRouter provider with a key, the list holds the models that key may use (OpenRouter filters it by the account's provider preferences, privacy settings and guardrails); without a key, or when that list can't be fetched, it is OpenRouter's full catalogue. The list has a search box and sorts by price (input plus output, cheapest first) or by name. For an OpenRouter provider each model shows its price per 1M input / output tokens, and the list starts with Recommended for notes (cheap): good multilingual models whose input and output prices are both at most a tenth of Claude Opus 5.5's, each with a "≈Nx cheaper than Opus 5.5" badge. Anthropic and Gemini models show OpenRouter's price for the same model where it has one; other providers show no prices. The chosen model's price is shown under the field. Empty uses the provider's own model. Picking a provider of another kind in Provider resets it: to empty (the new provider's own model), or to the default when the new provider is an OpenRouter one without a model. **Default:** `openai/gpt-6-luna`. Settings that still had the earlier default, `google/gemini-2.5-flash`, move to it once.

Catalog: [Choose which model writes your notes](../../features.md#choose-which-model-writes-your-notes).

### Manage LLM providers

`Notes › Settings › Provider and model › Manage LLM providers`

Opens `Advanced settings › LLM providers`. **Default:** not applicable; this is an action.

### Speaker models

`Notes › Settings › Speaker detection › Speaker models`

The two speaker models used only by `History › Recordings › Make note with speakers`: Not downloaded with a Download (32 MB) button, a progress bar while they download, and Ready with a Delete button once they are in place. Delete is refused while a note with speakers is being made. **Default:** not downloaded.

Catalog: [A note that says who said what](../../features.md#a-note-that-says-who-said-what).

## Saved notes

### Compare models

`Notes › Saved notes › Compare models`

A table at the top of the tab: for each model, how many notes it wrote, their average cost, average generation time and total cost, cheapest first. Costs that are not known are left out of the averages, and a model with none shows cost unknown. Clicking a row, or picking a model in Show next to the title, lists only that model's notes, each with the notes of the same text written by other models next to it; All models lists them all again. **Default:** All models.

Catalog: [Compare models on the same text, with cost and time](../../features.md#compare-models-on-the-same-text).

### Try another model

`Notes › Saved notes › Try another model`

An icon button on every note. It opens a model picker (the same list as Model, with prices) and Write note, which writes a new note from the same text with that model. It uses your current instructions, skills and output language, not the ones the first note was written with, so change none of them between the two notes when you compare models; the picker says so, and shows both skill lists when they differ. The new note appears next to the one it started from. It is disabled while Make note or Make note with speakers runs on the note's recording. **Default:** not applicable; this is a per-note action.

Catalog: [Compare models on the same text, with cost and time](../../features.md#compare-models-on-the-same-text).

### Go to transcript

`Notes › Saved notes › Go to transcript`

Opens `History › Recordings` at the recording a note was made from. When that recording was deleted, the button is disabled and reads Source transcript was deleted. Notes from the Manual note tab have no button. **Default:** not applicable; this is a per-note action.

Catalog: [Every note in one place, one click from its transcript](../../features.md#every-note-in-one-place).

## Manual note

### Generate note

`Notes › Manual note › Generate note`

Writes a note from the pasted or typed text with your instructions, the skills that are on, the output language and the model, and saves it without a source recording. Disabled until there is text and the note settings are complete. **Default:** not applicable; this is an action.

Catalog: [A note from any text](../../features.md#a-note-from-any-text).

### Output language

`Notes › Manual note › Output language`

The language new notes are written in: Same as the transcript, or one of the app's languages, listed by their own names (English, Polski, Deutsch…). A dropdown next to Generate note, disabled while a note is being written. It is the same setting as `History › Recordings › Output language`. It is the last thing the model is told, and it says the skills and instructions may be in another language, so an English skill on a Polish recording still gives a Polish note. A note written in a picked language shows that language on its meta line, next to the model and cost; Same as the transcript shows nothing there. **Default:** Same as the transcript.

Catalog: [Notes in the language you want, whatever the skill is written in](../../features.md#notes-in-the-language-you-want).
