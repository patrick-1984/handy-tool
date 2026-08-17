# Transcribe & Submit

## The moment

Sometimes you want the text so you can inspect it. Sometimes you already trust what you said, it is good enough, and moving your hand to Enter is wasted motion. With a separate intent for each case, you can keep reading a solution while a finished thought goes to an LLM worker. Losing focus for a second is acceptable; hunting for the buried window is not.

## How it fits your day

Reserve this flow for fields where a submit key has a predictable meaning. Keep ordinary transcription for drafts and use this second shortcut when the next action is already decided.

## What it can do

- [Dictate and send in one keystroke](../features.md#dictate-and-send-in-one-keystroke)
- [Enter, Ctrl+Enter, or Super+Enter](../features.md#enter-ctrl-enter-or-super-enter)
- [It finishes a take you started with another key](../features.md#it-finishes-a-take-you-started-with-another-key)
- [The Enter key lands in the remote window](../features.md#the-enter-key-lands-in-the-remote-window)
- [Its own paste method, for the one app that needs it](../features.md#its-own-paste-method-for-the-one-app-that-needs-it)
- [Its own clipboard policy](../features.md#its-own-clipboard-policy)
- [Decide what a jump does at the start and at the end of a take](../features.md#what-a-jump-does-at-the-start-and-end-of-a-take)
- [Focus comes back to you](../features.md#focus-comes-back-to-you)

## Settings that matter

- [Advanced settings](../reference/settings/advanced.md)

## When it goes wrong

- [Pressing it when nothing is recording](../features.md#pressing-it-when-nothing-is-recording)
- [The paste didn't land — get the words back without re-dictating](../features.md#the-paste-didnt-land-get-the-words-back)
- [When delivery can't be verified, the text is still recoverable](../features.md#when-delivery-cant-be-verified-the-text-is-parked)

## Set it up

1. Give the flow its own binding at `Advanced › Transcription › Transcribe & Submit › Transcribe & Submit Shortcut = ctrl+alt+s`.
2. Match the target field at `Advanced › Transcription › Transcribe & Submit › Paste method = Clipboard (Ctrl+V)`.
3. Choose what sends in that target at `Advanced › Transcription › Transcribe & Submit › Submit key = Enter`.
4. Make an idle press begin a take at `Advanced › Transcription › Transcribe & Submit › When no recording is active = Start a recording`.
5. If you use the Windows Jumper, route the finish at `Advanced › Transcription › Transcribe & Submit › Jump slot action on finish = Jump / deliver to slot` *{Windows only}*.
6. Select `Hot 1` in the adjacent destination list for `Advanced › Transcription › Transcribe & Submit › Jump slot action on finish` *{Windows only}*.
7. Return to what you were reading with `Advanced › Transcription › Transcribe & Submit › Return focus after delivery = On` *{Windows only}*.
