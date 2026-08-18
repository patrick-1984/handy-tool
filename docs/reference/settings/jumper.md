# Jumper settings

The Jumper is Windows-only in code. Open `Jumper`; every control on this page carries *{Windows only}*.

## Remote desktop detection

<a id="remote-match-strings"></a>
### Remote match strings

`Jumper › Remote desktop detection › Remote match strings` *{Windows only}*

Edits the case-insensitive substrings used to classify a target by application, window class, or control class. The result selects the Remote timing values in [Advanced](advanced.md#transcribe-paste-delay-after-jump-windows). **Default:** `msrdc`, `mstsc`, `Citrix`.

Catalog: [Handy knows which of your windows is a remote session](../../features.md#handy-knows-which-of-your-windows-is-a-remote-session).

### Type into remote desktops instead of pasting

`Jumper › Remote desktop detection › Type into remote desktops instead of pasting` *{Windows only}*

Uses batched simulated keystrokes instead of the configured paste method when the current target
matches [Remote match strings](#remote-match-strings). Turning it off returns remote targets to
the configured paste method. Has no effect under
`Advanced › Clipboard Handling = Copy to Clipboard`, which deliberately leaves the transcript on
your clipboard after delivery — redirection carries it to the remote machine regardless, so the
paste is kept. Also has no effect on a transcript containing line breaks: typing sends each line
break as an Enter key, which submits in a form and sends in a chat box, so multi-line transcripts
keep the configured paste method. **Default:** On.

Catalog: [Your dictation stays out of the remote machine's clipboard](../../features.md#your-dictation-stays-out-of-the-remote-machines-clipboard).

## Hot slot (Anchor & Deliver)

### Set Anchor

`Jumper › Hot slot (Anchor & Deliver) › Set Anchor` *{Windows only}*

Sets the shortcut that captures the focused field as Hot 1. **Default:** `ctrl+alt+k`.

Catalog: [Send it where you were](../../features.md#send-it-where-you-were).

### Jump to Anchor

`Jumper › Hot slot (Anchor & Deliver) › Jump to Anchor` *{Windows only}*

Sets the shortcut that focuses Hot 1 without delivering text. **Default:** `ctrl+alt+j`.

Catalog: [Jump back to your draft without pasting anything](../../features.md#jump-back-to-your-draft).

### Current anchor

`Jumper › Hot slot (Anchor & Deliver) › Current anchor` *{Windows only}*

Shows Hot 1's current target and exposes `Test` and `Clear` when occupied. **Default:** `No anchor set`.

Catalog: [Check a destination before you trust it](../../features.md#check-a-destination-before-you-trust-it).

<a id="hot-1-save-mouse-position"></a>
### Save mouse position

`Jumper › Hot slot (Anchor & Deliver) › Save mouse position` *{Windows only}*

Stores and restores the pointer with Hot 1. It enables [Cursor position mode](#hot-1-cursor-position-mode). **Default:** Off.

Catalog: [The mouse goes back too](../../features.md#the-mouse-goes-back-too).

<a id="hot-1-cursor-position-mode"></a>
### Cursor position mode

`Jumper › Hot slot (Anchor & Deliver) › Cursor position mode` *{Windows only}*

Chooses window-relative or fixed-screen coordinates for Hot 1 and is disabled while [Save mouse position](#hot-1-save-mouse-position) is off. **Default:** `App-relative (follows the window)`.

Catalog: [Each destination remembers the cursor the way that app needs](../../features.md#each-destination-remembers-the-cursor-the-way-that-app-needs).

### Delivery options

`Jumper › Hot slot (Anchor & Deliver) › Delivery options` *{Windows only}*

This read-only signpost points to the Transcribe and Transcribe & Submit recipes on `Advanced › Transcription`. **Default:** not applicable.

Catalog: [Decide what a jump does at the start and at the end of a take](../../features.md#what-a-jump-does-at-the-start-and-end-of-a-take).

## Second hot slot (Anchor & Deliver)

### Set Anchor 2

`Jumper › Second hot slot (Anchor & Deliver) › Set Anchor 2` *{Windows only}*

Sets the shortcut that captures the focused field as Hot 2. **Default:** `ctrl+alt+h`.

Catalog: [Two live destinations at once](../../features.md#two-live-destinations-at-once).

### Jump to Anchor 2

`Jumper › Second hot slot (Anchor & Deliver) › Jump to Anchor 2` *{Windows only}*

Sets the shortcut that focuses Hot 2 without delivering text. **Default:** `ctrl+alt+g`.

Catalog: [Two live destinations at once](../../features.md#two-live-destinations-at-once).

### Current anchor

`Jumper › Second hot slot (Anchor & Deliver) › Current anchor` *{Windows only}*

Shows Hot 2's target and exposes `Test` and `Clear` when occupied. **Default:** `No anchor set`.

Catalog: [Check a destination before you trust it](../../features.md#check-a-destination-before-you-trust-it).

<a id="hot-2-save-mouse-position"></a>
### Save mouse position

`Jumper › Second hot slot (Anchor & Deliver) › Save mouse position` *{Windows only}*

Stores and restores the pointer with Hot 2. It enables [Cursor position mode](#hot-2-cursor-position-mode). **Default:** Off.

Catalog: [The mouse goes back too](../../features.md#the-mouse-goes-back-too).

<a id="hot-2-cursor-position-mode"></a>
### Cursor position mode

`Jumper › Second hot slot (Anchor & Deliver) › Cursor position mode` *{Windows only}*

Chooses window-relative or fixed-screen coordinates for Hot 2. **Default:** `App-relative (follows the window)`.

Catalog: [Each destination remembers the cursor the way that app needs](../../features.md#each-destination-remembers-the-cursor-the-way-that-app-needs).

## Persistence

### Remember slots across restarts

`Jumper › Persistence › Remember slots across restarts` *{Windows only}*

Persists identities for all eleven slots and re-resolves them against live windows after launch. **Default:** Off.

Catalog: [Anchors stay put, and can survive a restart](../../features.md#anchors-stay-put-and-can-survive-a-restart).

## On-finish behavior

### Only jump on finish if started the same way

`Jumper › On-finish behavior › Only jump on finish if started the same way` *{Windows only}*

Requires the starting and finishing flow to match before its on-finish Jumper action fires. The submit or paste itself is unaffected. **Default:** Off.

Catalog: [No surprise teleports when you mix shortcuts](../../features.md#no-surprise-teleports-when-you-mix-shortcuts).

## Static slot 1

### Set Jump Slot 1

`Jumper › Static slot 1 › Set Jump Slot 1` *{Windows only}*

Sets the shortcut that captures the focused field as static slot 1. **Default:** `ctrl+alt+shift+1`.

Catalog: [Nine memorised destinations](../../features.md#nine-memorised-destinations).

### Jump to Slot 1

`Jumper › Static slot 1 › Jump to Slot 1` *{Windows only}*

Sets the shortcut that focuses static slot 1 without delivering text. **Default:** `ctrl+alt+1`.

Catalog: [Nine memorised destinations](../../features.md#nine-memorised-destinations).

<a id="slot-1-target"></a>
### Slot 1 target

`Jumper › Static slot 1 › Slot 1 target` *{Windows only}*

Shows the slot's current target and exposes `Test` and `Clear` when occupied. **Default:** no target.

Catalog: [Check a destination before you trust it](../../features.md#check-a-destination-before-you-trust-it).

<a id="slot-1-save-mouse-position"></a>
### Save mouse position

`Jumper › Static slot 1 › Save mouse position` *{Windows only}*

Stores and restores the pointer with this slot. It enables [Cursor position mode](#slot-1-cursor-position-mode). **Default:** Off.

Catalog: [The mouse goes back too](../../features.md#the-mouse-goes-back-too).

<a id="slot-1-cursor-position-mode"></a>
### Cursor position mode

`Jumper › Static slot 1 › Cursor position mode` *{Windows only}*

Chooses window-relative or fixed-screen coordinates for this slot. **Default:** `App-relative (follows the window)`.

Catalog: [Each destination remembers the cursor the way that app needs](../../features.md#each-destination-remembers-the-cursor-the-way-that-app-needs).

## Static slot 2

### Set Jump Slot 2

`Jumper › Static slot 2 › Set Jump Slot 2` *{Windows only}*

Sets the shortcut that captures the focused field as static slot 2. **Default:** `ctrl+alt+shift+2`.

Catalog: [Nine memorised destinations](../../features.md#nine-memorised-destinations).

### Jump to Slot 2

`Jumper › Static slot 2 › Jump to Slot 2` *{Windows only}*

Sets the shortcut that focuses static slot 2 without delivering text. **Default:** `ctrl+alt+2`.

Catalog: [Nine memorised destinations](../../features.md#nine-memorised-destinations).

<a id="slot-2-target"></a>
### Slot 2 target

`Jumper › Static slot 2 › Slot 2 target` *{Windows only}*

Shows the slot's current target and exposes `Test` and `Clear` when occupied. **Default:** no target.

Catalog: [Check a destination before you trust it](../../features.md#check-a-destination-before-you-trust-it).

<a id="slot-2-save-mouse-position"></a>
### Save mouse position

`Jumper › Static slot 2 › Save mouse position` *{Windows only}*

Stores and restores the pointer with this slot. It enables [Cursor position mode](#slot-2-cursor-position-mode). **Default:** Off.

Catalog: [The mouse goes back too](../../features.md#the-mouse-goes-back-too).

<a id="slot-2-cursor-position-mode"></a>
### Cursor position mode

`Jumper › Static slot 2 › Cursor position mode` *{Windows only}*

Chooses window-relative or fixed-screen coordinates for this slot. **Default:** `App-relative (follows the window)`.

Catalog: [Each destination remembers the cursor the way that app needs](../../features.md#each-destination-remembers-the-cursor-the-way-that-app-needs).

## Static slot 3

### Set Jump Slot 3

`Jumper › Static slot 3 › Set Jump Slot 3` *{Windows only}*

Sets the shortcut that captures the focused field as static slot 3. **Default:** `ctrl+alt+shift+3`.

Catalog: [Nine memorised destinations](../../features.md#nine-memorised-destinations).

### Jump to Slot 3

`Jumper › Static slot 3 › Jump to Slot 3` *{Windows only}*

Sets the shortcut that focuses static slot 3 without delivering text. **Default:** `ctrl+alt+3`.

Catalog: [Nine memorised destinations](../../features.md#nine-memorised-destinations).

<a id="slot-3-target"></a>
### Slot 3 target

`Jumper › Static slot 3 › Slot 3 target` *{Windows only}*

Shows the slot's current target and exposes `Test` and `Clear` when occupied. **Default:** no target.

Catalog: [Check a destination before you trust it](../../features.md#check-a-destination-before-you-trust-it).

<a id="slot-3-save-mouse-position"></a>
### Save mouse position

`Jumper › Static slot 3 › Save mouse position` *{Windows only}*

Stores and restores the pointer with this slot. It enables [Cursor position mode](#slot-3-cursor-position-mode). **Default:** Off.

Catalog: [The mouse goes back too](../../features.md#the-mouse-goes-back-too).

<a id="slot-3-cursor-position-mode"></a>
### Cursor position mode

`Jumper › Static slot 3 › Cursor position mode` *{Windows only}*

Chooses window-relative or fixed-screen coordinates for this slot. **Default:** `App-relative (follows the window)`.

Catalog: [Each destination remembers the cursor the way that app needs](../../features.md#each-destination-remembers-the-cursor-the-way-that-app-needs).

## Static slot 4

### Set Jump Slot 4

`Jumper › Static slot 4 › Set Jump Slot 4` *{Windows only}*

Sets the shortcut that captures the focused field as static slot 4. **Default:** `ctrl+alt+shift+4`.

Catalog: [Nine memorised destinations](../../features.md#nine-memorised-destinations).

### Jump to Slot 4

`Jumper › Static slot 4 › Jump to Slot 4` *{Windows only}*

Sets the shortcut that focuses static slot 4 without delivering text. **Default:** `ctrl+alt+4`.

Catalog: [Nine memorised destinations](../../features.md#nine-memorised-destinations).

<a id="slot-4-target"></a>
### Slot 4 target

`Jumper › Static slot 4 › Slot 4 target` *{Windows only}*

Shows the slot's current target and exposes `Test` and `Clear` when occupied. **Default:** no target.

Catalog: [Check a destination before you trust it](../../features.md#check-a-destination-before-you-trust-it).

<a id="slot-4-save-mouse-position"></a>
### Save mouse position

`Jumper › Static slot 4 › Save mouse position` *{Windows only}*

Stores and restores the pointer with this slot. It enables [Cursor position mode](#slot-4-cursor-position-mode). **Default:** Off.

Catalog: [The mouse goes back too](../../features.md#the-mouse-goes-back-too).

<a id="slot-4-cursor-position-mode"></a>
### Cursor position mode

`Jumper › Static slot 4 › Cursor position mode` *{Windows only}*

Chooses window-relative or fixed-screen coordinates for this slot. **Default:** `App-relative (follows the window)`.

Catalog: [Each destination remembers the cursor the way that app needs](../../features.md#each-destination-remembers-the-cursor-the-way-that-app-needs).

## Static slot 5

### Set Jump Slot 5

`Jumper › Static slot 5 › Set Jump Slot 5` *{Windows only}*

Sets the shortcut that captures the focused field as static slot 5. **Default:** `ctrl+alt+shift+5`.

Catalog: [Nine memorised destinations](../../features.md#nine-memorised-destinations).

### Jump to Slot 5

`Jumper › Static slot 5 › Jump to Slot 5` *{Windows only}*

Sets the shortcut that focuses static slot 5 without delivering text. **Default:** `ctrl+alt+5`.

Catalog: [Nine memorised destinations](../../features.md#nine-memorised-destinations).

<a id="slot-5-target"></a>
### Slot 5 target

`Jumper › Static slot 5 › Slot 5 target` *{Windows only}*

Shows the slot's current target and exposes `Test` and `Clear` when occupied. **Default:** no target.

Catalog: [Check a destination before you trust it](../../features.md#check-a-destination-before-you-trust-it).

<a id="slot-5-save-mouse-position"></a>
### Save mouse position

`Jumper › Static slot 5 › Save mouse position` *{Windows only}*

Stores and restores the pointer with this slot. It enables [Cursor position mode](#slot-5-cursor-position-mode). **Default:** Off.

Catalog: [The mouse goes back too](../../features.md#the-mouse-goes-back-too).

<a id="slot-5-cursor-position-mode"></a>
### Cursor position mode

`Jumper › Static slot 5 › Cursor position mode` *{Windows only}*

Chooses window-relative or fixed-screen coordinates for this slot. **Default:** `App-relative (follows the window)`.

Catalog: [Each destination remembers the cursor the way that app needs](../../features.md#each-destination-remembers-the-cursor-the-way-that-app-needs).

## Static slot 6

### Set Jump Slot 6

`Jumper › Static slot 6 › Set Jump Slot 6` *{Windows only}*

Sets the shortcut that captures the focused field as static slot 6. **Default:** `ctrl+alt+shift+6`.

Catalog: [Nine memorised destinations](../../features.md#nine-memorised-destinations).

### Jump to Slot 6

`Jumper › Static slot 6 › Jump to Slot 6` *{Windows only}*

Sets the shortcut that focuses static slot 6 without delivering text. **Default:** `ctrl+alt+6`.

Catalog: [Nine memorised destinations](../../features.md#nine-memorised-destinations).

<a id="slot-6-target"></a>
### Slot 6 target

`Jumper › Static slot 6 › Slot 6 target` *{Windows only}*

Shows the slot's current target and exposes `Test` and `Clear` when occupied. **Default:** no target.

Catalog: [Check a destination before you trust it](../../features.md#check-a-destination-before-you-trust-it).

<a id="slot-6-save-mouse-position"></a>
### Save mouse position

`Jumper › Static slot 6 › Save mouse position` *{Windows only}*

Stores and restores the pointer with this slot. It enables [Cursor position mode](#slot-6-cursor-position-mode). **Default:** Off.

Catalog: [The mouse goes back too](../../features.md#the-mouse-goes-back-too).

<a id="slot-6-cursor-position-mode"></a>
### Cursor position mode

`Jumper › Static slot 6 › Cursor position mode` *{Windows only}*

Chooses window-relative or fixed-screen coordinates for this slot. **Default:** `App-relative (follows the window)`.

Catalog: [Each destination remembers the cursor the way that app needs](../../features.md#each-destination-remembers-the-cursor-the-way-that-app-needs).

## Static slot 7

### Set Jump Slot 7

`Jumper › Static slot 7 › Set Jump Slot 7` *{Windows only}*

Sets the shortcut that captures the focused field as static slot 7. **Default:** `ctrl+alt+shift+7`.

Catalog: [Nine memorised destinations](../../features.md#nine-memorised-destinations).

### Jump to Slot 7

`Jumper › Static slot 7 › Jump to Slot 7` *{Windows only}*

Sets the shortcut that focuses static slot 7 without delivering text. **Default:** `ctrl+alt+7`.

Catalog: [Nine memorised destinations](../../features.md#nine-memorised-destinations).

<a id="slot-7-target"></a>
### Slot 7 target

`Jumper › Static slot 7 › Slot 7 target` *{Windows only}*

Shows the slot's current target and exposes `Test` and `Clear` when occupied. **Default:** no target.

Catalog: [Check a destination before you trust it](../../features.md#check-a-destination-before-you-trust-it).

<a id="slot-7-save-mouse-position"></a>
### Save mouse position

`Jumper › Static slot 7 › Save mouse position` *{Windows only}*

Stores and restores the pointer with this slot. It enables [Cursor position mode](#slot-7-cursor-position-mode). **Default:** Off.

Catalog: [The mouse goes back too](../../features.md#the-mouse-goes-back-too).

<a id="slot-7-cursor-position-mode"></a>
### Cursor position mode

`Jumper › Static slot 7 › Cursor position mode` *{Windows only}*

Chooses window-relative or fixed-screen coordinates for this slot. **Default:** `App-relative (follows the window)`.

Catalog: [Each destination remembers the cursor the way that app needs](../../features.md#each-destination-remembers-the-cursor-the-way-that-app-needs).

## Static slot 8

### Set Jump Slot 8

`Jumper › Static slot 8 › Set Jump Slot 8` *{Windows only}*

Sets the shortcut that captures the focused field as static slot 8. **Default:** `ctrl+alt+shift+8`.

Catalog: [Nine memorised destinations](../../features.md#nine-memorised-destinations).

### Jump to Slot 8

`Jumper › Static slot 8 › Jump to Slot 8` *{Windows only}*

Sets the shortcut that focuses static slot 8 without delivering text. **Default:** `ctrl+alt+8`.

Catalog: [Nine memorised destinations](../../features.md#nine-memorised-destinations).

<a id="slot-8-target"></a>
### Slot 8 target

`Jumper › Static slot 8 › Slot 8 target` *{Windows only}*

Shows the slot's current target and exposes `Test` and `Clear` when occupied. **Default:** no target.

Catalog: [Check a destination before you trust it](../../features.md#check-a-destination-before-you-trust-it).

<a id="slot-8-save-mouse-position"></a>
### Save mouse position

`Jumper › Static slot 8 › Save mouse position` *{Windows only}*

Stores and restores the pointer with this slot. It enables [Cursor position mode](#slot-8-cursor-position-mode). **Default:** Off.

Catalog: [The mouse goes back too](../../features.md#the-mouse-goes-back-too).

<a id="slot-8-cursor-position-mode"></a>
### Cursor position mode

`Jumper › Static slot 8 › Cursor position mode` *{Windows only}*

Chooses window-relative or fixed-screen coordinates for this slot. **Default:** `App-relative (follows the window)`.

Catalog: [Each destination remembers the cursor the way that app needs](../../features.md#each-destination-remembers-the-cursor-the-way-that-app-needs).

## Static slot 9

### Set Jump Slot 9

`Jumper › Static slot 9 › Set Jump Slot 9` *{Windows only}*

Sets the shortcut that captures the focused field as static slot 9. **Default:** `ctrl+alt+shift+9`.

Catalog: [Nine memorised destinations](../../features.md#nine-memorised-destinations).

### Jump to Slot 9

`Jumper › Static slot 9 › Jump to Slot 9` *{Windows only}*

Sets the shortcut that focuses static slot 9 without delivering text. **Default:** `ctrl+alt+9`.

Catalog: [Nine memorised destinations](../../features.md#nine-memorised-destinations).

<a id="slot-9-target"></a>
### Slot 9 target

`Jumper › Static slot 9 › Slot 9 target` *{Windows only}*

Shows the slot's current target and exposes `Test` and `Clear` when occupied. **Default:** no target.

Catalog: [Check a destination before you trust it](../../features.md#check-a-destination-before-you-trust-it).

<a id="slot-9-save-mouse-position"></a>
### Save mouse position

`Jumper › Static slot 9 › Save mouse position` *{Windows only}*

Stores and restores the pointer with this slot. It enables [Cursor position mode](#slot-9-cursor-position-mode). **Default:** Off.

Catalog: [The mouse goes back too](../../features.md#the-mouse-goes-back-too).

<a id="slot-9-cursor-position-mode"></a>
### Cursor position mode

`Jumper › Static slot 9 › Cursor position mode` *{Windows only}*

Chooses window-relative or fixed-screen coordinates for this slot. **Default:** `App-relative (follows the window)`.

Catalog: [Each destination remembers the cursor the way that app needs](../../features.md#each-destination-remembers-the-cursor-the-way-that-app-needs).

