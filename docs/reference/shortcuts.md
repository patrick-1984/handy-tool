# Shortcut reference

Handy Tool 1.0.0 ships for Windows x64. This table lists every action with a default binding in that release. Rebind an action in the app, then make your deck or mouse emit the configured chord.

“Suggested deck key” refers to the [6-, 9-, and 12-key layouts](../start/08-the-deck.md). Push-to-Talk is the only action that needs distinct key-down and key-up events. Most mouse and display-deck hotkeys emit a tap, so use Transcribe unless the device supports separate press and release events.

## Recording and delivery

Nothing in either table runs anywhere but Windows today. The `Survives to planned builds` column marks which actions would still exist in the planned macOS and Linux builds; the Jumper family would not.

| Bindable action | Default chord | What it does | Survives to planned builds | Suggested deck key |
| --- | --- | --- | --- | --- |
| Transcribe | `ctrl+space` | Toggles recording. | Yes | **Transcribe (toggle)** on every layout |
| Push-to-Talk | `ctrl+alt+space` | Records while held. | Yes | **PTT hold**, center key on every layout |
| Transcribe with Post-Processing | `ctrl+shift+space` | Toggles recording and runs the post-processing prompt. | Yes | **Post-Proc.** on 9/12 keys |
| Transcribe & Submit | `ctrl+alt+s` | Toggles recording, delivers, and sends the submit key. | Yes | **Transcribe & Submit** on every layout |
| Cancel | `escape` | Applies Cancel behavior to the running take. | Yes | **Cancel** on every layout |
| Type Text | `ctrl+alt+t` | Types the prepared Keyboard Typer text. | Yes | **Type Text** on 12 keys; Fn layer on 9 keys |
| Paste Last Transcription | `ctrl+alt+p` | Delivers the most recent transcription again. | Yes | **Paste Last** on every layout |

Rebind them at:

- `General › Transcribe Shortcut`
- `General › Push-to-Talk Shortcut`
- `Advanced › Transcription › Transcribe & Submit › Transcribe & Submit Shortcut`
- `Keyboard Typer › Type Text Shortcut`
- `General › Paste last transcription › Paste Last Transcription`
- `Post Process › Hotkey › Post-Processing Hotkey` *{requires: Post-processing enabled}*
- `Debug › Cancel Shortcut` *{requires: Debug mode}*

