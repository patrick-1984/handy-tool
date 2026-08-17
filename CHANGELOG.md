# Changelog


## [1.1.0] - 2026-08-17

### Fixed

- **"Don't Modify Clipboard" now actually leaves your clipboard alone.** If you had this set — it
  is the default — old transcriptions could still end up on your clipboard and stay there,
  displacing what you had copied. Several separate faults combined to cause it, and all of them
  are fixed:

  - **Handy could hand you back one of its own transcripts as if it were your clipboard.** When a
    transcript was already sitting on the clipboard, the next dictation captured *that* as "your
    previous clipboard" and faithfully restored it afterwards — so one leak became permanent, and
    every later dictation re-restored the same old text. Handy now recognises its own writing and
    restores what was really yours.

  - **A failed delivery wrote your clipboard anyway.** When an anchored jump or a paste could not
    be verified, the transcript was parked on the clipboard to avoid losing it — ignoring your
    setting entirely. It no longer does. The take is not lost: it is saved to History *before*
    delivery is attempted, and the failure message now tells you to press your Paste Last
    Transcription shortcut, naming the actual key you have bound to it.

  - **The restore could silently never happen.** If the paste keystroke failed, or focus moved
    mid-paste, the transcript was left on the clipboard with no restore ever scheduled. The
    restore is now armed the moment the clipboard is written, whatever happens next.

  - **Restoring could overwrite something you had just copied.** Handy now checks it still owns
    the clipboard before putting the old content back, so anything you copy while a dictation is
    landing is left alone.

  - **Restore failures were invisible.** A failed restore is the one case that leaves your
    transcript on the clipboard, and it was discarded without a word — which is why this was so
    hard to pin down. It is now retried, and then logged as a warning saying how many characters
    may still be there.

- **A copied image or file is no longer replaced by an empty clipboard.** Only text can be
  restored, so an image was previously overwritten with an empty string. Handy now clears the
  clipboard instead of leaving a blank entry where your image used to be, and says so in the log.

- **Your transcript is saved to History before delivery is attempted, not alongside it.** The
  save was started and then immediately left to finish on its own while the paste went ahead. That
  was fine when a failure parked the text on your clipboard, but it is not fine now that a failure
  deliberately leaves the clipboard alone — so the row is written first, and the recovery route is
  real rather than merely likely.

- **A recovery that could go stale.** The in-memory "last transcription" buffer behind the Paste
  Last shortcut was skipped rather than recovered if its lock had been poisoned by an unrelated
  crash, which would have left the shortcut re-pasting an *older* take. It now recovers.

- **Multi-line dictation with unusual line endings.** A lone carriage return was dropped entirely
  when typing, silently joining two lines into one.

- **Typed delivery now re-checks the target window first.** `Direct` verifies the anchored target
  still has focus before the first keystroke, matching what the paste path has always done.

### Changed

- **`Direct` paste method now really types the characters on Windows and macOS.** It always said
  it did. In fact it quietly fell back to a clipboard Ctrl+V paste, so anyone who chose it to keep
  dictation off the clipboard was getting exactly what they were trying to avoid. It is now the
  one delivery method that never reads or writes the clipboard.

  It is still not the default, on purpose: typed characters can set off autocomplete, bracket
  auto-closing and IME behaviour that a paste does not, they arrive as many undo steps rather than
  one, and they can drop characters in RDP/Citrix sessions and VM consoles. Choose it when keeping
  the clipboard clean matters more than those trade-offs.

### Known limits

- A clipboard-based paste method has to put the transcript on the real system clipboard for a
  moment in order to paste it. "Don't Modify Clipboard" governs what is there **afterwards**, not
  whether it was ever there. A clipboard manager, Windows Clipboard History, or remote-desktop
  clipboard redirection can still capture it during that window. Only `Direct` avoids it entirely.

## [1.0.4] - 2026-08-07

### Added

- **Native macOS disk images for both CPU families.** The build pipeline now
  produces and verifies separate Intel (`x86_64`) and Apple Silicon (`arm64`)
  DMGs on native GitHub-hosted Macs. These builds are ad-hoc signed, not
  notarized; use right-click **Open** on first launch.

### Fixed

- **The Windows installer now starts on a clean machine.** Microsoft-signed,
  architecture-matched Visual C++ runtime DLLs are deployed app-locally beside
  `handy.exe`. This preserves the current-user/no-admin install while fixing the
  `MSVCP140.dll was not found` loader failure reproduced by WinGet Sandbox.



## [1.0.3] - 2026-08-07

### Added

- **A macOS build exists for the first time.** Intel (x86_64) only, and it should be
  treated as experimental: it compiles, bundles, passes signature verification, and the
  binary runs — but it has not been used in anger. Nobody has yet granted it Microphone
  and Accessibility permission and dictated a sentence with it. If you try it, expect
  rough edges and please report them.

  Two limits worth stating plainly. It is **not notarized**, so macOS will call it an
  unidentified developer — right-click the app and choose **Open** to get past that;
  double-clicking alone will not offer the option. And there is **no Apple Silicon
  build**: the machine that produces these is an Intel Mac, and it cannot run an ARM
  binary to check one, so shipping an unverifiable ARM build would be worse than
  shipping none.

### Fixed

- **The macOS deployment target was impossible.** The bundle declared support for macOS
  10.13, but the vendored whisper.cpp uses `std::filesystem`, which Apple marks
  unavailable before 10.15. No macOS build could ever have succeeded. Now 10.15.

## [1.0.2] - 2026-08-07

### Fixed

- **Windows blocking FLM no longer looks like a working engine.** FastFlowLM ships an
  unsigned `flm.exe`, and enforced Smart App Control refuses to let Handy start it. Handy
  logged the refusal and then carried on as though the engine were available, so takes
  failed later for no visible reason. The block is now recognised for what it is
  (OS error 4551), reported plainly, and remembered, so a blocked FLM stops presenting
  itself as a usable engine. Handy cannot make a blocked binary run — but it can stop
  pretending it did.
- **Shortcuts that fail to register can now be reviewed.** The warning appeared as a
  notification and vanished before it could be read, leaving no way to find out which
  binding had failed or why. The failures now persist on the General page until resolved.

### Changed

- **New default shortcuts: `Ctrl+Space` to transcribe, `Ctrl+Alt+Space` to transcribe and
  submit.** The previous letter-based defaults collided with AltGr on European layouts,
  where AltGr+letter types an accented character. Existing bindings are untouched — only a
  missing binding is filled in, so nothing you have set will be rewritten.
- **The update check no longer generates a UUID.** Picking the daily check time needs a
  random minute, and it used a random identifier as the source. Nothing was ever
  transmitted, but an ID generator sitting in update code reads like tracking to anyone
  auditing the source. It now derives that minute from clock noise instead.

### Documentation

- **FLM is documented as the AMD Ryzen AI NPU path.** It was previously undocumented, so
  owners of AMD Ryzen AI machines had no way to discover that transcription can run on the
  NPU and leave the CPU and GPU to the work you are actually doing. Includes how Handy
  finds an FLM you installed yourself, and a troubleshooting entry for the Smart App
  Control block that states plainly what it is not: **not an antivirus detection, and an
  antivirus exclusion will not fix it**, because Smart App Control is a separate mechanism
  with no exclusion list.
- **Update checks state what they send.** The About page and its tooltip now say that
  checking sends nothing about you — no telemetry, analytics, or account — and reads one
  public file from the project's GitHub releases.
- **Corrected the logging section of the privacy page.** It still claimed release builds
  log at Debug; that changed in 1.0.0. It now also records the case that actually catches
  people: a profile created by 0.63.0 or earlier has the old level saved, and a saved value
  wins over the new default, so upgrading does not lower it for you.

## [1.0.1] - 2026-08-07

### Added

