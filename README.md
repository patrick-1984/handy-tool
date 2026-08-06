# Handy Tool

Handy Tool 1.0.0 is a local-first dictation tool for Windows: press a key, speak, and the words land in the field you were working in, in the form that field expects.

It exists because of an escalation. “I type too slowly. My brain is faster than my hands.” You start dictating, and then the promotion arrives: “Wait — I can run more than one session.” Soon it is, “Actually, five sessions and two remote hosts.” Then the cost catches up with the throughput: “…and now I’m losing it.” The terminal you need is buried, your left hand keeps traveling to Enter, one thought lands in the wrong session, and a stray key threatens a long take. Handy Tool is the set of pieces that keeps that escalation from collapsing: capture the thought, preserve it, and deliver it to the work that needs it.

Note from current author: 
- original Handy (tool) was my choice to go when I got stuck typing too slow for  my ADHD brain - blame Claude Code and Codex for it!
-- big kudos to CJ Pais - oryginal author of the app when I took it over (for myself) in v. ...... 0.8 ? - man - thank you for your work!
- very quickly it become my number one app I use on Windows ... but you know how it is - always want more so I've added:
- key typer tool to enter passwords on virtual machines in scenarios when policy prohibits copy-paste
- VULKAN! support to use Whisper (large) model as fast as smaller and weaker models on my Intel and AMD iGPU's
- Model testing: I wanted to see how my local and paid models solve the same challenge - and what's costs associated - for better cost estimation on agentic workloads in my job!
- token count ... let me tell you that was a wake-up call - it is NOT as you would thought!
- Translator - hey maybe I'll index all my transcripts? (I will) or I have extra audio/video folders I need auto transcript (I have) - this is sooo cool - love it
- PTT mode - so you see as you speak - you translation - sometimes it's a life saver (sometimes only for me) :D
- and JUMPER - this is a BIG one - it made config section to say at least ... bit complex. Hey but I'm an engineer (you too?) . Jumper is your TOOL to jump between windows (absolute or relative position to app / screen) , get back where you were .... man sooo many combinations. If you are on 2+ screens rig - this is your buddy :D One comment though - you DO need separate - left sided functions keyboard. I have REDRAGON K585 (should I say dinosaur ...?) but it hardcode wire keyboard mappings in flash on it - so I can live with its sooo lame programming UI
- yeah , there is backup, and fuckton of tweaks that makes it possible to work with RDP, Citrix, also most very flexible paste / jump / anchor system . Should also mention transcribe and submit mode. If you vibe code a lot - you are going to love it.
- and of course some stats for nerds like me + changes/fixes to UI + resilience to recordings + space saving + more tweaks I remember - study features list - U R going to love it (if you are nerd like me) otherwise stick to defaults :D
- would forgot - it will also work on lame-ass rigs if you travel etc. - just plug openrouter etc. and you good

## At a glance

- [Press one key, speak, and the text appears where you were typing](docs/features.md#press-one-key-and-speak)
- [Dictate and send in one keystroke](docs/features.md#dictate-and-send-in-one-keystroke)
- [Send it where you were](docs/features.md#send-it-where-you-were)
- [Your remote session pastes the right thing](docs/features.md#your-remote-session-pastes-the-right-thing)
- [A crash mid-dictation costs you nothing](docs/features.md#a-crash-mid-dictation-costs-you-nothing)
- [Pick the engine that fits the machine](docs/features.md#pick-the-engine-that-fits-the-machine)
- [When paste is blocked, type it instead](docs/features.md#when-paste-is-blocked-type-it-instead)
- [Which model is actually better at my task?](docs/features.md#which-model-is-actually-better-at-my-task)
- [A folder of recordings, transcribed while you sleep](docs/features.md#a-folder-of-recordings-transcribed-while-you-sleep)
- [Let an agent drive the app](docs/features.md#let-an-agent-drive-the-app)

## Built for your left hand

Moving between large screens and apps buried under other windows costs time. While your right hand stays on the mouse, your left hand travels to Enter, you look down, and the flow breaks again when you come back for another chord. Put one key per intent on a small programmable pad under your resting left hand: speak, submit, cancel, recover, or jump without hunting across the keyboard. “It’s almost a meme at this point — the dedicated Claude keyboard. Joke and not a joke.” The serious point is that fixed keys turn repeated multi-key sequences into movements you can make without looking. For simple tasks, plain keyboard shortcuts are completely fine. [Build the deck when the ordinary shortcuts start getting in your way](docs/start/08-the-deck.md).

## Install

Handy Tool 1.0.0 is the first public release. It is distributed as a Windows x64 NSIS installer.

1. Download the Windows x64 `.exe` from the [latest release](https://github.com/patrick-1984/handy-tool/releases/latest).
2. Run the installer. It is currently unsigned, so Windows SmartScreen may show a warning. If you downloaded it from the repository above, select **More info**, verify the file you chose, then select **Run anyway**.
3. Complete the first-run onboarding and grant microphone access.
4. Choose and download a transcription model. A fresh installation has no model selected, and the download starts only after you choose one. Model downloads range from hundreds of megabytes to multiple gigabytes.

## Defaults at a glance

[Defaults — what you get out of the box](docs/features.md#defaults) is the single day-one reference for the shipped shortcuts, clipboard handling, paste and submit keys, delivery delays, Escape behavior, recording retention, local storage, and network behavior. Read it before your first take if you want every keypress to be predictable.

## Platform status

| Platform    | 1.0.0 status                                             |
| ----------- | -------------------------------------------------------- |
| Windows x64 | Built, tested, and released                              |
| macOS       | Planned — in the queue; no build is produced or released |
| Linux       | Planned — in the queue; no build is produced or released |

The Jumper family is Windows-only by construction. For the complete boundary, see [What runs today, and what is planned](docs/features.md#what-runs-today-and-what-is-planned).

## Documentation

- [Documentation hub](docs/README.md) — choose the learning path, a tool, or a reference page.
- [01 — Install and say your first words](docs/start/01-install.md) — the first rung of the learning path, which runs from the first model to your own working setup.
- [Feature catalog](docs/features.md) — look up each capability by the problem it removes.
- [Changelog](CHANGELOG.md) — the version-by-version record of the work that led to 1.0.0.
- [Build from source](BUILD.md) — prepare the development toolchain and build the application.

## License and lineage

Handy Tool is released under the [MIT License](LICENSE). It began as a fork of [cjpais/Handy](https://github.com/cjpais/Handy), created by CJ Pais, and is now developed independently. That upstream project and its contributors provided the foundation this repository builds on.