Catalog: [Press one key, speak, and the text appears where you were typing](../features.md#press-one-key-and-speak), [Push-to-talk you can trust](../features.md#push-to-talk-you-can-trust), [Dictate and send in one keystroke](../features.md#dictate-and-send-in-one-keystroke), [The paste didn't land — get the words back without re-dictating](../features.md#the-paste-didnt-land-get-the-words-back), [A second key for "clean this up with AI"](../features.md#a-second-key-for-clean-this-up), and [When paste is blocked, type it instead](../features.md#when-paste-is-blocked-type-it-instead).

## Jumper and slots

All 22 actions below are Windows-only. Set remembers the focused field; Jump returns focus to it. Hot 1 and Hot 2 are frequently changed destinations. Static slots 1–9 are longer-lived destinations.

| Bindable action | Default chord | What it does | Survives to planned builds | Suggested deck key |
| --- | --- | --- | --- | --- |
| Set Anchor | `ctrl+alt+k` | Stores the focused field as Hot 1. | No | **Fn + Jump Hot 1** on 9/12 keys; keyboard on 6 keys |
| Jump to Anchor | `ctrl+alt+j` | Focuses Hot 1 without delivery. | No | **Jump Hot 1** on every layout |
| Set Anchor 2 | `ctrl+alt+h` | Stores the focused field as Hot 2. | No | **Fn + Jump Hot 2** on 9/12 keys; keyboard on 6 keys |
| Jump to Anchor 2 | `ctrl+alt+g` | Focuses Hot 2 without delivery. | No | **Jump Hot 2** on 9/12 keys; keyboard on 6 keys |
| Set Jump Slot 1 | `ctrl+alt+shift+1` | Stores the focused field as static slot 1. | No | **Fn + Slot 1** on 12 keys |
| Jump to Slot 1 | `ctrl+alt+1` | Focuses static slot 1. | No | **Slot 1** on 12 keys |
| Set Jump Slot 2 | `ctrl+alt+shift+2` | Stores the focused field as static slot 2. | No | **Fn + Slot 2** on 12 keys |
| Jump to Slot 2 | `ctrl+alt+2` | Focuses static slot 2. | No | **Slot 2** on 12 keys |
| Set Jump Slot 3 | `ctrl+alt+shift+3` | Stores the focused field as static slot 3. | No | Optional layer; leave unassigned until needed |
| Jump to Slot 3 | `ctrl+alt+3` | Focuses static slot 3. | No | Optional layer; leave unassigned until needed |
| Set Jump Slot 4 | `ctrl+alt+shift+4` | Stores the focused field as static slot 4. | No | Optional layer; leave unassigned until needed |
| Jump to Slot 4 | `ctrl+alt+4` | Focuses static slot 4. | No | Optional layer; leave unassigned until needed |
| Set Jump Slot 5 | `ctrl+alt+shift+5` | Stores the focused field as static slot 5. | No | Optional layer; leave unassigned until needed |
| Jump to Slot 5 | `ctrl+alt+5` | Focuses static slot 5. | No | Optional layer; leave unassigned until needed |
| Set Jump Slot 6 | `ctrl+alt+shift+6` | Stores the focused field as static slot 6. | No | Optional layer; leave unassigned until needed |
| Jump to Slot 6 | `ctrl+alt+6` | Focuses static slot 6. | No | Optional layer; leave unassigned until needed |
| Set Jump Slot 7 | `ctrl+alt+shift+7` | Stores the focused field as static slot 7. | No | Optional layer; leave unassigned until needed |
| Jump to Slot 7 | `ctrl+alt+7` | Focuses static slot 7. | No | Optional layer; leave unassigned until needed |
| Set Jump Slot 8 | `ctrl+alt+shift+8` | Stores the focused field as static slot 8. | No | Optional layer; leave unassigned until needed |
| Jump to Slot 8 | `ctrl+alt+8` | Focuses static slot 8. | No | Optional layer; leave unassigned until needed |
| Set Jump Slot 9 | `ctrl+alt+shift+9` | Stores the focused field as static slot 9. | No | Optional layer; leave unassigned until needed |
| Jump to Slot 9 | `ctrl+alt+9` | Focuses static slot 9. | No | Optional layer; leave unassigned until needed |

The hot-slot bindings are at:

- `Jumper › Hot slot (Anchor & Deliver) › Set Anchor` *{Windows only}*
- `Jumper › Hot slot (Anchor & Deliver) › Jump to Anchor` *{Windows only}*
- `Jumper › Second hot slot (Anchor & Deliver) › Set Anchor 2` *{Windows only}*
- `Jumper › Second hot slot (Anchor & Deliver) › Jump to Anchor 2` *{Windows only}*

Static slots repeat the pair on each numbered card, for example `Jumper › Static slot 1 › Set Jump Slot 1` and `Jumper › Static slot 1 › Jump to Slot 1` *{Windows only}*.

Catalog: [Two live destinations at once](../features.md#two-live-destinations-at-once), [Nine memorised destinations](../features.md#nine-memorised-destinations), [Jump back to your draft without pasting anything](../features.md#jump-back-to-your-draft), and [It never pastes blind](../features.md#it-never-pastes-blind).

## Deck cautions

These seven cautions apply to any programmable pad, mouse, or macro keyboard. [08 — The deck](../start/08-the-deck.md) links here rather than repeating them.

- Put Set actions on a layer; a stray Set press replaces the destination.
- Keep Push-to-Talk on hardware with genuine press and release events. See [Push-to-talk you can trust](../features.md#push-to-talk-you-can-trust).
- Avoid Windows-key chords and reserved sequences. `ctrl+alt+delete` cannot be captured.
- On European layouts, `ctrl+alt` can act as AltGr. The shipped `k`, `j`, `h`, `g`, and digit choices avoid common AltGr characters.
- Rebind a chord another application owns. GPU overlays, laptop utilities, meeting clients, and clipboard managers are common conflicts. See [A hotkey another app already owns](../features.md#a-hotkey-another-app-already-owns).
- A macro that physically holds a chord holds up delivery. See [The re-paste happens the moment you let go](../features.md#the-re-paste-happens-the-moment-you-let-go) and [Your trigger chord doesn't become part of the text](../features.md#your-trigger-chord-doesnt-become-part-of-the-text).
- Keep keyboard defaults usable after programming a deck so you retain a fallback away from your desk.

The Cancel row above follows the `General › Cancel behavior` choice; see [Escape stops the delivery, not your words](../features.md#escape-stops-the-delivery-not-your-words).

The internal `test` action has no default binding and only writes a log line. It is not part of the bindable release surface.