- **A portable build.** Download the ZIP, extract it anywhere — a USB stick, a network share, a work PC that will not let you run installers — and run `handy.exe`. Everything it creates (settings, history, models, recordings, logs) lives in the `data\` folder beside the executable, so deleting the folder removes every trace. It is also the way to try a new version without disturbing an install you depend on, and the way in if Smart App Control refuses to run the installer.
- **Install with winget:** `winget install patrick-1984.HandyTool`.
- **Two update buttons on the About page.** **Check on the web** opens the releases page in your browser — the escape hatch when an in-app check is blocked by a proxy or simply fails. **Check for updates** runs the check immediately and tells you what it found, then asks what you want to do: install and restart, remind you later, or turn on automatic updates. It will not start an install while a recording or transcription is running, and it says so rather than failing quietly.
- **Portable copies can now enable autostart, but only if you say so.** Turning it on asks first, and explains what it is about to do: a Windows startup entry is machine-wide state that is not part of the portable folder, it will point at that exact folder, and it replaces the startup entry of any normally installed copy. Decline and nothing is written, as before.

### Fixed

- **A portable copy will no longer install a second, normal copy of itself.** The updater would have downloaded the Windows installer and run it, quietly creating an ordinary installation in `%LOCALAPPDATA%` with registry entries and leaving the portable folder stale. Portable builds now check for updates and point you at the new ZIP instead, and the nightly silent-update schedule can never fire an installer there. No portable build was ever published before this release, so no one could have hit it — it is closed before the first portable ZIP exists.
## [1.0.0] - 2026-08-06

**First public release.** The version number marks the point where this stops being one
person's private tool and becomes something other people install — it is not a rewrite.
Everything below this entry is the same product: sixty releases of dictation, delivery and
recovery work, nearly all of it written because something broke in real use. This release
adds the pieces a stranger needs that an owner never did — updates, a full set of
languages, and documentation — and removes the things that only made sense to the person
who wrote them.

**Only Windows x64 ships.** macOS and Linux builds are planned, not released. The whole
Jumper family (anchors, jump slots, cursor save/restore, remote-desktop delays) is
Windows-only by design, and the settings that control it are hidden on other platforms.

**What you get before you configure anything** — the shipped Windows defaults, as cause and
effect:

- **Ctrl+Space** starts recording; **Ctrl+Space** again stops it. The audio is transcribed
  after you stop (Post-Recording), then pasted into whatever window you were in with
  **Ctrl+V**. Your clipboard is put back the way it was afterwards.
- **Ctrl+Alt+Space** is push-to-talk: hold, speak, release. It runs in **Live** mode, so
  with a local model the text builds up as you speak.
- **Ctrl+Alt+S** is **Transcribe & Submit**: the same paste, then **Enter**. Plain
  Transcribe never presses Enter — that is the entire difference between the two keys.
- **Escape** while recording finishes the take, saves the text to **History**, and delivers
  nothing — no paste, no clipboard write, no Enter, no jump.
- **Ctrl+Alt+P** re-pastes the last transcription. It is the recovery key for a paste that
  did not land in the target app.
- Transcription runs **on your machine**. LLM post-processing is **off**. There is no
  telemetry, no analytics and no crash reporting; the only request Handy Tool makes on its
  own is the daily update check described below, and you can switch that off.
- **History** keeps the 5 most recent transcriptions with their audio and deletes the rest.
  Recordings live in `{app_data}\recordings\`.

Everything above is a setting. Ctrl+V and Shift+Insert both mean "paste", but many
terminals and remote-desktop clients accept only one of them — which is exactly why the
paste method is a dropdown. See `docs/start/` for the guided route and
`docs/reference/settings/` for the screen-by-screen index.

### Added

- **Handy Tool can update itself.** Until now every fix in this changelog reached you only
  if you noticed a new release and re-ran the installer. The app now checks the public
  GitHub releases page for a newer version — one static file fetch per day, not an API
  poll — and can install it for you. The defaults, spelled out:
  - **Checking is on**, **silent installing is off.** By default you get a banner and
    decide; nothing is downloaded and installed behind your back.
  - Turning silent updates on installs inside a **nightly window, 04:00 local time ±30
    minutes** (03:30–04:30 by default; both the time and the jitter are configurable). The
    exact minute is rolled once per day and remembered, so restarting the app does not
    re-roll it and a laptop that wakes at 09:00 is not updated at 09:00.
  - **It never updates while you are dictating.** The updater asks the recording
    coordinator for an idle reservation and waits; a take in progress always finishes. If
    the window closes while it is still waiting, the install is deferred to the next night
    rather than forced.
  - If the app was closed through the whole window, it performs one catch-up **check** at
    the next start when no check has succeeded in 24 hours — but it will not silently
    install outside the window.
  - A **manual "Update now"** ignores the schedule and the silent-update setting. It does
    not ignore the recording guard or the signature check.
  - **Portable installs are check-only.** Running the NSIS installer against a
    `portable.marker` copy would create a *second*, installed copy rather than replace the
    portable one, so in-place updating is refused there and you are pointed at the download.
- **An update banner below About** in the sidebar, showing exactly one state at a time:
  checking, available, downloading with progress, ready to restart, waiting for dictation
  to finish, or failed with the real error and a link to download the installer by hand.
  General → Updates has the toggles, the window, and a **Check now** button with the time of
  the last successful check.
- **A "Configure providers" link on Model Testing and Token Count.** Both tools are useless
  until at least one LLM provider has a key, and both used to leave you to find
  Advanced → Providers yourself.
- **Current Audio now explains itself.** The panel says when text appears progressively
  (the shortcut you pressed is set to Live *and* you are on a local model) and when the
  whole transcript arrives at once (Post-Recording, API Transcription, OpenRouter
  Transcription) — instead of looking broken while it waits.
- **A written documentation set** in `docs/`: a numbered install-to-working-setup path, a
  page per tool, a 166-entry feature catalog named after the problems it solves, a
  screen-by-screen settings reference, an audited privacy page, troubleshooting, and CLI,
  shortcut and glossary references.
- **The documentation is machine-checked against the app.** The docs contain no
  screenshots on purpose — the app changes too fast for images to stay honest — so
  navigation is written as text breadcrumbs like
  `General -> "Transcribe & Submit" -> "Paste method"`. Every quoted label is verified
  against the app's English strings by `scripts/check-docs-breadcrumbs.mjs`; renaming a
  setting now fails the check and names the file and line, instead of quietly leaving the
  docs pointing at a control that no longer exists.

### Changed

- **Every delay you can pick now has usable steps: Off, then 100 ms increments up to 1 s,
  then 1.5 s and 2 s.** The old scales jumped 250 → 500 → 1000 ms, so tuning a paste into a
  slow Citrix session meant doubling the wait when 100 ms more would have done. This covers
  the post-jump paste delay, the submit delay (both in their Local and Remote variants) and
  the clipboard-restore delays. Defaults are unchanged: paste delay 250 ms local / 1000 ms
  remote, submit delay 250 ms local / 500 ms remote, clipboard restore Off. 250 ms stays on
  the list because it is the shipped default. **Longer legacy clipboard-restore values (2.5 s
  and 5 s) keep working** if you already had one selected — they load and behave exactly as
  before, they are simply no longer offered to new users.
- **All 16 non-English languages are complete.** Arabic, Czech, German, Spanish, French,
  Italian, Japanese, Korean, Polish, Portuguese, Russian, Turkish, Ukrainian, Vietnamese,
  Simplified and Traditional Chinese went from heavily partial — where a translated app
  fell back to English mid-screen — to roughly 7,500 translated strings each, including the
  tray menu, the recording overlay and the floating window. A terminology pass followed, so
  the same concept uses the same word on every screen in every language rather than three
  synonyms picked one string at a time. English remains the source of truth and a checker
  fails the build on any missing or extra key.
- **Windows updates are delivered through the NSIS per-user installer** (installed to
  `%LOCALAPPDATA%\Handy Tool`, run quietly, no UAC prompt): the app closes, the installer
  replaces the files, and the app reopens. Honest limits: **1.0.0 itself must be installed
  by hand** — 0.63.0 and earlier have no updater and cannot bootstrap into one, so automatic
  updating only starts working from this release forward. If you installed from the MSI,
  install the NSIS package once to join the update channel; your settings, models and
  history are in `%APPDATA%\pr.handy\` and are not touched by either installer.
- **Advanced → Experimental is now Advanced → Post-processing**, and the Keyboard
  implementation control moved to the **App** tab where the rest of the input plumbing
  lives.

### Fixed

- **A switch that did nothing has been removed.** The "Experimental Features" toggle on
  Advanced had not been wired to any behaviour for several releases: turning it on changed
  nothing, which is worse than not offering it. It is gone, and the tab that held it now
  contains the post-processing settings.

### Security

- **Release builds no longer write your dictated words to a log file at the default
  logging level.** File logging defaulted to Debug even in release builds, and Debug records
  can include transcript fragments, complete API transcription results and LLM prompt
  previews — so a dictation app was quietly keeping a plain-text copy of your speech on disk
  for anyone with access to the machine. Release builds now default to **Info**, which does
  not log transcript content. Debug logging is still available when you are diagnosing
  something (Debug page → log level), and it still writes what it always did — that is now a
  choice you make, not the default.
- **Log files no longer accumulate forever.** Rotation kept *every* rotated 10 MB file, with
  nothing cleaning them up — history retention does not touch logs — so the oldest dictation
  metadata on the machine could outlive the recordings it described. At most three files are
  now retained (~30 MB), which is enough recent context to diagnose a failure and a bounded
  amount of history to leave lying around.
- **Updates are cryptographically signed and verified before anything runs.** The download
  is checked against the release signing key embedded in the app; a corrupt, truncated or
  unsigned artifact fails loudly and the installer is never launched, so a failed or tampered
  update leaves your installation untouched. Signature failures surface as a visible error
  with the manual-download fallback, never as "no update available". This is minisign
  verification of the update payload — it is not Authenticode, so a manually downloaded
  installer can still show a SmartScreen "unknown publisher" warning.
## [0.63.0] - 2026-08-01

### Added

- **"Cancel behavior" setting (General) — and cancelling now keeps your words by default.** Previously, cancelling always threw the recording away. There is now a choice, and the new default is **"Finish, save to history only"**: the recording stops and is transcribed exactly as if you had pressed Transcribe, the result is saved to **History** — and nothing is delivered. No pasting, no clipboard change, no submit/Enter key, no Jumper jump or slot action. It's the "I don't want this typed into whatever is in front of me, but don't throw away what I said" button. The old behavior is still one dropdown away as **"Discard recording"**.

  The setting governs **every** way you cancel — the Escape shortcut, the tray **Cancel** item, the in-app cancel command and `handy --cancel` — so cancelling means the same thing however you trigger it. Cancelling _after_ the recording has stopped (while it's still transcribing) now also just suppresses the delivery instead of tearing the pipeline down.

  Scope of the "delivers nothing" guarantee: it is absolute for the whole time a recording is running — which is the entire window in which the Escape shortcut is even active, so pressing Escape always gets you the new behavior. The tray item and `handy --cancel` can also be used later, while a take is still transcribing, and those are suppressed too. What a cancel can _not_ do is unwind a delivery that has already started: once the paste is under way (including while it is sitting in a configured Jumper paste/submit delay, which can be up to two seconds), the clipboard write and submit key may still land. That behavior is unchanged from previous versions and is not specific to this setting.

### Changed

- **Cancelling no longer discards your recording unless you ask it to.** This changes on upgrade, not just on fresh installs. Because a cancelled take is now _kept_, please note:
  - The transcript **and its audio recording** are written to History (`{app_data}/recordings/`) and are subject to your recording-retention setting; compressed `.opus` recordings are also included in full backups. A mis-spoken or sensitive take is no longer thrown away — **delete the History entry** to remove both the text and the audio, or switch the setting to "Discard recording". (As before, a take that is both very short _and_ empty is dropped without a row.)
  - If your transcription engine is a **remote API** (API Transcription / OpenRouter), a cancelled take is still uploaded to that endpoint — cancelling no longer prevents the upload.
  - If the take was started with the **post-processing** shortcut, LLM post-processing still runs on it, so it still costs that provider's tokens.
  - The **"Paste last transcription"** shortcut is deliberately _not_ updated by a silent finish, so it keeps re-pasting your last real delivery. (After an app restart it falls back to reading History, where a silently-finished take can surface — as can the tray's "Copy last transcript".)
  - One deliberate exception: if the app's internal transcription coordinator is not reachable (it has crashed), cancelling falls back to the old discard behavior rather than risk leaving the microphone running with nothing able to stop it.

### Fixed

- **A cancelled take no longer poisons "Paste last transcription".** Cancelling while a transcription was already running still recorded that transcript as the "last delivered" one, so the manual re-paste shortcut would paste text that had never been delivered. The buffer is now only updated when a take is actually delivered.

## [0.62.0] - 2026-08-01

### Changed

- **The Jumper "jump slot" picker is now always visible.** On the "Jump slot action on start / on finish" rows (both the Transcribe and the Transcribe & Submit groups), the dropdown that selects _which_ slot to jump to / deliver into used to be hidden until you first changed the action away from "Do nothing" — so it looked like there was no way to choose the slot. It's now always shown, just greyed out until you pick an action, matching how the per-slot cursor-mode dropdown already behaves.

## [0.61.0] - 2026-07-30

### Fixed

- **You can now anchor and jump to the new Microsoft Teams message field** (and other WebView2/Electron-hosted fields). Setting an anchor on Teams and jumping to it always failed with _"target field was replaced by another"_ while other apps worked. New Teams hosts its editable field in a separate WebView2 process (`msedgewebview2.exe`), and Handy's safety re-check required the focused field to live in the _same process_ as the top-level window — so it wrongly rejected the cross-process field every time. Handy now records the field's **own** process at anchor time and checks against that, so a legitimately out-of-process input is accepted while a handle genuinely recycled into a _different_ process is still rejected (all the other identity checks — window process/thread, class match, foreground — are unchanged).

### Security

- **Password-field refusal now covers WebView2/HTML password inputs too.** Handy already refuses to anchor or deliver into a password box; that check scans the focused UI-Automation element, but it was scoped to the top-level window's process, so a browser/Electron-hosted `<input type="password">` (which the classic Win32 password style can't see) was invisible to it. It's now scoped to the field's own process, closing a gap that the Teams cross-process fix above would otherwise have made reachable.

## [0.60.0] - 2026-07-24

### Fixed

- **A recording can now be finished from either mode, both directions.** Previously a take started with **Transcribe & Submit** couldn't be stopped by pressing **Transcribe** (the press was ignored with a "busy" beep) — only the reverse worked. The Transcribe toggle now finishes a recording started by _any_ binding (mirroring what Transcribe & Submit already did), finishing it as a plain output paste (no submit key). It stops the recording's actual owner so the recorder can't be left running, and honors the "only jump on finish if started the same way" setting symmetrically.

## [0.59.0] - 2026-07-24

### Changed

- **"Paste last transcription" is now near-instant.** 0.58.0 fixed it by pasting on key-press and then _waiting_ for you to release the trigger modifiers — which added up to ~1 s of lag when the key (or a macro key) was held. It now pastes the instant you **release** the shortcut: the trigger key is already up (so it can't leak repeated characters into the target) and any still-held modifier — including right Alt/AltGr — is cleared first, so the injected chord stays clean but fires immediately. (Content was never the bottleneck — it's read from the in-memory last-transcript buffer, SQLite only as a post-restart fallback, never the clipboard.)

## [0.58.0] - 2026-07-23

### Fixed

- **"Paste last transcription" now actually pastes.** The shortcut fires on key-press while its trigger modifiers (e.g. Ctrl+Alt) are still physically held, so the synthesized paste chord came out as Ctrl+Alt+V / Ctrl+Alt+Shift+Insert and the target ignored it — nothing appeared, for any paste method. It now waits (off the UI thread) for all modifier keys (Ctrl/Alt/Shift/Win) to be physically released — polling the real key state, bounded to ~2 s — before injecting the paste, so the correct chord lands. (The shortcut firing and the history text were never the problem; the content still comes from the last delivered transcript / the SQLite history, never the clipboard.)

## [0.57.0] - 2026-07-23

### Added

- **"Paste last transcription" shortcut.** A new global shortcut (default **Ctrl+Alt+P**, configurable in **General**) that re-pastes the most recent transcription from history into the focused window — a manual fallback for when the automatic paste didn't land. It has its own **paste method** (Ctrl+V / Ctrl+Shift+V / Shift+Insert / Direct / None / External script) and **clipboard handling** (keep your clipboard or leave the transcription on it), independent of the global paste settings, so you can tune it per target app. Pastes the final text you received (post-processed when present, else the raw transcription); no anchor/jump and no submit key — just a plain paste where you're working.

## [0.56.0] - 2026-07-23

### Added

- **RDP/Citrix-aware jump delays (Windows).** The post-jump paste and submit delays are now split into a **Local apps** value and a **Remote desktop** value, side by side (Advanced → Transcription). Remote desktop sessions settle far slower than local windows, so you can wait ~1 s for RDP/Citrix without slowing local jumps. A new **Remote desktop detection** list on the Jumper page classifies a target as remote when its app/window-class/control-class contains one of your match strings (seeded with `msrdc`, `mstsc`, `Citrix`; case-insensitive; editable). Matching anchors show a **Remote ✓** badge. The paste-delay control now also appears for pure **Transcribe** (it jumps + pastes too), not just Transcribe & Submit.

### Fixed

- **Transcribe (& Submit) sometimes failed to deliver into RDP/Citrix — "not pasted" / error toast.** The v0.55.0 post-jump wait runs before the strict focus re-check, and a freshly-activated remote-desktop window is often still moving focus between its inner controls at that moment, so the strict check aborted and parked the text. On a **remote** jump the re-check now tolerates the target still settling — it polls every 25 ms for up to 250 ms while the expected window stays foreground — instead of parking on the first transitional state. It still aborts immediately (and never reactivates) if a different window takes focus, and never pastes into a password field or a foreign window. Local jumps are unchanged.

## [0.55.0] - 2026-07-22

### Added

- **"Paste delay after jump" (Windows).** A new setting in **General → Transcribe & Submit** that inserts an extra wait _after_ a Transcribe & Submit (or any anchored delivery) jumps the foreground to an anchored target and _before_ the paste keystroke. A freshly-activated window — especially RDP/Citrix — can still be transitioning (completing activation, moving focus) when Ctrl+V fires, so the paste is swallowed and nothing appears. This is separate from the existing "Submit delay after jump" (which waits before the Enter key). Options: Off / 100 / 250 / 500 / 1000 / 2000 ms, default **250 ms**; only applies on a real jump — when you're already in the target, paste stays instant.

## [0.54.0] - 2026-07-21

### Changed

- **Custom transcription providers now have their own cards on the Providers page.** The **API Transcription (OpenAI-compatible)** and **OpenRouter Transcription** engines are each configured in an always-visible card under **Advanced → Providers** — no longer hidden in Advanced → Transcription behind having already selected that model (the confusing "I picked the model but can't find where to enter the URL/key" flow). OpenRouter transcription now has its own **API URL + API key** fields directly, decoupled from the registered LLM providers list. Existing setups migrate automatically on first launch: your previous OpenRouter provider's base URL and key are copied into the new dedicated fields (the referenced provider is left untouched for post-processing/model-testing).

## [0.53.0] - 2026-07-21

### Added

- **Configurable "Submit delay after jump" (Windows).** New setting in **General → Transcribe & Submit** that inserts an extra wait before the submit key (Enter) when a Transcribe & Submit — or any anchored auto-submit — jumps the foreground to an anchored target. Options: Off / 100 / 250 / 500 / 1000 / 2000 ms, default **250 ms**. Fixes the submit key being dropped when jumping into a freshly-activated window (especially RDP/Citrix), while leaving the already-in-target case instant.

### Changed

- **Transcription settings moved to the General page.** Transcription mode, Push-to-talk mode, GPU device, custom words, and "append trailing space" now sit in a **Transcription** group on General (right after the language settings) instead of Advanced. Engine-specific options (translate-to-English, API/OpenRouter endpoint config, cost report) stay in Advanced.

### Fixed

- **Transcribe & Submit often didn't submit when jumping to a target.** The Enter key was pressed a fixed 50 ms after the paste and — when the delivery had jumped the foreground — focus was returned to the previous window immediately afterward, racing the just-injected Enter so the target never processed it. (That is why it worked when you were already in the field but not when jumping, especially over RDP.) Now, ONLY on a real jump: an extra configurable settle (above) before Enter, plus a short internal grace that keeps the target foreground until it has processed the Enter before focus returns. The already-focused path is byte-for-byte unchanged.
- **Orphaned FLM (NPU) processes are now cleaned up — and can no longer pile up.** A crash or a force-kill (including the installer's `taskkill`) bypassed the normal shutdown that stops Handy's `flm serve` child, leaking it. Leaked instances squat port 52625 and the single-tenant NPU, so the next start failed with a context error ("failed to start FLM") — and they compounded across restarts, wasting memory. Handy now sweeps its own orphaned `flm serve --asr` instances (matched by its exact port + args, so another app's flm — Lemonade/FLMTray — is never touched) on every launch and again right before each spawn, so it always cleans up after its own crash and never stacks instances.
- **Logs no longer look wiped after a restart.** File logging kept only a tiny 500 KB window and a single rotation, so a normal session filled it and the next launch rotated it away — losing the history right when you needed it (e.g. to see an FLM failure that only showed as a red toast). Logs now rotate by size at 10 MB and keep rotated files, so history persists across restarts. FLM start failures are also logged at error level, not only surfaced in the UI.

## [0.52.2] - 2026-07-21

### Fixed

- **Idle model-unload silently stopped after the first recording** (and could, in a rarer race, contribute to a hang). The transcription manager's background model-load path dropped a clone whose destructor tore down the _shared_ idle-unload watcher thread — so after the first load, the "unload model after N minutes" setting stopped working for the rest of the session and the log filled with spurious "Shutting down TranscriptionManager" lines. Ownership is now tracked so only the real owner tears the watcher down (at app shutdown), and the teardown no longer holds a lock across the thread join.
- **"Unload model" / model-load-status commands** requested the wrong managed type and would fail at runtime; corrected to match the registered state.

## [0.52.1] - 2026-07-21

### Fixed

- **App freeze when cancelling a recording** (overlay bar goes stale, requires force-restart). The recording overlay's audio-level updater was calling a synchronous window query from the audio worker thread; if you cancelled a recording at that instant, the main thread (tearing down the microphone) and the worker deadlocked on each other and the whole app hung. The updater now uses a lock-free visibility flag and never blocks on the UI thread. Cancellation also commits its state before any device teardown, as a safety margin.

## [0.52.0] - 2026-07-20

### Added

- **Five more static jump slots — now nine (Static 1–9).** The Jumper gained Static 5 through 9, identical to the existing static slots (each with its own set/jump shortcuts, save-mouse-position toggle, and cursor-position mode). Defaults: `Ctrl+Alt+Shift+N` to set, `Ctrl+Alt+N` to jump (N = 5–9). Existing slots and their data are untouched on upgrade.

### Changed

- **Slot pickers list Hot 2 directly under Hot 1**, then Static 1–9. In the anchor-action and track-last-output dropdowns Hot 2 now appears right below Hot 1 instead of last.

## [0.51.0] - 2026-07-20

### Changed

- **Cursor-position mode is now per-slot.** Every Jumper slot — Hot 1, Hot 2, and the four static slots — has its own **App-relative / Screen-absolute** mode dropdown, sitting right under that slot's "Save mouse position" switch (previously only Hot 1 showed a single shared mode). The dropdown is always visible but **grayed out** when that slot's save switch is off, instead of appearing/disappearing. Existing setups are migrated: every slot inherits your previous shared mode, so nothing changes until you adjust a slot.

## [0.50.0] - 2026-07-20

### Added

- **Second hot anchor.** The Jumper now has two hot anchors — **Hot 1** and **Hot 2** — each with its own quick set/jump shortcuts (Hot 2 defaults to `Ctrl+Alt+H` / `Ctrl+Alt+G`). Either can be targeted by the dictate and Transcribe & Submit flows independently, so each flow can own a distinct hot anchor. Existing slots are unchanged; Hot 2 starts empty.

### Changed

- **Current Audio keeps the last transcript.** When a new take starts, the previous transcript stays visible until the new one produces text (live / push-to-talk streams the in-progress result; post-recording keeps the last until the final arrives), instead of blanking between takes.
- **Copy button moved into the transcript box** (top-right, GitHub-style), appearing when there's text; "Open floating window" stays in the header.

## [0.49.0] - 2026-07-20

### Added

- **Jumper cursor save/restore is now per-slot.** Each jump slot (the hot slot and the four static slots) has its own "save mouse position" switch on the Jumper page, plus a shared App-relative/Screen-absolute mode. The old per-flow toggles in General / Transcribe & Submit are gone — everything cursor-related now lives on the Jumper page.
- **"Only jump on finish if started the same way" option.** When enabled, a flow's on-finish jump fires only if the recording was both started and finished by the same flow — e.g. a take started with plain Transcribe but finished via Transcribe & Submit no longer triggers the submit flow's jump (the submit itself still happens).
- **Copy button on the Current Audio view** — a top-right button copies the current transcript to the clipboard.

### Changed

- **Settings menu tidy-up.** The Transcribe and Transcribe & Submit option groups moved from the (crowded) General page to the top of Advanced → Transcription. The Transcribe and Push-to-Talk shortcuts, the model card, and Sound stay on General.

## [0.48.0] - 2026-07-20

### Added

- **Translator can use its own model, loaded in parallel.** The Translator's batch model can now differ from your live-transcription model and stay resident **at the same time** — e.g. dictation on the NPU (FLM) while the Translator runs a Whisper model on the GPU — instead of swapping one shared model in and out. They take turns gracefully on shared hardware (no GPU race), and the Translator model has its own idle-unload setting (unload / unload-after) like the main model. This also removes the model reload that could make stopping a recording hang.
- **Jumper can save & restore the mouse cursor.** Optionally, jumping to a saved window also moves the mouse pointer to a spot you saved with the anchor. Per-flow opt-in (dictate and Transcribe & Submit have separate switches; the quick/hot slot follows the dictate switch). Two modes: **App-relative** (same spot inside the app window — follows it across moves, resizes, and different-DPI monitors; default) and **Screen-absolute** (a fixed monitor pixel). Multi-monitor aware; the cursor moves only after the paste, and never on a machine that isn't per-monitor-DPI aware. Windows only.

## [0.47.0] - 2026-07-20

### Changed

- **Clearer FLM/NPU error when the NPU is busy.** When FLM's speech-to-text model can't create an NPU inference context (error `0xc01e0009`), the message now explains the most common cause — another process (FLMTray, a standalone `flm serve`, or Lemonade) already holds the single NPU context — and tells you to close it and reselect the model, before suggesting a driver update.

## [0.46.0] - 2026-07-20

### Fixed

- **FLM (NPU) models no longer silently produce empty recordings.** When FLM's speech-to-text model fails to load on the NPU — for example, the NPU cannot create an inference context — Handy now detects this at model-selection time and reports a clear error, instead of appearing to work and then saving every recording with no text. More generally, any transcription engine error that would leave a recording with no text now raises a visible "transcription failed" notice rather than saving a silent, empty result.
- **FLM start/stop is more robust.** Selecting FLM reliably restarts a stopped server, a failed start no longer leaves the app thinking a model is loaded, and rapid re-selection of an external model no longer restarts a healthy server or races itself.

### Changed

- **"Track last output location" is now an independent switch per flow.** Dictation (Transcribe) and Transcribe & Submit each have their own toggle instead of sharing one global switch. Existing settings are migrated so nothing changes until you adjust them.

## [0.45.0] - 2026-07-20

### Fixed

- **Model-download status is race-hardened.** A download that finishes (or starts) at the exact moment the app refreshes its on-disk model list can no longer be shown with a stale "not downloaded" state — each model carries a refresh revision so the refresh skips any model whose state changed mid-probe. Cleanup of leftover extraction folders now runs only at startup, so it can never remove an extraction that is currently in progress.

## [0.44.0] - 2026-07-20

### Fixed

- **Cancelling a model download now stops it immediately.** Previously, cancelling while a download had stalled (server gone quiet) left the transfer, connection, and file handle alive until the next byte arrived or a 60 s stall timeout elapsed. Cancellation now interrupts a stalled download at once and preserves the partial file for resume. The whole download lifecycle is tied to a single attempt identity, so a cancelled or superseded download can no longer clobber a fresh retry's progress — or the model you have since selected.

## [0.43.0] - 2026-07-20

### Added

- **Portable mode.** Place a `portable.marker` file next to `handy.exe` and Handy keeps everything — settings, models, history, recordings, logs, and its web-view data — in a `data\` folder beside the executable, mutating no machine-level state (no autostart entry, no CLI self-install). Falls back to the normal per-user location if that folder isn't writable.

### Fixed

- **First-run model step shows correctly.** A fresh install with no local model but an unconfigured API/OpenRouter entry no longer skips the model-download step.
- **LLM post-processing, token counting, and model testing now have full network timeouts** (connect + total) and a bounded response reader, so a hung or oversized provider response can't stall the app.
- **"Track last output location" is reachable from the Transcribe & Submit settings**, not only the General page — it is one shared switch governing both flows.

## [0.42.0] - 2026-07-20

### Added

- **GPU device picker for Whisper (Vulkan).** Choose a specific GPU, Auto, or CPU in Advanced settings; an invalid or unavailable choice validates and falls back to Auto rather than failing the load.

### Fixed

- **Jumper anchor and slot-persistence hardening.** Anchored delivery re-verifies the captured window/control identity (guarding against recycled handles), detects password fields via UI Automation for browser and Electron logins, and persists jump slots through a keyed, versioned, torn-write-safe sidecar file.

## [0.41.0] - 2026-07-19

### Added

- **Appearance selector** (system / light / dark) in settings.

### Fixed

- **Jumper delivery is take-scoped and TOCTOU-safe.** Each recording's delivery target is captured per-take and re-verified — down to the focused control, with a password-field re-check — immediately before every keystroke, failing closed (parking the text on the clipboard) if focus has moved. Translator batch hardening (stable folder scanning, per-job settings snapshot, path-keyed rows) and per-request engine-identity revalidation on every transcription route. Recording-start latency is now instrumented for diagnosis on slower machines.

## [0.40.0] - 2026-07-19

### Changed

- **Jumper settings reworked.** A single global "track last output location" switch with a slot picker, shared by both the dictate and Transcribe & Submit flows; anchors are always kept after delivery; "remember slots across restarts" is now a standalone all-slots setting; and each flow can optionally return focus to where you started.

### Fixed

- **Windows installer prerequisites.** The installer now detects and guides WebView2 and Visual C++ runtime prerequisites. Windows builds use an AVX2 CPU baseline for broader hardware compatibility.

## [0.39.0] - 2026-07-19

### Fixed

- **Post-processing prompt-injection isolation.** The transcript is now passed to the post-processing model as data, separated from the system and processing instructions, so dictated text can't hijack the prompt; each provider's "disable thinking" switch is sent only in the dialect that provider accepts. Linux/X11 fixes: push-to-talk auto-repeat no longer strands a recording, microphone level-gating on the overlay, and correct macOS dock-activation order.

## [0.25.0] - 2026-07-06

### Fixed

- **Live mode no longer loses the tail of your dictation.** The pasted text came from the accumulated live preview, which misses everything after the last ~3 s emit (or a whole final segment if the last update was skipped while the engine was busy). Live mode now transcribes the **complete** audio on stop — the same cost as one more live update, since live updates already re-transcribe the full audio — with the live text kept as a fallback if that final pass fails. (Related hardening: the wait for an in-flight segment is now a generous backstop instead of 5 s, and immediate model-unload can no longer evict the engine between stop and the final pass — both would have silently re-created the tail loss.)
- **The VAD no longer holds back trailing speech at stop.** Voiced frames buffered during an unconfirmed speech onset were silently dropped when you stopped recording (a clipped final word). The recorder now flushes them into the recording and the final segment.
- **Stop can no longer hang forever** if the microphone stream dies mid-recording (commands were only processed when audio arrived; the recorder now services stop/cancel on a bounded tick).
- **Cancelling a live recording no longer leaks stale text** into a later recording's result.

### Added

- **Restore from backup (Configuration → Backup).** Pick a Handy backup archive and choose what to bring back: **configuration & history** (settings + history DB) and/or **recordings** (audio files) — works with or without metadata/data. Hardened: only known Handy files are extracted (path-traversal-safe, regular files only), decompression size caps, and a partial-restore report with per-item errors; after restoring settings/history a one-click **Restart Handy** loads them.

## [0.24.0] - 2026-07-05

### Fixed

- **Parakeet (and Moonshine/SenseVoice) no longer cut off the end of your dictation.** When you stopped recording, the final audio segment ended the instant you released the hotkey — mid-word or right at the last word. Whisper decodes that fine, but transducer-style models (Parakeet v3, Moonshine, SenseVoice) need trailing acoustic context to emit their final tokens, so the last word(s) were almost always dropped unless you paused in silence before stopping. Handy now pads one second of silence onto the audio for these engines before decoding (segments cut mid-recording already ended at VAD-confirmed silence, which is why waiting "fixed" it). Whisper, FLM, API, and OpenRouter paths are byte-identical to before.

### Added

- **Full architecture + UX audit.** An architecture spine with a verification report, plus a UI/UX audit carrying a Windows-first redesign spec. (Planning artifacts are kept outside the published repository.)

### Changed (UI wave 1 — Windows-first polish)

- **Keyboard focus is now always visible.** An app-wide `:focus-visible` outline (2px, brand rose) covers every control; the removed/1px-on-any-focus rings in buttons and the focus-less dropdown are gone. Overlay cancel/float controls are now real buttons with labels for screen readers.
- **The floating transcription window follows your system theme** (it was permanently `#1a1a2e` dark) and is fully translated (en/es/fr/vi) — no more hardcoded "Waiting for transcription...". The recording overlay's colors are tokenized too.
- **Better contrast.** Primary buttons use a deepened rose (`#b83d75`, ≥5:1 with white text — the old pink was ~3.4:1); secondary text/labels darkened for AA; danger buttons use a themed token instead of raw red.
- **Windows platform fit.** Segoe UI Variable font stack, styled scrollbars (the stock WebView2 ones clashed), `color-scheme` so native widgets follow dark mode, the main window can now be maximized (Win+Up / snap layouts), and `prefers-reduced-motion` is honored everywhere (pulsing animations included).
- **Icons use `currentColor`** instead of hardcoded pink, so they adapt to theme and state.

