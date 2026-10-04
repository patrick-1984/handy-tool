# Shortcuts settings

Open `Shortcuts`. Every row here is the same control as on the feature page that owns it —
changing a shortcut in either place changes both. For the default chords, see
[the shortcut reference](../shortcuts.md).

### Shortcut Keeper

`Shortcuts › Shortcut Keeper › Keep chosen shortcuts on this PC in Remote Desktop (Windows)`

While a Remote Desktop Connection or Windows App session has the keyboard, the shortcuts ticked
with `Keep on this PC` run Handy Tool on this PC instead of reaching the remote; every other key
still goes to the remote. With the switch on, every row on this page gets a `Keep on this PC`
box. A box is greyed out with an exclamation mark naming the reason when that shortcut cannot be kept
(it acts while the key is held, uses the Windows key or Alt without Ctrl, is reserved by Windows,
or uses a key outside the supported set), and a ticked one shows a question mark when keeping it may have a
side effect in the remote. `I type Chinese, Japanese or Korean` (default Off) decides whether a
ticked Ctrl+Space shows its input-method note. A status line below the switch says whether a session has the
keyboard now. It needs the default keyboard implementation; after the other one ran, restart
Handy Tool. _{Windows only}_ **Default:** Off, and no shortcut ticked.

Catalog: [Your shortcuts work inside a Remote Desktop session](../../features.md#your-shortcuts-work-inside-a-remote-desktop-session).

### Dictation

`Shortcuts › Dictation`

Transcribe, Push-to-Talk, Transcribe & Submit, Paste Last Transcription and Cancel. The
Post-Processing Hotkey is listed while post-processing is on, and Pause / Resume and Undo Last
Word while their options on General are on. Live Text Box On/Off is always listed. Cancel, Pause / Resume and Undo Last
Word are not listed on Linux, where they are never registered. Cycle Sound Source (no default key) is listed on Windows
only. _{Windows only}_

Catalog: [Every shortcut on one page](../../features.md#every-shortcut-on-one-page), [Switch between your voice and the system audio without opening Settings](../../features.md#switch-sound-source-without-settings).

### Keyboard Typer

`Shortcuts › Keyboard Typer`

The Type Text shortcut.

### Jumper

`Shortcuts › Jumper`

The four anchor shortcuts and the eighteen slot shortcuts. _{Windows only}_

### Show hints for AltGr shortcuts

`Shortcuts › Hints › Show hints for AltGr shortcuts`

Shows a hint (a question mark marked Attention) next to a Ctrl+Alt shortcut that AltGr also types with (Windows reports AltGr as Ctrl+Alt). On Windows it warns only when one of your installed keyboards types a character with AltGr on that key (AltGr+Shift for a chord with Shift), and names the language Windows lists that keyboard under (e.g. "A keyboard you use for Polish (Poland) types a character with AltGr+O"); a US English keyboard has none, so no hint. A Ctrl+Alt+Space chord is flagged on any keyboard with AltGr characters, as AltGr is often still held for the space after one. Elsewhere every Ctrl+Alt letter is flagged. The hint's tooltip has a `Turn off these hints` button that jumps here. **Default:** On.

Catalog: [Your shortcuts don't eat the accented letters you type](../../features.md#shortcuts-dont-eat-accented-letters).

### Set to None

Every row has a × button that switches the shortcut off; the row then reads **None** and the keys
are free for other applications. The reset button restores the default. **Default:** every action
has a chord.

Catalog: [Turn off a shortcut you don't want](../../features.md#turn-off-a-shortcut-you-dont-want).

### Conflicts

When two actions share the same keys, both rows show an exclamation mark marked Conflict naming the other action and a
banner appears at the top of the page. Only one of the two works until one of them is changed or
set to None.

Catalog: [Every shortcut on one page](../../features.md#every-shortcut-on-one-page).
