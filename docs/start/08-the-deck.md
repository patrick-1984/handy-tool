# 08 — The deck

**Platform:** Windows x64  
**Time:** about 30 minutes  
**Optional purchase:** a 6–12-key programmable pad

You reached this stage without buying hardware. The previous stage used buttons already on your desk. A dedicated pad is the summit for a high-volume workflow, not an entry fee; ordinary keyboard shortcuts remain completely fine.

## Why put another keyboard on the desk?

> My right hand is on the mouse, moving between big screens and apps stacked on those screens. My left hand has a long trip to Enter. I start dictating, break my flow to look down, move to Enter, then move back again for Ctrl+C or another chord. The keystroke is cheap; the glance down and finding my hand position again are not.

Put a small programmable pad under the resting left hand and give one physical key to each intent. The left hand stays home while the right keeps navigating. A capable multi-button mouse is still a valid version of the same idea, but it loads more work onto the hand that is already aiming.

> “It is almost like a meme with this Claude dedicated keyboard.” Joke, no joke.

The serious point is muscle memory: a stable key position replaces a chord, a glance, and the hand movement that follows.

## What to look for

A 6-, 9-, or 12-key pad is enough. Prefer firmware-backed remapping such as QMK/VIA, arbitrary chords, layers, real key-down/key-up events, enough weight not to slide, and a distinct switch under push-to-talk. Vendor software works, but it must be running; an OS remapper applies to every keyboard unless it can identify devices.

If the pad can emit F13–F24, you can bind those keys directly in Handy and avoid multi-modifier chords. If it cannot, program it to emit the existing shortcuts. Keep the normal keyboard bindings available as your fallback on another machine.

Weigh the *storage* of the mapping more heavily than the quality of the configuration app. Handy's own author runs a Redragon K585 — his description of its configuration software is "sooo lame" — and keeps it anyway, because that pad writes the key mappings into flash on the device. Program it once and the layout belongs to the keyboard: it survives reboots, it needs no background process, and it follows the hardware to another machine. A pad with a beautiful configurator that depends on a vendor service running is the worse buy, because the day that service does not start is the day your muscle memory types nothing.

## Six keys: the starter

The bottom row is home.

```text
┌────────────────┬────────────────┬────────────────┐
│ Cancel         │ Jump Hot 1     │ Paste Last     │
├────────────────┼────────────────┼────────────────┤
│ Transcribe     │ PTT hold       │ Transcribe     │
│ toggle         │                │ & Submit       │
└────────────────┴────────────────┴────────────────┘
```

Set Hot 1 from the normal `ctrl+alt+k` chord. The pad's jump key emits `ctrl+alt+j`. Put the heaviest or most tactile switch under PTT so you can find and hold it without looking.

## Nine keys: the daily deck

```text
┌────────────────┬────────────────┬────────────────┐
│ Post-process   │ Paste Last     │ Fn layer       │
├────────────────┼────────────────┼────────────────┤
│ Cancel         │ Jump Hot 1     │ Jump Hot 2     │
├────────────────┼────────────────┼────────────────┤
│ Transcribe     │ PTT hold       │ Transcribe     │
│ toggle         │                │ & Submit       │
└────────────────┴────────────────┴────────────────┘
```

While Fn is held, turn Jump Hot 1 into Set Anchor 1 and Jump Hot 2 into Set Anchor 2. Keep the recording row unchanged under Fn so a mistimed layer press cannot turn PTT into another action.

## Twelve keys: two fixed destinations

```text
┌────────────────┬────────────────┬────────────────┐
│ Post-process   │ Type Text      │ Fn layer       │
├────────────────┼────────────────┼────────────────┤
│ Paste Last     │ Slot 1         │ Slot 2         │
├────────────────┼────────────────┼────────────────┤
│ Cancel         │ Jump Hot 1     │ Jump Hot 2     │
├────────────────┼────────────────┼────────────────┤
│ Transcribe     │ PTT hold       │ Transcribe     │
│ toggle         │                │ & Submit       │
└────────────────┴────────────────┴────────────────┘
```

Under Fn, each jump becomes its matching set action: the two hot slots and static slots 1 and 2. Do not map all nine static slots until you can name nine targets you use often.

## Program it without creating new problems

- Windows reserves many `Win` chords and always owns `ctrl+alt+delete`. Do not build the deck around them.
- On European layouts, `ctrl+alt` can be AltGr. Handy's default `k`, `j`, `h`, `g`, and number combinations avoid the common letter collisions; bare F13–F24 avoid the issue entirely.
- Per-application profiles can change a pad key underneath your fingers. Use them deliberately and test the profile switches.
- Test delivery into an elevated administrator window before trusting a software-remapped device; Windows can block input from a lower-privilege process.
- Before you program a chord a second application may want: [A hotkey another app already owns](../features.md#a-hotkey-another-app-already-owns).
- Before you give a pad key to push-to-talk: [Hold a key for a one-line thought](../features.md#hold-to-talk).
- Before you macro a paste or type action: [The re-paste happens the moment you let go](../features.md#the-re-paste-happens-the-moment-you-let-go).
- Before you give a pad key to the second dictation intent: [Dictate and send in one keystroke](../features.md#dictate-and-send-in-one-keystroke).

Previous: [07 — The buttons you already own](07-mouse-buttons.md) · Next: [09 — Where next](09-where-next.md)

**You can stop here.** Your left hand now has a stable command deck, and the plain shortcuts still work when the deck is absent.
