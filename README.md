# Handy Tool

Handy Tool is a local-first dictation tool for Windows: press a key, speak, and the words land in the field you were working in, in the form that field expects.

## Download

Each link always fetches the newest release.

- **Windows** — [installer (.exe)][win], or with winget: `winget install patrick-1984.HandyTool`
- **Linux x64** _(untested)_ — [.rpm][lx-rpm] · [.deb][lx-deb] · [AppImage][lx-ai]
- **Linux ARM64** _(untested)_ — [.rpm][la-rpm] · [.deb][la-deb] · [AppImage][la-ai]

The Windows installer is not code-signed yet, so Windows warns the first time — [Install](#install)
says what to do. The [release page](https://github.com/patrick-1984/handy-tool/releases/latest) has
the release notes and every file.

[win]: https://github.com/patrick-1984/handy-tool/releases/latest/download/Handy-Tool-windows-x64-setup.exe
[lx-rpm]: https://github.com/patrick-1984/handy-tool/releases/latest/download/Handy-Tool-linux-x64.rpm
[lx-deb]: https://github.com/patrick-1984/handy-tool/releases/latest/download/Handy-Tool-linux-x64.deb
[lx-ai]: https://github.com/patrick-1984/handy-tool/releases/latest/download/Handy-Tool-linux-x64.AppImage
[la-rpm]: https://github.com/patrick-1984/handy-tool/releases/latest/download/Handy-Tool-linux-arm64.rpm
[la-deb]: https://github.com/patrick-1984/handy-tool/releases/latest/download/Handy-Tool-linux-arm64.deb
[la-ai]: https://github.com/patrick-1984/handy-tool/releases/latest/download/Handy-Tool-linux-arm64.AppImage

## Features

The pages you use most sit in the sidebar. The tools have an entry of their own, **More Tools**, and
the rest of the settings are one click away under **Advanced settings**. Every setting has one home,
[search finds any of them by name](docs/features.md#find-a-setting-by-typing-its-name), [Help mode](docs/features.md#help-mode) explains
whichever one you point at, and [What's new](docs/features.md#whats-new) takes you straight to each new setting.
This list follows the same layout ([how the pages are arranged](docs/features.md#six-pages-and-more)).

### In the sidebar

- **History** — [a crash mid-dictation costs you nothing](docs/features.md#a-crash-mid-dictation-costs-you-nothing);
  [find what you dictated last Tuesday](docs/features.md#what-did-i-dictate-last-tuesday), [hear what it heard](docs/features.md#hear-what-it-heard),
  and [see how much you have dictated](docs/features.md#how-much-have-i-dictated).
- **Notes** — [turn a recording into a note](docs/features.md#turn-a-recording-into-a-note),
  [one that says who said what](docs/features.md#a-note-that-says-who-said-what), [written your way, with a skill](docs/features.md#notes-written-your-way-with-a-skill),
  and [every note in one place](docs/features.md#every-note-in-one-place).
- **General** — the dictation itself. [Press one key, speak, and the text appears where you were typing](docs/features.md#press-one-key-and-speak),
  or [hold a key for a one-line thought](docs/features.md#hold-to-talk). [Watch the text appear, or wait for the most accurate pass](docs/features.md#live-or-post-recording);
  with the [live text box](docs/features.md#live-text-box) your words sit next to the recording pill while you talk, and you can
  [take back the last word](docs/features.md#undo-last-word) or [pause a take](docs/features.md#pause-a-take). Custom words
  [stop names and jargon coming back mangled](docs/features.md#names-and-jargon-stop-coming-back-mangled). Sound can
  [capture the other side of a call](docs/features.md#record-system-audio) (Windows); App sets the look of the pill and the
  window; Updates [lets you decide when to update](docs/features.md#updates-you-decide-to-take).
- **Setups** — guided walk-throughs where every step can be skipped: the first-start guide (your languages,
  a suggested model, shortcuts, microphone, a first try), [the look, with moving previews](docs/features.md#a-guided-setup-for-the-look),
  [the Jumper](docs/features.md#a-guided-setup-for-the-jumper) and [post-processing](docs/features.md#a-guided-setup-for-post-processing).
- **Transcription** — how the text is delivered: [a paste method for the app where Ctrl+V doesn't work](docs/features.md#ctrl-v-doesnt-work-in-that-app),
  [a clipboard that is left alone](docs/features.md#dictation-doesnt-steal-your-clipboard), [sending without reaching for Enter](docs/features.md#send-it-without-reaching-for-enter),
  [translation to English](docs/features.md#speak-any-language-get-english), and the timing that makes
  [a remote session (RDP, Citrix) paste the right thing](docs/features.md#your-remote-session-pastes-the-right-thing).
- **Shortcuts** — [every shortcut on one page](docs/features.md#every-shortcut-on-one-page) with conflict warnings;
  [any of them can be turned off](docs/features.md#turn-off-a-shortcut-you-dont-want), and
  [they don't eat the accented letters you type](docs/features.md#shortcuts-dont-eat-accented-letters).
  [Dictate and send in one keystroke](docs/features.md#dictate-and-send-in-one-keystroke), or
  [get the words back when a paste didn't land](docs/features.md#the-paste-didnt-land-get-the-words-back).
- **Models** — [pick the engine that fits the machine](docs/features.md#pick-the-engine-that-fits-the-machine): Whisper, Parakeet,
  Moonshine or SenseVoice on your own computer — [on the GPU, integrated graphics included](docs/features.md#gpu-acceleration-including-integrated-graphics)
  or [on a laptop's NPU](docs/features.md#use-the-npu-in-your-laptop) — or a remote speech endpoint.
  [Ratings on one scale](docs/features.md#model-ratings-on-one-scale) make them comparable.
- **Files** — [transcribe an audio file as accurately as the model can](docs/features.md#one-file-transcribed-as-accurately-as-the-model-can)
  (WAV, MP3, M4A, MP4 audio track, AAC, FLAC, Ogg Opus), and [a folder of recordings, transcribed while you sleep](docs/features.md#a-folder-of-recordings-transcribed-while-you-sleep).
- **Jumper** _(Windows)_ — [send the text where you were](docs/features.md#send-it-where-you-were) or
  [jump back to your draft](docs/features.md#jump-back-to-your-draft), from any window. [It never pastes blind](docs/features.md#it-never-pastes-blind)
  and [refuses to type into a password box](docs/features.md#it-refuses-to-dictate-into-a-password-box).
- **More Tools** — the tools, [listed below](#more-tools).
- **What's new** — what changed in the last releases, each with **Show me**.
- **About** — the update controls (check for a new version, silent updates), the version
  and licences. The version you are running is also shown at the bottom-left of the sidebar.

### Advanced settings

- **Transcription providers** — [any OpenAI-compatible speech endpoint](docs/features.md#point-it-at-any-openai-compatible-speech-endpoint) or
  [OpenRouter](docs/features.md#one-openrouter-key-many-speech-models) as a transcription engine.
- **LLM providers** — [configure an AI provider once and use it everywhere](docs/features.md#configure-a-provider-once-use-it-everywhere):
  post-processing, Token Count and Model Testing all use them.
- **Post-processing** — [a second key that cleans the text up with AI](docs/features.md#a-second-key-for-clean-this-up), with a prompt
  [your dictated words can't hijack](docs/features.md#your-dictated-words-cant-hijack-the-model).
- **MCP & CLI** — [let an agent drive the app](docs/features.md#let-an-agent-drive-the-app), or script it with
  [a `handy` command on your PATH](docs/features.md#a-handy-command-on-your-path).
- **Backup** — [one file that carries your whole setup](docs/features.md#one-file-that-carries-your-whole-setup).
- **Debug** — appears here once debug mode is on.

### More Tools

- **Keyboard Typer** — [when paste is blocked, type it instead](docs/features.md#when-paste-is-blocked-type-it-instead): virtual machines,
  remote consoles, password prompts.
- **Token Count** — [what a prompt will cost](docs/features.md#what-will-this-prompt-cost), across providers.
- **Model Testing** — [which model is actually better at your task](docs/features.md#which-model-is-actually-better-at-my-task),
  with a panel of judges, costs and timings.
- **Current Audio** — [the words arriving in a window you can park anywhere](docs/features.md#watch-the-words-arrive-in-a-window-you-can-park-anywhere).

Nothing leaves the machine unless you set it up to: [there are no calls you didn't ask for](docs/features.md#the-app-makes-no-calls-you-didnt-ask-for).
The [feature catalog](docs/features.md) has every capability, each explained by the problem it removes.

## Install

Every release is built and signed by GitHub Actions from the published source. The
[Download](#download) links above fetch the newest release; the same files, with the version in
their names, are on the [latest release](https://github.com/patrick-1984/handy-tool/releases/latest) page.

### Windows x64 — installer

Download the [installer][win] (`Handy.Tool_<version>_x64-setup.exe` on the release page) and run
it. It installs per-user, so no administrator rights are needed, and it can update itself in place
from then on — every update is signature-checked before it is applied.

> **Windows will warn you.** The installer is not signed with a code-signing certificate, so
> SmartScreen or Smart App Control will say the publisher is unknown — and on some machines Smart App
> Control blocks it outright. Choose **More info → Run anyway**. Signing is on the roadmap and will
> remove this.

**Portable use.** Handy Tool can run from a folder of its own and leave nothing behind
([how portable mode works](docs/features.md#run-it-from-a-usb-stick)). A ready-made portable ZIP is
not attached to releases at the moment (the last one shipped with 1.3.0); [Portable](docs/portable.md)
explains how to assemble one from a release build.

### Windows — winget

```powershell
winget install patrick-1984.HandyTool
```

The same per-user installer, from the winget community repository. A new version reaches winget
a little after each release, once Microsoft's review has passed; the app keeps itself up to date
either way.

### Linux x64 and ARM64 — untested

`.rpm`, `.deb` and `.AppImage` packages are attached to each release and linked under
[Download](#download). They are built by CI but have not been tested on a real machine.

Then, whichever route you took: grant microphone access and go through the short first-start setup:
your languages, one of three speech models suggested for them, your shortcuts and your microphone. A
fresh install has no model — the download starts only once you pick one, and models range from a few
hundred megabytes to several gigabytes.

## Built for your left hand

Moving between large screens and apps buried under other windows costs time. While your right hand stays on the mouse, your left hand travels to Enter, you look down, and the flow breaks again when you come back for another chord. Put one key per intent on a small programmable pad under your resting left hand: speak, submit, cancel, recover, or jump without hunting across the keyboard. “It’s almost a meme at this point — the dedicated Claude keyboard. Joke and not a joke.” The serious point is that fixed keys turn repeated multi-key sequences into movements you can make without looking. For simple tasks, plain keyboard shortcuts are completely fine. [Build the deck when the ordinary shortcuts start getting in your way](docs/start/08-the-deck.md).

## Defaults at a glance

[Defaults — what you get out of the box](docs/features.md#defaults) is the single day-one reference for the shipped shortcuts, clipboard handling, paste and submit keys, delivery delays, Escape behavior, recording retention, local storage, and network behavior. Read it before your first take if you want every keypress to be predictable.

## Platform status

| Platform            | Status                                                               |
| ------------------- | -------------------------------------------------------------------- |
| Windows x64         | Built, tested, and released                                          |
| macOS Intel         | **Not working yet.** Built by CI, but not usable yet.                |
| macOS Apple Silicon | **Not working yet.** Built by CI, but not usable yet.                |
| Linux x64 and ARM64 | **Untested.** Built and released by CI; not tested on real hardware. |

Every platform is compiled on GitHub's runners. The Linux packages ship with each release, but
nobody has yet granted them microphone and accessibility permission and dictated a sentence on a
real machine — treat them as a first cut, not a finished port. macOS is not ready to use yet, so it
has no download link.

The Jumper family, system-audio capture and portable mode are Windows-only by construction. For the
complete boundary, see [What runs today, and what is planned](docs/features.md#what-runs-today-and-what-is-planned).

## Documentation

- [Documentation hub](docs/README.md) — choose the learning path, a tool, or a reference page.
- [01 — Install and say your first words](docs/start/01-install.md) — the first rung of the learning path, which runs from the first model to your own working setup.
- [Feature catalog](docs/features.md) — look up each capability by the problem it removes.
- [Changelog](CHANGELOG.md) — the version-by-version record.
- [Build from source](BUILD.md) — prepare the development toolchain and build the application.

## Why it exists

It exists because of an escalation. “I type too slowly. My brain is faster than my hands.” You start dictating, and then the promotion arrives: “Wait — I can run more than one session.” Soon it is, “Actually, five sessions and two remote hosts.” Then the cost catches up with the throughput: “…and now I’m losing it.” The terminal you need is buried, your left hand keeps traveling to Enter, one thought lands in the wrong session, and a stray key threatens a long take. Handy Tool is the set of pieces that keeps that escalation from collapsing: capture the thought, preserve it, and deliver it to the work that needs it.

### A note from the author

- original Handy (tool) was my choice to go when I got stuck typing too slow for my ADHD brain - blame Claude Code and Codex for it!
  - big kudos to CJ Pais - oryginal author of the app when I took it over (for myself) in v. ...... 0.8 ? - man - thank you for your work!
- very quickly it become my number one app I use on Windows ... but you know how it is - always want more so I've added:
  - key typer tool to enter passwords on virtual machines in scenarios when policy prohibits copy-paste
  - VULKAN! support to use Whisper (large) model as fast as smaller and weaker models on my Intel and AMD iGPU's
  - Model testing: I wanted to see how my local and paid models solve the same challenge - and what's costs associated - for better cost estimation on agentic workloads in my job!
  - token count ... let me tell you that was a wake-up call - it is NOT as you would thought!
  - Translator - hey maybe I'll index all my transcripts? (I will) or I have extra audio/video folders I need auto transcript (I have) - this is sooo cool - love it
  - PTT mode - so you see as you speak - you translation - sometimes it's a life saver (sometimes only for me) :D
  - and JUMPER - this is a BIG one - it made config section to say at least ... bit complex. Hey but I'm an engineer (you too?) . Jumper is your TOOL to jump between windows (absolute or relative position to app / screen) , get back where you were .... man sooo many combinations. If you are on 2+ screens rig - this is your buddy :D One comment though - you DO need separate - left sided functions keyboard. I have REDRAGON K585 (should I say dinosaur ...?) but it hardcode wire keyboard mappings in flash on it - so I can live with its sooo lame programming UI
- yeah , there is backup, and fuckton of tweaks that makes it possible to work with RDP, Citrix, also most very flexible paste / jump / anchor system . Should also mention transcribe and submit mode. If you vibe code a lot - you are going to love it.
- and of course some stats for nerds like me + changes/fixes to UI + resilience to recordings + space saving + MCP/AI + more tweaks I remember - study features list - U R going to love it (if you are nerd like me) otherwise stick to defaults :D
- would forgot - it will also work on lame-ass rigs if you travel etc. - just plug openrouter etc. and you good

## License and lineage

Handy Tool is released under the [MIT License](LICENSE). It began as a fork of [cjpais/Handy](https://github.com/cjpais/Handy), created by CJ Pais, and is now developed independently. That upstream project and its contributors provided the foundation this repository builds on.

Make note with speakers downloads two third-party models on demand: pyannote segmentation 3.0 by Hervé Bredin and pyannote.audio (MIT License) and the WeSpeaker ResNet34 VoxCeleb model by the WeNet community (CC BY 4.0), both in the ONNX versions published by the [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) project.
