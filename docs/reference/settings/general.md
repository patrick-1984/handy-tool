# General settings

Open `General`. The first group repeats the page name, so its breadcrumbs omit that duplicate segment.

## General

### Record/Transcribe Shortcut

`General › Record/Transcribe Shortcut`

Sets the toggle shortcut that starts or finishes ordinary dictation. **Default:** `ctrl+space`.

Catalog: [Press one key, speak, and the text appears where you were typing](../../features.md#press-one-key-and-speak).

### Push-to-Talk Shortcut

`General › Push-to-Talk Shortcut`

Sets the hold-to-record shortcut. It uses [Transcription Mode (PTT)](#transcription-mode-ptt) and the separate [Paste Method (PTT)](advanced.md#paste-method-ptt) on `More › Output`. **Default:** `ctrl+shift+space`.

Catalog: [Hold a key for a one-line thought](../../features.md#hold-to-talk).

### Cancel behavior

`General › Cancel behavior`

Chooses whether every recording-cancel entry point finishes without delivery or destroys the take. **Default:** `Finish, save to history only`.

Catalog: [Escape stops the delivery, not your words](../../features.md#escape-stops-the-delivery-not-your-words).

### Pause button

`General › Pause button`

Adds a pause button to the recording overlay. While paused nothing is recorded; resuming continues the same take. After 10 minutes paused the microphone is released until you resume. When on, a Pause / Resume shortcut row appears under it. **Default:** Off; the shortcut has no default.

Catalog: [Pause a take without ending it](../../features.md#pause-a-take).

### Transcribe & Submit Shortcut

`General › Transcribe & Submit Shortcut`

Sets the shortcut that uses this group's delivery recipe. **Default:** `ctrl+shift+f9`.

Catalog: [Dictate and send in one keystroke](../../features.md#dictate-and-send-in-one-keystroke).

### Paste Last Transcription

`General › Paste Last Transcription`

Sets the recovery shortcut that re-pastes the most recent in-memory transcription, or falls back to History after restart. A delivery-failure toast names this shortcut when it is bound. It uses the two controls below and never submits. **Default:** `ctrl+shift+f10`.

Catalog: [The paste didn't land — get the words back without re-dictating](../../features.md#the-paste-didnt-land-get-the-words-back).

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

### Live text box

`General › Transcription › Live text box`

Shows the words being spoken in a small box next to the recording overlay, updated about every 1.5 seconds. While it is on, every take runs in Live mode and the box's text is the transcript; at stop only the last second or two is added. Needs a model that runs on this PC: with a remote engine selected, switching it on shows which of your models work instead. The overlay's T button, and the Live Text Box On/Off shortcut (shown under it while it is on; no default key), switch the same setting. **Default:** Off.

Catalog: [See your words next to the overlay while you talk](../../features.md#live-text-box).

### Live text shows

`General › Transcription › Live text shows`

Shown while [Live text box](#live-text-box) is on. `Last words` is one line of the newest words; `Whole text` is the take so far in a box that starts at one line and grows, up to 40% of the screen height, after which the oldest lines slide away. **Default:** `Last words`.

Catalog: [See your words next to the overlay while you talk](../../features.md#live-text-box).

### Fade when you stop talking

`General › Transcription › Fade when you stop talking`

Shown while [Live text box](#live-text-box) is on. The live text fades away three seconds after it stops changing, and your next words bring it back. **Default:** Off.

Catalog: [See your words next to the overlay while you talk](../../features.md#live-text-box).

### Box width

`General › Transcription › Box width`

Shown while [Live text box](#live-text-box) is on. How wide the box is: `Narrow`, `Medium`, `Wide` or `Extra wide` (360, 460, 640 or 860 px). A wider box needs fewer lines. **Default:** `Medium`.

Catalog: [See your words next to the overlay while you talk](../../features.md#live-text-box).

### Box height

`General › Transcription › Box height`

Shown in Whole text mode. How many lines the live text box shows before the oldest lines slide out: `Small (3 lines)`, `Medium (6 lines)`, `Large (10 lines)` or `Extra large (16 lines)`, at most half the screen's height. **Default:** `Medium (6 lines)` (`live_text_lines` = 6).

### Text size

`General › Transcription › Text size`

How big the text in the live text box is: `Small` (13 px), `Normal` (15 px), `Large` (18 px) or `Extra large` (22 px). The one-line box grows with it. **Default:** `Normal` (`live_text_font_size` = 15).

Catalog: [See your words next to the overlay while you talk](../../features.md#live-text-box).

### Show the text as it's transcribed

`General › Transcription › Show the text as it's transcribed`

For a take without the live text box: when it stops, the box appears and the transcript is typed into it as it comes in — the segments already transcribed during the take at once, each later one as it lands, then the final text; the box stays until the typing is done plus 1.5 s. Just to watch: delivery is unchanged. Uses the live text box's style and width. **Default:** Off.

Catalog: [See your words next to the overlay while you talk](../../features.md#live-text-box).

### Undo last word

`General › Transcription › Undo last word`

Each press of its shortcut first brings the live text up to date, then removes the newest word from it and cuts the recording back to where that word started; holding the shortcut keeps removing words. Needs a Parakeet model. When on, an Undo Last Word shortcut row appears under it. **Default:** Off; the shortcut is `ctrl+backspace`, registered only during a live take.

Catalog: [Take back the last word without starting over](../../features.md#undo-last-word).

### GPU Device

`General › Transcription › GPU Device` _{Windows only}_

Chooses automatic selection, CPU-only processing, or a named Vulkan adapter for local Whisper. It appears only while a Whisper model is selected. **Default:** `Auto (Default)`.

Catalog: [Pick which GPU transcribes](../../features.md#pick-which-gpu-transcribes).

### Custom Words

`General › Transcription › Custom Words`

Edits the terms used by transcript word correction: type a word and press Enter to add it, or click the × on a word to remove it. Correction aggressiveness is controlled by [Word Correction Threshold](debug.md#word-correction-threshold). **Default:** empty list.

Catalog: [Names and jargon stop coming back mangled](../../features.md#names-and-jargon-stop-coming-back-mangled).

### Append Trailing Space

`General › Transcription › Append Trailing Space`

Adds one space to the delivered text so consecutive takes do not run together. **Default:** Off.

Catalog: [The next dictation doesn't run into the last one](../../features.md#the-next-dictation-doesnt-run-into-the-last-one).

## Sound

### Microphone

`General › Sound › Microphone`

Selects the input device for subsequent takes; reset returns to the system device. **Default:** system default (`None` stored).

Catalog: [Change microphone without restarting](../../features.md#change-microphone-without-restarting).

### Keep microphone ready

`General › Sound › Keep microphone ready`

Keeps the microphone open for 1, 5 or 15 minutes after each take so the next take starts without an idle microphone's wake-up delay; the system microphone indicator stays lit meanwhile. Hidden while Always-On Microphone is on. **Default:** Off (`0` stored).

Catalog: [The microphone light is off when you're not dictating](../../features.md#the-microphone-light-is-off-when-youre-not-dictating).

### Wait for the microphone to warm up

`General › Sound › Wait for the microphone to warm up`

After a cold start (the microphone took over 250 ms to deliver audio), the overlay keeps reading **Starting mic...** until the microphone has warmed up: never while it still sends digital silence (some microphones send exact zeros for a while after they start or are plugged in), and then until its fade-in has passed since its first sound. Each cold start measures its own fade-in from its first 3 s of levels (from its first sound to the moment the level settles within 6 dB of its floor); the wait is the average of the last 5 measurements plus 100 ms, at most 3 s, and 0.6 s before the first measurement. A microphone still silent 3 s after its first audio is shown as ready anyway. The description shows the measured figure. A warm start (Keep microphone ready) is never delayed. Hidden while Always-On Microphone is on. **Default:** On (`mic_warmup_wait`; measurements in `mic_fade_in_measured_ms`).

Catalog: [The microphone light is off when you're not dictating](../../features.md#the-microphone-light-is-off-when-youre-not-dictating).

### Warn when you speak too quietly

`General › Sound › Warn when you speak too quietly`

While recording, **Too quiet — speak up** shows for 2 s (see [Show it in its own box](#show-it-in-its-own-box) for where) when it hears voice-like sound that the speech detector is not keeping: for about 0.7 s within 1.5 s, frames with a speech probability from 0.08 (the detector keeps them only above 0.3) standing 8 dB or more above the room's noise floor. It then waits 4.5 s before it can say so again. It cannot tell when words were heard wrong. **Default:** On (`too_quiet_hint`).

Catalog: [The microphone light is off when you're not dictating](../../features.md#the-microphone-light-is-off-when-youre-not-dictating).

### Show it in its own box

`General › Sound › Warn when you speak too quietly › Show it in its own box`

Shown while Warn when you speak too quietly is on. On: the hint appears in a small box just under the recording overlay, which keeps its sound bars; with the overlay at the bottom of the screen the box sits in the gap above the taskbar, so it never covers the live text box. Off: the overlay shows the hint instead of its sound bars, where longer translations are cut off when the pause button is on. On macOS the overlay always shows it itself. **Default:** On (`too_quiet_hint_box`).

Catalog: [The microphone light is off when you're not dictating](../../features.md#the-microphone-light-is-off-when-youre-not-dictating).

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

## App

The app itself: how it looks and starts, and the recording overlay. (Until 1.13 a tab of More.)

### Appearance setup

`General › App › Appearance setup`

A tinted row at the top of the group: `Start setup` opens `Setups` and starts the Appearance setup, which shows every look choice moving before you pick it. See [Setups › Appearance](setups.md#appearance).

### Appearance

`General › App › Appearance`

Sets the theme for the main and auxiliary windows. **Default:** `System`.

Catalog: [Light, dark, or follow the system](../../features.md#light-dark-or-follow-the-system).

### Start Hidden

`General › App › Start Hidden`

Starts Handy Tool without opening its main window. **Default:** Off.

Catalog: [Starts with your session and stays out of the way](../../features.md#starts-with-your-session-and-stays-out-of-the-way).

### Reopen Last Page

`General › App › Reopen Last Page`

Opens Handy Tool on the page that was open when it was closed (for example History) instead of General. The page is remembered on this PC only. **Default:** On (`reopen_last_page`).

### Launch on Startup

`General › App › Launch on Startup`

Registers Handy Tool to launch at sign-in. **Default:** Off.

Catalog: [Starts with your session and stays out of the way](../../features.md#starts-with-your-session-and-stays-out-of-the-way).

### Show Tray Icon

`General › App › Show Tray Icon`

Controls whether the tray icon is present. When off, closing the main window quits the app. **Default:** On.

Catalog: [The tray tells you what it is doing](../../features.md#the-tray-tells-you-what-it-is-doing).

### Overlay Position

`General › App › Overlay Position`

Places the recording overlay at the top or bottom, or disables it. **Default:** `Bottom`.

Catalog: [See that it is listening](../../features.md#see-that-it-is-listening).

### Progress Style

`General › App › Progress Style`

How the recording overlay shows how far a transcription is: `Line along the bottom` (a thin cyan line filling along its bottom edge), `Light around the edge` (a glowing light running round the overlay from the bottom centre, centred on its border, lit and glowing evenly behind it up to the figure) or `Light circling the edge` (a 1 px light with a fading tail circling the border, one lap every 1.6 s, no glow; the figure is in the text; with reduced motion it becomes the line). **Default:** `Line along the bottom` (`progress_style` = `line`).

Catalog: [See that it is listening](../../features.md#see-that-it-is-listening).

### Preview

`General › App › Preview`

The recording pill as the settings around it make it, moving: recording on the left (the T lit while the live text box is on, the sound bars) and transcribing on the right, its progress running from 0 to 100% over and over with the progress style, Glowing Line, Glow Strength and Progress Colour, at the Overlay Size. Nothing to set; it follows every change at once.

Catalog: [See that it is listening](../../features.md#see-that-it-is-listening).

### Glowing Line

`General › App › Glowing Line`

With `Line along the bottom`, gives the line the same glow as the light around the edge; the glow reaches past the overlay's edge. Greyed out with `Light around the edge`, which always glows. **Default:** Off (`progress_line_glow` = `false`).

Catalog: [See that it is listening](../../features.md#see-that-it-is-listening).

### Progress Colour

`General › App › Progress Colour`

The colour of the pill's glowing parts: the progress light and its glow (`Line along the bottom`, `Light around the edge`, `Light circling the edge`), set with three sliders: `Hue` (0-360°, which colour), `Saturation` (how vivid) and `Lightness` (20-85%). The Preview row shows the result on the pill; the reset button brings back the default. The light's bright centre is the same colour mixed with white. The glowing T (live text box on) and the sound bars take the colour too, quiet bars a darker shade of it. **Default:** cyan (`progress_color` empty; a chosen colour is stored as `#rrggbb`).

Catalog: [See that it is listening](../../features.md#see-that-it-is-listening).

### Glow Strength

`General › App › Glow Strength`

How strongly the progress light glows (the light around the edge, and the line while Glowing Line is on), from `0%` (no glow) to `200%`, in steps of 10. The glow always fades out within the overlay window, so a stronger glow is brighter, not wider. Greyed out while nothing glows (a plain line). **Default:** `100%` (`progress_glow` = 100).

Catalog: [See that it is listening](../../features.md#see-that-it-is-listening).

### Overlay Size

`General › App › Overlay Size`

Scales the recording overlay, and the "Too quiet" box under it: `Normal`, `Large` (125%) or `Extra large` (150%). **Default:** `Normal` (`pill_scale` = 100).

Catalog: [See that it is listening](../../features.md#see-that-it-is-listening).

<a id="keyboard-implementation"></a>

### Wide Sound Bars

`General › App › Wide Sound Bars`

Wider sound bars on the recording overlay: 2.5 px bars with 2.5 px gaps (77.5 px for the 16 bars, the designer's size) instead of 2 px (62 px). **Default:** Off (`sound_bars_wide` = `false`).

Catalog: [See that it is listening](../../features.md#see-that-it-is-listening).

### Keyboard Implementation

`General › App › Keyboard Implementation`

Chooses which backend registers the global shortcuts with Windows. Switch to `Handy Keys` when a shortcut you set never fires; every binding is re-registered on the switch, and the change rolls back if that fails. **Default:** `Tauri Global Shortcut`.

Catalog: [A hotkey another app already owns](../../features.md#a-hotkey-another-app-already-owns).

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