### Fixed

- **No more console window flashing.** FLM detection ran `flm --version` on every model-list rebuild, and on Windows that spawned a console subprocess **without the `CREATE_NO_WINDOW` flag** — so a black command-line window popped up and vanished periodically (whenever `flm` was on your PATH). FLM's subprocesses (`flm --version` and `flm serve`) now run hidden, and detection is **cached** so it runs at most once per session instead of on every model refresh.

## [0.23.0] - 2026-07-02

### Fixed

- **OpenRouter transcription: pick a Whisper model + a keyed provider.** The model picker now lists actual **speech-to-text** models (Whisper, GPT-4o-transcribe, Chirp, …) — OpenRouter excludes STT models from its normal `/models` list, so Handy now queries `?output_modalities=transcription` (with a built-in fallback list). The provider dropdown now shows only OpenRouter providers that are **enabled and have an API key** (keyless slots were selectable and produced silent 401s); if none qualify you get a clear hint. If the model is left blank it defaults to `openai/whisper-large-v3`, and a request is never sent without a key.

### Changed

- **OpenRouter transcribes the whole recording once, at the end.** To avoid mid-recording network disruptions, OpenRouter no longer sends audio in chunks/segments during recording. On-disk chunked Opus recording (crash-safety) works exactly as before, but transcription now buffers the audio and sends it in a **single request when you stop**. (It takes a little longer, as expected.)

