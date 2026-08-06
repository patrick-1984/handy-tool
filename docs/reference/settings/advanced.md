# Advanced settings

Open `Advanced`. The page has six tabs in this order: App, Transcription, Providers, MCP & CLI, History, Post-processing.

## App

### Appearance

`Advanced › App › Appearance`

Sets the theme for the main and auxiliary windows. **Default:** `System`.

Catalog: [Light, dark, or follow the system](../../features.md#light-dark-or-follow-the-system).

### Start Hidden

`Advanced › App › Start Hidden`

Starts Handy Tool without opening its main window. **Default:** Off.

Catalog: [Starts with your session and stays out of the way](../../features.md#starts-with-your-session-and-stays-out-of-the-way).

### Launch on Startup

`Advanced › App › Launch on Startup`

Registers Handy Tool to launch at sign-in. **Default:** Off.

Catalog: [Starts with your session and stays out of the way](../../features.md#starts-with-your-session-and-stays-out-of-the-way).

### Show Tray Icon

`Advanced › App › Show Tray Icon`

Controls whether the tray icon is present. When off, closing the main window quits the app. **Default:** On.

Catalog: [The tray tells you what it is doing](../../features.md#the-tray-tells-you-what-it-is-doing).

### Overlay Position

`Advanced › App › Overlay Position`

Places the recording overlay at the top or bottom, or disables it. **Default:** `Bottom`.

Catalog: [See that it is listening](../../features.md#see-that-it-is-listening).

<a id="keyboard-implementation"></a>

### Keyboard Implementation

`Advanced › App › Keyboard Implementation`

Chooses which backend registers the global shortcuts with Windows. Switch to `Handy Keys` when a shortcut you set never fires; every binding is re-registered on the switch, and the change rolls back if that fails. **Default:** `Tauri Global Shortcut`.

Catalog: [A hotkey another app already owns](../../features.md#a-hotkey-another-app-already-owns).

## Transcription

### Transcribe

<a id="paste-method"></a>
#### Paste Method

`Advanced › Transcription › Transcribe › Paste Method`

Chooses how ordinary dictation inserts text. `Direct` types without a paste keystroke; `None` leaves delivery to you. **Default:** `Clipboard (Ctrl+V)`.

Catalog: [Ctrl+V doesn't work in that app](../../features.md#ctrl-v-doesnt-work-in-that-app).

<a id="paste-method-ptt"></a>
#### Paste Method (PTT)

`Advanced › Transcription › Transcribe › Paste Method (PTT)`

Chooses the delivery method used by [Push-to-Talk Shortcut](general.md#push-to-talk-shortcut), independently of ordinary dictation. **Default:** `Clipboard (Ctrl+V)`.

Catalog: [Ctrl+V doesn't work in that app](../../features.md#ctrl-v-doesnt-work-in-that-app).

#### Typing Tool

`Advanced › Transcription › Transcribe › Typing Tool` *{planned}*

Chooses which Linux input-injection utility backs `Direct` delivery. The control is not present in the shipped Windows build; macOS and Linux builds are planned. **Default:** `Auto (Recommended)`.

#### Clipboard Handling

`Advanced › Transcription › Transcribe › Clipboard Handling`

Chooses whether delivery restores the previous clipboard text or leaves the transcription there. It combines with [Clipboard restore delay](#transcribe-clipboard-restore-delay). **Default:** `Don't Modify Clipboard`.

Catalog: [Dictation doesn't steal your clipboard](../../features.md#dictation-doesnt-steal-your-clipboard).

#### Auto Submit

`Advanced › Transcription › Transcribe › Auto Submit`

Chooses whether ordinary dictation sends Enter, Ctrl+Enter, or Super+Enter after pasting. `Off` sends no submit key. **Default:** `Off`.

Catalog: [Send it without reaching for Enter](../../features.md#send-it-without-reaching-for-enter).

<a id="transcribe-clipboard-restore-delay"></a>
#### Clipboard restore delay

`Advanced › Transcription › Transcribe › Clipboard restore delay`

Adds a wait before restoring clipboard text; it matters only with [Clipboard Handling](#clipboard-handling) set to preserve the clipboard. **Default:** `Off (instant)`, in addition to the built-in delay.

Catalog: [Your remote session pastes the right thing](../../features.md#your-remote-session-pastes-the-right-thing).

<a id="transcribe-paste-delay-after-jump-windows"></a>
#### Paste delay after jump (Windows)

`Advanced › Transcription › Transcribe › Paste delay after jump (Windows)` *{Windows only}*

Sets the shared post-jump wait for local and remote targets. [Remote match strings](jumper.md#remote-match-strings) chooses the column. The choices are `Off`, `100`, `200`, `300`, `400`, `500`, `600`, `700`, `800`, `900`, `1000`, `1500`, and `2000` ms. **Default:** Local apps `300 ms`; Remote desktop `600 ms`.

Catalog: [Separate timing for remote desktops and local apps](../../features.md#separate-timing-for-remote-desktops-and-local-apps).

<a id="transcribe-jump-slot-action-on-start"></a>
#### Jump slot action on start

`Advanced › Transcription › Transcribe › Jump slot action on start` *{Windows only}*

Chooses a slot and the additional Jumper action taken when an idle Transcribe press starts a take. **Default:** slot `Hot 1`; `Do nothing`.

Catalog: [Decide what a jump does at the start and at the end of a take](../../features.md#what-a-jump-does-at-the-start-and-end-of-a-take).

<a id="transcribe-jump-slot-action-on-finish"></a>
#### Jump slot action on finish

`Advanced › Transcription › Transcribe › Jump slot action on finish` *{Windows only}*

Chooses a slot and the Jumper action taken when Transcribe finishes a take. A jump action delivers to that slot. **Default:** slot `Hot 1`; `Do nothing`.

Catalog: [Decide what a jump does at the start and at the end of a take](../../features.md#what-a-jump-does-at-the-start-and-end-of-a-take).

<a id="transcribe-track-last-output-location"></a>
#### Track last output location

`Advanced › Transcription › Transcribe › Track last output location` *{Windows only}*

When enabled, records the ordinary flow's delivery target into the selected `Save location into` slot. **Default:** Off; slot `Hot 1`.

Catalog: [Remember where the text actually landed](../../features.md#remember-where-the-text-actually-landed).

<a id="transcribe-return-focus-after-delivery"></a>
#### Return focus after delivery

`Advanced › Transcription › Transcribe › Return focus after delivery` *{Windows only}*

Returns focus to the starting window after an anchored ordinary delivery, unless you changed windows yourself. **Default:** On.

Catalog: [Focus comes back to you](../../features.md#focus-comes-back-to-you).

### Transcribe & Submit

#### Transcribe & Submit Shortcut

`Advanced › Transcription › Transcribe & Submit › Transcribe & Submit Shortcut`

Sets the shortcut that uses this group's delivery recipe. **Default:** `ctrl+alt+s`.

Catalog: [Dictate and send in one keystroke](../../features.md#dictate-and-send-in-one-keystroke).

#### Paste method

`Advanced › Transcription › Transcribe & Submit › Paste method`

Chooses the paste method for this flow only. **Default:** `Clipboard (Ctrl+V)`.

Catalog: [Its own paste method, for the one app that needs it](../../features.md#its-own-paste-method-for-the-one-app-that-needs-it).

#### Submit key

`Advanced › Transcription › Transcribe & Submit › Submit key`

Chooses the key always sent after this flow pastes. **Default:** `Enter`.

Catalog: [Enter, Ctrl+Enter, or Super+Enter](../../features.md#enter-ctrl-enter-or-super-enter).

#### When no recording is active

`Advanced › Transcription › Transcribe & Submit › When no recording is active`

Chooses whether an idle press starts a recording or does nothing. **Default:** `Start a recording`.

Catalog: [Pressing it when nothing is recording](../../features.md#pressing-it-when-nothing-is-recording).

#### Clipboard

`Advanced › Transcription › Transcribe & Submit › Clipboard`

Sets this flow's clipboard policy independently of [Clipboard Handling](#clipboard-handling). **Default:** `Don't Modify Clipboard`.

Catalog: [Its own clipboard policy](../../features.md#its-own-clipboard-policy).

<a id="submit-clipboard-restore-delay"></a>
#### Clipboard restore delay

`Advanced › Transcription › Transcribe & Submit › Clipboard restore delay`

Adds this flow's wait before restoring preserved clipboard text. **Default:** `Off (instant)`, in addition to the built-in delay.

Catalog: [Your remote session pastes the right thing](../../features.md#your-remote-session-pastes-the-right-thing).

<a id="submit-paste-delay-after-jump-windows"></a>
#### Paste delay after jump (Windows)

`Advanced › Transcription › Transcribe & Submit › Paste delay after jump (Windows)` *{Windows only}*

Shows the same shared local and remote paste-delay values as the Transcribe group. The choices are `Off`, `100`, `200`, `300`, `400`, `500`, `600`, `700`, `800`, `900`, `1000`, `1500`, and `2000` ms. **Default:** Local apps `300 ms`; Remote desktop `600 ms`.

Catalog: [The paste is swallowed right after a jump](../../features.md#the-paste-is-swallowed-right-after-a-jump).

#### Submit delay after jump (Windows)

`Advanced › Transcription › Transcribe & Submit › Submit delay after jump (Windows)` *{Windows only}*

Waits before sending the submit key after a real jump; [Remote match strings](jumper.md#remote-match-strings) selects the timing. The choices are `Off`, `100`, `200`, `300`, `400`, `500`, `600`, `700`, `800`, `900`, `1000`, `1500`, and `2000` ms. **Default:** Local apps `300 ms`; Remote desktop `600 ms`.

Catalog: [The Enter key lands in the remote window](../../features.md#the-enter-key-lands-in-the-remote-window).

<a id="submit-jump-slot-action-on-start"></a>
#### Jump slot action on start

`Advanced › Transcription › Transcribe & Submit › Jump slot action on start` *{Windows only}*

Chooses a slot and additional Jumper action for an idle press of this flow. **Default:** slot `Hot 1`; `Do nothing`.

Catalog: [Decide what a jump does at the start and at the end of a take](../../features.md#what-a-jump-does-at-the-start-and-end-of-a-take).

<a id="submit-jump-slot-action-on-finish"></a>
#### Jump slot action on finish

`Advanced › Transcription › Transcribe & Submit › Jump slot action on finish` *{Windows only}*

Chooses a slot and Jumper action when this flow finishes a take. **Default:** slot `Hot 1`; `Do nothing`.

Catalog: [You can see which slot an action targets](../../features.md#you-can-see-which-slot-an-action-targets).

<a id="submit-return-focus-after-delivery"></a>
#### Return focus after delivery

`Advanced › Transcription › Transcribe & Submit › Return focus after delivery` *{Windows only}*

Returns focus after this flow's anchored delivery. **Default:** On.

Catalog: [Focus comes back to you](../../features.md#focus-comes-back-to-you).

<a id="submit-track-last-output-location"></a>
#### Track last output location

`Advanced › Transcription › Transcribe & Submit › Track last output location` *{Windows only}*

When enabled, records this flow's delivery target into its selected `Save location into` slot. **Default:** Off; slot `Hot 1`.

Catalog: [Remember where the text actually landed](../../features.md#remember-where-the-text-actually-landed).

### Transcription

#### Translate to English

`Advanced › Transcription › Transcription › Translate to English`

Requests English output and is disabled when the selected model cannot translate. It is the same stored choice shown on General when that model exposes it. **Default:** Off.

Catalog: [Speak any language, get English](../../features.md#speak-any-language-get-english).

#### Transcription cost report

`Advanced › Transcription › Transcription › Transcription cost report`

Shows duration and cost summaries and provides `Recalculate durations` and `Download CSV` actions. **Default:** no metered usage on a fresh install.

Catalog: [Know what your dictation costs](../../features.md#know-what-your-dictation-costs).

## Providers

### API Transcription (OpenAI-compatible)

#### API URL

`Advanced › Providers › API Transcription (OpenAI-compatible) › API URL`

Sets the base URL used by the custom OpenAI-compatible speech engine. **Default:** empty.

Catalog: [Point it at any OpenAI-compatible speech endpoint](../../features.md#point-it-at-any-openai-compatible-speech-endpoint).

#### API Key

`Advanced › Providers › API Transcription (OpenAI-compatible) › API Key`

Stores the optional bearer key for that endpoint. **Default:** empty.

Catalog: [Where do I put the URL and the key?](../../features.md#where-do-i-put-the-url-and-the-key).

#### Model

`Advanced › Providers › API Transcription (OpenAI-compatible) › Model`

Sets the remote speech-model identifier. **Default:** empty.

Catalog: [Point it at any OpenAI-compatible speech endpoint](../../features.md#point-it-at-any-openai-compatible-speech-endpoint).

### OpenRouter Transcription

#### API URL

`Advanced › Providers › OpenRouter Transcription › API URL`

Sets the OpenRouter-compatible base URL for speech requests. **Default:** `https://openrouter.ai/api/v1`.

Catalog: [One OpenRouter key, many speech models](../../features.md#one-openrouter-key-many-speech-models).

#### API Key

`Advanced › Providers › OpenRouter Transcription › API Key`

Stores the key used for OpenRouter transcription. **Default:** empty.

Catalog: [One OpenRouter key, many speech models](../../features.md#one-openrouter-key-many-speech-models).

#### Transcription model

`Advanced › Providers › OpenRouter Transcription › Transcription model`

Sets the OpenRouter model identifier. **Default:** `openai/whisper-large-v3`.

Catalog: [The model list actually contains speech models](../../features.md#the-model-list-actually-contains-speech-models).

#### Endpoint

`Advanced › Providers › OpenRouter Transcription › Endpoint`

Chooses the dedicated speech route or an audio-capable chat route. **Default:** `Transcription (Whisper-style)`.

Catalog: [Whisper-style, or an audio-capable chat model](../../features.md#whisper-style-or-an-audio-capable-chat-model).

#### Audio format

`Advanced › Providers › OpenRouter Transcription › Audio format`

Chooses Opus for smaller chat-route uploads or WAV for wider compatibility. The speech route still sends WAV. **Default:** `Opus — smaller (recommended)`.

Catalog: [Ten times less audio over the wire](../../features.md#ten-times-less-audio-over-the-wire).

### Registered LLM Providers

These controls repeat for every registered provider. Fresh settings contain eleven seeded entries; their endpoint, model, price, enablement, and concurrency values differ by provider.

#### Enable this provider

`Advanced › Providers › Registered LLM Providers › Enable this provider`

Includes or excludes the provider from tools that use enabled registry entries. **Default:** the seeded value for that provider; cloud seats without configuration are disabled.

Catalog: [Unconfigured seats stay out of the run](../../features.md#unconfigured-seats-stay-out-of-the-run).

#### Base URL

`Advanced › Providers › Registered LLM Providers › Base URL`

Edits the provider name and endpoint where the seeded provider permits it. **Default:** the provider's seeded endpoint.

Catalog: [Configure a provider once, use it everywhere](../../features.md#configure-a-provider-once-use-it-everywhere).

#### API key

`Advanced › Providers › Registered LLM Providers › API key`

Stores that provider's credential. **Default:** empty.

Catalog: [Configure a provider once, use it everywhere](../../features.md#configure-a-provider-once-use-it-everywhere).

#### Model

`Advanced › Providers › Registered LLM Providers › Model`

Selects or free-types the provider model identifier; refresh fetches advertised models. **Default:** the provider's seeded model, which may be empty.

Catalog: [Find a model among hundreds](../../features.md#find-a-model-among-hundreds).

#### Cost / 1M

`Advanced › Providers › Registered LLM Providers › Cost / 1M`

Sets input and output USD per million tokens; `Persist` prevents automatic price lookup from replacing manual values. **Default:** provider-specific seeded prices; `Persist` Off.

Catalog: [Prices filled in for providers that don't publish them](../../features.md#prices-filled-in-for-providers-that-dont-publish-them).

#### Concurrency

`Advanced › Providers › Registered LLM Providers › Concurrency`

Makes a provider run sequentially with other entries in the same family. **Default:** provider-specific; seeded FLM and LM Studio entries use their family and sequential execution.

Catalog: [Several slots, one local loader](../../features.md#several-slots-one-local-loader).

## MCP & CLI

### Enable MCP & CLI server

`Advanced › MCP & CLI › Enable MCP & CLI server`

Starts the loopback server used by MCP and the command-line companion. [Port](#port) and [Token](#token) define its connection. **Default:** Off.

Catalog: [Let an agent drive the app](../../features.md#let-an-agent-drive-the-app).

### Port

`Advanced › MCP & CLI › Port`

Sets the loopback TCP port from 1024 through 65535. **Default:** `8765`.

Catalog: [Bound to localhost, behind a token — and what that does not cover](../../features.md#bound-to-localhost-behind-a-token).

### Token

`Advanced › MCP & CLI › Token`

Shows, hides, or regenerates the bearer token. It is generated on first enable. **Default:** empty until generated.

Catalog: [Bound to localhost, behind a token — and what that does not cover](../../features.md#bound-to-localhost-behind-a-token).

### Command-line companion

`Advanced › MCP & CLI › Command-line companion`

Installs or reinstalls the `handy` command and shows connection snippets. **Default:** not installed.

Catalog: [A handy command on your PATH](../../features.md#a-handy-command-on-your-path).

## History

### Crash-Safe Recording

`Advanced › History › Crash-Safe Recording`

Writes incremental Opus chunks that can be recovered after interruption. Turning it off produces uncompressed recording files that full backup does not include. **Default:** On.

Catalog: [A crash mid-dictation costs you nothing](../../features.md#a-crash-mid-dictation-costs-you-nothing).

### Recordings Folder

`Advanced › History › Recordings Folder`

Opens the recordings directory. **Default:** `File › %APPDATA%\pr.handy\recordings`.

Catalog: [Your own files in the recordings folder are never touched](../../features.md#your-own-files-in-the-recordings-folder-are-never-touched).

### History Limit

`Advanced › History › History Limit`

Sets how many newest unsaved history entries are retained; zero is allowed. [Auto-Delete Recordings](#auto-delete-recordings) can tie audio retention to it. **Default:** `5` entries.

Catalog: [Don't keep audio forever](../../features.md#dont-keep-audio-forever).

### Auto-Delete Recordings

`Advanced › History › Auto-Delete Recordings`

Chooses the retention rule for unsaved recordings. The preserve-limit label includes the current [History Limit](#history-limit). **Default:** `Keep latest 5`.

Catalog: [Don't keep audio forever](../../features.md#dont-keep-audio-forever).

## Post-processing

<a id="post-processing"></a>

### Post Processing

`Advanced › Post-processing › Post Processing`

Enables post-processing and reveals the `Post Process` page. It is the only control on this tab. **Default:** Off.

Catalog: [A second key for "clean this up with AI"](../../features.md#a-second-key-for-clean-this-up).
