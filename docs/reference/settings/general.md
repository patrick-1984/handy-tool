# General settings

Open `General`. The first group repeats the page name, so its breadcrumbs omit that duplicate segment.

## General

### Transcribe Shortcut

`General › Transcribe Shortcut`

Sets the toggle shortcut that starts or finishes ordinary dictation. **Default:** `ctrl+space`.

Catalog: [Press one key, speak, and the text appears where you were typing](../../features.md#press-one-key-and-speak).

### Push-to-Talk Shortcut

`General › Push-to-Talk Shortcut`

Sets the hold-to-record shortcut. It uses [Transcription Mode (PTT)](#transcription-mode-ptt) and the separate Advanced [Paste Method (PTT)](advanced.md#paste-method-ptt). **Default:** `ctrl+alt+space`.

Catalog: [Hold a key for a one-line thought](../../features.md#hold-to-talk).

### Cancel behavior

`General › Cancel behavior`

Chooses whether every recording-cancel entry point finishes without delivery or destroys the take. **Default:** `Finish, save to history only`.

Catalog: [Escape stops the delivery, not your words](../../features.md#escape-stops-the-delivery-not-your-words).

## Model settings

This group is named `{{model}} Settings` on screen, with the active model name substituted. It appears only when a selected model exposes at least one of these controls.

### Language

`General › Language`

Provides a spoken-language hint when the active model accepts one. Auto-detect-only models show the same label as a read-only row with `Auto-detected`. **Default:** `Auto`.

Catalog: [Why can't I force the language on this model?](../../features.md#why-cant-i-force-the-language-on-this-model).

### Translate to English

`General › Translate to English`

Requests English output instead of same-language transcription; the toggle appears only for a model that supports translation. It uses the selected [Language](#language). **Default:** Off.

Catalog: [Speak any language, get English](../../features.md#speak-any-language-get-english).

## Transcription

### Transcription Mode

`General › Transcription › Transcription Mode`

Chooses Live or Post-Recording processing for the ordinary toggle flow. **Default:** `Post-Recording`.

Catalog: [Watch the text appear, or wait for the most accurate pass](../../features.md#live-or-post-recording).

### Transcription Mode (PTT)

`General › Transcription › Transcription Mode (PTT)`

Makes the same choice specifically for [Push-to-Talk Shortcut](#push-to-talk-shortcut). **Default:** `Live`.

Catalog: [Watch the text appear, or wait for the most accurate pass](../../features.md#live-or-post-recording).

### GPU Device

`General › Transcription › GPU Device` *{Windows only}*

Chooses automatic selection, CPU-only processing, or a named Vulkan adapter for local Whisper. It appears only while a Whisper model is selected. **Default:** `Auto (Default)`.

Catalog: [Pick which GPU transcribes](../../features.md#pick-which-gpu-transcribes).

### Custom Words

`General › Transcription › Custom Words`

Edits the terms used by transcript word correction. Correction aggressiveness is controlled by [Word Correction Threshold](debug.md#word-correction-threshold). **Default:** empty list.

Catalog: [Names and jargon stop coming back mangled](../../features.md#names-and-jargon-stop-coming-back-mangled).

### Append Trailing Space

`General › Transcription › Append Trailing Space`

Adds one space to the delivered text so consecutive takes do not run together. **Default:** Off.

Catalog: [The next dictation doesn't run into the last one](../../features.md#the-next-dictation-doesnt-run-into-the-last-one).

## Paste last transcription

### Paste Last Transcription

`General › Paste last transcription › Paste Last Transcription`

Sets the recovery shortcut that re-pastes the most recent in-memory transcription, or falls back to History after restart. A delivery-failure toast names this shortcut when it is bound. It uses the two controls below and never submits. **Default:** `ctrl+alt+p`.

Catalog: [The paste didn't land — get the words back without re-dictating](../../features.md#the-paste-didnt-land-get-the-words-back).

### Paste method

`General › Paste last transcription › Paste method`

Chooses the delivery method used only by [Paste Last Transcription](#paste-last-transcription). **Default:** `Clipboard (Ctrl+V)`.

Catalog: [Ctrl+V doesn't work in that app](../../features.md#ctrl-v-doesnt-work-in-that-app).

### Clipboard

`General › Paste last transcription › Clipboard`

Chooses whether re-pasting restores the previous clipboard text or leaves the transcription there. **Default:** `Don't Modify Clipboard`.

Catalog: [Dictation doesn't steal your clipboard](../../features.md#dictation-doesnt-steal-your-clipboard).

## Sound

### Microphone

`General › Sound › Microphone`

Selects the input device for subsequent takes; reset returns to the system device. **Default:** system default (`None` stored).

Catalog: [Change microphone without restarting](../../features.md#change-microphone-without-restarting).

### Mute While Recording

`General › Sound › Mute While Recording`

Mutes system output for the duration of each recording. **Default:** Off.

Catalog: [Your music doesn't end up in the transcript](../../features.md#your-music-doesnt-end-up-in-the-transcript).

### Audio Feedback

`General › Sound › Audio Feedback`

Enables the start and stop cue sounds. Turning it on enables [Output Device](#output-device) and [Volume](#volume). **Default:** Off.

Catalog: [Hear when the microphone is hot](../../features.md#hear-when-the-microphone-is-hot).

### Output Device

`General › Sound › Output Device`

Selects where cue sounds play; it is disabled while [Audio Feedback](#audio-feedback) is off. **Default:** system default (`None` stored).

Catalog: [Hear when the microphone is hot](../../features.md#hear-when-the-microphone-is-hot).

### Volume

`General › Sound › Volume`

Sets cue-sound volume from 0 to 100 percent; it is disabled while [Audio Feedback](#audio-feedback) is off. **Default:** `100%`.

Catalog: [Hear when the microphone is hot](../../features.md#hear-when-the-microphone-is-hot).

## Updates

This group is the only part of Handy Tool that contacts the network without you asking. It reads the public GitHub releases feed; no audio, transcript, setting, or key is sent.

<a id="check-for-updates-automatically"></a>

### Check for updates automatically

`General › Updates › Check for updates automatically`

Checks the public GitHub releases feed once a day and shows a banner in the sidebar when a newer release exists. Turning it off disables the three controls below and stops all background network activity. **Default:** On.

### Install updates silently

`General › Updates › Install updates silently`

Lets a downloaded update close, replace, and reopen the app inside the allowed window instead of waiting for you. It is disabled while [Check for updates automatically](#check-for-updates-automatically) is off. **Default:** Off.

### Silent update time

`General › Updates › Silent update time`

Sets the center of the local-time window used by [Install updates silently](#install-updates-silently). It is disabled unless both toggles above are on. **Default:** `04:00`.

### Daily randomization

`General › Updates › Daily randomization`

Spreads the silent install across a different minute each day, from 0 to 180 minutes either side of the chosen time. It is disabled unless both toggles above are on. **Default:** `30` minutes.

### Check now

`General › Updates › Check now`

Runs one check immediately and reports the result and the time of the last check. A portable copy reports that it cannot update in place and points you at the portable release. **Default:** no check has run on a fresh install.