## [0.22.0] - 2026-07-01

### Fixed

- **OpenRouter transcription now actually works.** It was producing no text (and no history entry): the dedicated STT endpoint doesn't accept the chat-only `usage:{include:true}` field, doesn't reliably accept Opus, and had no request timeout (so a bad call could hang). Handy now sends WAV on the STT route (Opus/ogg support varies by model), drops the `usage` field there (cost still comes back in `usage.cost`), and bounds the request with a 120 s timeout.

### Added

- **Richer history entries.** Each entry now shows, in brackets after the date, `(HH:MM:SS · <model> · $cost)` — the recording length, which engine/model produced it (e.g. `Whisper Large — local` or `openai/whisper-large-v3 — OpenRouter`), and the real cost when known.
- **"Last 7 days" in the cost report**, above the weekly breakdown (same columns), plus a **Recalculate durations** button.
- **Retroactive duration backfill.** On startup Handy now fills in the recording length for older history rows that predate duration tracking, by reading each audio file (WAV header, or the Ogg/Opus granule) — fixing the "188 recordings, 32 seconds" totals. The Recalculate button re-runs it on demand.
- **Backup (Configuration → Backup).** Export your data as a `.tar.gz`: **Configuration + history** (settings + the history DB — timestamps, text, cost, duration; no audio) or **Full** (adds your compressed `.opus`/`.ogg` recordings). Downloaded models and large uncompressed audio (WAV/FLAC) are always excluded. Save dialog defaults to `handy-backup-<profile>-<timestamp>.tar.gz`.

