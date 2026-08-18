# Remote desktops

## The moment

The paste is aimed at your RDP or Citrix window, but that window was activated a heartbeat ago and is still settling. Nothing arrives, or the session pastes what you copied ten minutes earlier. The waits around activation, paste, submit, and clipboard restoration exist because that race is real.

## How it fits your day

Tune remote delivery only after the same flow works in a local application. Your local paths can stay fast while remote targets get enough time to accept focus, fetch the clipboard, and receive the submit key.

## What it can do

- [Your remote session pastes the right thing](../features.md#your-remote-session-pastes-the-right-thing)
- [Separate timing for remote desktops and local apps](../features.md#separate-timing-for-remote-desktops-and-local-apps)
- [Handy knows which of your windows is a remote session](../features.md#handy-knows-which-of-your-windows-is-a-remote-session)
- [Your dictation stays out of the remote machine's clipboard](../features.md#your-dictation-stays-out-of-the-remote-machines-clipboard)
- [Type it instead, when the console refuses a paste](../features.md#type-it-instead-when-the-console-refuses-a-paste)
- [The paste is swallowed right after a jump](../features.md#the-paste-is-swallowed-right-after-a-jump)
- [The paste delay applies to plain dictation too](../features.md#the-paste-delay-applies-to-plain-dictation-too)
- [The Enter key lands in the remote window](../features.md#the-enter-key-lands-in-the-remote-window)

## Settings that matter

- [Advanced settings](../reference/settings/advanced.md)
- [Jumper settings](../reference/settings/jumper.md)

## When it goes wrong

- [When delivery can't be verified, the text is still recoverable](../features.md#when-delivery-cant-be-verified-the-text-is-parked)
- [The app stays responsive during a two-second remote delivery](../features.md#the-app-stays-responsive-during-a-two-second-remote-delivery)
- [The paste didn't land — get the words back without re-dictating](../features.md#the-paste-didnt-land-get-the-words-back)

## Set it up

1. Confirm that the remote client is recognized at `Jumper › Remote desktop detection › Remote match strings` *{Windows only}*.
2. Give the activated remote target time before a plain paste at `Advanced › Transcription › Transcribe › Paste delay after jump (Windows) = 600 ms` *{Windows only}*.
3. Give Citrix or RDP time to fetch the clipboard at `Advanced › Transcription › Transcribe › Clipboard restore delay = 1 s`.
4. For the sending flow, delay its paste at `Advanced › Transcription › Transcribe & Submit › Paste delay after jump (Windows) = 600 ms` *{Windows only}*.
5. Delay its submit separately at `Advanced › Transcription › Transcribe & Submit › Submit delay before Enter (Windows) = 600 ms` *{Windows only}*.
6. Test with harmless text, then shorten or lengthen one delay at a time until the target is reliable.
