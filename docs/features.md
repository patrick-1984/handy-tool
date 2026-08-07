# Feature catalog

Everything Handy Tool does, named for the problem it removes.

This is the one place in the documentation where features are described. Every other page —
the learning path, the tool pages, the settings reference, the troubleshooting index — links
here rather than repeating an explanation. If two pages ever disagree about what something
does, this file is the one that is right.

**Platform.** Windows x64 is the only build produced and released. macOS and Linux builds are
planned and in the queue; nothing below is available on them today. A number of features are
Windows-only *in code* — the entire Jumper family, portable mode, the GPU picker — and they
will not arrive with the macOS and Linux builds either. Those carry a *{Windows only}* marker.

**Where.** Locations are written as breadcrumbs in the app's own words:
`Advanced › Transcription › Transcribe › Paste Method`. Read them as page, then tab, then
group, then control; `= Value` means "set it to this". Labels are copied character-for-character
from the app, including capitalization, `&` and `...`.

**Since.** 1.0.0 is the first public release, so everything in this catalog ships in it. The
**Since** line records the internal development version in which the behavior landed, so you
can see how long a guarantee has been in service. Three of the largest features — the Jumper,
Transcribe & Submit and the Translator — were built during a window with no published
changelog; their entries say "the 0.3x series" rather than invent a number.

---

## What is in this file

<a id="section-index"></a>

