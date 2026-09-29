# More settings

Open `More`, the last entry in the sidebar. Its Settings row has these tabs in this order: App, Output, Providers, Post-processing, MCP & CLI, Backup, Debug, About. This page covers the first five; [Backup](backup.md), [Debug](debug.md) and [About](about.md) have their own pages, as do the Tools row's [Token Count](token-count.md), [Model Testing](model-testing.md) and [Current Audio](current-audio.md). More reopens the tab you used last.

## Output

### Transcribe

<a id="paste-method"></a>

#### Paste Method

`More › Output › Transcribe › Paste Method`

Chooses how ordinary dictation inserts text. `Direct` uses simulated keystrokes without reading or writing the clipboard; `None` leaves delivery to you. **Default:** `Clipboard (Ctrl+V)`.

Catalog: [Ctrl+V doesn't work in that app](../../features.md#ctrl-v-doesnt-work-in-that-app).

<a id="paste-method-ptt"></a>

#### Paste Method (PTT)

`More › Output › Transcribe › Paste Method (PTT)`

Chooses the delivery method used by [Push-to-Talk Shortcut](general.md#push-to-talk-shortcut), independently of ordinary dictation. **Default:** `Clipboard (Ctrl+V)`.

Catalog: [Ctrl+V doesn't work in that app](../../features.md#ctrl-v-doesnt-work-in-that-app).

#### Typing Tool

`More › Output › Transcribe › Typing Tool` _{planned}_

Chooses which Linux input-injection utility backs `Direct` delivery. The control is not present in the shipped Windows build; macOS and Linux builds are planned. **Default:** `Auto (Recommended)`.

#### Clipboard Handling

`More › Output › Transcribe › Clipboard Handling`

Chooses the final clipboard state after clipboard-based delivery. `Don't Modify Clipboard` restores previous text and withholds a recovery write after failure; the take remains in History. It combines with [Clipboard restore delay](#transcribe-clipboard-restore-delay). **Default:** `Don't Modify Clipboard`.

Catalog: [Dictation doesn't steal your clipboard](../../features.md#dictation-doesnt-steal-your-clipboard); [The paste didn't land — get the words back without re-dictating](../../features.md#the-paste-didnt-land-get-the-words-back).

#### Auto Submit

`More › Output › Transcribe › Auto Submit`

Chooses whether ordinary dictation sends Enter, Ctrl+Enter, or Super+Enter after pasting. `Off` sends no submit key. **Default:** `Off`.

Catalog: [Send it without reaching for Enter](../../features.md#send-it-without-reaching-for-enter).

<a id="transcribe-clipboard-restore-delay"></a>

#### Clipboard restore delay

`More › Output › Transcribe › Clipboard restore delay`

Adds a wait before restoring clipboard text; it matters only with [Clipboard Handling](#clipboard-handling) set to preserve the clipboard. **Default:** `Off (instant)`, in addition to the built-in delay.

Catalog: [Your remote session pastes the right thing](../../features.md#your-remote-session-pastes-the-right-thing).

<a id="transcribe-paste-delay-after-jump-windows"></a>

#### Clipboard restore delay for remote desktops

`More › Output › Transcribe › Clipboard restore delay for remote desktops` _{Windows only}_

Overrides the restore delay when the delivery target is classified remote by [Remote match strings](jumper.md#remote-match-strings). `Not set` inherits the value above — and is the default, so this control changes nothing until you pick a value. The remaining choices match the delay above. Raising it trades exposure for reliability: the transcript stays on your clipboard longer. **Default:** `Not set`.

Catalog: [A remote paste gets the right clipboard, not the one before it](../../features.md#a-remote-paste-gets-the-right-clipboard).

#### Paste delay after jump (Windows)

`More › Output › Transcribe › Paste delay after jump (Windows)` _{Windows only}_

Sets the shared post-jump wait for local and remote targets. [Remote match strings](jumper.md#remote-match-strings) chooses the column. The choices are `Off`, `100`, `200`, `300`, `400`, `500`, `600`, `700`, `800`, `900`, `1000`, `1500`, and `2000` ms. **Default:** Local apps `300 ms`; Remote desktop `600 ms`.

Catalog: [Separate timing for remote desktops and local apps](../../features.md#separate-timing-for-remote-desktops-and-local-apps).

<a id="transcribe-jump-slot-action-on-start"></a>

#### Jump slot action on start

`More › Output › Transcribe › Jump slot action on start` _{Windows only}_

Chooses a slot and the additional Jumper action taken when an idle Transcribe press starts a take. **Default:** slot `Hot 1`; `Do nothing`.

Catalog: [Decide what a jump does at the start and at the end of a take](../../features.md#what-a-jump-does-at-the-start-and-end-of-a-take).

<a id="transcribe-jump-slot-action-on-finish"></a>

#### Jump slot action on finish

`More › Output › Transcribe › Jump slot action on finish` _{Windows only}_

Chooses a slot and the Jumper action taken when Transcribe finishes a take. A jump action delivers to that slot. **Default:** slot `Hot 1`; `Do nothing`.

Catalog: [Decide what a jump does at the start and at the end of a take](../../features.md#what-a-jump-does-at-the-start-and-end-of-a-take).

<a id="transcribe-track-last-output-location"></a>

#### Track last output location

`More › Output › Transcribe › Track last output location` _{Windows only}_

When enabled, records the ordinary flow's delivery target into the selected `Save location into` slot. **Default:** Off; slot `Hot 1`.

Catalog: [Remember where the text actually landed](../../features.md#remember-where-the-text-actually-landed).

<a id="transcribe-return-focus-after-delivery"></a>

#### Return focus after delivery

`More › Output › Transcribe › Return focus after delivery` _{Windows only}_

Returns focus to the starting window after an anchored ordinary delivery, unless you changed windows yourself. **Default:** On.

Catalog: [Focus comes back to you](../../features.md#focus-comes-back-to-you).

### Transcribe & Submit

#### Paste method

`More › Output › Transcribe & Submit › Paste method`

Chooses the paste method for this flow only. **Default:** `Clipboard (Ctrl+V)`.

Catalog: [Its own paste method, for the one app that needs it](../../features.md#its-own-paste-method-for-the-one-app-that-needs-it).

#### Submit key

`More › Output › Transcribe & Submit › Submit key`

Chooses the key always sent after this flow pastes. **Default:** `Enter`.

Catalog: [Enter, Ctrl+Enter, or Super+Enter](../../features.md#enter-ctrl-enter-or-super-enter).

#### When no recording is active

`More › Output › Transcribe & Submit › When no recording is active`

Chooses whether an idle press starts a recording or does nothing. **Default:** `Start a recording`.

Catalog: [Pressing it when nothing is recording](../../features.md#pressing-it-when-nothing-is-recording).

#### Clipboard

`More › Output › Transcribe & Submit › Clipboard`

Sets this flow's final clipboard state independently of [Clipboard Handling](#clipboard-handling). With `Don't Modify Clipboard`, a failed delivery does not park the transcript; recovery remains available from History. **Default:** `Don't Modify Clipboard`.

Catalog: [Its own clipboard policy](../../features.md#its-own-clipboard-policy); [The paste didn't land — get the words back without re-dictating](../../features.md#the-paste-didnt-land-get-the-words-back).

<a id="submit-clipboard-restore-delay"></a>

#### Clipboard restore delay

`More › Output › Transcribe & Submit › Clipboard restore delay`

Adds this flow's wait before restoring preserved clipboard text. **Default:** `Off (instant)`, in addition to the built-in delay.

Catalog: [Your remote session pastes the right thing](../../features.md#your-remote-session-pastes-the-right-thing).

#### Submit delay before Enter (Windows)

`More › Output › Transcribe & Submit › Submit delay before Enter (Windows)` _{Windows only}_

Waits before sending the submit key. It applies after a real jump, and always for a remote desktop target even when that window was already focused; an already-focused local target submits instantly. [Remote match strings](jumper.md#remote-match-strings) selects the timing. Changed in 1.3.0 - before that an already-focused remote target got no wait at all. The choices are `Off`, `100`, `200`, `300`, `400`, `500`, `600`, `700`, `800`, `900`, `1000`, `1500`, and `2000` ms. **Default:** Local apps `300 ms`; Remote desktop `600 ms`.

Catalog: [The Enter key lands in the remote window](../../features.md#the-enter-key-lands-in-the-remote-window).

<a id="submit-jump-slot-action-on-start"></a>

#### Jump slot action on start

`More › Output › Transcribe & Submit › Jump slot action on start` _{Windows only}_

Chooses a slot and additional Jumper action for an idle press of this flow. **Default:** slot `Hot 1`; `Do nothing`.

Catalog: [Decide what a jump does at the start and at the end of a take](../../features.md#what-a-jump-does-at-the-start-and-end-of-a-take).

<a id="submit-jump-slot-action-on-finish"></a>

#### Jump slot action on finish

`More › Output › Transcribe & Submit › Jump slot action on finish` _{Windows only}_

Chooses a slot and Jumper action when this flow finishes a take. **Default:** slot `Hot 1`; `Do nothing`.

Catalog: [You can see which slot an action targets](../../features.md#you-can-see-which-slot-an-action-targets).

<a id="submit-return-focus-after-delivery"></a>

#### Return focus after delivery

`More › Output › Transcribe & Submit › Return focus after delivery` _{Windows only}_

Returns focus after this flow's anchored delivery. **Default:** On.

Catalog: [Focus comes back to you](../../features.md#focus-comes-back-to-you).

<a id="submit-track-last-output-location"></a>

#### Track last output location

`More › Output › Transcribe & Submit › Track last output location` _{Windows only}_

When enabled, records this flow's delivery target into its selected `Save location into` slot. **Default:** Off; slot `Hot 1`.

Catalog: [Remember where the text actually landed](../../features.md#remember-where-the-text-actually-landed).

### Paste last transcription

#### Paste method

`More › Output › Paste last transcription › Paste method`

Chooses the delivery method used only by [Paste Last Transcription](general.md#paste-last-transcription). **Default:** `Clipboard (Ctrl+V)`.

Catalog: [Ctrl+V doesn't work in that app](../../features.md#ctrl-v-doesnt-work-in-that-app).

#### Clipboard

`More › Output › Paste last transcription › Clipboard`

Chooses whether re-pasting restores the previous clipboard text or leaves the transcription there. **Default:** `Don't Modify Clipboard`.

Catalog: [Dictation doesn't steal your clipboard](../../features.md#dictation-doesnt-steal-your-clipboard).

## Providers

### API Transcription (OpenAI-compatible)

#### API URL

`More › Providers › API Transcription (OpenAI-compatible) › API URL`

Sets the base URL used by the custom OpenAI-compatible speech engine. **Default:** empty.

Catalog: [Point it at any OpenAI-compatible speech endpoint](../../features.md#point-it-at-any-openai-compatible-speech-endpoint).

#### API Key

`More › Providers › API Transcription (OpenAI-compatible) › API Key`

Stores the optional bearer key for that endpoint. **Default:** empty.

Catalog: [Where do I put the URL and the key?](../../features.md#where-do-i-put-the-url-and-the-key).

#### Model

`More › Providers › API Transcription (OpenAI-compatible) › Model`

Sets the remote speech-model identifier. **Default:** empty.

Catalog: [Point it at any OpenAI-compatible speech endpoint](../../features.md#point-it-at-any-openai-compatible-speech-endpoint).

### OpenRouter Transcription

#### API URL

`More › Providers › OpenRouter Transcription › API URL`

Sets the OpenRouter-compatible base URL for speech requests. **Default:** `https://openrouter.ai/api/v1`.

Catalog: [One OpenRouter key, many speech models](../../features.md#one-openrouter-key-many-speech-models).

#### API Key

`More › Providers › OpenRouter Transcription › API Key`

Stores the key used for OpenRouter transcription. **Default:** empty.

Catalog: [One OpenRouter key, many speech models](../../features.md#one-openrouter-key-many-speech-models).

#### Transcription model

`More › Providers › OpenRouter Transcription › Transcription model`

Sets the OpenRouter model identifier. **Default:** `openai/whisper-large-v3`.

Catalog: [The model list actually contains speech models](../../features.md#the-model-list-actually-contains-speech-models).

#### Endpoint

`More › Providers › OpenRouter Transcription › Endpoint`

Chooses the dedicated speech route or an audio-capable chat route. **Default:** `Transcription (Whisper-style)`.

Catalog: [Whisper-style, or an audio-capable chat model](../../features.md#whisper-style-or-an-audio-capable-chat-model).

#### Audio format

`More › Providers › OpenRouter Transcription › Audio format`

Chooses Opus for smaller chat-route uploads or WAV for wider compatibility. The speech route still sends WAV. **Default:** `Opus — smaller (recommended)`.

Catalog: [Ten times less audio over the wire](../../features.md#ten-times-less-audio-over-the-wire).

#### Transcription cost report

`More › Providers › OpenRouter Transcription › Transcription cost report`

Shows duration and cost summaries and provides `Recalculate durations` and `Download CSV` actions. **Default:** no metered usage on a fresh install.

Catalog: [Know what your dictation costs](../../features.md#know-what-your-dictation-costs).

### Registered LLM Providers

These controls repeat for every registered provider. Fresh settings contain eleven seeded entries; their endpoint, model, price, enablement, and concurrency values differ by provider.

#### Enable this provider

`More › Providers › Registered LLM Providers › Enable this provider`

Includes or excludes the provider from tools that use enabled registry entries. **Default:** the seeded value for that provider; cloud seats without configuration are disabled.

Catalog: [Unconfigured seats stay out of the run](../../features.md#unconfigured-seats-stay-out-of-the-run).

#### Base URL

`More › Providers › Registered LLM Providers › Base URL`

Edits the provider name and endpoint where the seeded provider permits it. **Default:** the provider's seeded endpoint.

Catalog: [Configure a provider once, use it everywhere](../../features.md#configure-a-provider-once-use-it-everywhere).

#### API key

`More › Providers › Registered LLM Providers › API key`

Stores that provider's credential. **Default:** empty.

Catalog: [Configure a provider once, use it everywhere](../../features.md#configure-a-provider-once-use-it-everywhere).

#### Model

`More › Providers › Registered LLM Providers › Model`

Selects or free-types the provider model identifier; refresh fetches advertised models. **Default:** the provider's seeded model, which may be empty.

Catalog: [Find a model among hundreds](../../features.md#find-a-model-among-hundreds).

#### Cost / 1M

`More › Providers › Registered LLM Providers › Cost / 1M`

Sets input and output USD per million tokens; `Persist` prevents automatic price lookup from replacing manual values. **Default:** provider-specific seeded prices; `Persist` Off.

Catalog: [Prices filled in for providers that don't publish them](../../features.md#prices-filled-in-for-providers-that-dont-publish-them).

#### Concurrency

`More › Providers › Registered LLM Providers › Concurrency`

Makes a provider run sequentially with other entries in the same family. **Default:** provider-specific; seeded FLM and LM Studio entries use their family and sequential execution.

Catalog: [Several slots, one local loader](../../features.md#several-slots-one-local-loader).

## MCP & CLI

### Enable MCP & CLI server

`More › MCP & CLI › Enable MCP & CLI server`

Starts the loopback server used by MCP and the command-line companion. [Port](#port) and [Token](#token) define its connection. **Default:** Off.

Catalog: [Let an agent drive the app](../../features.md#let-an-agent-drive-the-app).

### Port

`More › MCP & CLI › Port`

Sets the loopback TCP port from 1024 through 65535. **Default:** `8765`.

Catalog: [Bound to localhost, behind a token — and what that does not cover](../../features.md#bound-to-localhost-behind-a-token).

### Token

`More › MCP & CLI › Token`

Shows, hides, or regenerates the bearer token. It is generated on first enable. **Default:** empty until generated.

Catalog: [Bound to localhost, behind a token — and what that does not cover](../../features.md#bound-to-localhost-behind-a-token).

### Command-line companion

`More › MCP & CLI › Command-line companion`

Installs or reinstalls the `handy` command and shows connection snippets. **Default:** not installed.

Catalog: [A handy command on your PATH](../../features.md#a-handy-command-on-your-path).

## Post-processing

<a id="post-processing"></a>

### Post Processing

`More › Post-processing › Post Processing`

Enables post-processing. Once it is on, its hotkey, provider and prompt controls appear below it on the same tab; they are described in [Post-processing settings](post-processing.md). **Default:** Off.

Catalog: [A second key for "clean this up with AI"](../../features.md#a-second-key-for-clean-this-up).