## [0.21.0] - 2026-07-01

### Added

- **Per-transcription cost tracking (OpenRouter).** Each OpenRouter transcription now records its real USD cost (from OpenRouter's `usage.cost`) and the recording length. The cost is shown in brackets next to the date in History (e.g. `Jul 1, 2026, 2:23 PM ($0.0034)`).
- **Transcription cost report** at the bottom of Advanced → Transcription: a live breakdown by **last 4 weeks**, **last 12 months**, **by year**, and an **all-time total** (recordings, total length, total cost). A **Download CSV** button saves the full report — every recording (timestamp, HH:MM:SS length, cost) plus the weekly/monthly/yearly summaries and grand total — via a save dialog defaulting to `transcription cost report-<timestamp>.csv` in Downloads.

### Notes

- Cost is captured per request and summed across a recording's segments/chunks, so live and chunked recordings tally correctly. Local engines (Whisper, Parakeet, etc.) have no cost and show none.

## [0.20.1] - 2026-07-01

### Fixed

- **OpenRouter Transcription (and API Transcription) can now be selected.** Picking an external/API transcription engine used to silently fail to stick because the app tried to fully load/validate it before saving the choice — but those engines are configured separately, so it errored ("not configured") and reverted. Selecting one now persists immediately; it validates lazily and shows a status if configuration is still needed. This removes the select-then-configure deadlock.
- **Advanced → Transcription is no longer cluttered with unrelated config.** The API-Transcription (URL/key/model) and OpenRouter-Transcription (provider/model/route/format) fields — which look like LLM/post-processing config — now appear only when that engine is the selected transcription model, so you see just the config for the engine you're actually using.

## [0.20.0] - 2026-06-30

### Added

- **MCP server + CLI companion — drive Handy as a scriptable engine.** Handy can now expose a token-protected local server (Advanced → **MCP & CLI**) so Claude and a `handy` CLI can run the same tools the app does. Listens on `127.0.0.1:<port>` only.
  - **MCP transports:** HTTP for the Claude app (point a custom MCP server at `http://127.0.0.1:<port>/mcp` with the shown bearer token) **and** stdio for Claude Code (`handy mcp --stdio`, which bridges to the same server).
  - **CLI companion:** the app installs a `handy` binary onto your PATH (Windows: `%LOCALAPPDATA%\Microsoft\WindowsApps`). Commands: `handy model-test`, `token-count`, `type`, `history-list/get`, `providers-list/set/models`, plus `handy mcp --stdio` and `handy install-cli`.
  - **Tools (MCP + CLI):** `model_test` (select run/judge by id or name, prompts, presets, **separate model & judge temperature + thinking**, image attach, and a `save_path`/`--out` so the report flows back inline or to a file), `token_count`, `keyboard_type`, `history` list/get, and full **provider config** mirroring the UI — change a model (auto-fills cost from OpenRouter), set concurrency/sequential/family/enable, and query/refresh models. **API keys are write-only** over MCP/CLI (never read back).
- **Separate temperature + thinking for models vs the judge** in Model Testing — e.g. run the models with thinking off but judge with thinking on. Both are recorded in the saved report.

## [0.19.0] - 2026-06-30

### Changed

- **Presets now select their parts.** Selecting a model-testing preset sets the model-prompt and judge-prompt pickers to the saved prompts that make up the preset (instead of just showing the preset name), so you can see and tweak each part. Presets reference their saved prompts by id; older presets still load from their stored text.
- **Prompt fields always show the text.** The model- and judge-prompt fields are now always the editable, fixed-height textareas (no collapsing to a name chip), so the field size doesn't jump and you can see how much prompt there is. Editing a field (or its image) detaches it from the loaded saved prompt.
- **OpenRouter price is now visible.** Selecting an OpenRouter model auto-fills the cost-per-1M fields from the catalogue (same mapping as Gemini/Anthropic) and shows the numbers, with the same "persist price" lock. The real per-request cost is still used at run time.
- **Model-testing rows show price.** Each model in the run/judge list shows `in $… · out $…/1M` before its Run/Judge toggles.

### Added

- **Images persist with saved prompts.** A saved model prompt now stores its attached image (as part of the prompt library); selecting the prompt restores the picture, and the picker shows the image's filename.

### Fixed

- **Model picker opens on click.** Focusing the model field in Registered LLM Providers now shows the full model list (and selects the existing text) instead of filtering by the already-present full id — no more clearing the field first to get the dropdown.

## [0.18.0] - 2026-06-30

### Added

- **Auto-filled token prices.** Gemini and Anthropic don't publish per-token prices through their own APIs, so selecting one of their models now maps it to the equivalent OpenRouter slug (exact, then fuzzy match across the dash/dot and date-suffix differences, e.g. `claude-haiku-4-5` → `anthropic/claude-haiku-4.5`) and fills the cost-per-1M fields from OpenRouter's **pass-through** pricing (taken as-is — research confirmed no added markup). A per-provider **"persist price"** checkbox freezes the price across model changes; otherwise it re-queries on each change. The OpenRouter catalogue is cached in `localStorage` (24 h TTL) so look-ups work **offline** (falls back to cache; never queries or overwrites when offline), and the price is always manually overridable.
- **Thinking on/off selector** for both runner and judge models in Model Testing (Auto / On / Off). Mapped per engine: OpenRouter `reasoning.enabled`, OpenAI-compatible `think`, Gemini `thinkingConfig.thinkingBudget` (0 off / −1 on), and Anthropic — adaptive thinking (`{type:"adaptive"}`) on current models (4.6+/Fable), legacy `budget_tokens` extended thinking on 4.5-and-earlier.
- **Image attachment for runner models** (vision). Attach via a button or drag-and-drop; send prompt + image, prompt-only, or image-only. The image is delivered in each engine's native multimodal shape (OpenAI `image_url`, Anthropic `image`/base64, Gemini `inline_data`).
- **Save / Save-as for the report.** "Save as…" opens a dialog defaulting to Downloads with a smart filename (`<preset>-<timestamp>.md` when a preset was used, else `custom-<slug>-<timestamp>.md`) and remembers the chosen path; "Save" then one-click writes to that last-used path (greyed until first use, with the path shown).

### Fixed

- **Anthropic thinking on current models.** Toggling thinking on for a modern Claude model (Opus 4.6/4.7/4.8, Sonnet 4.6, Fable 5) previously sent the deprecated `{type:"enabled",budget_tokens:N}` shape plus a forced `temperature=1`, both of which now return HTTP 400. Modern models now use adaptive thinking and omit the rejected sampling param; 4.5-and-earlier keep the legacy budget form.
- **Attached images are no longer silently dropped.** A malformed image data URL now fails the run with a clear error on the Anthropic/Gemini engines instead of quietly sending a text-only request.
- **Price auto-fill no longer clobbers concurrent edits.** The cost result from a slow (network) price look-up is applied as an isolated by-id patch against the live settings, so edits made to this or other providers during the look-up are preserved.
- **Report timestamp** now reflects when the run finished rather than the latest re-render.

## [0.17.0] - 2026-06-24

### Added

- **Searchable model picker.** OpenRouter exposes hundreds of models, so the model field (in Registered LLM Providers and in OpenRouter transcription) is now a searchable combobox: it fetches the live model list, filters as you type, and still accepts any custom id.
- **Model-testing prompt library + presets.** Save and reuse model prompts and judge prompts separately, plus combined **presets** (a model prompt + judge prompt under one name). When a saved prompt/preset is selected its name is shown (compact); with no selection you get the full editable textarea.
- **Live status feed in Model Testing.** A running activity log shows each model/judge as it finishes (✓/✗ with timing) plus phase markers, so you can see what's happening between dispatch and verdict.
- **Resizable sidebar.** Drag the left pane's edge to widen/narrow it; the width persists across launches (fixes long provider labels overflowing into a scrollbar).
- **Translate-to-English in Advanced → Transcription**, greyed out for models that don't support translation (`supports_translation`).

### Changed

- **Tools menu order**: History, Model Testing, Keyboard Typer, Token Count, Current Audio.
- **Main content is now fluid** — it scales with the window/sidebar instead of a fixed max width.

### Fixed

- **Local judges (LM Studio, FLM) now evaluate every answer.** The arbiter instructions were in the system message, which small local models down-weight — so they "didn't see" the other answers and returned junk while cloud models were fine. Instructions + all numbered answers are now in one user message.
- **Disabled/unconfigured providers no longer appear in Model Testing** (e.g. unused OpenRouter seats); the run/judge lists and the backend scheduler now skip disabled providers.

## [0.16.0] - 2026-06-24

### Added

- **OpenRouter transcription.** A new "OpenRouter Transcription" model (in the Models page) transcribes speech via OpenRouter. OpenRouter does **not** accept OpenAI's multipart `/audio/transcriptions` upload, so this is a distinct engine that sends **JSON + base64 audio** with the correct shape. Two endpoints, selectable in Advanced → Transcription: the dedicated **`/audio/transcriptions` STT route** (Whisper-style models such as `openai/whisper-large-v3`) and the **chat-completions `input_audio` route** (audio-capable LLMs like `google/gemini-2.5-flash` or `gpt-4o-audio`). Audio is sent as **Ogg/Opus by default** (~10× smaller than WAV — light on the network), with a WAV option for maximum compatibility. It reuses a registered OpenRouter provider's API key. Unlike the OpenAI-compatible API engine, it runs **per VAD segment / per chunk**, so it supports live ("as-you-go") and chunked transcription — each clip stays well under OpenRouter's ~60 s provider timeout — as well as full-recording mode.

## [0.15.0] - 2026-06-24

### Added

- **Registered LLM Providers — one unified registry.** The old "Token Count Providers" list and the separate post-processing provider config are now a single registry of LLM providers (Advanced → Providers). Each provider has a stable id (shown as #1, #2, …), editable name/base-URL/key/model, cost per 1M input/output tokens (auto-reported for OpenRouter, so its fields are greyed out), and a "run sequentially within family" flag with a family name (so several FLM or LM Studio slots sharing one loader serialize, while different families and cloud providers run in parallel). Ships with three pre-filled OpenRouter seats. Token counting, post-processing, and the new Model Testing tool all reference this one list.
- **Model Testing tool** (Tools → Model Testing). Run one prompt across any set of selected providers and compare cost, speed, and output. Pick run-models and judge-models independently from the registry. Concurrency honours each provider's family (sequential within a family, parallel across families); the reported round-trip is the wall-clock until the last model finished, not the sum. An optional judge panel runs a second (arbiter) prompt over the original input plus every model's answer, assembled as XML-tagged Markdown. Produces one Markdown artifact (input → summary table with input/output tokens, cost and time → judge panel → per-model answers) that you can copy or save; OpenRouter rows show the real monetary cost.
- **Temperature control for post-processing** (0–1 slider; lower is more deterministic/firm).

### Changed

- **Advanced settings is now tabbed.** A scrollable tab strip (App, Output, Transcription, Typer, Providers, History, Experimental) replaces the single long scroll, so every section — including the experimental Tauri-vs-HandyKeys keyboard-implementation selector — is reachable.
- **Top-level menu split into Tools and Configuration.** Tools (Typer, Token Count, Current Audio, History, Model Testing) sits above Configuration (General, Models, Advanced, Post-Processing, Debug, About).
- **Post-processing now selects a provider from the registry by id** instead of its own provider list — edit the provider once and the change applies everywhere. Existing token-count provider configs (and their API keys) migrate automatically.
- **New default post-processing prompt ("Structure & Clean").** It structures dictated text (short summary on top, then content-appropriate paragraphs or numbered/bulleted lists) while preserving your wording, flags low-confidence transcription guesses inline as `!!! …— confirm?`, and switches to following instructions when the text opens with a directive like "instructions"/"processing".

## [0.13.1] - 2026-06-11

### Added

- **Two "count with all" modes.** "Count with all" keeps the serialized sweep (one provider at a time — right when several slots share one local service that must load and unload each model in turn). The new "Count with all (parallel)" queries every enabled provider simultaneously, so the sweep takes as long as the slowest provider instead of the sum — right for independent services and cloud APIs. In both modes the built-in tokenizers (cl100k/o200k/estimate) run first, rows stream into the table as they complete, and failures stay silent.

## [0.13.0] - 2026-06-11

### Fixed

- **Token counts via local servers were wildly inflated (one word → 18-25 tokens).** `usage.prompt_tokens` from `/chat/completions` includes the server's chat-template wrapping (role markers, BOS, system scaffold — measured +17 tokens on LM Studio, +13 on FLM). Counting now uses the raw `/v1/completions` endpoint and calibrates away the server's fixed overhead with a known 1-token probe (`tokens = count(text) − count("a") + 1`) — verified exact against both running servers. Servers without a completions endpoint fall back to the chat endpoint with the same calibration.
- **FLM counting failed with "Response missing usage.prompt_tokens".** FLM's `/v1/chat/completions` returns HTTP 500 ("invalid string position") wrapped in a 200 response. The new raw-completions path avoids that endpoint entirely, and error objects inside 200 responses are now detected and reported properly.

### Changed

- **"Count with all" now also runs the built-in tokenizers** (cl100k, o200k, estimate) as the first rows of the comparison table, ahead of the configured providers. The estimate heuristic is excluded from the Δ-vs-smallest baseline so it can't skew the comparison between exact tokenizers.
- **Main window is one third larger by default** (680×570 → 907×760). Minimum size unchanged.
- **Token Count page redesigned**: the tokenizer dropdown and Count button are gone. All counting options — the three local tokenizers and all seven provider slots — are one row of clickable chips: click a chip to count immediately. Unconfigured providers stay visible but grayed out (tooltip points to Advanced settings). Below the chips: "Count with all" and "Open file...". The paste area is also substantially taller.

## [0.12.1] - 2026-06-11

### Fixed

- **The "Type Text Shortcut" input in Advanced > Keyboard Typer showed an empty "not found" row for existing users.** Bindings added in newer versions (like `type_text`) were only merged into saved settings during shortcut initialization, which runs after the frontend has already fetched its settings — so on the first launch after upgrading, the UI saw a bindings map without the new shortcut and offered nothing to configure. Missing default bindings are now merged on every settings read.

## [0.12.0] - 2026-06-11

### Added

- **History search**: a search bar on the History page filters entries in place as you type, with case-insensitive matching across the transcript, post-processed text and title. Queries containing regex metacharacters are treated as live regular expressions (a `.*` badge indicates this); invalid patterns fall back to literal text. Matches are highlighted, long transcripts show a snippet centered on the first hit, and a counter shows "N of M".
- **Keyboard Typer sub-app**: a new sidebar page that types arbitrary text into any application via simulated keystrokes — for remote sessions and apps where pasting is blocked. Configurable start delay (default 10 s, quick 1/3/5 s buttons) and per-keystroke delay (presets 5/15/50/500 ms, default 15 ms — reliable over RDP). A global shortcut (default `Ctrl+Alt+T`, configurable in Advanced) types the text into the focused window and toggles cancel; Escape also cancels. The text lives in memory only and is **never persisted** (safe for passwords).
- **Token Count providers**: the Token Count page can now count via external LLM APIs. Seven preconfigured slots in Advanced > Token Count Providers: Gemini (`countTokens`, free), Anthropic (`count_tokens`, free), OpenAI (bundled tiktoken, offline), FLM service 1/2 (local FLM/FLMTray at ports 52626/52625) and LM Studio 1/2 (port 1234). OpenAI-compatible servers are probed with a 1-token completion and `usage.prompt_tokens`. Each slot has a custom display name, base URL, API key and a model picker populated live from the endpoint's models API.
- **Count with all**: one click runs the text through every enabled provider sequentially and renders a comparison table (provider, model, tokens, Δ vs smallest count, time) with rows appearing as they finish. Failures are silent — failed providers are simply excluded, summarized as "X of Y providers responded" with details in a collapsed list.
- **Token Count file upload**: an "Open file..." button counts a text file (up to 10 MB) instead of pasted text.

### Fixed

- **History limit input lost focus after each digit.** The number field wrote to the settings store on every keystroke, re-rendering the settings tree mid-typing. It now commits on blur/Enter, and accepts up to 4 digits (was capped at 1000).

### Removed

- **Updater removed completely.** This is a custom build with no GitHub releases: the auto-update check on startup, the tray "Check for Updates" item, the Debug toggle, the `tauri-plugin-updater`/`@tauri-apps/plugin-updater` dependencies, the updater capability and the update endpoint config are all gone. The only automatic outbound network call the app made is thereby eliminated (a full audit found everything else — model downloads, LLM post-processing, API transcription — is user-triggered and user-configured; no telemetry). The `Referer: github.com/cjpais/Handy` header on LLM requests was also dropped.

## [0.11.2] - 2026-06-10

### Changed

- **Transcription now genuinely happens on the fly during recording.** Transcription was tied to the 10-minute Opus _file_ chunks, so a recording under ~10 min was a single chunk that only transcribed _at stop_ — the GPU sat idle during recording and the whole thing processed at the end. Transcription is now decoupled from file storage: distinct **transcription segments are cut at silence every ~20–45 s** and transcribed in the background while you keep talking, then concatenated in order on stop (cut at silence, so no words are split). A long recording now finishes almost instantly because only the final segment remains. The ~10-minute `.opus` file chunking (and single-file-under-10-min) is unchanged — it's purely storage now.

## [0.11.1] - 2026-06-10

### Fixed

- **Chunked transcription dropped the result for recordings whose transcription took >30 s.** The chunked Post-Recording path waited a fixed 30 s for the final chunk's background transcription, but a single-chunk recording (under ~10 min) only starts transcribing at stop, and a multi-minute chunk can take longer than 30 s — so the wait timed out, an empty transcript was saved to history, and nothing was pasted (the real result arrived seconds later with no listener). The wait now blocks until transcription actually completes, with a generous 15-minute deadlock backstop (matching the legacy non-chunked path, which never timed out). On the backstop it saves whatever chunks finished rather than nothing.

## [0.11.0] - 2026-06-10

### Changed

- **Recordings are now chunked Opus instead of WAV.** A recording is written to compact `.opus` files (16 kHz mono, ~24 kbps) split at silence into ~10-minute chunks (`handy-{ts}-chunk-N.opus`) plus one glued full file (`handy-{ts}.opus`). Opus is ~11–16× smaller than the old WAV and, being page-based, is readable after a crash with no repair tool. Encoding is pure-Rust (`audiopus`), no FFmpeg dependency.
- **Default transcription is now chunked**: each chunk is transcribed in the background as it closes, so when you stop a long recording it's already mostly transcribed; the per-chunk transcripts are concatenated (cut at silence, so no words are split). Live mode and the API engine are unchanged.
- **Crash-safety reworked around chunks**: in-progress chunks use a `-temp.opus` name; on the next launch any leftover `-temp` chunk is repaired (the torn trailing Ogg page is dropped), glued, and added to history as "(Recovered)". This supersedes the v0.10.0 single-WAV crash-safety copy.
- History entries now point at the glued `handy-{ts}.opus`; the audio player and Linux blob playback handle Ogg/Opus. Deleting or pruning a recording also removes its chunk siblings. The "only ever delete files Handy created" guard now covers `.opus`/`.ogg` too — your own files in the recordings folder are still never touched.

## [0.10.0] - 2026-06-10

### Added

- **Crash-safe recording**: Recordings are now streamed to a growing, playable `.wav` in the recordings folder as you speak, instead of only being written after transcription finishes. If Handy crashes mid-recording, the audio is recovered into your history (marked "Recovered") on the next launch. On a normal stop the temporary safety file is removed and the canonical history WAV is written as before. New "Crash-Safe Recording" toggle (on by default) and an "Open Recordings Folder" button in Advanced > History.
- **Auto-detect language note**: Models that are multilingual but auto-detect only (e.g. Parakeet V3) now show a clear note that the language is detected automatically and cannot be forced, with guidance to switch to a Whisper/SenseVoice/FLM/API model to lock the input language.

### Changed

- **Recordings folder safety**: History cleanup, manual delete, and crash recovery now only ever touch files Handy created (`handy-*.wav`). Any other files you keep in the recordings folder (e.g. `.txt` notes) are never renamed or deleted.

### Fixed

- **FLM translate flag**: `FLM Whisper V3 Turbo` was incorrectly advertised as supporting translation. whisper-v3-turbo was trained without the translate objective, so the translate-to-English toggle no longer appears for it. The backend also ignores a stale "translate to English" setting when the active model cannot translate.

## [0.8.2] - 2026-02-26

### Added

- **Transcription Mode setting**: Choose between Live (progressive text, instant stop) and Post-Recording (re-transcribe full audio on stop for best accuracy). Configurable in Advanced > Transcription settings.
- **API Transcription engine**: New `ApiWhisper` engine type that works with any OpenAI-compatible `/v1/audio/transcriptions` endpoint. Configure URL, API key, and model name in Advanced settings. Works with FLM, Groq, OpenAI, faster-whisper-server, or any compatible endpoint.
- **FLM model selection**: FLM Whisper V3 Turbo (NPU) available as a model choice when FLM is installed. FLM auto-downloads missing models on first use.
- **FLM debug logging**: Verbose info-level logging for FLM detection, process spawning, health polling, and stdout/stderr drain. Stdout is now piped (was discarded) so FLM startup messages are visible.
- **Floating window copy button**: Semi-transparent copy button (top-right) on the floating transcription window using clipboard plugin.
- **Floating window wider**: Default size increased from 400x300 to 800x300, min from 250x150 to 400x150.
- **Clipboard capabilities**: Added `clipboard-manager:allow-write-text` and `clipboard-manager:allow-read-text` to Tauri capabilities for floating window clipboard access.

### Changed

- **whisper.cpp upgraded**: Updated to whisper.cpp v1.8.2+183 via local whisper-rs 0.15.1 / whisper-rs-sys 0.14.1 (from whisper-rs 0.13.2). Includes Vulkan iGPU acceleration support for AMD and Intel integrated graphics.
- **transcribe-rs forked locally**: Updated whisper engine for whisper-rs 0.15 API changes (`set_suppress_nst`, `get_segment()` API, `full_n_segments()` return type).
- **Build path shortened**: `CARGO_TARGET_DIR=C:\tmp\hb` set in `.cargo/config.toml` and `check.cmd` to avoid MSVC 250-char path limit with whisper.cpp Vulkan shader builds.
- **cmake compatibility**: Patched whisper.cpp CMakeLists.txt with `cmake_minimum_required(VERSION 3.5...4.1)` for cmake 4.x compatibility.
- **Release log level**: File logs default to INFO in release builds, DEBUG in dev builds (via `cfg!(debug_assertions)`).
- **App identifier**: Changed from `com.pais.handy` to `pr.handy`.
- **Author**: Changed from `cjpais` to `pr`.
- **Tauri NPM packages updated**: `@tauri-apps/api` 2.10.1, `@tauri-apps/plugin-dialog` 2.6.0, `@tauri-apps/plugin-updater` 2.10.0.

### Fixed

- **Engine lock race condition**: In Live mode, `stop()` now waits for in-flight segment transcription to finish before accessing the engine. Uses `SEGMENT_BUSY` atomic flag with async spin-wait (off main thread).
- **App crash on recording stop**: Moved spin-wait and live text grabbing from main thread into async task. All inner mutex locks use `.ok()` instead of `.unwrap()` to handle poisoned mutexes gracefully.
- **Post-Recording mode skips live transcription**: Segment callback is no longer set up in Post-Recording mode, preventing unnecessary engine calls during recording.
- **API model skips live segments**: API transcription models always skip the segment callback (single POST on stop), avoiding repeated progressively-longer audio uploads.
- **Settings text input focus loss**: API transcription URL/key/model inputs use local `useState` + `onBlur` pattern instead of `onChange` → `updateSetting`, preventing global re-renders that steal focus.
- **Transcription mode setting persistence**: Command changed to accept `String` and parse manually (matching pattern used by other enum settings), with info-level logging on mode change.
- **FLM stderr pipe blocking**: FLM stderr is now drained in a background thread to prevent the process from blocking on a full pipe buffer. Timeout error now captures and logs accumulated stderr.

## [0.3.0] - 2025-07-11

### Added

- **Translate to English** setting: Added automatic translation of speech to English
- Settings refactored into React hooks for better state management
- Audio device switching capability
- Hysteresis to VAD (Voice Activity Detection) for more stable recording

### Changed

- Major audio backend refactor for improved performance and reliability
- Moved audio toolkit into src-tauri directory for better permissions handling
- Model files no longer need to be downloaded separately for releases
- Updated settings components and transcription logic

### Fixed

- Audio toolkit permissions issues
- Various stability improvements

## [0.2.3] - 2025-07-03

### Fixed

- Keycode bug that was causing input issues
- Whisper model optimization: switched to unquantized Whisper Turbo, updated Whisper Medium quantization to 4_1

## [0.2.2] - 2025-07-02

### Fixed

- Removed 50ms delay feature flag for Windows (now applies to all platforms for consistency)

## [0.2.1] - 2025-07-01

### Added

- Ctrl+Space key binding for Windows platform

### Fixed

- Windows crash issue
- Model loading on startup when available
- Windows paste functionality bug

## [0.2.0] - 2025-06-30

### Added

- **Microphone activation on demand**: More efficient resource usage
- Less permissive VAD settings for better accuracy

### Changed

- Improved microphone management and activation system

## [0.1.6] - 2025-06-30

### Added

- **Multiple models support**: Users can now select from different transcription models
- Model selection onboarding flow
- Cleanup and refactoring of model management

### Changed

- Enhanced user experience with model selection interface
- Better language and UI tweaks

## [0.1.5] - 2025-06-27

### Added

- **Different start and stop recording sounds**: Enhanced audio feedback
- Recording sound samples for better user experience

## [0.1.4] - 2025-06-27

### Fixed

- Build issues
- Auto-update functionality improvements

## [0.1.3] - 2025-06-26

### Fixed

- Paste functionality using enigo library for better cross-platform compatibility

## [0.1.2] - 2025-06-26

### Added

- **Auto-update functionality**: Application can now automatically update itself
- Footer displaying current version
- Improved menu system

### Changed

- Better user interface for version management
- Enhanced update workflow

## [0.1.1] - 2025-06-25

### Added

- **Comprehensive build system**: Support for Windows, macOS, and Linux
- Windows code signing for trusted installation
- Ubuntu/Linux build support with Vulkan
- Model file download and packaging for releases
- GitHub Actions CI/CD workflow

### Changed

- Improved build process and release workflow
- Better cross-platform compatibility

### Fixed

- Various build-related issues across platforms

## [0.1.0] - 2025-05-16

### Added

- **Initial release** of Handy
- Basic speech-to-text transcription functionality
- Voice Activity Detection (VAD) for automatic recording
- Cross-platform support (macOS, Windows, Linux)
- **Tauri-based desktop application** with React frontend
- **Global keyboard shortcuts** for activation
- **Clipboard integration** for automatic text insertion
- **LLM integration** for enhanced transcription processing
- **Configurable settings** including:
  - Custom key bindings
  - Audio device selection
  - Microphone settings
  - Push-to-talk functionality
- **System tray integration** with recording indicators
- **Accessibility permissions** handling for macOS
- **Settings persistence** with unified settings store
- **Background operation** capability
- **Multiple audio format support** with on-the-fly resampling
- **Whisper model integration** for high-quality transcription
- **MIT License** for open-source distribution

### Technical Implementation

- Built with Tauri (Rust backend) and React (TypeScript frontend)
- Audio processing with cpal and whisper-rs
- Real-time transcription with performance optimizations
- Cross-platform keyboard event handling
- Modular architecture with managers for audio, models, and transcription
