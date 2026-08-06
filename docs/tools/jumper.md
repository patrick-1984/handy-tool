# Jumper

**Windows only.** Jumper is built on Windows focus APIs. The macOS and Linux builds are planned, not shipped, and Jumper will remain Windows-only.

## The moment

You start a prompt, then look across other screens and documentation while you keep talking. By the time you finish, the original code or prompt window is buried under everything else. One deliberate jump replaces the mouse hunt and takes you back to the field you marked.

## How it fits your day

Use Jumper when your attention moves but the destination of your words does not. Keep one or two changing destinations close at hand; add static slots only for fields you return to often enough to name.

## What it can do

- [Send it where you were](../features.md#send-it-where-you-were)
- [Jump back to your draft without pasting anything](../features.md#jump-back-to-your-draft)
- [Two live destinations at once](../features.md#two-live-destinations-at-once)
- [Nine memorised destinations](../features.md#nine-memorised-destinations)
- [Focus comes back to you](../features.md#focus-comes-back-to-you)
- [Decide what a jump does at the start and at the end of a take](../features.md#what-a-jump-does-at-the-start-and-end-of-a-take)
- [Anchors stay put, and can survive a restart](../features.md#anchors-stay-put-and-can-survive-a-restart)
- [Each destination remembers the cursor the way that app needs](../features.md#each-destination-remembers-the-cursor-the-way-that-app-needs)
- [The mouse goes back too](../features.md#the-mouse-goes-back-too)

## Settings that matter

- [Jumper settings](../reference/settings/jumper.md)
- [Advanced settings](../reference/settings/advanced.md)

## When it goes wrong

- [It never pastes blind](../features.md#it-never-pastes-blind)
- [A recycled window handle can't hijack your anchor](../features.md#a-recycled-window-handle-cant-hijack-your-anchor)
- [It refuses to dictate into a password box](../features.md#it-refuses-to-dictate-into-a-password-box)
- [Check a destination before you trust it](../features.md#check-a-destination-before-you-trust-it)

## Set it up

1. Put the cursor in your main destination and capture it with `Jumper › Hot slot (Anchor & Deliver) › Set Anchor = ctrl+alt+k` *{Windows only}*.
2. Leave that window, then return to the captured field with `Jumper › Hot slot (Anchor & Deliver) › Jump to Anchor = ctrl+alt+j` *{Windows only}*.
3. If the mouse position matters inside that application, enable `Jumper › Hot slot (Anchor & Deliver) › Save mouse position = On` *{Windows only}*.
4. For a target that moves with its window, choose `Jumper › Hot slot (Anchor & Deliver) › Cursor position mode = App-relative (follows the window)` *{Windows only}*.
5. After the hot slot behaves as expected, keep it for future launches at `Jumper › Persistence › Remember slots across restarts = On` *{Windows only}*.