| Section                                                  | What it covers                                                         |
| -------------------------------------------------------- | ---------------------------------------------------------------------- |
| [If you have been burned before](#section-burned-before) | The entries that exist because something once cost somebody real work. |
| [Defaults — what you get out of the box](#defaults)      | What happens if you change nothing.                                    |
| [Transcription](#section-transcription)                  | Press a key, speak, get the text where your cursor is.                 |
| [Transcribe & Submit](#section-transcribe-and-submit)    | Dictate and send in one keystroke.                                     |
| [Jumper](#section-jumper)                                | Mark a window and deliver your words back into it. *{Windows only}*    |
| [Remote desktops](#section-remote-desktops)              | Making delivery land inside RDP and Citrix sessions.                   |
| [History and recovery](#section-history)                 | Finding, replaying and recovering a take.                              |
| [Providers and post-processing](#section-providers)      | Remote engines, LLM cleanup, cost and keys.                            |
| [Models and engines](#section-models)                    | Choosing what transcribes, and on which chip.                          |
| [Keyboard Typer](#section-keyboard-typer)                | Typing text into windows that refuse a paste.                          |
| [Model Testing](#section-model-testing)                  | Comparing models on your own prompt.                                   |
| [Token Count](#section-token-count)                      | What a prompt will cost before you send it.                            |
| [Translator](#section-translator)                        | Transcribing a folder of recordings in the background.                 |
| [MCP and CLI](#section-mcp-and-cli)                      | Driving the app from an agent, a script or a hotkey daemon.            |
| [Backup and portability](#section-backup)                | Moving, restoring and running without installing.                      |
| [Audio and feedback](#section-audio)                     | The microphone, the cues and the overlay.                              |
| [Platform and privacy](#section-platform)                | What ships, what leaves the machine, what is written down.             |

Elsewhere in the documentation: [the documentation hub](README.md) routes you to a task,
[the settings reference](reference/settings/index.md) lists every control with its shipped
default, [the shortcut reference](reference/shortcuts.md) lists every default binding, and
[the glossary](reference/glossary.md) defines the terms used here.

---

<a id="section-burned-before"></a>

## If you have been burned before

Short list of the entries that exist because something went wrong once, in a way that cost
somebody real work. If you have lived through one of these, start here.

- [Every engine keeps your last word](#every-engine-keeps-your-last-word) — the model was not worse, it was cut off
- [Your remote session pastes the right thing](#your-remote-session-pastes-the-right-thing) — Citrix pasted what you copied ten minutes ago
- [Dictate into the new Microsoft Teams message box](#dictate-into-the-new-microsoft-teams-message-box) — one app that always said "target field was replaced"
- [A crash mid-dictation costs you nothing](#a-crash-mid-dictation-costs-you-nothing) — half an hour of talking, gone
- [The paste is swallowed right after a jump](#the-paste-is-swallowed-right-after-a-jump) — the window was still waking up
- [Separate timing for remote desktops and local apps](#separate-timing-for-remote-desktops-and-local-apps) — you should not slow down every jump for one of them
- [Citrix and RDP deliveries land](#delivery-into-citrix-and-rdp-stops-failing-with-not-pasted) — the strict check fired a heartbeat too early
- [The Enter key lands in the remote window](#the-enter-key-lands-in-the-remote-window) — it submitted when you were already there, and not when it jumped
- [Escape stops the delivery, not your words](#escape-stops-the-delivery-not-your-words) — one stray keypress used to cost the whole take
- [Stop the recording with whichever key is under your finger](#stop-the-recording-with-whichever-key-is-under-your-finger) — you had to remember which key you started with
- [The re-paste happens the moment you let go](#the-re-paste-happens-the-moment-you-let-go) — a held macro key used to cost a second
- [Orphaned NPU servers can't block your next take](#orphaned-npu-servers-cant-block-your-next-take) — "failed to start" after a crash, forever
- [Windows blocked FLM — know which choices are real](#windows-blocked-flm-know-which-choices-are-real) — "can't verify", "unknown publisher", "blocked", or error 4551
- [A take that produced no text says so](#a-take-that-produced-no-text-says-so) — the worst failure is the silent one
- [Cancel a stuck download and it stops now](#cancel-a-stuck-download-and-it-stops-now) — the UI said canceled, the machine disagreed
- [Token counts you can trust from a local server](#token-counts-you-can-trust-from-a-local-server) — one word reported as 20 tokens
- [It never pastes blind](#it-never-pastes-blind) — text has to land where you meant it, or nowhere
- [Canceling can't freeze the app](#cancelling-cant-freeze-the-app) — one keypress at the wrong instant used to hang everything
- [No console window flashing on your desktop](#no-console-window-flashing-on-your-desktop) — a black box popping up every few minutes

---

## Defaults — what you get out of the box

<a id="defaults"></a>

Read this once and you can predict the app before you run it. Everything here is changeable;
this is what happens if you change nothing.

#### The keys

| You press                             | What happens                                                                                           |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `ctrl+space`                          | Start recording. Press again to stop; the text is pasted where your cursor is.                         |
| `ctrl+alt+space`                      | Hold to record, release to stop and paste.                                                             |
| `ctrl+alt+s`                          | Finish the recording, paste, then press **Enter** for you. Also starts a recording if none is running. |
| `ctrl+alt+p`                          | Paste the most recent transcription again, into whatever has focus now.                                |
| `ctrl+alt+t`                          | Type the Keyboard Typer text into the focused window, one keystroke at a time.                         |
| `Escape`                              | Stop the recording, transcribe it, save it to History — and deliver nothing.                           |
| `ctrl+alt+k` / `ctrl+alt+j`           | Mark the focused field as your hot destination / jump back to it. *{Windows only}*                     |
| `ctrl+alt+h` / `ctrl+alt+g`           | The same for a second hot destination. *{Windows only}*                                                |
| `ctrl+alt+shift+1…9` / `ctrl+alt+1…9` | Mark / jump to numbered destinations 1 to 9. *{Windows only}*                                          |

`ctrl+shift+space` runs a take through an LLM before delivering it, but the post-processing
page is hidden until you switch it on, so that key does nothing on a fresh install.

#### What happens to your clipboard

The default is `Advanced › Transcription › Transcribe › Clipboard Handling = Don't Modify Clipboard`,
which does not mean the clipboard is untouched — it means it is **put back**. Handy copies the
transcript to the clipboard, sends the paste keystroke, then restores your previous clipboard
text about 50 ms later. Two honest limits: only **text** is saved and restored, so an image or a
copied file on the clipboard is lost; and during those few milliseconds the transcript is
readable by anything watching the clipboard, including Windows clipboard history.

Choose `Copy to Clipboard` instead and Handy skips the restore entirely, leaving the
transcription on the clipboard on purpose.

#### Which paste keystroke fires, and why it matters

The default is `Clipboard (Ctrl+V)`. **Ctrl+V and Shift+Insert both mean "paste" but are not
interchangeable.** Console windows, terminal emulators, some Java applications and several
remote-desktop clients accept only one of them — a classic Windows console treats Ctrl+V as an
ordinary control character and ignores it, while Shift+Insert works. That is the whole reason
the paste method is a setting, and why each flow gets its own: your terminal shortcut can use
`Clipboard (Shift+Insert)` while everything else stays on Ctrl+V. `Direct` skips the clipboard
and types the characters; `None` puts the text on the clipboard and sends nothing.

#### Is a submit key sent?

Not for plain dictation: `Advanced › Transcription › Transcribe › Auto Submit = Off`. The
Transcribe & Submit shortcut is the one that sends a key, and it always sends one —
`Submit key = Enter` by default. **Enter and Ctrl+Enter are not interchangeable either**: chat
applications disagree about which one sends and which one inserts a newline. Slack and Teams
send on Enter; several LLM consoles and ticket systems want Ctrl+Enter. Bind Transcribe & Submit
only to places where you know which one means "send".

#### The delays

| Delay               | Default                                    | What it is for                                                                                                           |
| ------------------- | ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------ |
| Clipboard restore   | `Off (instant)` — the built-in ~50 ms only | Remote sessions fetch the clipboard *after* your paste keystroke arrives. Raise it if a remote app pastes stale content. |
| Paste after a jump  | 300 ms local, 600 ms remote                | A window activated a heartbeat ago is still settling and eats the keystroke.                                             |
| Submit after a jump | 300 ms local, 600 ms remote                | The target may still be committing the pasted text when Enter arrives.                                                   |

The jump delays fire **only on a real jump**. When you are already in the target window, the
paste is immediate.

Every delay picker uses the same uniform millisecond grid: `Off`, `100`, `200`, `300`, `400`,
`500`, `600`, `700`, `800`, `900`, `1000`, `1500`, and `2000` ms.

#### What Escape does

`General › Cancel behavior = Finish, save to history only`. Escape stops the recorder,
transcribes normally, writes the result to History — and delivers nothing: no paste, no
clipboard write, no submit key, no jump. It is the "do not type this into whatever is in front
of me, but do not lose it either" key. If you want the old throw-it-away behavior it is one
dropdown away as `Discard recording`.

#### Where recordings go, and for how long

Audio is written to `File › %APPDATA%\pr.handy\recordings` as 16 kHz mono Ogg/Opus while you
speak, transcripts to `File › %APPDATA%\pr.handy\history.db`, settings to
`File › %APPDATA%\pr.handy\settings_store.json`. History keeps the 5 most recent entries and
`Advanced › History › Auto-Delete Recordings` keeps audio for exactly those entries — an entry
you mark as saved is never cleaned up. Handy only ever deletes files it created; your own files
in that folder are not touched.

#### What never leaves your machine

With a downloaded local model selected, no microphone audio and no transcript is sent anywhere.
There is no telemetry, no analytics, no crash reporter and no account. Everything that can send
data off-machine — API transcription, OpenRouter, LLM post-processing, model testing, token
counting — is off until you configure it with your own endpoint and key.

One outbound request is on by default and is not the model download: Handy checks the public
GitHub releases page once a day at `General › Updates › Check for updates automatically = On`.
It sends nothing but the ordinary metadata of an HTTPS request and the version you are running,
and it installs nothing on its own — `General › Updates › Install updates silently` is **Off**.
Turn the check off and the model download you start yourself is the only outbound request on a
fresh install.

#### Everything else, on a fresh install

No model is selected: onboarding asks you to download one. Recording overlay at the bottom of
the screen, cue sounds off, tray icon on, autostart off, theme follows the system, the model
stays loaded until you quit, crash-safe recording on, post-processing off, MCP and CLI server
off, Translator off, push-to-talk transcribes live while the plain toggle transcribes in the
background and does one final accurate pass at the end.

---

<a id="section-transcription"></a>

## Transcription

The core loop: press a key, talk, press it again, the text lands where you were typing.

### Press one key, speak, and the text appears where you were typing

<a id="press-one-key-and-speak"></a>
**The situation.** Your hands are on the keyboard and your idea is three sentences long. Typing
it is slower than thinking it, and switching to a dictation window destroys the context you
were dictating about.
**What Handy does.** A global shortcut records from wherever you are. Press it again and the
transcription is delivered into the window that had focus — no window switch, no app to bring
up. Every shortcut is remappable, and a combination another app already owns is reported at
startup rather than silently failing.
**Where.** `General › Transcribe Shortcut` — `ctrl+space` by default.
**Since.** 0.1.0, inherited from upstream Handy.

### Hold a key for a one-line thought

<a id="hold-to-talk"></a>
**The situation.** For a short reply, pressing a key twice is one press too many, and you would
rather not think about whether the recorder is still running.
**What Handy does.** Push-to-talk records while the key is held and stops the instant you
release it. The microphone goes cold on release, before any transcription finishes, so
post-release chatter cannot leak into your text. It has its own transcription mode and its own
paste method, so it can behave differently from the toggle.
**Where.** `General › Push-to-Talk Shortcut` — `ctrl+alt+space` by default.
**Since.** 0.1.0; hardened in 0.30.0.

### Watch the text appear, or wait for the most accurate pass

<a id="live-or-post-recording"></a>
**The situation.** A long dictation used to be a black box: you talked for four minutes and only
found out at the end whether the microphone was even working.
**What Handy does.** Live mode transcribes progressively while you speak and shows the text
within seconds. Post-Recording records silently and transcribes in the background as you go.
Either way, the complete audio gets one final pass when you stop, so the delivered text is the
accurate one and the live text is only a preview. The two shortcuts are configured separately —
the toggle defaults to Post-Recording, push-to-talk to Live.
**Where.** `General › Transcription › Transcription Mode = Live` and
`General › Transcription › Transcription Mode (PTT) = Live`.
**Since.** 0.8.2.

### A thirty-minute dictation is already transcribed when you stop

<a id="a-thirty-minute-dictation-is-already-transcribed-when-you-stop"></a>
**The situation.** You finish a long recording and then wait, watching a spinner, while the
machine works through everything you had said.
**What Handy does.** Transcription is cut loose from file storage. Your speech is split at
natural silences every 20 to 45 seconds and each segment is transcribed in the background while
you keep talking, then joined in order. When you stop, only the last segment is left to do, so
a long take finishes almost immediately. Cuts are always at silence, so no word is ever split
across a boundary.
**Where.** No control — this is always active.
**Since.** 0.11.2.

### Live mode delivers the end of your sentence

<a id="live-mode-stopped-eating-the-end-of-your-sentence"></a>
**The situation.** In live mode the pasted text was missing the last few words — whatever you
said after the final on-screen update.
**What Handy does.** The delivered text no longer comes from the accumulated live preview. On
stop, the complete audio is transcribed once more — the cost of roughly one extra live update —
and that result is what you get. The live text is kept only as a fallback if the final pass
cannot run.
**Where.** No control — this is always active.
**Since.** 0.25.0.

### A long recording that came back empty

<a id="a-long-recording-that-came-back-empty"></a>
**The situation.** You dictate for several minutes, nothing is pasted, and History shows an
empty entry. The real text arrives seconds later with nowhere to go.
**What Handy does.** The wait for background transcription runs until it is actually finished
instead of giving up after a fixed 30 seconds. A very generous backstop still exists so a
genuine deadlock cannot hang the app forever, and it saves whatever did finish.
**Where.** No control — this is always active.
**Since.** 0.11.1.

### Escape stops the delivery, not your words

<a id="escape-stops-the-delivery-not-your-words"></a>
**The situation.** You do long recordings and you click around while you talk. One stray Escape
and the discussion you were having — or the prompt you were building — was gone.
**What Handy does.** The default cancel behavior stops the recorder, transcribes the take
normally, writes it to History, and delivers nothing at all: no paste, no clipboard write, no
submit key, no jump. Your words survive in History with their audio; the window in front of you
is left alone. The old behavior is still available as `Discard recording`.
**Where.** `General › Cancel behavior = Finish, save to history only`.
**Since.** 0.63.0.

### Cancel means the same thing however you trigger it

<a id="cancel-means-the-same-thing-however-you-trigger-it"></a>
**The situation.** A script cancels a take and gets different behavior from pressing Escape.
**What Handy does.** Escape, the tray item, the in-app command and the CLI flag all run the same
path and honor the same setting. Canceling *after* the recorder has already stopped — while
transcription is still running — suppresses only the delivery instead of tearing the pipeline
down mid-flight.
**Where.** `Tray › Cancel` and `CLI › handy --cancel`; the shortcut itself is
`Debug › Cancel Shortcut` *{requires: Debug mode}*.
**Since.** 0.63.0.

### What a cancel can and cannot take back

<a id="what-a-cancel-can-and-cannot-take-back"></a>
**The situation.** You hit Escape a fraction of a second too late and want to know exactly what
already happened.
**What Handy does.** The "delivers nothing" guarantee is absolute for the whole window in which
Escape is still active. Once a paste is actually under way — including inside a configured jump
delay of up to two seconds — the clipboard write and the submit key may still land. Three more
things are stated rather than papered over: a canceled take on a remote engine has already been
uploaded, a post-processing take has already cost tokens, and the audio is kept under your
retention setting.
**Where.** No control — this is always active.
**Since.** 0.63.0.

### Stop the recording with whichever key is under your finger

<a id="stop-the-recording-with-whichever-key-is-under-your-finger"></a>
**The situation.** You started a take with Transcribe & Submit, then pressed Transcribe to stop
it — and got a "busy" beep. You had to remember which key you started with.
**What Handy does.** The plain Transcribe toggle finishes a recording started by *any* binding,
delivering it as an ordinary paste with no submit key. It stops the recording's real owner, so
the microphone can never be left running by a mismatched pair of keypresses.
**Where.** No control — this is always active.
**Since.** 0.60.0.

### A key pressed too early tells you, instead of eating the words

<a id="a-key-pressed-too-early-tells-you"></a>
**The situation.** You start the next thought before the previous take has finished processing,
and the utterance disappears into nothing.
**What Handy does.** A recording key pressed while the pipeline is still busy plays an audible
cue and declines, rather than accepting the press and losing what you say next.
**Where.** Audible only if `General › Sound › Audio Feedback = On`.
**Since.** 0.30.0.

### Push-to-talk you can trust

<a id="push-to-talk-you-can-trust"></a>
**The situation.** Hold-to-talk is the flow where small bugs hurt most: a mic left hot, a
hotkey that silently failed to register, a take discarded because you changed a setting.
**What Handy does.** A full audit of the push-to-talk path produced a set of guarantees that
hold together: the recorder stops the instant you release, before any in-flight transcription
finishes; the live-versus-chunked decision is snapshotted when the recording starts and
consumed at stop, so changing a setting mid-sentence cannot discard the take; hotkey
registration failures are recorded and surfaced at startup instead of failing quietly; a failed
start rolls the recording UI back; and rebinding a key while it is held synthesizes the release
so nothing gets stranded.
**Where.** No control — this is always active.
**Since.** 0.30.0.

### A take that produced no text says so

<a id="a-take-that-produced-no-text-says-so"></a>
**The situation.** You dictate, everything looks normal, and the history entry is blank. Nothing
warned you — you find out later, when the words matter.
**What Handy does.** Any engine error that would otherwise leave a take textless raises a
visible "transcription failed" notice instead of writing a silent empty row. The specific case
that prompted this: an NPU model that failed to load while its server still answered "ready",
which is now detected at model-selection time and refused with a clear error.
**Where.** No control — this is always active.
**Since.** 0.46.0.

### Names and jargon stop coming back mangled

<a id="names-and-jargon-stop-coming-back-mangled"></a>
**The situation.** Every engine turns your product names, colleagues and internal jargon into
something phonetically close and completely wrong.
**What Handy does.** You keep a list of words that matter. After transcription, Handy matches
against that list using edit distance plus a phonetic comparison and substitutes the right
spelling — no retraining, no model surgery. How eager the matching is can be tuned if it starts
correcting things you did not mean.
**Where.** `General › Transcription › Custom Words`; aggressiveness at
`Debug › Word Correction Threshold` *{requires: Debug mode}*.
**Since.** Present since the fork's early releases.

### The next dictation doesn't run into the last one

<a id="the-next-dictation-doesnt-run-into-the-last-one"></a>
**The situation.** You dictate two sentences in a row and get `...end of one.Beginning of two`.
**What Handy does.** Optionally appends a single trailing space to every delivered
transcription, so consecutive takes and anything you type afterwards stay separated.
**Where.** `General › Transcription › Append Trailing Space = On`.
**Since.** Present since the fork's early releases.

### Ctrl+V doesn't work in that app

<a id="ctrl-v-doesnt-work-in-that-app"></a>
**The situation.** The text is transcribed, the clipboard is correct, and the target window
ignores the paste — because a console, a Java client or a remote viewer only accepts a different
paste keystroke.
**What Handy does.** The paste keystroke is a setting, and each flow has its own: ordinary
dictation, push-to-talk, Transcribe & Submit and the manual re-paste can each use a different
one. `Direct` bypasses the clipboard and types the characters instead; `None` puts the text on
the clipboard and sends nothing at all, which is the right answer for apps where you want to
paste by hand.
**Where.** `Advanced › Transcription › Transcribe › Paste Method = Clipboard (Shift+Insert)`
and `Advanced › Transcription › Transcribe › Paste Method (PTT)`.
**Since.** Present since the fork's early releases.

### Dictation doesn't steal your clipboard

<a id="dictation-doesnt-steal-your-clipboard"></a>
**The situation.** You had something you needed on the clipboard, dictated one sentence, and now
it is gone.
**What Handy does.** With the default clipboard handling, the transcript is placed on the
clipboard only long enough to paste, then your previous clipboard **text** is restored on a
background thread — guarded so a slow restore can never overwrite a newer take. Choose
`Copy to Clipboard` and it deliberately stays. The honest limits: only text is preserved, not
images or files; and if a delivery fails, Handy parks the transcript on the clipboard on
purpose rather than lose it.
**Where.** `Advanced › Transcription › Transcribe › Clipboard Handling = Don't Modify Clipboard`.
**Since.** Present since the fork's early releases; the delayed restore since 0.29.0.

### Send it without reaching for Enter

<a id="send-it-without-reaching-for-enter"></a>
**The situation.** You dictate into a chat box and then have to move your hand to press Enter,
every single time.
**What Handy does.** Optionally sends a submit key about 50 ms after every ordinary paste. Enter,
Ctrl+Enter or Super+Enter, because applications disagree about which one sends. It is ignored
when the paste method is `None`, and it is separate from the dedicated Transcribe & Submit
shortcut, which always submits.
**Where.** `Advanced › Transcription › Transcribe › Auto Submit = Enter`.
**Since.** Present since the fork's early releases.

### The paste didn't land — get the words back without re-dictating

<a id="the-paste-didnt-land-get-the-words-back"></a>
**The situation.** You told Handy not to touch your clipboard, because you had something
valuable in it. Then a delivery misfired — wrong window, target not ready, paste swallowed — and
the text is not on your clipboard either, because you asked for that. Under any other tool those
words are gone.
**What Handy does.** Keeps the most recent transcription in memory (and in History) regardless,
and gives it its own shortcut that pastes it into whatever has focus now. It has its own paste
method and its own clipboard policy, so you can tune it for the one stubborn target. It never
submits and never jumps, and it never reads the clipboard — after a restart it falls back to
History. This is why
[Dictation doesn't steal your clipboard](#dictation-doesnt-steal-your-clipboard) is safe to
switch on.
**Where.** `General › Paste last transcription › Paste Last Transcription` — `ctrl+alt+p` by
default; `General › Paste last transcription › Paste method`.
**Since.** 0.57.0.

### Your re-paste key fires while you are still holding it

<a id="your-re-paste-key-fires-while-you-are-still-holding-it"></a>
**The situation.** You press the re-paste shortcut and absolutely nothing appears — in any app,
with any paste method.
**What Handy does.** The shortcut used to fire while its own Ctrl and Alt were still physically
down, so the synthesized paste came out as Ctrl+Alt+V and every target ignored it. Handy now
polls real key state, bounded at about two seconds, and injects only once the modifiers are
genuinely released. The general rule this taught: a global shortcut that *synthesizes*
keystrokes must first prove its own trigger keys are up.
**Where.** No control — this is always active.
**Since.** 0.58.0.

### The re-paste happens the moment you let go

<a id="the-re-paste-happens-the-moment-you-let-go"></a>
**The situation.** The fix above worked but cost up to a second of dead time — exactly the case
where a macro pad holds the chord for you.
**What Handy does.** The action now runs on key *release* instead of key press: the trigger key
is already up, so nothing repeats into the target, still-held modifiers (including right
Alt / AltGr) are force-cleared, and the paste lands immediately. Honest caveat, because it is
inherent rather than fixable: if your macro physically holds the chord for a second, the paste
fires when it lets go. A tap is instant.
**Where.** No control — this is always active.
**Since.** 0.59.0.

### When delivery can't be verified, the text is parked, not lost

<a id="when-delivery-cant-be-verified-the-text-is-parked"></a>
**The situation.** The paste fails — the target rejected it, the window moved, focus went
somewhere unexpected — and you would like to know rather than discover it three sentences later.
**What Handy does.** A failed or unverifiable delivery raises a toast and parks the transcript
on the clipboard so you can place it yourself, instead of firing keystrokes into a surprise
window. The parked write deliberately supersedes any pending clipboard restore.
**Where.** No control — this is always active.
**Since.** 0.38.0.

### Watch the words arrive in a window you can park anywhere

<a id="watch-the-words-arrive-in-a-window-you-can-park-anywhere"></a>
**The situation.** You want to see the transcription as it forms without giving up the window
you are dictating into.
**What Handy does.** A small always-available floating window shows the current transcription,
with a copy button in the corner. It is created hidden at startup so its web view cannot block
the app's first keystroke.
**Where.** `Current Audio › Open floating window`.
**Since.** 0.8.2.

### The transcript panel doesn't go blank between takes

<a id="the-transcript-panel-doesnt-go-blank-between-takes"></a>
**The situation.** You glance at the transcript view to re-read what you had said, and it is
empty because a new take has started.
**What Handy does.** Current Audio keeps the previous transcript on screen until the new take
actually produces text — live and push-to-talk stream the in-progress result, post-recording
holds the last one until the final arrives. A copy button sits in the transcript box itself.
**Where.** `Current Audio › Copy`.
**Since.** 0.49.0 and 0.50.0.

### Speak any language, get English

<a id="speak-any-language-get-english"></a>
**The situation.** You think in one language and the document has to be in another.
**What Handy does.** Engines that support it can translate to English instead of transcribing.
The switch is greyed out — with a reason — for models that cannot translate, rather than
silently doing nothing. You can also pin the spoken language for engines that accept a hint,
which is usually more accurate than auto-detection.
**Where.** `Advanced › Transcription › Translate to English = On`; language at
`General › Language`.
**Since.** 0.3.0; the greyed-out honesty in 0.10.0 and 0.17.0.

### Chinese output in the script you expect

<a id="chinese-output-in-the-script-you-expect"></a>
**The situation.** The engine returns Simplified when your document is Traditional, or the other
way round.
**What Handy does.** Converts between Simplified and Traditional Chinese in the pipeline, using a
deliberately conservative mapping so it does not rewrite characters it should leave alone.
**Where.** Follows your language selection at `General › Language`.
**Since.** Conservative conversion in 0.38.0.

---

<a id="section-transcribe-and-submit"></a>

## Transcribe & Submit

The second intent: not "give me the text" but "I already trust what I said — send it".

### Dictate and send in one keystroke

<a id="dictate-and-send-in-one-keystroke"></a>
**The situation.** Sometimes you want the text so you can look at it first. And sometimes you
already trust what you said, it is good enough, and moving your hand to Enter is pure waste —
especially while your other hand is on the mouse reading something else.
**What Handy does.** One shortcut finishes any active recording, pastes with **its own** paste
method, presses **its own** submit key and applies **its own** clipboard policy. "Still
speaking" becomes "message sent" in a chat box, or a command executed in a terminal, without
touching the mouse. Bind it only where Enter means send.
**Where.** `Advanced › Transcription › Transcribe & Submit › Transcribe & Submit Shortcut` —
`ctrl+alt+s` by default.
**Since.** The 0.3x series.

### Enter, Ctrl+Enter, or Super+Enter

<a id="enter-ctrl-enter-or-super-enter"></a>
**The situation.** You bind the submit key, move to a different app, and it inserts a newline
instead of sending.
**What Handy does.** The submit key is a setting because applications genuinely disagree. Chat
clients usually send on Enter; several LLM consoles, comment boxes and ticket systems want
Ctrl+Enter. This key is always sent by this shortcut, independently of the global auto-submit
setting.
**Where.** `Advanced › Transcription › Transcribe & Submit › Submit key = Ctrl+Enter`.
**Since.** The 0.3x series.

### Its own paste method, for the one app that needs it

<a id="its-own-paste-method-for-the-one-app-that-needs-it"></a>
**The situation.** Your terminal needs Shift+Insert but everything else is happier with Ctrl+V,
and you do not want to change a global setting for one target.
**What Handy does.** Transcribe & Submit carries a complete delivery recipe of its own — paste
method, clipboard policy, clipboard restore delay — that shadows the global settings for this
one shortcut. Nothing you set here affects ordinary dictation.
**Where.** `Advanced › Transcription › Transcribe & Submit › Paste method`.
**Since.** The 0.3x series.

### Its own clipboard policy

<a id="its-own-clipboard-policy"></a>
**The situation.** You want your normal dictation to leave the clipboard alone, but the submit
flow to keep a copy of what it sent — or the reverse.
**What Handy does.** The submit flow has a separate clipboard handling choice and a separate
restore delay, so the two intents can differ without compromise.
**Where.** `Advanced › Transcription › Transcribe & Submit › Clipboard = Copy to Clipboard`.
**Since.** The 0.3x series.

### Pressing it when nothing is recording

<a id="pressing-it-when-nothing-is-recording"></a>
**The situation.** You hit the submit key out of habit before starting to talk, and you would
like to control what that means.
**What Handy does.** Three outcomes, your choice: start an ordinary recording, do nothing at all
(making the key a pure finisher you cannot misfire), or start a recording that will paste and
submit when it stops.
**Where.** `Advanced › Transcription › Transcribe & Submit › When no recording is active = Do nothing`.
**Since.** The 0.3x series.

### It finishes a take you started with another key

<a id="it-finishes-a-take-you-started-with-another-key"></a>
**The situation.** You started dictating with the plain toggle and only then decided the result
should be sent.
**What Handy does.** The submit shortcut finishes whatever recording is active, whichever key
started it, and applies the submit flow's delivery recipe to it.
**Where.** No control — this is always active.
**Since.** The 0.3x series.

### The Enter key lands in the remote window

<a id="the-enter-key-lands-in-the-remote-window"></a>
**The situation.** Transcribe & Submit worked perfectly when you were already in the chat box,
and did not submit when it had to jump to it — reliably so over RDP.
**What Handy does.** Enter used to fire a fixed 50 ms after the paste, and on a jump the focus
was handed straight back to your previous window, racing the Enter that had been injected a moment earlier.
Now, only on a real jump, there is a configurable settle before Enter plus a short grace that
keeps the target in the foreground until the key has been processed. The already-focused path is
byte-for-byte unchanged.
**Where.** `Advanced › Transcription › Transcribe & Submit › Submit delay after jump (Windows)`
*{Windows only}*.
**Since.** 0.53.0.

---

<a id="section-jumper"></a>

## Jumper (Windows only)

Send the text where you were, not where you drifted to. The whole Jumper family is built on
Win32 focus APIs; it exists only in the Windows build and will not arrive with the planned
macOS and Linux builds.

### Send it where you were

<a id="send-it-where-you-were"></a>
**The situation.** You start prompting about something, then go and look at other screens and
documentation while still talking about it. By the time you finish, the window you were
prompting into is buried under everything else, and you have to dig it out with the mouse.
**What Handy does.** Click into the target field once and press Set Anchor: the exact window
*and control* are remembered. Wander anywhere. When the transcription finishes, Handy activates
the anchored window, focuses the anchored field, verifies both actually happened, delivers the
text, and hands focus back to where you were.
**Where.** `Jumper › Hot slot (Anchor & Deliver) › Set Anchor` — `ctrl+alt+k` by default — then
`Advanced › Transcription › Transcribe › Jump slot action on finish = Jump / deliver to slot`
*{Windows only}*.
**Since.** 0.31.0.

### Jump back to your draft without pasting anything

<a id="jump-back-to-your-draft"></a>
**The situation.** You are four windows deep in documentation and want to be back in the field
you were writing in, cursor in place, without hunting for it.
**What Handy does.** Jump to Anchor is pure navigation: it brings the anchored window and field
into focus and delivers nothing. A "back to my draft" key.
**Where.** `Jumper › Hot slot (Anchor & Deliver) › Jump to Anchor` — `ctrl+alt+j` by default
*{Windows only}*.
**Since.** 0.31.0.

### It never pastes blind

<a id="it-never-pastes-blind"></a>
**The situation.** Anything that types into a window you are not looking at is one race
condition away from putting your words somewhere terrible.
**What Handy does.** Each recording's delivery target is captured per take and re-verified
immediately before **every** keystroke: the window really is in the foreground, the expected
control really has focus, the identity still matches. If any of that fails it aborts and parks
the text on the clipboard rather than typing. It fails closed, every time.
**Where.** No control — this is always active.
**Since.** 0.41.0.

### A recycled window handle can't hijack your anchor

<a id="a-recycled-window-handle-cant-hijack-your-anchor"></a>
**The situation.** You close the app you anchored, something else opens, and Windows hands out
the same numeric handle.
**What Handy does.** The anchor stores the target's identity — process, thread, window class,
control class and the control's own process — not only a handle, and re-resolves it before
delivery. A handle genuinely recycled into a different process is refused. Saved slots also
survive a crash during a save: an interrupted write leaves the previous slots intact rather than
a corrupt file.
**Where.** No control — this is always active.
**Since.** 0.42.0.

### Dictate into the new Microsoft Teams message box

<a id="dictate-into-the-new-microsoft-teams-message-box"></a>
**The situation.** Anchoring works in every application you own except one: Teams anchors fine
and then always fails to deliver with "target field was replaced by another".
**What Handy does.** Anchors work in applications that host their text box in a separate process
— new Teams, Slack, Discord, VS Code and other web-tech applications. Handy identifies the field
itself and not only the window around it, so those targets are accepted while a window handle
Windows has recycled into a different application is still refused.
**Where.** No control — this is always active.
**Since.** 0.61.0.

### It refuses to dictate into a password box

<a id="it-refuses-to-dictate-into-a-password-box"></a>
**The situation.** The one field you never want an automated paste to reach.
**What Handy does.** Anchoring refuses a classic Win32 password control, refuses a browser,
Electron or WinUI field that UI Automation reports as a password, and refuses Handy's own
windows. Delivery re-checks before it types. Honest limits, because this is defense in depth and
not a guarantee: **ordinary dictation has no password check at all** — a normal paste goes
wherever focus is; the Keyboard Typer deliberately supports password prompts; UI Automation
failures are treated as "could not determine" rather than "is a password"; and inside a remote
desktop session Handy can only see the remote window, not the field within it.
**Where.** No control — this is always active.
**Since.** 0.31.0; extended to browser and Electron password inputs in 0.61.0.

### Two live destinations at once

<a id="two-live-destinations-at-once"></a>
**The situation.** Notes belong in your document and chat replies belong in Teams, and you are
alternating between them all afternoon.
**What Handy does.** A second hot anchor with its own set and jump keys. Ordinary dictation and
Transcribe & Submit can each own a different hot anchor, so the two intents land in two places
without touching the numbered slots.
**Where.** `Jumper › Second hot slot (Anchor & Deliver) › Set Anchor 2` — `ctrl+alt+h` by
default *{Windows only}*.
**Since.** 0.50.0.

### Nine memorised destinations

<a id="nine-memorised-destinations"></a>
**The situation.** In one shift you drive a ticket system, a chat client, a terminal and a CRM,
and two hot slots are not enough.
**What Handy does.** Nine numbered static slots, each with its own set and jump shortcuts, its
own cursor behavior and its own status row showing what it currently points at, with Test and
Clear buttons.
**Where.** `Jumper › Static slot 1 › Set Jump Slot 1` — `ctrl+alt+shift+1` to set, `ctrl+alt+1`
to jump *{Windows only}*.
**Since.** 0.52.0.

### Decide what a jump does at the start and at the end of a take

<a id="what-a-jump-does-at-the-start-and-end-of-a-take"></a>
**The situation.** Sometimes you want to be taken to the target *before* you start talking;
sometimes you want to stay where you are and have only the text travel.
**What Handy does.** Each flow has an on-start action and an on-finish action, and each picks
its own slot. The action can be: do nothing, jump and deliver to the slot, set the slot to
whatever has focus right now, or clear the slot. Ordinary dictation and Transcribe & Submit
configure these independently.
**Where.** `Advanced › Transcription › Transcribe › Jump slot action on start` and
`Advanced › Transcription › Transcribe › Jump slot action on finish` *{Windows only}*.
**Since.** 0.36.0.

### You can see which slot an action targets

<a id="you-can-see-which-slot-an-action-targets"></a>
**The situation.** You set an action to jump and cannot find anywhere to say *which* slot it
should jump to.
**What Handy does.** The slot picker sits to the left of the action dropdown and is always
rendered, greyed out until you choose an action. It used to be hidden entirely while the action
was "Do nothing", which made the slot look unselectable.
**Where.** `Advanced › Transcription › Transcribe & Submit › Jump slot action on finish`
*{Windows only}*.
**Since.** 0.62.0.

### Remember where the text actually landed

<a id="remember-where-the-text-actually-landed"></a>
**The situation.** You dictate into a field, wander off, and want the *next* jump to come back
to that same field without setting an anchor by hand.
**What Handy does.** With tracking on, the chosen slot auto-captures where the text landed after
every paste of that flow, before any focus return. Ordinary dictation and Transcribe & Submit
track independently, into slots of your choosing.
**Where.** `Advanced › Transcription › Transcribe › Track last output location = On`
*{Windows only}*.
**Since.** 0.46.0.

### Focus comes back to you

<a id="focus-comes-back-to-you"></a>
**The situation.** You were reading a solution on the internet while feeding context to an agent
in another window. Losing focus for a second is fine; being dumped in the other window is not.
**What Handy does.** After an anchored delivery, focus returns to wherever you started — a
location captured automatically at the moment delivery begins. It is conditional: if you
switched windows yourself while it was working, it leaves you where you chose to be. Each flow
decides for itself whether to return focus.
**Where.** `Advanced › Transcription › Transcribe › Return focus after delivery = On`
*{Windows only}*.
**Since.** 0.40.0.

### Anchors stay put, and can survive a restart

<a id="anchors-stay-put-and-can-survive-a-restart"></a>
**The situation.** You dictate three paragraphs into the same document, one take at a time, and
do not want to re-anchor between them.
**What Handy does.** An anchor is never consumed by a delivery — it is kept until the window is
destroyed or you clear it. Optionally, slot targets are remembered across restarts: window
handles cannot survive a reboot, so the saved identity is re-resolved against live windows and
an unresolved slot shows red until its application comes back.
**Where.** `Jumper › Persistence › Remember slots across restarts = On` *{Windows only}*.
**Since.** 0.40.0.

### The mouse goes back too

<a id="the-mouse-goes-back-too"></a>
**The situation.** Jumping puts the keyboard where you need it and leaves the pointer on the
other monitor, so you still reach for the mouse.
**What Handy does.** Optionally saves the mouse position with the anchor and restores it on a
jump — after the paste, so the pointer cannot interfere with delivery. Multi-monitor aware, and
disabled outright on machines that are not per-monitor-DPI aware rather than producing wrong
coordinates.
**Where.** `Jumper › Hot slot (Anchor & Deliver) › Save mouse position = On` *{Windows only}*.
**Since.** 0.48.0.

### Each destination remembers the cursor the way that app needs

<a id="each-destination-remembers-the-cursor-the-way-that-app-needs"></a>
**The situation.** A window you move and resize needs a different notion of "where the mouse
was" than a fixed dashboard that never moves.
**What Handy does.** Every slot has its own cursor mode. App-relative restores the same spot
inside the window — surviving moves, resizes and monitors of different DPI. Screen-absolute
restores a fixed monitor pixel. The dropdown is always visible, greyed when that slot is not
saving the cursor at all.
**Where.** `Jumper › Hot slot (Anchor & Deliver) › Cursor position mode = App-relative (follows the window)`
*{Windows only}*.
**Since.** 0.51.0.

### No surprise teleports when you mix shortcuts

<a id="no-surprise-teleports-when-you-mix-shortcuts"></a>
**The situation.** You start a take with plain Transcribe, finish it with Transcribe & Submit,
and get thrown to the submit flow's target — which is not where you were working.
**What Handy does.** With this on, a flow's on-finish jump fires only when the take was both
started and finished by that flow. The submit itself still happens; only the jump is gated.
**Where.** `Jumper › On-finish behavior › Only jump on finish if started the same way = On`
*{Windows only}*.
**Since.** 0.49.0.

### Check a destination before you trust it

<a id="check-a-destination-before-you-trust-it"></a>
**The situation.** You set a slot an hour ago and want to know whether it still points at
anything real before you dictate two paragraphs at it.
**What Handy does.** Each slot's status row names the application and control it currently holds
— or says there is no anchor — and offers Test, which performs the jump without delivering
anything, and Clear. Targets recognized as remote sessions carry a badge, so you can see why a
slot is being treated differently.
**Where.** `Jumper › Hot slot (Anchor & Deliver) › Current anchor` and
`Jumper › Static slot 1 › Slot 1 target` *{Windows only}*.
**Since.** 0.36.0; the remote badge in 0.56.0.

---

<a id="section-remote-desktops"></a>

## Remote desktops

RDP, Citrix and VM consoles break the assumptions every desktop paste is built on. These delays
are not trivia — they are the whole reason delivery into a remote session works.

### Your remote session pastes the right thing

<a id="your-remote-session-pastes-the-right-thing"></a>
**The situation.** You dictate into a Citrix ticket field and what lands is the text you copied
ten minutes ago, not what you had said.
**What Handy does.** Remote sessions fetch clipboard data on demand *after* the paste keystroke
arrives, over a separate and slower virtual channel — so restoring your previous clipboard 50 ms
after pasting hands the remote application the pre-recording content, a race Citrix lost
reliably. The restore now runs on a background thread after 50 ms plus your configured delay,
guarded by a counter so a pending restore can never clobber a newer paste, and is skipped
entirely when you have asked Handy to keep the transcription on the clipboard.
**Where.** `Advanced › Transcription › Transcribe › Clipboard restore delay = 1 s`, with a
separate value at
`Advanced › Transcription › Transcribe & Submit › Clipboard restore delay`.
**Since.** 0.29.0.

### The paste is swallowed right after a jump

<a id="the-paste-is-swallowed-right-after-a-jump"></a>
**The situation.** The jump works — the remote window comes to the front — and then nothing is
pasted into it.
**What Handy does.** A window that was activated a heartbeat ago is still completing activation
and eats the keystroke. A settle is inserted after the foreground changes and before the paste.
It applies only on a real jump; when you are already in the target, pasting stays instant.
**Where.** `Advanced › Transcription › Transcribe › Paste delay after jump (Windows)`
*{Windows only}*.
**Since.** 0.55.0.

### Separate timing for remote desktops and local apps

<a id="separate-timing-for-remote-desktops-and-local-apps"></a>
**The situation.** Your remote window needs a full second to settle. Your local editor needs
nothing. Before this you had to slow down every jump to make the remote one work.
**What Handy does.** The post-jump paste delay and the submit delay are each split into a Local
and a Remote value, shown side by side. Handy picks the column based on what the target is.
**Where.** `Advanced › Transcription › Transcribe & Submit › Paste delay after jump (Windows)`
and `Advanced › Transcription › Transcribe & Submit › Submit delay after jump (Windows)`
*{Windows only}*.
**Since.** 0.56.0.

### Handy knows which of your windows is a remote session

<a id="handy-knows-which-of-your-windows-is-a-remote-session"></a>
**The situation.** You have three remote clients and a dozen local apps, and the app has to know
which is which to apply the right timing.
**What Handy does.** A target counts as remote when its application name, window class or control
class contains one of your match strings — case-insensitive, seeded with `msrdc`, `mstsc` and
`Citrix`, and fully editable. Matching anchors show a badge so you can see the classification
rather than guess at it.
**Where.** `Jumper › Remote desktop detection › Remote match strings` *{Windows only}*.
**Since.** 0.56.0.

### Citrix and RDP deliveries land

<a id="delivery-into-citrix-and-rdp-stops-failing-with-not-pasted"></a>
**The situation.** The jump lands, and instead of your text you get an error toast and the
transcript parked on the clipboard.
**What Handy does.** The post-jump wait used to run before the strict focus re-check, and a
freshly activated remote window is usually still moving focus between its inner controls at that
exact moment — so the check aborted. On a remote jump the re-check now tolerates a target that is
still settling, polling every 25 ms for up to 250 ms while the expected window stays in the
foreground. It still aborts instantly if a *different* window takes focus, and it still never
pastes into a password field or a foreign window. Local jumps are unchanged.
**Where.** No control — this is always active. *{Windows only}*
**Since.** 0.56.0.

### The paste delay applies to plain dictation too

<a id="the-paste-delay-applies-to-plain-dictation-too"></a>
**The situation.** You tuned the delay on the submit flow and your ordinary dictation still lost
its paste after a jump.
**What Handy does.** Ordinary dictation jumps and pastes as well, so the paste-delay control is
offered for that flow too, using the same setting.
**Where.** `Advanced › Transcription › Transcribe › Paste delay after jump (Windows)`
*{Windows only}*.
**Since.** 0.56.0.

### The app stays responsive during a two-second remote delivery

<a id="the-app-stays-responsive-during-a-two-second-remote-delivery"></a>
**The situation.** With a one-second paste delay and a half-second submit delay, the app itself
went unresponsive while it waited.
**What Handy does.** Delivery on Windows runs off the interface thread, with only the small
amount of work that genuinely must touch the window system marshaled back — which also avoids a
known deadlock between the tray, the overlay and the theme reader.
**Where.** No control — this is always active. *{Windows only}*
**Since.** 0.56.0.

### Type it instead, when the console refuses a paste

<a id="type-it-instead-when-the-console-refuses-a-paste"></a>
**The situation.** Clipboard redirection is disabled on the remote session, or the console is a
raw canvas that has no clipboard at all.
**What Handy does.** The Keyboard Typer sends the text as individual keystrokes with a
configurable gap, which is what remote consoles and VM viewers actually accept. See
[When paste is blocked, type it instead](#when-paste-is-blocked-type-it-instead).
**Where.** `Keyboard Typer › Type Text Shortcut` — `ctrl+alt+t` by default.
**Since.** 0.12.0.

---

<a id="section-history"></a>

## History and recovery

Nothing you said is ever gone.

### A crash mid-dictation costs you nothing

<a id="a-crash-mid-dictation-costs-you-nothing"></a>
**The situation.** "Often I was talking, talking, something went wrong and I lost my recording."
A crash, a dead battery or a forced update used to take the whole take with it.
**What Handy does.** Audio is encoded to disk continuously while you speak, in chunks cut at
silence, with the in-progress chunk under a temporary name. On the next launch any leftover
temporary chunk is repaired — the torn trailing page is dropped — glued to its siblings and
added to History marked as recovered. Opus is page-based, so a half-written file is readable
without any repair tool.
**Where.** `Advanced › History › Crash-Safe Recording = On`.
**Since.** 0.10.0 as WAV; the chunked Opus design in 0.11.0.

### Recordings that don't eat your disk

<a id="recordings-that-dont-eat-your-disk"></a>
**The situation.** Keeping every recording sounds good until the folder is 40 GB.
**What Handy does.** Recordings are 16 kHz mono Opus at roughly 24 kbps — about eleven to sixteen
times smaller than the WAV files the app used to write — encoded in pure Rust with no FFmpeg
dependency. Long recordings are split into roughly ten-minute files plus one glued full file; a
short recording is a single file with no redundant chunk copy.
**Where.** No control — this is always active.
**Since.** 0.11.0.

### Your own files in the recordings folder are never touched

<a id="your-own-files-in-the-recordings-folder-are-never-touched"></a>
**The situation.** You keep notes, exports or reference audio in the same folder, and an
automatic cleanup is about to run.
**What Handy does.** Every delete, rename and retention sweep is restricted to files Handy
created and named itself. Anything else in that folder is invisible to cleanup. Deleting one
recording also removes its chunk siblings, and nothing else.
**Where.** `Advanced › History › Recordings Folder`.
**Since.** 0.10.0, extended to Opus in 0.11.0.

### What did I dictate last Tuesday?

<a id="what-did-i-dictate-last-tuesday"></a>
**The situation.** You know you said it. You do not know when, and scrolling is not working.
**What Handy does.** A search bar filters History as you type, case-insensitively across the
transcript, the post-processed text and the title. A query containing regular-expression
characters becomes a live regular expression, marked with a badge, falling back to a literal
search if the pattern is invalid. Matches are highlighted, long transcripts show a snippet
centered on the first hit, and a counter tells you where you are.
**Where.** `History › Search history (text or regex)...`.
**Since.** 0.12.0.

### How long it was, which engine ran it, what it cost

<a id="how-long-which-engine-what-it-cost"></a>
**The situation.** You want to know whether the expensive engine is actually earning its place.
**What Handy does.** Every entry carries its duration, the model that produced it and — for
metered engines — the real cost of that recording, shown next to the date. Costs are captured per
request and summed across a recording's segments, so chunked and live takes tally correctly.
Local engines show none, because they cost nothing.
**Where.** `History` — the entry list.
**Since.** 0.21.0 and 0.22.0.

### "188 recordings, 32 seconds" — fixed retroactively

<a id="188-recordings-32-seconds"></a>
**The situation.** Your totals are nonsense because most of your history predates duration
tracking.
**What Handy does.** At startup, older rows are backfilled by reading each audio file's own
header, so historical totals become real rather than being written off. A button re-runs the
pass on demand.
**Where.** `Advanced › Transcription › Transcription cost report`.
**Since.** 0.22.0.

### Hear what it heard

<a id="hear-what-it-heard"></a>
**The situation.** The transcript says something odd and you cannot tell whether the engine
misheard you or you actually said that.
**What Handy does.** Every history entry with audio has a player. Only one plays at a time, so
clicking through entries does not build a chorus.
**Where.** `History` — the entry list.
**Since.** Present since the fork's early releases; single-player behavior in 0.38.0.

### Don't keep audio forever

<a id="dont-keep-audio-forever"></a>
**The situation.** You are happy to keep transcripts but do not want months of voice recordings
sitting on a work laptop.
**What Handy does.** Retention is a choice: keep nothing beyond the newest few entries, keep for
three days, two weeks or three months, or keep everything. Cleanup runs after a new entry is
saved and when you change the setting; it never touches entries you have marked as saved.
**Where.** `Advanced › History › Auto-Delete Recordings = After 3 days` and
`Advanced › History › History Limit`.
**Since.** Present since the fork's early releases.

### Keep the ones that matter

<a id="keep-the-ones-that-matter"></a>
**The situation.** One transcript out of a hundred is worth keeping and your retention setting
is about to delete it.
**What Handy does.** Marking an entry as saved excludes it from every automatic cleanup — the
row and its audio stay until you delete them yourself.
**Where.** `History › Save transcription`.
**Since.** Present since the fork's early releases.

### Copy or delete a single entry

<a id="copy-or-delete-a-single-entry"></a>
**The situation.** You want yesterday's transcript on the clipboard, or you want one entry gone
right now.
**What Handy does.** Per-row buttons copy the transcription to the clipboard or delete the entry
together with its audio and any chunk siblings.
**Where.** `History › Copy transcription to clipboard` and `History › Delete entry`.
**Since.** Present since the fork's early releases.

### Open the folder the audio actually lives in

<a id="open-the-folder-the-audio-lives-in"></a>
**The situation.** You want to hand one recording to someone, or point another tool at the whole
folder.
**What Handy does.** Opens the recordings directory in the file manager. The files are ordinary
`.opus` files with predictable names; nothing stops you using them elsewhere.
**Where.** `History › Open Recordings Folder`.
**Since.** 0.10.0.

---

<a id="section-providers"></a>

## Providers and post-processing

Bring a bigger brain when you want one — for cleaning up a transcript, or for transcribing on a
laptop that cannot do it locally.

### A second key for "clean this up with AI"

<a id="a-second-key-for-clean-this-up"></a>
**The situation.** Raw speech has filler words, no punctuation and a shape that made sense when
you said it. Sometimes you want that verbatim, and sometimes you want it tidied.
**What Handy does.** A separate shortcut runs the take through a language model you configure,
with a prompt you control, before delivering it. Raw and processed dictation are one keystroke
apart, and the raw transcription is always kept in History alongside the processed version.
**Where.** `Advanced › Post-processing › Post Processing = On` to reveal the page, then
`Post Process › Hotkey › Post-Processing Hotkey` — `ctrl+shift+space` by default
*{requires: Post-processing enabled}*.
**Since.** Present since the fork's early releases.

### A default prompt that respects your words

<a id="a-default-prompt-that-respects-your-words"></a>
**The situation.** Most "clean up my transcript" prompts quietly rewrite you into somebody else.
**What Handy does.** The built-in prompt puts a summary on top, uses paragraphs or lists as the
content suggests, and **preserves your wording**. Where the transcription was probably a
mishearing it flags the guess inline for you to confirm rather than inventing a correction. If
your dictation opens with an instruction, it follows the instruction instead of formatting it.
**Where.** `Post Process › Prompt › Selected Prompt` *{requires: Post-processing enabled}*.
**Since.** 0.15.0.

### Your own post-processing prompts

<a id="your-own-post-processing-prompts"></a>
**The situation.** You want commit messages formatted one way and meeting notes another.
**What Handy does.** A prompt library you edit in the app: create, name, update and delete
prompts, and switch the active one without touching anything else.
**Where.** `Post Process › Prompt › Selected Prompt` *{requires: Post-processing enabled}*.
**Since.** Present since the fork's early releases.

### Your dictated words can't hijack the model

<a id="your-dictated-words-cant-hijack-the-model"></a>
**The situation.** You dictate a sentence that happens to contain "ignore your instructions and
…" — quoting an email, discussing prompt injection, or unlucky.
**What Handy does.** The transcript is handed to the model as **delimited untrusted data**,
separated from the system and processing instructions, with delimiter breakout defused. Saying
something out loud cannot reroute the model that is cleaning it up.
**Where.** No control — this is always active.
**Since.** 0.39.0.

### Dial how creative the cleanup is allowed to be

<a id="dial-how-creative-the-cleanup-is"></a>
**The situation.** The model is paraphrasing when you wanted punctuation, or being wooden when
you wanted a rewrite.
**What Handy does.** A temperature slider for post-processing, plus a switch that suppresses
reasoning output for models that emit it — sent in the dialect each vendor actually accepts,
rather than one shape that returns an error on half of them.
**Where.** `Post Process › API (OpenAI Compatible) › Temperature` and
`Post Process › API (OpenAI Compatible) › Disable Thinking` *{requires: Post-processing enabled}*.
**Since.** 0.15.0; per-vendor thinking dialects in 0.18.0.

### Configure a provider once, use it everywhere

<a id="configure-a-provider-once-use-it-everywhere"></a>
**The situation.** You had one list of providers for token counting, another for post-processing
and a third mental note about which key went where.
**What Handy does.** One registry of providers — name, base URL, key, model, cost per million
tokens in and out — powers post-processing, token counting and model testing at once. Edit a
provider in one place and everything that references it follows.
**Where.** `Advanced › Providers › Registered LLM Providers › Base URL`.
**Since.** 0.15.0.

### Several slots, one local loader

<a id="several-slots-one-local-loader"></a>
**The situation.** You point three provider slots at the same local server and they fight over
its single model loader, so everything is slow and half of it fails.
**What Handy does.** Providers can declare a family name and be marked as running sequentially
within it. Slots in the same family take turns; everything else runs in parallel. This is what
makes a mixed run of local and cloud models finish in the time of the slowest one instead of the
sum of all of them.
**Where.** `Advanced › Providers › Registered LLM Providers › Concurrency`.
**Since.** 0.15.0.

### Point it at any OpenAI-compatible speech endpoint

<a id="point-it-at-any-openai-compatible-speech-endpoint"></a>
**The situation.** Your laptop cannot run a good local model, or your team already runs a
transcription server.
**What Handy does.** An engine that POSTs your recording to any `/v1/audio/transcriptions`
server — Groq, OpenAI, faster-whisper-server, your own — configured with a URL, a key and a
model name. It always sends one request at the end of the recording rather than uploading
repeatedly during it. When the language is set to automatic it sends English explicitly, because
these endpoints need to be told.
**Where.** `Advanced › Providers › API Transcription (OpenAI-compatible) › API URL`.
**Since.** 0.8.2.

### One OpenRouter key, many speech models

<a id="one-openrouter-key-many-speech-models"></a>
**The situation.** You want to try several hosted speech models without an account and a billing
relationship for each.
**What Handy does.** A dedicated OpenRouter engine that sends audio in the shape OpenRouter
actually accepts — JSON with base64 audio, not the usual multipart upload — with its own URL,
key and model fields separate from the LLM provider registry.
**Where.** `Advanced › Providers › OpenRouter Transcription › API Key`.
**Since.** 0.16.0; dedicated fields in 0.54.0.

### Whisper-style, or an audio-capable chat model

<a id="whisper-style-or-an-audio-capable-chat-model"></a>
**The situation.** The model you want to use is not a speech model at all — it is a chat model
that happens to accept audio.
**What Handy does.** Two routes. The transcription route talks to the dedicated speech endpoint
for Whisper-style models. The chat route sends the audio as part of a chat message, which is how
audio-capable models such as Gemini and GPT-4o-audio expect to receive it.
**Where.** `Advanced › Providers › OpenRouter Transcription › Endpoint = Chat model (Gemini / GPT-4o-audio)`.
**Since.** 0.16.0.

### Ten times less audio over the wire

<a id="ten-times-less-audio-over-the-wire"></a>
**The situation.** You are uploading recordings on a hotel connection or a metered tether.
**What Handy does.** Sends Opus by default, roughly ten times smaller than WAV, reusing the
encoder Handy already uses for its own recordings. WAV stays available for models that will not
take anything else.
**Where.** `Advanced › Providers › OpenRouter Transcription › Audio format = Opus — smaller (recommended)`.
**Since.** 0.16.0.

### A network blip can't shred your take

<a id="a-network-blip-cant-shred-your-take"></a>
**The situation.** Transcription was being streamed out in pieces while you spoke, so a dropped
connection in the middle left you with a transcript full of holes.
**What Handy does.** The OpenRouter engine buffers and sends one request when you stop. Slightly
slower to finish, far less exposed to a mid-recording disruption. Crash-safe local recording is
unchanged: your audio is on disk regardless of what the network does.
**Where.** No control — this is always active.
**Since.** 0.23.0.

### Where do I put the URL and the key?

<a id="where-do-i-put-the-url-and-the-key"></a>
**The situation.** You select the API engine and there is nowhere to configure it — because the
fields only appeared once the engine was already working.
**What Handy does.** Both custom transcription engines have permanent, named cards on the
Providers tab, visible whether or not you are using them. Existing setups migrate automatically
on first launch, and the old provider entry is left intact for post-processing and model testing.
**Where.** `Advanced › Providers › API Transcription (OpenAI-compatible) › Model`.
**Since.** 0.54.0.

### Selecting a remote engine no longer reverts

<a id="selecting-a-remote-engine-no-longer-reverts"></a>
**The situation.** You pick the API engine, the selection silently snaps back to the previous
model, and the error says "not configured" — which you cannot fix without selecting it first.
**What Handy does.** The choice is saved immediately and validated lazily, with a status hint,
instead of demanding a working configuration before it will accept the selection.
**Where.** `Models › Downloaded Models`.
**Since.** 0.20.1.

### The model list actually contains speech models

<a id="the-model-list-actually-contains-speech-models"></a>
**The situation.** You open the OpenRouter model picker to choose a Whisper model and there are
none.
**What Handy does.** OpenRouter leaves speech models out of its normal catalogue, so Handy asks
for transcription-capable models specifically, with a built-in fallback list if the query fails.
A blank model defaults to a sensible Whisper model, and no request is ever sent without a key.
**Where.** `Advanced › Providers › OpenRouter Transcription › Transcription model`.
**Since.** 0.23.0.

### Find a model among hundreds

<a id="find-a-model-among-hundreds"></a>
**The situation.** The provider offers four hundred models and the field is a plain text box.
**What Handy does.** The model field fetches the live list, filters as you type, and still
accepts any identifier you type by hand for models the endpoint does not advertise.
**Where.** `Advanced › Providers › Registered LLM Providers › Model`.
**Since.** 0.17.0.

### Know what your dictation costs

<a id="know-what-your-dictation-costs"></a>
**The situation.** You are transcribing through a paid API and have no idea whether that is
cents or tens of euros a month.
**What Handy does.** Each metered transcription records its real cost, reported by the provider,
alongside the recording length. A report breaks it down by the last 7 days, the last 4 weeks, the
last 12 months and by year, with an all-time total, and exports every recording plus the
summaries as CSV.
**Where.** `Advanced › Transcription › Transcription cost report`.
**Since.** 0.21.0 and 0.22.0.

### Prices filled in for providers that don't publish them

<a id="prices-filled-in-for-providers-that-dont-publish-them"></a>
**The situation.** You want per-run costs, but two of the three vendors do not expose prices
through their API, so you would be typing numbers from a web page.
**What Handy does.** Maps the model to its public catalogue entry — exactly where possible, and
tolerantly across dash, dot and date-suffix differences where not — and fills in the cost fields.
A per-provider lock freezes a price you entered yourself so an automatic lookup cannot overwrite
it, and the catalogue is cached for a day so lookups work offline.
**Where.** `Advanced › Providers › Registered LLM Providers › Cost / 1M`.
**Since.** 0.18.0.

### A hung provider can't stall the app

<a id="a-hung-provider-cant-stall-the-app"></a>
**The situation.** An endpoint accepts your connection and then never answers, and the app waits
forever.
**What Handy does.** Post-processing, token counting and model testing all have connect and total
timeouts and a bounded response reader, and report which phase timed out rather than a generic
failure.
**Where.** No control — this is always active.
**Since.** 0.43.0.

---

<a id="section-models"></a>

## Models and engines

Eight engines behind one dropdown, because no single one wins on privacy, speed, quality and
hardware at the same time.

### Pick the engine that fits the machine

<a id="pick-the-engine-that-fits-the-machine"></a>
**The situation.** A desktop with a GPU, a thin laptop with an NPU and a travel machine with
neither should not all run the same model.
**What Handy does.** Whisper (small through large-v3) runs locally on the GPU and is the best
all-rounder. Parakeet runs on CPU only at roughly five times realtime with automatic language
detection. Moonshine is small and fast for short clips, with a streaming variant. SenseVoice
covers a wide multilingual range. FastFlowLM (FLM) runs Whisper on the NPU. API Transcription
and OpenRouter are the two remote engines. Each model card states its size and what it is good
at, and models are labeled by where they run.
**Where.** `Models › Downloaded Models` and `Models › Available to Download`.
**Since.** 0.1.6; the current registry grew through 0.36.0.

### Every engine keeps your last word

<a id="every-engine-keeps-your-last-word"></a>
**The situation.** You stop the recording the instant you finish speaking and the last word or
two are missing. It reads as "this model is worse than Whisper".
**What Handy does.** Transducer-style models — Parakeet, Moonshine, SenseVoice — need trailing
acoustic context to emit their final tokens, and the audio ends mid-word when you release the
key. Handy pads one second of silence onto the audio for exactly those engines before decoding.
Whisper, NPU and remote engines are byte-identical to before, because they do not have the
problem. The result: every engine keeps the end of your sentence, and the model comparison you
make is about quality rather than about who got cut off.
**Where.** No control — this is always active.
**Since.** 0.24.0.

### GPU acceleration, including integrated graphics

<a id="gpu-acceleration-including-integrated-graphics"></a>
**The situation.** "Local Whisper" is only useful if it is fast, and most laptops do not have a
discrete GPU.
**What Handy does.** Ships a custom-built whisper.cpp with Vulkan acceleration, which covers AMD
and Intel integrated graphics as well as discrete cards. This is what makes live transcription
feel immediate on an ordinary laptop.
**Where.** No control — this is always active.
**Since.** 0.8.2.

### Pick which GPU transcribes

<a id="pick-which-gpu-transcribes"></a>
**The situation.** Your discrete card is busy with something you care about more, or the
integrated one is actually the better choice for a small model.
**What Handy does.** A device picker for local Whisper: automatic, CPU only, or a specific
adapter by name. An unavailable or invalid choice falls back to automatic instead of failing the
model load.
**Where.** `General › Transcription › GPU Device = CPU Only` *{Windows only}*.
**Since.** 0.42.0.

### Transcribe without tying up the CPU or GPU

<a id="use-the-npu-in-your-laptop"></a>
**The situation.** A recent AMD Ryzen AI processor has an NPU — a low-power neural processing
unit — while compiling or rendering already needs the CPU and GPU.
**What Handy does.** FastFlowLM (FLM) can run Handy's Whisper transcription on that NPU, leaving
the CPU and GPU free for the work already using them. FLM is a separate, third-party program:
you install it yourself, and Handy does not bundle, install or download it. Handy auto-detects
`flm` on `PATH`, then `%LOCALAPPDATA%\flm\flm.exe`, `~/.flm/flm.exe`, and
`C:\Program Files\flm\flm.exe`, in that order; there is no path setting. Once detected, its
`whisper-v3:turbo` model appears with the other model choices. Handy starts FLM on
`127.0.0.1`, so transcription audio stays on the machine. If the language is `auto`, this
engine uses English. The engine path exists in the Windows and Linux code, but Handy releases
only Windows x64 today; Linux remains planned and in the queue.
**Where.** No path control — install FLM in one of the auto-detected locations, then open
`Models › Downloaded Models` and select **FLM Whisper V3 Turbo (NPU)**.
**Since.** 0.8.2.
<!-- prov: FLM documentation audit | src: src-tauri/src/managers/flm.rs; src-tauri/src/managers/model.rs; src-tauri/src/managers/transcription.rs; src/components/settings/models/ModelsSettings.tsx -->

### Windows blocked FLM — know which choices are real

<a id="windows-blocked-flm-know-which-choices-are-real"></a>
**The situation.** Windows says it "can't verify" `flm.exe`, calls it an "unknown publisher",
says it was "blocked", or Handy reports OS error 4551. The Code Integrity log may record event
3077 or 3033 and say that `flm.exe` did not meet the signing requirements.
**What Handy does.** FastFlowLM currently ships an unsigned `flm.exe`, and enforced Windows
Smart App Control can refuse to let Handy start it. This is **not an antivirus detection**:
Microsoft Defender antivirus reports zero threat detections, and adding an antivirus exclusion
does nothing. Smart App Control is a separate mechanism with no exclusion list, so one program
cannot be allowed through it.

The low-cost choice is to use any other Handy transcription engine; FLM is an optional
accelerator, not a requirement. You can also wait for FastFlowLM to publish a signed binary,
which is outside Handy's control. The remaining choice is to turn Smart App Control off, but
that is one-way: it cannot be switched back on without resetting or reinstalling Windows. Weigh
that cost before changing it. Smart App Control blocks unsigned binaries generally, so an
enforced machine may reject other unsigned developer tools too.
**Where.** `Windows Security › App & browser control › Smart App Control settings` *{Windows only}*. <!-- drift-ok -->
**Since.** 1.0.0.
<!-- prov: verified 2026-08-07 | src: src-tauri/src/managers/flm.rs; src/lib/flm.ts; src/App.tsx; src/components/model-selector/ModelSelector.tsx; src/i18n/locales/en/translation.json | claim: safety -->

### Orphaned NPU servers can't block your next take

<a id="orphaned-npu-servers-cant-block-your-next-take"></a>
**The situation.** After a crash — or after the installer force-closed the app — the NPU engine
fails to start, forever. Each restart leaves another stuck process behind, eating memory.
**What Handy does.** The NPU serves one application at a time, and a leaked server keeps both
the port and the accelerator. Handy binds its child process to a Windows job object so the
operating system kills it the instant Handy dies, and sweeps for its **own** orphans at launch
and before every start. The sweep is signature-exact: another application's server is never
touched.
**Where.** No control — this is always active. *{Windows only}*
**Since.** 0.53.0.

### The NPU error tells you what to close

<a id="the-npu-error-tells-you-what-to-close"></a>
**The situation.** You get a hexadecimal error code and a suggestion to update your driver,
which is not the problem.
**What Handy does.** When the NPU cannot create an inference context, the message names the
actual cause first — the accelerator serves one application at a time and something else is
holding it — and tells you to close that and reselect the model. Driver advice comes second.
**Where.** No control — this is always active.
**Since.** 0.47.0.

### The NPU engine starts in the mode FLM now requires

<a id="the-npu-engine-starts-in-the-mode-flm-requires"></a>
**The situation.** You update FLM and the engine stops working with "unsupported model family".
**What Handy does.** Recent FLM versions refuse a speech model as a positional argument and have
dropped their health endpoint, so Handy starts
`flm serve --port 52625 --host 127.0.0.1 --asr 1` and polls `GET /v1/models` for readiness.
Transcription uses `POST /v1/audio/transcriptions` with model `whisper-v3:turbo`. A failed start
sets a 60-second cooldown so a broken installation cannot keep blocking your takes.
**Where.** No control — this is always active.
**Since.** 0.38.0.
<!-- prov: D-169 | src: src-tauri/src/managers/flm.rs; src-tauri/src/managers/transcription.rs -->

### Cancel a stuck download and it stops now

<a id="cancel-a-stuck-download-and-it-stops-now"></a>
**The situation.** A model download stalls, you cancel it, and the interface says canceled while
the machine keeps the connection and the file handle open — sometimes for a full minute.
**What Handy does.** Cancellation interrupts a parked read immediately and **keeps the partial
file so you can resume**. The whole download lifecycle is tied to one attempt identity, so a
canceled or superseded attempt can never clobber a retry's progress or overwrite the model you
have since selected.
**Where.** `Models › Available to Download`.
**Since.** 0.44.0.

### A downloaded model shows as downloaded

<a id="a-finished-download-never-shows-as-not-downloaded"></a>
**The situation.** The download completes and the card still offers to download it.
**What Handy does.** Each model carries a refresh revision so a stale snapshot of the disk cannot
overwrite a newer state, and the cleanup of leftover extractions runs only at startup, where it
cannot delete an extraction that is in flight.
**Where.** `Models › Downloaded Models`.
**Since.** 0.45.0.

### Bring your own Whisper model

<a id="bring-your-own-whisper-model"></a>
**The situation.** You have a fine-tuned or community Whisper model that is not in the list.
**What Handy does.** Any Whisper GGML `.bin` file dropped into the models folder is discovered on
the next start and appears as a custom model, selectable like any other.
**Where.** `File › %APPDATA%\pr.handy\models`.
**Since.** Present since the fork's early releases.

### Free the memory when you stop dictating

<a id="free-the-memory-when-you-stop-dictating"></a>
**The situation.** A large model holds a lot of RAM or VRAM, and you dictate in bursts with an
hour in between.
**What Handy does.** An idle timeout evicts the model — never, immediately after each use, after
a set number of minutes, or a custom interval. Keeping it loaded gives the fastest start; letting
it go returns the memory to whatever you are actually working on.
**Where.** `Models › Unload Model = After 15 minutes`.
**Since.** Present since the fork's early releases.

### Why can't I force the language on this model?

<a id="why-cant-i-force-the-language-on-this-model"></a>
**The situation.** The language dropdown has vanished and you cannot tell whether that is a bug.
**What Handy does.** Models that are multilingual but detect the language themselves say so
explicitly, with a read-only "auto-detected" value, and point you at the engines that do let you
pin a language. Likewise, a model that was never trained to translate no longer offers a
translate switch.
**Where.** `General › Language`.
**Since.** 0.10.0.

---

<a id="section-keyboard-typer"></a>

## Keyboard Typer

For the places a paste will never work.

### When paste is blocked, type it instead

<a id="when-paste-is-blocked-type-it-instead"></a>
**The situation.** The VM console ignores Ctrl+V. Clipboard redirection is switched off on the
remote session. The password prompt must never see a clipboard at all.
**What Handy does.** A page where you put text and a shortcut that types it into whatever window
has focus, as individual simulated keystrokes. No clipboard is involved at any point.
**Where.** `Keyboard Typer › Enter the text to type...` then
`Keyboard Typer › Type Text Shortcut` — `ctrl+alt+t` by default.
**Since.** 0.12.0.

### The text never touches your disk

<a id="the-text-never-touches-your-disk"></a>
**The situation.** You are about to type a password into a locked-down console, and you would
like to know exactly where that string ends up.
**What Handy does.** The Keyboard Typer's text lives in memory only. It is deliberately not part
of the settings file, is not written to History, and the module does not log its content. Honest
limit: the value is not wiped after use and remains in process memory until it is replaced or
the app exits, and what the destination application does with it is outside Handy's control.
**Where.** No control — this is always active.
**Since.** 0.12.0.

### Slow enough for a remote console

<a id="slow-enough-for-a-remote-console"></a>
**The situation.** Injecting text at full speed into an RDP session or a VM console drops
characters, so half the string arrives.
**What Handy does.** A per-keystroke delay, 15 ms by default — fast enough to feel instant,
slow enough to be reliable over a remote link — with quick presets for slower targets.
**Where.** `Keyboard Typer › Key delay`.
**Since.** 0.12.0.

### Ten seconds to put the cursor where it belongs

<a id="ten-seconds-to-put-the-cursor-where-it-belongs"></a>
**The situation.** You press Go and then have to click into a console that takes a moment to
focus.
**What Handy does.** A countdown before typing starts, ten seconds by default with one, three and
five second presets. Escape cancels, as does pressing the typing shortcut again, and an in-flight
session is canceled cleanly rather than half-typed.
**Where.** `Keyboard Typer › Start delay` and `Keyboard Typer › Cancel`.
**Since.** 0.12.0.

### Your trigger chord doesn't become part of the text

<a id="your-trigger-chord-doesnt-become-part-of-the-text"></a>
**The situation.** You trigger typing with a chord and the first characters arrive as hotkeys
because you were still holding Ctrl.
**What Handy does.** Waits out a modifier-release grace before the first keystroke, and reports
its state — counting down, typing, done, canceled — so you are never guessing whether it is
running.
**Where.** No control — this is always active.
**Since.** 0.12.0.

---

<a id="section-model-testing"></a>

## Model Testing

A second, smaller product inside the first: run one prompt across the models you actually pay
for and see what happens.

### Which model is actually better at my task?

<a id="which-model-is-actually-better-at-my-task"></a>
**The situation.** A new model ships and everybody has an opinion. None of those opinions were
formed on your work.
**What Handy does.** Runs one prompt across any set of your registered providers and puts the
answers side by side, with tokens, cost, and wall-clock round-trip for each. Concurrency respects
provider families, so local slots sharing one loader take turns while cloud models run at once,
and the reported time is to the last finisher rather than the sum.
**Where.** `Model Testing › Prompt for all models` then `Model Testing › Run test`.
**Since.** 0.15.0.

### Let a panel score the answers

<a id="let-a-panel-score-the-answers"></a>
**The situation.** Six answers, and reading them all carefully takes longer than the work you
were trying to automate.
**What Handy does.** An optional judge panel: chosen models receive your arbiter prompt, the
original input and every candidate answer, and return their assessments. Judges are picked
independently of the models being tested, and they get their own temperature and thinking
settings, both recorded in the report.
**Where.** `Model Testing › Judge / arbiter prompt (optional)`.
**Since.** 0.15.0; separate judge parameters in 0.20.0.

### Local models can judge too

<a id="local-models-can-judge-too"></a>
**The situation.** Your local judge returns nonsense while a cloud judge on the same input is
fine, which reads as "small models are useless as judges".
**What Handy does.** The arbiter instructions used to live in the system message, which small
local models down-weight — so they genuinely did not see the answers they were meant to score.
Instructions and all numbered answers now go in a single user message, and local judges work.
**Where.** No control — this is always active.
**Since.** 0.17.0.

### Stop retyping the same test prompts

<a id="stop-retyping-the-same-test-prompts"></a>
**The situation.** You have a standard evaluation you run against every new model, and you
retype it from memory each time.
**What Handy does.** A library of saved model prompts and judge prompts, plus presets that pair
one of each under a name. Selecting a preset fills both pickers with the prompts it is made of,
so you can see and adjust each half rather than getting an opaque bundle. Saved prompts keep
their attached image.
**Where.** `Model Testing › Preset`.
**Since.** 0.17.0; visible preset parts in 0.19.0.

### Test vision models with a real image

<a id="test-vision-models-with-a-real-image"></a>
**The situation.** The task you care about is "read this screenshot", and a text prompt tells you
nothing about it.
**What Handy does.** Attach an image by button or drag and drop, and send prompt plus image,
prompt only, or image only. Each provider receives it in its own native multimodal shape. An
image that cannot be read fails loudly instead of quietly sending a text-only request.
**Where.** `Model Testing › Image (optional, for vision models)`.
**Since.** 0.18.0.

### Thinking on or off, per model

<a id="thinking-on-or-off-per-model"></a>
**The situation.** You want to compare like with like, and half the models default to extended
reasoning while the other half do not.
**What Handy does.** Automatic, on, or off, for the tested models and for the judges separately —
translated into each vendor's own parameter, including the modern adaptive form for current
models rather than a deprecated shape that returns an error.
**Where.** `Model Testing › Thinking = Off`.
**Since.** 0.18.0.

### See what's happening between dispatch and verdict

<a id="see-whats-happening-between-dispatch-and-verdict"></a>
**The situation.** You start a run against eight providers and stare at a spinner with no idea
which one is holding things up.
**What Handy does.** A live activity feed logs each model and judge as it finishes, with a
success or failure mark and its timing, plus markers for each phase of the run.
**Where.** `Model Testing › Run test`.
**Since.** 0.17.0.

### One Markdown artifact you can keep

<a id="one-markdown-artifact-you-can-keep"></a>
**The situation.** The comparison is only useful if you can put it in a ticket, a commit message
or a decision log.
**What Handy does.** Every run produces one Markdown document — input, a summary table with
tokens, cost and time, the judge panel, then each model's full answer. Copy it, or save it with
a sensible default filename and re-save to the same path with one click afterwards.
**Where.** `Model Testing › Copy Markdown` and `Model Testing › Save as…`.
**Since.** 0.15.0; save-as in 0.18.0.

### Unconfigured seats stay out of the run

<a id="unconfigured-seats-stay-out-of-the-run"></a>
**The situation.** Your provider list has spare slots you have not filled in, and they show up in
every run list to be unticked again.
**What Handy does.** Only enabled, configured providers appear in the run and judge lists, and
the scheduler skips the rest.
**Where.** `Advanced › Providers › Registered LLM Providers › Enable this provider`.
**Since.** 0.17.0.

---

<a id="section-token-count"></a>

## Token Count

### What will this prompt cost?

<a id="what-will-this-prompt-cost"></a>
**The situation.** You are about to paste 40 KB of context into a model and would like to know
what that means before you do it.
**What Handy does.** Paste or load text and count it with a tokenizer, or against a real
provider's own counting endpoint. Chips across the top cover the built-in tokenizers and every
configured provider; click one to count with it.
**Where.** `Token Count › Paste text here to count tokens...`.
**Since.** 0.12.0.

### Token counts you can trust from a local server

<a id="token-counts-you-can-trust-from-a-local-server"></a>
**The situation.** You count one word against a local server and it reports 18 to 25 tokens.
**What Handy does.** Chat endpoints report tokens including the server's own chat-template
wrapping — measured at plus seventeen on one popular local server and plus thirteen on another.
Counting now uses the raw completions endpoint and calibrates that fixed overhead away with a
known one-token probe, which is exact against both servers, with a fallback for endpoints that
have no completions route.
**Where.** `Token Count › Count with all`.
**Since.** 0.13.0.

### Counts without a network call

<a id="counts-without-a-network-call"></a>
**The situation.** You are offline, or you do not want the text you are counting to leave the
machine at all.
**What Handy does.** Three built-in counters run entirely locally — two exact tokenizers and a
rough estimate — and they are the first rows of every comparison. The estimate is excluded from
the difference baseline so it cannot skew a comparison between exact tokenizers.
**Where.** `Token Count › cl100k (GPT-4)` and `Token Count › o200k (GPT-4o)`.
**Since.** 0.12.0 and 0.13.0.

### One click, every provider, one table

<a id="one-click-every-provider-one-table"></a>
**The situation.** The same text tokenizes differently on every vendor, and you want the spread.
**What Handy does.** Counts with every enabled provider and renders provider, model, token count,
difference against the smallest, and time — rows appearing as they finish. Two modes: serialized,
which is correct when several slots share one local service, and parallel, which takes as long
as the slowest provider instead of the sum.
**Where.** `Token Count › Count with all (parallel)`.
**Since.** 0.12.0; parallel mode in 0.13.1.

### Count a file instead of pasting it

<a id="count-a-file-instead-of-pasting-it"></a>
**The situation.** The thing you want to count is a 3 MB transcript, and pasting it into a text
box is not a good plan.
**What Handy does.** Opens a text file up to 10 MB and counts it directly.
**Where.** `Token Count › Open file...`.
**Since.** 0.12.0.

---

<a id="section-translator"></a>

## Translator

Drop a folder of recordings and get text back, while you get on with something else.

### A folder of recordings, transcribed while you sleep

<a id="a-folder-of-recordings-transcribed-while-you-sleep"></a>
**The situation.** You have interviews, voice memos or meeting captures piling up, and
transcribing them one at a time by hand is the whole afternoon.
**What Handy does.** Watches folders you choose and transcribes new audio files into a `.txt`
file next to the source, using your engines and your settings. It runs in the background while
you keep using the app normally.
**Where.** `Translator › Watch folders = On` then `Translator › Add a folder`.
**Since.** The 0.3x series.

### Your existing files are left alone

<a id="your-existing-files-are-left-alone"></a>
**The situation.** You point it at a folder with two thousand old recordings and it starts
grinding through all of them.
**What Handy does.** Only files that appear *after* watching starts are queued. The existing
contents are snapshotted and deliberately ignored. Handy's own recorder-internal files — chunk
parts, temporary files, partial downloads — are never picked up.
**Where.** `Translator › Watched folders`.
**Since.** The 0.3x series.

### A file is never transcribed twice

<a id="a-file-is-never-transcribed-twice"></a>
**The situation.** You restart the machine mid-batch and it starts again from the beginning.
**What Handy does.** The `.txt` sidecar is the record that a file is done, so it stays done across
restarts forever. The pending queue is persisted as well, so unfinished work resumes rather than
restarting. A file recreated under the same name is detected as new by its modification time.
**Where.** `Translator › Status`.
**Since.** The 0.3x series.

### Never reads a file that is still being written

<a id="never-reads-a-file-that-is-still-being-written"></a>
**The situation.** Your recorder is still writing a file and the watcher grabs it half-finished.
**What Handy does.** A file must be size-stable across scans before it is read at all.
**Where.** No control — this is always active.
**Since.** The 0.3x series.

### Live dictation always wins

<a id="live-dictation-always-wins"></a>
**The situation.** You press your dictation key and nothing happens, because a batch job is
holding the engine.
**What Handy does.** Three policies decide who gets the engine. Live-first pauses the batch at
the next segment boundary as soon as a recording starts — the default, and the one that keeps
dictation feeling instant. Folder-first keeps the batch running and queues live segments behind
it, though the batch always yields while a take is finishing. First-come-first-served finishes
the current file's segments before the next job. Batch work is cut into roughly forty-second
segments so pausing never discards progress.
**Where.** `Translator › Priority = Live dictation first`.
**Since.** The 0.3x series.

### Batch on one accelerator, dictation on another

<a id="batch-on-one-accelerator-dictation-on-another"></a>
**The situation.** You have an NPU and a GPU, and running batch work means your dictation model
is constantly being unloaded and reloaded.
**What Handy does.** The Translator can use a different model from your dictation model and keep
it resident in parallel — dictation on the NPU while the batch grinds a Whisper model on the
integrated GPU. Shared hardware serializes gracefully rather than racing, and the batch model has
its own idle-unload setting so it can be released independently.
**Where.** `Translator › Batch model` and `Translator › Unload batch model after`.
**Since.** 0.48.0.

### You can see what it is working on

<a id="you-can-see-what-it-is-working-on"></a>
**The situation.** You want to know whether it is working, queued or finished, without opening a
log.
**What Handy does.** A status row that reads off, watching with nothing to do, a count of queued
files, or the current file and which segment of it is being transcribed.
**Where.** `Translator › Status`.
**Since.** The 0.3x series.

---

<a id="section-mcp-and-cli"></a>

## MCP and CLI

Let a script, a window manager or an agent drive the same logic the interface uses.

### Let an agent drive the app

<a id="let-an-agent-drive-the-app"></a>
**The situation.** Your coding agent could use Handy's token counter, your history or its model
comparison, and there is no way in.
**What Handy does.** An optional MCP server, off by default, bound to `127.0.0.1` only and
guarded by a bearer token. It speaks HTTP for the Claude app and stdio through a bridge for
Claude Code, and exposes Handy's own tools: token counting, typing text into the focused window,
listing and reading history, listing and setting providers, saving model-test prompts and
running a model test.
**Where.** `Advanced › MCP & CLI › Enable MCP & CLI server = On`.
**Since.** 0.20.0.

### A handy command on your PATH

<a id="a-handy-command-on-your-path"></a>
**The situation.** You want the same capabilities from a shell script or a scheduled job, not
from an agent.
**What Handy does.** Installs a `handy` command that talks to the running app over the same
local server: model tests, token counts, typing, history listing, provider configuration, plus
the stdio bridge. It finds the running instance by itself through a small discovery file, so you
never pass a port or a token on the command line.
**Where.** `Advanced › MCP & CLI › Command-line companion`, then `CLI › handy install-cli`.
**Since.** 0.20.0.

### An agent can set a key but never read one

<a id="an-agent-can-set-a-key-but-never-read-one"></a>
**The situation.** You are giving an autonomous process access to the app that holds your API
keys.
**What Handy does.** Provider API keys are write-only across MCP and the CLI: a caller can set a
key, change a model, family or enablement, but every response replaces the key with a flag saying
whether one exists. Honest limit: this is about the interface, not storage — keys remain in
plain text in the settings file and in backups.
**Where.** On by default when the server is enabled — nothing to configure.
**Since.** 0.20.0.

### Bound to localhost, behind a token — and what that does not cover

<a id="bound-to-localhost-behind-a-token"></a>
**The situation.** You want to know exactly what you are exposing before you switch a server on.
**What Handy does.** It binds to the loopback address only, never to a network interface, and
every call except a liveness check requires the exact bearer token. What it does *not* do is
isolate Handy from other processes running as you: the token is stored in plain text in the
settings file and in a small discovery file, so any local process running as your user can read
it and then read your history, change provider URLs or run a model test. Traffic is plain HTTP
over loopback, not TLS. Leave the server off unless you want it.
**Where.** `Advanced › MCP & CLI › Token` and `Advanced › MCP & CLI › Port`.
**Since.** 0.20.0.

### Scriptable model tests that produce the same artifact as the interface

<a id="scriptable-model-tests-that-match-the-interface"></a>
**The situation.** You want a nightly model comparison in a repository, and a script that
reimplements the report will drift from the app within a month.
**What Handy does.** The report assembly, judge-prompt construction and price handling used by
the command line and MCP are the same logic the interface uses, so a scripted run produces the
same document and the same cost figures. Models and judges are selected by identifier or name,
temperatures and thinking are separate for run and judge, an image can be attached, and the
report comes back inline or is written to a path you choose.
**Where.** `CLI › handy model-test`.
**Since.** 0.20.0.

### Drive it from your window manager or a hotkey daemon

<a id="drive-it-from-your-window-manager"></a>
**The situation.** Something else owns your hotkeys — an autostart script, a macro tool, a
window manager — and you want it to control recording.
**What Handy does.** Command-line flags toggle recording, toggle recording with post-processing,
or cancel on the already-running instance. A second launch forwards its arguments and
exits. Two more flags control how the app itself starts.
**Where.** `CLI › handy --toggle-transcription`, `CLI › handy --cancel`,
`CLI › handy --start-hidden`, `CLI › handy --no-tray`.
**Since.** Present since the fork's early releases.

### Your agent can read your history — know that before you enable it

<a id="your-agent-can-read-your-history"></a>
**The situation.** The convenience of an agent reading back what you dictated is also a
disclosure.
**What Handy does.** The history tools return recent entries as short snippets, and a full entry
including the filesystem path of its audio — a path, not the audio itself. The typing tool goes
through the ordinary clipboard paste path, so its text can appear on the clipboard and in debug
logs. This is stated so you can decide, not buried.
**Where.** `CLI › handy history-list`.
**Since.** 0.20.0.

---

<a id="section-backup"></a>

## Backup and portability

### One file that carries your whole setup

<a id="one-file-that-carries-your-whole-setup"></a>
**The situation.** New machine on Monday, and rebuilding eleven shortcuts, six providers and a
prompt library by hand is an afternoon you do not have.
**What Handy does.** Exports a single `.tar.gz`. The configuration profile carries settings and
the history database — timestamps, text, cost, duration. The full profile adds your compressed
recordings.
**Where.** `Backup › Configuration + history › Export config + history` and
`Backup › Full data (with compressed audio) › Export full backup`.
**Since.** 0.22.0.

### Move machines, or undo a bad week

<a id="move-machines-or-undo-a-bad-week"></a>
**The situation.** You want your settings back but not the recordings, or the recordings back
but not a settings file from three weeks ago.
**What Handy does.** Restore picks an archive and lets you choose what comes back —
configuration and history, recordings, or both — and reports per-item errors instead of failing
the whole operation on one bad entry.
**Where.** `Backup › Restore from backup › Configuration & history (settings, history DB)` and
`Backup › Restore from backup › Restore from backup…`.
**Since.** 0.25.0.

### A crafted archive can't write outside the app

<a id="a-crafted-archive-cant-write-outside-the-app"></a>
**The situation.** An archive is an executable format if you treat it carelessly — a path like
`../../Startup/evil.exe` inside a tarball is the classic attack.
**What Handy does.** Restore extracts only entries whose names are on a known list, accepts only
regular files, refuses any path traversal, and caps decompressed size.
**Where.** No control — this is always active.
**Since.** 0.25.0.

### Why a restore asks for a restart

<a id="why-a-restore-asks-for-a-restart"></a>
**The situation.** You restore your settings, and the running app overwrites them again on its
next write.
**What Handy does.** The running app holds settings in memory, so a settings or history restore
is followed by an explicit restart button rather than a silent inconsistency.
**Where.** `Backup › Restore from backup › Restore from backup…`.
**Since.** 0.25.0.

### What a backup deliberately leaves out

<a id="what-a-backup-deliberately-leaves-out"></a>
**The situation.** You assume a "full" backup means everything, and find out otherwise at the
worst moment.
**What Handy does.** Downloaded models are always excluded — they are large and re-downloadable.
Even a full backup excludes uncompressed WAV and FLAC audio and in-progress temporary chunks, so
recordings made with crash-safe recording switched off are **not** in the archive, although their
history rows are. And the honest one: a settings backup contains your API keys and the server
token in plain text, so treat the file as a secret.
**Where.** `Backup › Full data (with compressed audio) › Export full backup`.
**Since.** 0.22.0.

### Run it from a USB stick and leave no trace

<a id="run-it-from-a-usb-stick"></a>
**The situation.** A locked-down corporate desktop, a shared lab machine, or you do not
want an application writing into your profile.
**What Handy does.** Put a marker file next to the executable and everything — settings, models,
history, recordings, logs, web-view data — lives in a folder beside it, with no machine-level
change: no autostart entry, no command-line installation. If that folder cannot be written it
falls back to the normal per-user location rather than failing.
**Where.** `File › portable.marker` beside `handy.exe` *{Windows only}*.
**Since.** 0.43.0.

---

<a id="section-audio"></a>

## Audio and feedback

### It records what you say, not the silence

<a id="it-records-what-you-say-not-the-silence"></a>
**The situation.** Long pauses while you think should not become long pauses in the file or work
for the transcription engine.
**What Handy does.** A voice-activity model runs locally on the incoming audio and only speech is
kept, with a debounce so a short breath does not chop the recording. The same silence boundaries
are what makes it safe to cut transcription segments without splitting a word. Honest scope: the
smoothing keeps a little surrounding silence, so this is not an exact speech-only cut.
**Where.** No control — this is always active.
**Since.** 0.1.0; hysteresis in 0.3.0.

### Nothing is dropped at the moment you stop

<a id="nothing-is-dropped-at-the-moment-you-stop"></a>
**The situation.** You stop mid-breath and the last fragment of speech is missing, or a
microphone that died mid-recording means the stop never completes at all.
**What Handy does.** Voiced frames buffered during an unconfirmed speech onset are flushed into
both the recording and the final segment at stop rather than discarded. Separately, stop and
cancel are serviced on a timer rather than only when audio arrives, so a stream that died cannot
hang the stop.
**Where.** No control — this is always active.
**Since.** 0.25.0.

### Hear when the microphone is hot

<a id="hear-when-the-microphone-is-hot"></a>
**The situation.** You start talking before the recorder started, or keep talking after it
stopped, because you were looking at something else.
**What Handy does.** Distinct start and stop cue sounds, so you never have to look at the screen
to know the state. Volume, output device and the sound set are all configurable, including
supplying your own two files.
**Where.** `General › Sound › Audio Feedback = On`, `General › Sound › Volume`, and
`Debug › Sound Theme = Custom` *{requires: Debug mode}*.
**Since.** 0.1.5.

### The microphone light is off when you're not dictating

<a id="the-microphone-light-is-off-when-youre-not-dictating"></a>
**The situation.** A dictation tool that holds the microphone open shows a permanent recording
indicator, which is both a privacy question and a distraction.
**What Handy does.** The microphone is opened when a take starts and released when it ends.
Optionally you can keep it open — which removes the small first-syllable clip at the very start
of a take — at the cost of a permanently active indicator. That is an explicit choice, not the
default.
**Where.** `Debug › Always-On Microphone = On` *{requires: Debug mode}*.
**Since.** 0.2.0.

### Change microphone without restarting

<a id="change-microphone-without-restarting"></a>
**The situation.** You plug in a headset and the app is still recording from the laptop's array.
**What Handy does.** The input device is a dropdown that takes effect on the next take, with a
reset to the system default.
**Where.** `General › Sound › Microphone`.
**Since.** 0.3.0.

### Your music doesn't end up in the transcript

<a id="your-music-doesnt-end-up-in-the-transcript"></a>
**The situation.** You dictate over speakers and the engine faithfully transcribes the podcast
you were half-listening to.
**What Handy does.** Optionally mutes system output for the duration of the recording.
**Where.** `General › Sound › Mute While Recording = On`.
**Since.** Present since the fork's early releases.

### See that it is listening

<a id="see-that-it-is-listening"></a>
**The situation.** You are not sure whether the recorder is running, and the only way to find out
used to be to talk and hope.
**What Handy does.** A small overlay shows live audio levels while recording, at the top or the
bottom of the screen, or not at all. The tray icon carries the same state — idle, recording,
transcribing — in light, dark and color variants so it stays legible on any theme.
**Where.** `Advanced › App › Overlay Position = Bottom`.
**Since.** Present since the fork's early releases.

### Canceling can't freeze the app

<a id="cancelling-cant-freeze-the-app"></a>
**The situation.** You press Escape at exactly the wrong instant and the whole application hangs
with the overlay stuck mid-frame, requiring a force-restart.
**What Handy does.** The overlay's level updater used to make a synchronous window query from the
audio thread; cancelling at that moment deadlocked it against the main thread tearing down the
microphone. The updater now uses a lock-free visibility flag and never blocks on the interface
thread, and cancellation commits its state before any device teardown.
**Where.** No control — this is always active.
**Since.** 0.52.1.

### The idle-unload timer keeps working all session

<a id="the-idle-unload-setting-keeps-working-all-session"></a>
**The situation.** You set the model to unload after ten minutes and it never does — after the
very first recording of the session.
**What Handy does.** A transient copy of the transcription manager used to tear down the shared
idle watcher when it went out of scope. Ownership is now tracked, so only the real owner stops
the watcher, at shutdown.
**Where.** `Models › Unload Model`.
**Since.** 0.52.2.

---

<a id="section-platform"></a>

## Platform and privacy

### What runs today, and what is planned

<a id="what-runs-today-and-what-is-planned"></a>
**The situation.** You are on a Mac or a Linux box and want a straight answer.
**What Handy does.** Windows x64 is the only build produced, released and tested. macOS and Linux
builds are planned and in the queue; there is no download for them today and none of this
documentation describes something you can run on them. Some features would not follow even then:
the whole Jumper family is built on Win32 focus APIs, and portable mode is Windows-only. In the
other direction, some code exists only for platforms that do not ship yet — on-device Apple
Intelligence post-processing and closed-lid microphone switching on macOS, and the native text
injection backends on Linux — and none of it is reachable today.
**Where.** `About › Version`.
**Since.** 1.0.0 is the first public release.

### The app makes no calls you didn't ask for

<a id="the-app-makes-no-calls-you-didnt-ask-for"></a>
**The situation.** "Local-first" is claimed by everything, and you would like to know what it
means here.
**What Handy does.** There is no telemetry, no analytics, no crash reporter and no account. Every
outbound request that carries your content is one you configured and triggered: a model download
you start, a transcription to an endpoint you entered, a post-processing call, a model test, a
token count against a provider. With a local model selected, ordinary dictation initiates no
network activity at all. The one request Handy makes on its own is the daily update check
described in [Updates you decide to take](#updates-you-decide-to-take), which carries no audio,
transcript, prompt, history entry, setting or key — and which you can turn off.
**Where.** No control — this is always active.
**Since.** 0.12.0.

### Updates you decide to take

<a id="updates-you-decide-to-take"></a>
**The situation.** You want the fixes, and you do not want an application restarting itself in
the middle of your working day or phoning home without saying so.
**What Handy does.** Once a day Handy asks the public GitHub releases page whether a newer
version exists. That check is **on** by default and sends nothing but the ordinary metadata of an
HTTPS request and the version you are running. Installing is a separate decision: silent
installation is **off** by default, so by default a newer version is announced in the sidebar and
waits for you. Switch silent installation on and the update is applied inside a window you choose
— 04:00 local time by default, moved by up to 30 minutes each day so every copy does not call at
the same instant. Releases are signed and the signature is verified before anything is applied. A
portable copy refuses to update itself in place and tells you to download the new portable
release instead. `Check now` runs the same check on demand.
**Where.** `General › Updates › Check for updates automatically`,
`General › Updates › Install updates silently`, `General › Updates › Silent update time`,
`General › Updates › Daily randomization`, and `General › Updates › Check now`.
**Since.** 1.0.0.

### Where your data lives on disk

<a id="where-your-data-lives-on-disk"></a>
**The situation.** Before you trust an application with everything you say, you want to know what
it keeps and where.
**What Handy does.** Everything is under one folder: settings including your API keys and prompts
in a JSON file, transcripts in a SQLite database, audio in a recordings folder, downloaded models
and rotated logs. Nothing is encrypted by Handy — file-system permissions are the protection —
and there is a button in the app for both the data folder and the log folder.
**Where.** `About › App Data Directory` and `About › Log Directory`.
**Since.** Present since the fork's early releases.

### Your dictation is not written into logs at the normal level

<a id="your-dictation-is-not-written-into-logs"></a>
**The situation.** Diagnostics are useless if they are empty and dangerous if they contain
everything you ever said.
**What Handy does.** Transcript previews were moved off the informational log level, and the
released build writes its file log at **info**, so ordinary logging does not record what you
dictated. The honest counterpart: raise the level to `Debug` yourself and transcript fragments,
complete API responses and prompt previews **can** appear in the log file. Put it back to `Info`
when you are finished diagnosing.
**Where.** `Debug › Log Level = Info` *{requires: Debug mode}*.
**Since.** 0.38.0.

### The logs still exist when you finally need them

<a id="the-logs-still-exist-when-you-need-them"></a>
**The situation.** Something goes wrong, you go looking for the evidence, and the launch that
followed the problem has already rotated it away.
**What Handy does.** Logs rotate at 10 MB instead of 500 KB and rotated files are kept rather
than being overwritten by the next session. Engine start failures are written to the log at error
level rather than existing only as a toast you already dismissed.
**Where.** `About › Log Directory`.
**Since.** 0.53.0.

### The honest limits of clipboard safety

<a id="the-honest-limits-of-clipboard-safety"></a>
**The situation.** You want the real worst case, not the reassuring version.
**What Handy does.** Handy saves and restores clipboard **text** only, so an image, a copied file
or rich formatting on the clipboard is lost when a paste restores an empty or plain-text value.
During the paste the transcript is readable by anything that watches the clipboard, including
Windows clipboard history and remote-session clipboard redirection. If the app is killed between
the paste and the restore, the transcript stays on the clipboard. And a deliberate park after a
failed delivery overwrites the previous clipboard on purpose, because losing your words is judged
worse.
**Where.** `Advanced › Transcription › Transcribe › Clipboard Handling`.
**Since.** Documented behavior of the current release.

### The tray tells you what it is doing

<a id="the-tray-tells-you-what-it-is-doing"></a>
**The situation.** The window is closed, you pressed the key, and you want to know whether
anything is happening.
**What Handy does.** The tray icon changes between idle, recording and transcribing, in variants
that stay readable on light, dark and colored taskbars, with a tooltip. Its menu opens settings,
copies the last transcript, unloads the model, cancels a take and quits. Its labels are generated
at build time from the same translation files the interface uses, so they cannot drift out of
sync.
**Where.** `Tray › Copy Last Transcript` and `Advanced › App › Show Tray Icon = On`.
**Since.** Present since the fork's early releases.

### Starts with your session and stays out of the way

<a id="starts-with-your-session-and-stays-out-of-the-way"></a>
**The situation.** A dictation tool is only useful if it is already running when the thought
arrives, and only tolerable if it is not in your face.
**What Handy does.** Optional launch at login, optional start with no window, and an optional
tray-only existence. Note one consequence: with the tray icon switched off, closing the window
quits the application.
**Where.** `Advanced › App › Launch on Startup = On` and `Advanced › App › Start Hidden = On`.
**Since.** 0.1.0.

### Light, dark, or follow the system

<a id="light-dark-or-follow-the-system"></a>
**The situation.** Half the application follows your system theme and the other half is
permanently bright at midnight.
**What Handy does.** One appearance setting applied consistently across the main window, the
recording overlay and the floating transcription window, following the system by default and
tracking it live.
**Where.** `Advanced › App › Appearance = Dark`.
**Since.** 0.41.0.

### It looks and behaves like a Windows app

<a id="it-looks-and-behaves-like-a-windows-app"></a>
**The situation.** Web-technology desktop applications tend to feel like a web page in a frame —
wrong font, wrong scrollbars, native widgets stuck in light mode.
**What Handy does.** The system font stack, styled scrollbars, native widgets that follow dark
mode, a maximisable window that respects snap layouts, and a sidebar you can drag to fit long
provider names, remembered between launches.
**Where.** `Advanced › App › Appearance`.
**Since.** 0.24.0; resizable sidebar in 0.17.0.

### Usable with the keyboard, and readable

<a id="usable-with-the-keyboard-and-readable"></a>
**The situation.** You navigate by keyboard, or you need real contrast, or animations make you
ill.
**What Handy does.** A visible focus outline on every control, the overlay's controls as real
labeled buttons for screen readers, primary buttons and secondary text deepened to meet contrast
requirements, danger buttons themed rather than raw red, and reduced-motion preferences honored
everywhere including the pulsing recording animation.
**Where.** No control — this is always active.
**Since.** 0.24.0.

### No console window flashing on your desktop

<a id="no-console-window-flashing-on-your-desktop"></a>
**The situation.** Every couple of minutes a black command window appears somewhere on your
screen and vanishes. It looks like malware and it steals focus.
**What Handy does.** Detection of the optional NPU runtime was running a command-line probe on
every model-list rebuild, which on Windows spawns a visible console. Those subprocesses now run
hidden, and detection is cached to once per session.
**Where.** No control — this is always active. *{Windows only}*
**Since.** 0.24.0.

### The installer tells you what's missing, and the build runs on your CPU

<a id="the-installer-tells-you-whats-missing"></a>
**The situation.** An installer that succeeds and an application that then fails to start is the
worst combination.
**What Handy does.** The Windows installer detects missing web-view and Visual C++ runtime
prerequisites and guides you, without blocking the installation. Separately, Windows builds
target a broad CPU baseline so they run on hardware that a natively-tuned build would crash on.
Note that the installer is not code-signed, so Windows will show a SmartScreen warning on first
run.
**Where.** No control — this is always active. *{Windows only}*
**Since.** 0.40.0.

### A hotkey another app already owns

<a id="a-hotkey-another-app-already-owns"></a>
**The situation.** You set a shortcut, nothing happens, and there is no explanation — some other
tool claimed it first.
**What Handy does.** Registration failures are recorded and surfaced when the app starts rather
than failing quietly. If the operating system's shortcut interface is the problem rather than the
combination, an alternative key backend can be selected.
**Where.** `Advanced › App › Keyboard Implementation = Handy Keys`.
**Since.** 0.30.0.

### Use it in your language

<a id="use-it-in-your-language"></a>
**The situation.** An English-only interface is a daily friction if English is not your language.
**What Handy does.** Seventeen interface languages with right-to-left support. Tray menu strings
are generated at build time from the same translation files as the interface, so they cannot fall
out of step.
**Where.** `About › Application Language`.
**Since.** Present since the fork's early releases.

### Diagnose without guessing

<a id="diagnose-without-guessing"></a>
**The situation.** Something is wrong and the only information you have is that it did not work.
**What Handy does.** A debug mode unlocks a page with the log level, direct links to the log and
data folders, the fuzzy-correction threshold, the paste timing knob, the always-on microphone
switch and the cancel shortcut. A command-line flag turns on verbose file logging without
touching anything else.
**Where.** Press `ctrl+shift+d` to reveal the `Debug` page; `CLI › handy --debug`.
**Since.** Present since the fork's early releases.

### Not a black box on first launch

<a id="not-a-black-box-on-first-launch"></a>
**The situation.** A fresh install with no model does nothing when you press the key, and nothing
tells you why.
**What Handy does.** A first-run flow walks through microphone permission, choosing and
downloading a model, and your first shortcut. An unconfigured remote engine no longer counts as
"a usable model exists", so a clean install cannot skip the download step and leave you with
nothing.
**Where.** Shown automatically on first launch.
**Since.** 0.1.6; the empty-install fix in 0.43.0.

---

## Conventions used in this file

Anchors are permanent. An entry's heading may be reworded, but the `id` under it never changes
and is never reused, so a link written today keeps working. Retired features keep their entry
with a note rather than disappearing.

Every other page links here with link text equal to the entry heading, character for character.
If you are editing the documentation and find yourself explaining what a feature does anywhere
except this file, that is the bug.
