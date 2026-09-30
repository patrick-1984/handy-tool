# Changelog

## [2.0.2] - 2026-09-30

A fix release: silent updates install again, and a few sidebar changes.

### Fixed

- **Silent updates install again.** An update installed in the background (the scheduled
  night-time update, or any update while a speech model was loaded) could fail without a word:
  Handy stayed on the old version and did not restart. Every failure on record happened with a
  speech model loaded, and every update made right after a restart, before a model loaded, worked
  (which is why "Install and restart now" kept working). The likely cause: the old Handy was still
  closing when the installer checked for it, and a silent install gives up if the check then cannot
  close it or a file is still in use. The installer now waits, up to a minute, until Handy's
  program file is no longer in use before that check, and writes the outcome to `installer.log` in
  the log folder. If the wait runs out, the installer carries on as before. This takes effect from
  the update to 2.0.2 on, because the waiting is in the new version's installer.
- The message after an update that did not take effect no longer blames a security policy alone:
  the installer may not have started, or may have found Handy still closing.

### Changed

- **About is a sidebar page**, right after What's new, and its first group is **Updates**: check
  for a new version, silent updates and their time. The same controls stay on General. It was a tab
  of Advanced settings.
- **History is the first page** in the sidebar, with a line under it.
- **The version number is back** at the bottom-left of the sidebar.
- **Download links that always fetch the newest release**, one per system, at the top of the
  README. Each release now also carries its installers under names without the version
  (`Handy-Tool-windows-x64-setup.exe`, `Handy-Tool-macos-apple-silicon.dmg`, …), so a
  `releases/latest/download/` link keeps working from one release to the next.

## [2.0.1] - 2026-09-29

A fix release for 2.0.0, with a clearer sidebar.

### Changed

- **Transcription has its own page** in the sidebar, just under Setups: how your text is
  delivered (Paste Method, the clipboard, auto submit, jump slot actions, translate to English).
  It was More's Output tab.
- **More Tools** is its own sidebar entry, right after Jumper (Keyboard Typer, Token Count, Model
  Testing, Current Audio), and **More is now Advanced settings**, at the bottom of the sidebar.
  Each reopens the tab used last.
- **Two provider pages** in Advanced settings: Transcription providers (API and OpenRouter
  transcription) and, beside it, LLM providers (Registered LLM Providers).
- **Linux:** the Cancel, Pause and Undo word shortcuts are no longer offered; Linux never
  registered them. The pill's pause button still works.
- **Linux Wayland:** no live text box or separate "Too quiet" box (the compositor would place such
  windows anywhere); the pill shows "Too quiet" itself.
- **Explanations corrected** in all 17 languages: the live text is the transcript only with a
  Parakeet model (other models transcribe the whole take again at stop, and the 2.0.0 Appearance
  setup said otherwise); Post-Recording transcribes while you talk only with Crash-Safe Recording
  on; the Files page and watched folders read Ogg Opus (not Ogg Vorbis) and the sound of MP4
  files.

### Fixed

- **Undo last word pressed just before stopping** is carried out; it used to be lost, or the
  removed word came back in the pasted text. An undo from a take that has ended can no longer cut
  into the next take.
- **Clicking the recording pill during a take** (pause, the T, its menus) could leave it in front,
  so the text was typed into the pill and lost. Handy never types into its own pill now: the text
  is kept on the clipboard, as the clipboard setting allows, with a message.
- The update banner's **Remind me later** is no longer forgotten when typing in the sidebar
  search.
- The **Appearance setup** no longer switches off a live text box that was on while an online
  model is selected.
- A **Files** job whose transcription could not start no longer leaves the Files page, and updates,
  waiting for it.

### Security

- Release pipeline: every workflow token is read-only except the approval-gated publish job;
  checkouts keep no credentials; the release builds and signs exactly the commit it was published
  from and refuses to sign if the tag has moved.

## [2.0.0] - 2026-09-29

The first release since 1.6.2. Versions 1.6.3 to 1.12.0 below were built locally and never
released on their own. This one was built locally as 1.13.0 and is released as 2.0.0: the whole
app has been redesigned and its pages rearranged.

### Changed

- **The live text is the transcript only with Parakeet.** Only Parakeet gives word timings, so
  only its live text is cut between words. With Whisper, Moonshine or SenseVoice the live windows
  are cut wherever a snapshot ends, so those takes are transcribed again in full at stop, as in
  1.6.2, and the live text only shows what is coming.
- **What's new follows the GitHub releases:** 2.0.0 lists everything since 1.6.2 (1.6.3-1.13.0 were
  never released on their own), most important first - 30 items, 17 of them new - then 1.6.2 and
  1.6.1 as they were released.
- **The AltGr "Risky" warning only where it applies:** on Windows it now asks your installed
  keyboards and warns only when one of them types a character with AltGr on that key (AltGr+Shift
  for a chord with Shift), naming the language it belongs to ("A keyboard you use for Polish
  (Poland) types a character with AltGr+O..."); Ctrl+Alt+Space gets its own explanation. A US
  English keyboard has no AltGr, so no warning.
- **A moving pill preview in General › App** (under Progress Style): recording and transcribing
  side by side, the progress running 0-100% over and over with your style, glow, colour and size.
  It replaces the small swatch next to Progress Colour.
- **The Appearance setup is shorter:** Lines and Fade are left to the settings; its last step says
  where the rest is.
- **Clearer setups:** the Jumper setup (and its card and row) says what it is for - Handy Tool
  pastes your dictation into the right field for you, even from another window; the Appearance
  setup's live text box step says that with the box on the take is not transcribed again at the
  end (ready at once, a little less accurate).
- **More's App tab is now a group on General** (Appearance, Start Hidden, Reopen Last Page, Launch
  on Startup, Show Tray Icon, the overlay's position, size and progress, Keyboard Implementation),
  above Updates: these are settings you want at hand. More opens on Output now.
- **A second design round**, in light and dark:
  - **Setups:** each setup is a row with an icon tile and a secondary **Start →** (one primary
    button per page); the Jumper's "Windows only" is a tag instead of a sentence.
  - **Setup steps:** the name, the step count and its segments (the current one longer) and
    Close setup sit above a divider; Back, Skip (now a text button) and Save & next under a second
    one. The first-start guide the same, its buttons always at the bottom of the window.
  - **Choices are cards with a radio:** picked is the radio's dot, an accent edge and a soft tint
    together; hover only darkens the edge. The first-start guide's model cards the same.
  - **Providers** (Post-processing setup): the model in monospace, the status a badge with an
    icon - Ready, Needs a key, and Needs a model in amber, the only one that blocks.
  - **Jumper setup places:** a target tile, "Remembered: **app**" with a tick and a button to show
    that place again; during the countdown the row is tinted and shows its number.
  - **Language chips** have square corners and keep their weight when picked.
  - **Where should the text be saved?** 440 px over a dimmed backdrop; each choice is a row with
    an icon and an arrow (paths in monospace, shortened in the middle), Remember my choice and
    Cancel in a band at the bottom. Focus starts on the first row; Esc cancels.
  - **Files:** the model menu is grouped (Your dictation model, Downloaded, Not downloaded), each
    with its size and a download glyph instead of "(not downloaded)"; a model to download shows
    **Download · 487 MB** as the main button. A 4 px progress bar, a success tick on the result,
    the saved path in monospace, 32 px icon buttons in the list.
  - **Help mode:** the ? fills when it is on, the hint sits on a tinted pill with an Esc key cap,
    the setting pointed at gets a rounded outline while the others dim, and its description
    hangs from its name.
  - **What's new:** **Show me →**, and the installed version is marked "Installed".
  - **Recording pill:** the T glows with a two-step cyan glow (3 + 8 px); paused shows four grey
    dots; a microphone problem has an amber icon and white text (was all red); quiet sound bars
    are a darker cyan instead of faded; the "Too quiet" box is 22 px tall and 6 px under the pill,
    with an amber speaker icon.
  - Both versions where the design differed from what was chosen before: **Progress Style** has
    a third option, **Light circling the edge** (the designer's 1 px light with a fading tail, one
    lap every 1.6 s, no glow; with reduced motion it becomes the line), next to the filling edge
    light; and **Wide Sound Bars** (General › App, default Off) gives the designer's 2.5 px bars
    and gaps (77.5 px) instead of the narrower 2 px ones (62 px).
- **What's new covers the last three versions:** 1.11.0 added (the live text as the transcript,
  History tabs and statistics, the slow-PC warning, fading live text, the live text box
  shortcut, the AltGr warning switch). Search and What's new now outline Recording statistics
  and the Transcription cost report too.
- **Keyboard Typer moved to More › Tools** (first in that row), so the sidebar keeps the
  pages used most. Its shortcut stays on the Shortcuts page.
- **A smaller pill with shorter sound bars:** 188 px wide (was 204) and 16 bars (was 23). A
  microphone problem ("Microphone blocked", "No microphone") uses the whole pill; while the
  microphone starts, "Starting mic..." gets the pause button's room; "Transcribing 42%" drops
  its spinner once there is a percentage. Every pill text fits in all 17 languages (a few
  translations were shortened); anything longer ends in "…".
- **The T glows** while the live text box is on: no filled circle any more, the letter itself
  glows cyan, like the edge light.
- **Model ratings on one scale:** the speed and accuracy bars rank all the models against each
  other (before, "Ultra-fast" and "Fast" models both showed 5/5), and the descriptions use the
  same words - Fastest, Very fast, Fast, Medium speed, Slow; top, very, fairly accurate, basic.
  Accuracy from the English error rates of the Open ASR Leaderboard and the Moonshine v2 paper;
  speed from published comparisons, checked on a mid-range laptop.
- **Transcribe Shortcut is now called Record/Transcribe Shortcut** (General, Shortcuts, the
  setup), in every language.
- **Setup buttons**: each step that saves something now has **Skip** (go on without saving
  that step) and **Save & next** instead of Next.
- **Redesign: the designer's whole-app design**, in light and dark:
  - Warm neutral greys with the brand cyan kept for switches, sliders and selection (and a
    stronger cyan for text, so it stays readable). Depth comes from 1px borders and a very soft
    shadow, like Windows 11 Settings. Settings sit on white cards; rows are at least 52px tall.
  - Inputs, dropdowns and secondary buttons are 32px with a slightly darker bottom edge that
    turns into a 2px cyan line while you type in them (no extra focus ring on top). Switches
    say On/Off beside them; shortcuts show their keys as small key caps, with a "Conflict" or
    "Risky" badge where it applies.
  - Settings a switch reveals use the designer's **connected rail**: the parent setting has a
    bold title with a one-line summary, and below it a cyan rail with the revealed settings in
    a card of their own (Post-processing: its Hotkey, API and Prompt sections).
  - Every sidebar page has a title with its icon; the sidebar shows the app icon and name, and
    the open page has a light fill with a thin cyan bar. Tabs are plain words. Duplicate
    headings removed; content stops at a comfortable width on very wide windows.
  - Models are cards with accuracy and speed bars; the one in use has a cyan edge and "Active".
  - **Custom Words** is one field with the words as chips: Enter adds a word, × removes one.
  - Keyboard Typer, MCP & CLI and Backup use the same rows and cards as the other pages (MCP's
    server is a real switch that says Running or Stopped).
  - **Recording pill:** a bold T, filled pause and play, a plain ×, a small spinner while the
    microphone starts and while transcribing, the progress as a thin line along the pill's
    bottom, grey flat bars while paused, an icon with each microphone problem, and the pause
    button dimmed until the microphone is live. While transcribing, a thin cyan line fills along the
    pill's bottom - or, with **General › App › Progress Style** set to "Light around the edge", a
    glowing light runs round the pill from the bottom centre, centred on its border, its trail
    staying lit - and glowing evenly, no brighter at the head - up to the figure. Its aura
    stays close to the light and fades out before the edge of the pill's window (a wider one
    was cut off in a straight line); the pill's shadow is smaller for the same reason ("Transcribing 42%" stays in the middle). The sound bars are longer: 23 bars across most
    of the pill instead of 9, low to high pitch over the voice range (400 Hz - 4 kHz), each
    its own band (the first three of the old bars used to move as one). The higher bands are
    lifted by 8 dB per octave above 500 Hz, so the right-hand bars move as much as the left:
    measured over 12 takes, they used to average 0.01-0.03 against 0.23-0.28 on the left.
  - **Cancel while transcribing:** the pill keeps its × during "Transcribing N%". It hides
    the pill and the text is not pasted; the take still lands in History (the transcription
    finishes in the background) and skips post-processing. Cancelling at that point used to
    run the recording teardown, which could break the transcription still running.
  - **Live text box:** larger text (15px), a cyan caret where the next words appear and a thin
    border; the one-line box fades its left edge only when the text is cut off.
  - **Live Transcription window** follows the app's colours, with bigger text, a caret, and a
    footer showing "● Listening · <model>" and Copy (Copy no longer covers the text).
  - One colour system: 158 hard-coded dark-only colours (grey/blue) in 18 files now use the
    app's theme tokens, so inputs, tables and buttons also look right in light mode.
  - The tray icon is the designer's plain line glyph (white on a dark taskbar, black on a
    light one), with a dot while recording and three dots while transcribing.
  - The recording pill and the dark-mode focus ring are brand cyan instead of the pink left
    over from upstream Handy. The sidebar no longer shows a stray horizontal scroll bar.
- **Live takes no longer lose words.** Read on its own, a short stretch of speech often came
  back empty from Parakeet (the log showed most 1.5 s windows, and some whole 2-5 s sentences,
  read as nothing), and the live preview locked that "nothing" in when you paused - so those
  words were missing from the box and from the delivered text. Now a window is only settled
  when it holds a plausible number of words (at least 0.8 per second of speech); otherwise it
  stays open and is read again together with what follows (up to 20 s). Settling a sentence
  also reads the 4 s before it along for context (8 s at stop), keeping only the new words.
- **The live text box shows the final text:** when you stop, the take's final text appears
  whole at once and stays about 1.5 s before the box fades, so words said just before stopping
  are seen too (they used to start typing in just as the box closed). Cancel still hides it at
  once.

### Fixed

- **"Transcribing N%" no longer sits at 99%.** In logged timings, 57% of long transcriptions
  ran past the estimate and then showed 99% for 10-70% of the wait. Three causes, all fixed:
  - The estimate used one average speed, but a long stretch costs more per second of audio
    than a short one (0.14 s per second for short clips, 0.21 s at 20-35 s). It now learns
    from recent transcriptions of a similar length, scaled to the length at hand.
  - Past about 70% the figure now slows down smoothly instead of running into a 99% wall,
    so an overrun keeps creeping on. It still never goes backwards.
  - After stop, each segment still waiting is weighed by its length (a short tail used to
    count as much as a long segment, so the figure could jump to 50% and then crawl), and a
    live take's figure also covers the wait for the preview update still running.

  Replayed on the logged timings: time spent at 98-99% drops from 44% of the wait to 8%.
  The pill also counts up to each new figure (1% at a time, faster across a bigger gap)
  instead of jumping, and a take that finishes before 100% runs the figure up to 100% in a
  quick burst before the pill fades (visual only; the text is not delayed).

- **Switching the live text box on mid-take shows text right away.** It used to read the
  whole take again before showing anything - 23 s for a 60 s take on a test PC, so nothing
  appeared before the take ended - and threw away the parts already transcribed. Now those
  parts fill the box at once and only what was said after them is read.
- **The pill's T sits exactly in its circle.** The letter came from the font and landed up
  to a screen pixel right of centre (0.8 px at 125% display scaling); it is now drawn.
- **Right-clicking a button on the recording pill** no longer opens the web view's own menu
  (Refresh, Save as, Print, More tools), and the button's name ("Live text box") no longer
  stays in the pill afterwards instead of the sound bars.
- **Texts no longer point to pages that are gone.** About 17 descriptions and hints in all 17
  languages still sent you to the "Advanced" or "Post Process" page (removed in 1.10.0). They
  now name where the setting really is, in each language's own words: More › Output (Paste
  Method, jump slot actions), More › Providers › Registered LLM Providers, the Shortcuts page
  (Keyboard Typer shortcut), and "right below this switch" for Post-processing's options.
  Two pointers were wrong even before: Jumper › Delivery options said "on the General page"
  (it is More › Output), and Current Audio's "set each shortcut's mode in…" link opened More ›
  Output - it now reads General › Transcription and opens General, where Transcription Mode is.

### Added

- **Progress Colour** (General › App): pick the colour of the pill's glowing parts - the progress
  light and its glow, the glowing T, the sound bars and the spinner - with three sliders (hue, saturation and
  lightness), a swatch and a button back to the default cyan. The Appearance setup has a colour
  step with six ready colours, and its previews use the colour.
- **"Switch it on to see more settings."** under every switch that reveals more settings
  (Post-processing, the live text box, Pause button, text before/after, ...) while it is off, so
  an empty space below it no longer reads as "there is nothing more".
- **The first start's Try it shows what the take is doing**, as the pill does: recording (press
  the shortcut again when done), paused, transcribing with its percentage and a bar, processing,
  then the words - or a microphone problem, which stays in view until the next take.
  The last screen marks the Appearance setup Recommended.
- **A Post-processing setup row** at the top of More › Post-processing, like General › App's and
  the Jumper page's.
- **Appearance setup** (Setups): the look of the recording pill and the live text box, one
  thing per step - theme, where the pill appears, its size, the sound bars, the progress style and
  its glow, the live text box (on, what it shows, width, text size, lines, fade) and where "Too
  quiet" shows. Every choice is shown side by side, moving (the real pill with its bars, progress
  and glow; the box with words appearing), so nothing has to be tried by recording. It opens by
  saying what the pill's parts are for - the T switches the live text box. `Save & next` keeps a
  choice; Skip and Close setup keep what was set.
- **A setup row at the top of the settings it walks through:** General › App starts the
  Appearance setup, the Jumper page the Jumper setup (the button opens Setups and starts it).
- **The first start ends with the other setups on offer** (Appearance, Post-processing, Jumper):
  optional, as the defaults already work.
- **What's new** in the sidebar: the new and changed things of the last releases, each with
  **Show me** (opens the page where the setting lives and outlines it, as search does: a live
  text box setting hidden while the box is off outlines the box's switch; setups outline their
  cards, model ratings your model's card). A dot marks news you have not opened yet.
- **Glow Strength and Glowing Line** (General › App, under Progress Style): how strongly the
  progress light glows, 0-200% (100% is about as bright near the light as the first edge
  light, and the glow always fades out within the pill's window), and a switch that gives the
  line along the bottom the same glow.
- **Jumper setup** (Setups, Windows): what the Jumper is for (jump back with a key, have your
  dictation land there, or both), whether to remember the mouse position, "Show me the place"
  (a 5-second countdown, then the field you clicked into is remembered; add as many places as you
  like), their shortcuts, and a test.
- **Post-processing setup** (Setups): what the AI should do (a ready prompt or your own), which AI
  (key, address and model right there, with List models), the shortcut, and a try on a sample
  sentence.
- **Help mode** replaces the "i" beside every setting: a **?** at the right end of each page's
  title line. While it is on, the cursor is the help cursor and pointing anywhere on a setting
  outlines it and shows its description; nothing can be changed - the first click anywhere
  (or the ? again, or Esc) only leaves help mode. Scrolling still works.
- **Files** page in the sidebar (after History), with two parts:
  - **Transcribe a file**: pick a WAV, MP3, M4A, AAC (Signal voice notes), FLAC, OGG or Opus
    file. It is cut at its pauses into pieces of up to about 40 seconds and each piece is
    transcribed once, so the text is final as it comes (no live text, no second pass). A
    progress bar follows the file's length, and Stop ends it. The text is shown with Copy text.
    The original file is only ever read.
  - **Model** menu: your dictation model by default, then the other downloaded models, then the
    ones not downloaded yet (pick one and Download it first). The model you pick is remembered.
    A model other than your dictation model loads beside it, so dictation stays ready, and is
    unloaded again after the file.
  - **Where the text goes**: when you pick a file, a pop-up asks - In Handy's folder, Next to
    the recording, or Don't save - with **Remember my choice**. The **Save the text** setting
    shows the remembered choice and can go back to Ask every time. Nothing is overwritten: a
    taken name gets "(2)". **Keep a copy of the audio** (off) copies the recording into Handy's
    `files` folder, or one you choose.
  - **Transcribed files**: every file transcribed there, newest first, with its date, length,
    model and text - copy it, show the saved .txt, or remove it from the list.
  - **Watched folders** moved here from More › Translator (that tab is gone). Its groups are
    now called Watched folders and Folders.
- **MP3, M4A, AAC and FLAC** can be read by the watched folders too (before: WAV, OGG, Opus).
  The decoder was already inside the app, so it adds almost nothing to its size.
- **Overlay Size** (General › App): Normal, Large (125%) or Extra large (150%); the "Too quiet"
  box grows with it.
- **Text size for the live text box** (General › Transcription › Live text box): Small,
  Normal, Large or Extra large; the one-line box grows with it. **Box height** for Whole text:
  3, 6 (default), 10 or 16 lines before the oldest slide out.
- **Reopen Last Page** (General › App, on by default): Handy Tool opens on the page that was open
  when you closed it.
- **Setup guide on the first start** (replaces the plain model list; every step can be skipped,
  or all of it). Pick the languages you speak — several at once — and it suggests three speech
  models that understand all of them: the most accurate, a balanced one and the fastest ("See
  all models" lists and searches the rest). One is always selected - the balanced one to start
  with, which is Parakeet V3 (many languages) whenever it understands yours - and the button
  reads "Download and continue" until a model is here. The one you pick downloads while you
  go on, and is selected when it lands. Then the basic shortcuts (Transcribe, Push-to-Talk,
  Cancel), each with its default shown so you can keep or change it, the microphone, and
  "Try it": press the shortcut, say a sentence, and your words appear right there. Next
  skips any step. The new **Setups** page (in the sidebar, after General) runs it again, and
  will list the guided setups for more features as they come.
- **"Wait for the microphone to warm up"** (General › Sound, on by default). Some microphones (a
  Realtek one on a test PC) start 15-20 dB quiet after a cold start and fades in: the pill
  already showed sound bars (it switched on the first audio, however faint), and the first
  words spoken into the quiet part were dropped as noise. Now each cold start measures how
  long its microphone takes to settle (from the levels of its first 3 s), and the next cold
  starts keep "Starting mic..." up for the average of the last 5 plus 0.1 s (0.6 s before the
  first measurement). It never says the microphone is ready while it still sends digital
  silence: after a replug this Realtek input sent 0.5 s of exact zeros before its 0.5 s
  fade-in, and the fade-in is now counted from its first sound. A warm start (Keep microphone
  ready) is never delayed. The setting shows the measured time.
- **"Too quiet — speak up"** (General › Sound › Warn when you speak too quietly, on by
  default), in a small box of its own just under the pill (Show it in its own box, on by
  default; with the pill at the bottom it sits in the gap above the taskbar, so it never covers
  the live text box, and the pill keeps its sound bars) or inside the pill: shown for 2 s instead of the sound bars when, for about 0.7 s, it hears
  sound that is almost speech (speech probability 0.08-0.3, just under what the detector
  keeps) and clearly above the room's noise. Silence, music and typing don't set it off.
- **Right-click menus on the recording pill's buttons.** T: last words / whole text, Live
  text box settings…, Change shortcut…. Pause: Change shortcut…, Hide the pause button (not
  while paused). Cancel: finish and keep in History only / discard, Change shortcut…. The
  "…" items open Handy on that setting, scrolled to and outlined. Native menus, so the small
  pill window does not cut them off.
- **"Show the text as it's transcribed"** (General › Transcription, off by default). For a
  take without the live text box, the box appears when you stop and the transcript is typed
  into it as it comes in: the parts already transcribed during the take at once, the rest as
  it lands. Just to watch; the text is delivered as usual. The box now waits for text still
  being typed in (plus 1.5 s) before it fades.
- **A visible note on live transcription's trade-offs** in General › Transcription whenever it
  is on (live text box, or Transcription Mode = Live): a little less accurate on long stretches
  without pauses, more CPU and battery, Undo only with Parakeet.

### Removed

- **PC speed on the pill** and the speed measurements behind it (1.11.0-1.12.0). Short clips
  made the figure swing (it could read 200%), and it showed "-" until five measurements existed.
  The file it kept, `transcription_speed.json` in the app data folder, is no longer used.

## [1.12.0] - 2026-09-26 22:48

### Added

- **New app icon** (the designer's 1A, "wave caret"): app, installer, taskbar and Start icons
  are all rendered from the 1024 px master drawing - none of the designer's separate small-size
  drawings are used. The tray uses the same drawing with a small state badge (red dot while
  recording, three dots while transcribing), so it reads on light and dark taskbars alike.
- **Box width** for the live text box (General › Transcription, under Live text box): Narrow,
  Medium, Wide or Extra wide (360-860 px). Default Medium, as before.
- **PC speed on the pill** (General, default Off): when on, a PC icon with this PC's
  transcription speed against its own normal sits left of the T for the whole take (amber below
  70%, red below 50%); the pill widens to fit. Replaces 1.11.0's chip that appeared by itself
  only when slow.
- **Hover labels on the pill:** hovering the T, pause, cancel or speed chip names it in the
  middle of the pill (Live text box, Pause / Resume, Cancel).
- **Hold Ctrl+Backspace to keep removing words:** a tap removes one word; held, it repeats
  after 0.45 s, about six words a second, until you let go.

### Changed

- **"Whole text" shows the whole text:** the box starts at one line and grows as you talk, up to
  40% of the screen height; only past that do the oldest lines slide away (it was a fixed
  three-line box).
- **History draws 20 recordings at a time** and adds the next 20 as you scroll, instead of
  drawing all of them (each with an audio player) at once, which made the page lag.
- **The ear icon on the pill is gone** while recording; the moving bars already show it is
  listening. (The icon component is removed.)

### Fixed

- **Pressing T during a take now makes that take live.** Before, switching the box on mid-take
  only took effect from the next take. The take stops being transcribed in chunks, what you
  said so far is transcribed once to fill the box, and stop delivers the live text.

## [1.11.0] - 2026-09-26 20:13

### Added

- **History has three tabs: Recordings, Statistics and Settings.** It opens on Recordings, so
  the settings no longer sit below a long list; a search result for a History setting opens the
  Settings tab.
- **Recording statistics** (History › Statistics): recordings and their length per day, week,
  month and year, with CSV export - the Providers cost report without the cost (which stays in
  Providers). The all-time line also counts recordings History Limit / Auto-Delete Recordings
  have since removed, from totals the database already kept (`purged_totals`) but nothing showed.
- **Live Text Box On/Off shortcut** (no default key): does what the overlay's T button does.
  Listed on the Shortcuts page, and under the Live text box switch while it is on.
- **The live text box needs a model that runs on this PC**, and now says so: switching it on
  with API transcription or OpenRouter selected (they get the audio only after stop) shows a
  warning listing your downloaded models that work, instead of a box that never shows text. The
  overlay's T button refuses the same way.

- **Slow-PC warning on the recording pill.** Every transcription is timed against this PC's own
  normal for that model (the median of its last 20, saved in `transcription_speed.json` in the
  app data folder; short live-preview clips and long chunks are compared separately). While the
  PC runs below 70% of its normal speed, the pill shows a PC icon with the figure - amber, red
  under 50%; at normal speed nothing is shown. It needs 5 measurements per model and clip
  length before it says anything.
- **Fade when you stop talking** (General › Transcription, under Live text box, default Off):
  the live text fades away 3 seconds after it stops changing; the next words bring it back.
- **Warn about AltGr shortcuts** (Shortcuts › Warnings, default On): the AltGr warning can be
  switched off. The warning's tooltip has a "Turn off this warning" button that opens the
  Shortcuts page and highlights the switch.

### Changed

- **A live take's transcript is its live text.** At stop, only the audio since the last fixed
  word (your last second or two) is transcribed and added, so the text is ready at once; Custom
  Words and the filler filter are applied as usual. Until now the whole take was transcribed a
  second time at stop, which on a long take could hold "Transcribing 99%" for minutes. The full
  pass remains only as a fallback when the live preview produced no text.
- **Live text is typed in.** New words appear letter by letter (finishing within about a
  second) instead of all at once, so the eye can follow them; removed words go at once.
- **Undo last word moved** to General › Transcription, next to the live text box.
- **Sub-settings are indented under their switch** with a connecting line (pause and undo
  shortcuts, live text options, text before/after, Track last output location's slot, the
  system-audio options, and post-processing's settings), so they read as part of it.
- **The tab rows stay in view:** More's Settings/Tools rows and History's tabs stick to the top
  while the page scrolls.
- **Icons on six settings** that are worth spotting at a glance: Appearance, Overlay Position,
  Show Tray Icon, Microphone, Pause button and Live text box.
- **Section titles are clearer:** full text colour instead of grey, a size up, and a line icon
  each.
- **The window opens wider** (1040 × 760 instead of 907 × 760), so More's Settings row fits
  without scrolling. In a narrower window, or with the Debug tab showing, the tabs wrap onto a
  second line instead of scrolling.

### Fixed

- **The last words before a pause went missing from the live text box.** A preview update that
  arrived while the previous one was still being transcribed was dropped - including the one
  sent when you stop speaking, after which silence brings no more updates. Updates now queue
  and are worked through in order.
- **Undo last word removed the wrong word.** It removed the last word _shown_, which runs a
  second or two behind your speech, and cut the audio there - taking newer, not yet shown words
  with it. A press now first brings the live text up to date, then removes the newest word, so
  you see exactly what disappears. Each undo is logged.

## [1.10.0] - 2026-09-26 17:18

### Added

- **Live text box** (General › Transcription): a small box next to the recording overlay shows
  what you are saying, updated about every 1.5 seconds. It shows either one line with the
  newest words ("Last words") or the take so far in a three-line box ("Whole text"); neither
  changes size while you talk. The box never takes focus and lets clicks through. Only the last
  ~5 seconds are re-transcribed each time - words before a pause are frozen - so the cost stays
  flat however long the take. While it is on, every take runs in Live mode; the delivered text
  is still one full pass on stop. The overlay's T button now switches this setting. Default
  Off. The older Live Transcription window stays on the Current Audio page.
- **Undo last word** (General): each press of its shortcut (default `ctrl+backspace`, active
  only during a live take) removes the newest word and cuts the recording back to where that
  word started, so it stays out of the final text too. Uses Parakeet's word timings; with other
  engines the shortcut does nothing. Default Off. The Opus audio saved to History still holds
  the removed words (chunks are append-only).
- **Pause button** (General): adds pause/resume to the recording overlay, plus an optional
  Pause / Resume shortcut (no default). While paused nothing is recorded and the overlay says
  "Paused"; resuming continues the same take. The microphone stays open for an instant resume
  but is released after 10 minutes paused, and reopened on resume. Default Off.
- Take-only shortcuts (Cancel, Pause / Resume, Undo Last Word) are registered only while a take
  runs, so `ctrl+backspace` keeps working normally in other apps between takes.

### Changed

- **Sidebar: six pages and More.** The sidebar now holds General, Shortcuts, Models, History,
  Jumper and Keyboard Typer, then **More**. More has two rows of tabs - Settings (App, Output,
  Providers, Post-processing, MCP & CLI, Backup, Debug, About) and Tools (Translator, Token
  Count, Model Testing, Current Audio) - and reopens the tab used last. The Advanced page and
  its tab bar are gone: its tabs are More tabs, "Transcription" is renamed **Output**, and its
  History tab moved to the bottom of the History page.
- **Duplicates merged.** Settings that were shown in two places now have one home: the
  post-processing hotkey, provider and prompt appear under the Post-processing switch (no
  separate Post Process page); the transcription cost report moved to Providers › OpenRouter
  Transcription; and the second copies of Translate to English (General keeps it), the
  recordings-folder button (History keeps it), the update check on About (General › Updates
  keeps it), the Cancel shortcut on Debug (Shortcuts keeps it) and the Transcribe & Submit
  paste delay were removed.
- **Search** follows the new layout: results on More read "More › Output › …", and
  post-processing's own controls are left out while it is off. The pause, undo-word and
  live-text-box settings are searchable.

### Fixed

- **The sidebar widens while you search** (to at least 320 px), so result names and their
  locations are no longer cut off, and narrows back once you pick a result or clear the box.
- **Warning icons respond to the whole icon.** The hover tooltip of the shortcut warnings
  (AltGr, single key, duplicate) only opened over the triangle's strokes; the hover area is now
  the full 24 px square.

## [1.9.0] - 2026-09-26 15:42

### Added

- **Search settings from the sidebar.** A search box above the page list finds settings by
  name, description or option, and every shortcut. Pick a result (or press Enter; arrow keys
  move) and its page opens - on the right Advanced tab - with the setting briefly outlined. The
  index is `scripts/nav-map.json`, the control map the docs checker already uses, plus the
  current shortcuts; hidden pages and other platforms' controls are left out.
- **Keep microphone ready** (General › Sound): keeps the microphone open for 1, 5 or 15
  minutes after a take, so the next one starts at once instead of waiting up to a second for an
  idle microphone to wake. Default Off. The system microphone indicator stays lit meanwhile;
  switching it Off closes a waiting microphone right away. Hidden while Always-On Microphone is
  on.
- **Warning for single-key shortcuts.** A shortcut that is one typing key with no modifier
  (such as `f`, which is easy to set by accident) fires every time you type that key anywhere;
  it now gets an amber warning next to it. Function keys, Escape and the like are fine alone.

### Changed

- **Running Handy Tool from the end of setup opens its window.** With Start Hidden on, the
  app used to start straight into the tray after you ran setup, so nothing appeared. An
  interactive install or update now leaves a one-time marker that makes that next launch show
  the window; silent updates (the updater's quiet mode, including the night-time update window)
  leave none and stay hidden. The installer hooks live in `src-tauri/nsis/installer-hooks.nsh`
  (the older `hooks.nsh` has not been wired in since the VC++ runtime was bundled).

## [1.8.0] - 2026-09-26 14:33

### Added

- **Transcription progress on the overlay.** When the transcription left after you stop takes
  more than half a second, the overlay shows **Transcribing 42%** instead of only
  "Transcribing...". Whisper models report their real progress (the whisper.cpp progress hook
  was there but never connected); Parakeet, Moonshine and SenseVoice have no such hook, so the
  figure is estimated from the audio length and how fast this machine transcribed with that
  model earlier in the session (it appears once a model has done one take of 2 s or more). The
  figure never goes backwards and holds at 99% until the text is ready. Remote engines
  (API, OpenRouter) show no figure.
- **The overlay says when the microphone cannot start.** "Microphone blocked" when Windows
  privacy settings deny apps the microphone (the stream start fails with E_ACCESSDENIED), and
  "Microphone error" for any other device that refuses to start. Until now such a take
  "started" anyway and recorded silence: the recorder reported success before its audio thread
  had even tried the device. Opening the microphone now waits up to 3 s for the stream to
  actually start (normally well under 300 ms); a slower start, such as a Bluetooth headset
  switching profile, still records.

### Changed

- **Info tooltips are wider and structured.** The (i) popups are 340 px wide instead of 200 px
  and left-aligned. Descriptions can now carry a bold summary line, separate paragraphs and
  bullet lists, and the 60 longest English descriptions were rewritten that way - a bold one-line
  summary, the default on its own line, options as bullets. The other 16 languages keep their
  existing text, shown wider. Page introductions (Models, Model Testing, Keyboard Typer, MCP) use
  the same formatting.

## [1.7.0] - 2026-09-26 02:59

### Added

- **Any shortcut can be set to None.** Every shortcut control has a × button that switches the
  shortcut off: it stays in your settings, is never registered, and its keys belong to other
  applications again. The typical case is Cancel on Escape, which you may need in the terminal
  or editor you are dictating into; the overlay's X button still cancels a take. Reset brings
  the default back. Until now an empty shortcut was rejected outright, and switching keyboard
  backend would have reset it to its default.
- **Shortcuts page: every shortcut in one place.** A new `Shortcuts` page lists all of them
  grouped by feature (Dictation, Keyboard Typer, Jumper), including Cancel, which was previously
  reachable only on the hidden Debug page. The feature pages keep their own shortcut rows.
- **Conflict warnings.** Two actions on the same keys are marked with a red warning naming the
  other action, on every shortcut control, plus a banner on the Shortcuts page. A duplicate is
  kept rather than refused, but only one of the pair can work: with the default keyboard backend
  the second stays inactive (and is listed as not registered) until the first moves off the keys,
  at which point it takes over by itself.

### Changed

- **Clicking the tray icon opens the window.** A left click used to open the tray menu, which is
  rarely what you want from it. The menu (Copy Last Transcript, Cancel, Quit, ...) is now on
  right click. macOS keeps the menu on a plain click, as menu-bar items do there.

### Fixed

- **A shortcut can be set to keys another shortcut already uses.** While a shortcut field was
  waiting for keys, every other shortcut stayed active, so pressing Ctrl+Space to put it on
  Paste Last started a recording instead and the field only saw "Ctrl". All shortcuts now switch
  off while you enter one and come back afterwards; the one that already had the keys keeps them.
  This applied to both keyboard backends.
- **Escape can be entered as a shortcut.** Escape stopped the shortcut editor instead of being
  recorded, so it could never be chosen - not even to put Cancel back on Escape without Reset.
  It is now recorded like any other key and shown as "Escape", the same as the default (a chord
  saved as "esc" by an earlier build is shown that way too); click anywhere else to stop editing.
- **Editing an inactive shortcut no longer switches off the one that holds its keys.**
  Unregistering is done by chord, so removing a binding that had failed to register tore down
  whichever binding did hold that chord. Bindings that are not registered are now skipped.

## [1.6.3] - 2026-09-25 21:52

### Fixed

- **The overlay no longer shows "recording" before the microphone is actually recording.**
  The microphone is opened when a take starts, and a device that has been idle for a while
  (about ten minutes, measured on a Realtek input) takes around 0.75 s to wake up and deliver
  its first audio. The overlay appeared instantly with a flat waveform, so the first words
  were spoken into nothing. It now reads **Starting mic...** until the first audio arrives and
  switches to the sound bars at that moment, so you know when to start speaking. A warm
  microphone arrives in a few tens of milliseconds, so the message is rarely seen then.
  Always-On Microphone remains the way to remove the wait entirely, and stays off by default.
- **No microphone connected: the overlay now says so instead of flashing away.** Pressing the
  shortcut with no input device showed the overlay for a split second and hid it again with no
  explanation - the error was logged and thrown away. The overlay now shows **No microphone**
  for 2.5 seconds. Pressing the shortcut again after plugging a microphone in starts normally.

## [1.6.2] - 2026-09-24

### Fixed

- **A blocked update no longer looks like a successful one.** The updater launches the
  installer and then exits the app - and it reported success at that point, before anything
  had actually been replaced. When the installer was refused (Windows Smart App Control does
  exactly this to an unsigned installer), the app restarted on the old version with no message
  at all, which looks identical to "already up to date" and leaves you stuck on the old version
  indefinitely. The app now records which version it is updating to before launching the
  installer, checks on the next start whether it is actually running that version, and shows a
  banner with a manual-download link if it is not.

  The banner says a security policy _may_ have blocked the update rather than asserting it: the
  same symptom can come from a cancelled installer, a full disk, or another copy of the app
  running. A portable copy never consumes an installed copy's update record, and installing a
  different version by hand in the meantime is not misreported as a failure.

### Infrastructure

- **Releases now build entirely on GitHub Actions.** Publishing a release triggers builds for
  Windows, macOS and Linux; the Windows installer is signed behind a required-reviewer approval
  gate, so no pull request or unapproved workflow change can obtain the signing key. Pull
  requests run tests, lint and formatting checks only.
- **The Rust test suite can run in CI.** Its mock transcription engine previously left one
  direct engine call unresolved, so the test workflow could never have compiled. That call now
  goes through the transcription manager like every other engine use.
- **The winget manifest description no longer uses the word "explicitly".** The Windows Package
  Manager content scanner treats it as an adult-content marker, which held the package for manual
  review.

## [1.6.1] - 2026-09-16

### Changed

- **Moved _Sound source_ to `General > Sound`, beside the microphone picker.** It was
  in `Advanced > Transcription`, which is the wrong place: the setting decides what a
  recording captures, so it belongs next to the microphone selector whose meaning it
  changes - and that is where people look for it. It now sits directly above that
  picker. The delay and level controls moved with it.

### Added

- **System audio delay** and **System audio level**, shown when a system-audio source
  is selected. How far system audio lags the microphone is a property of the hardware -
  a USB headset, a Bluetooth link and an HDMI monitor each buffer differently, and the
  endpoint driver adds its own - so it is adjustable rather than assumed. The level
  scales only the system leg; the microphone is never touched.

### Infrastructure

- **All platform builds now run on GitHub runners.** A new _All Platforms Build_
  workflow compiles seven targets on demand - Windows x64/ARM64, macOS Intel/Apple
  Silicon, and Linux deb/AppImage+RPM/ARM64 - and uploads them as artifacts.
  `scripts/sign-and-release.ps1` then signs and publishes from the maintainer's
  machine, so the updater private key never goes to a third party.

## [1.6.0] - 2026-09-16

### Added

- **Record what your computer is playing, not just your microphone.** A new _Sound source_ setting
  at `Advanced > Transcription` offers three choices: **Microphone** (unchanged, the default),
  **System audio**, or **Microphone + system audio** mixed into one transcription.

  The second one is the point: on a call, the other participants' voices come out of your speakers,
  and until now Handy could not hear them. It can now transcribe both sides of a conversation.

  **Windows only.** This uses WASAPI loopback, which cpal provides for any playback endpoint opened
  as an input. macOS needs Core Audio process taps and Linux needs a PipeWire monitor source;
  neither is available through the audio library this version uses, so the control is hidden there
  rather than offered and then silently recording nothing.

  A device picker appears once a system-audio mode is selected. Leaving it on _Follow system
  default_ is usually right, and it keeps working when Windows switches your playback device at the
  start of a call — which is exactly when a pinned device would go quiet.

### Fixed

- **`update_mode` could hang the app.** The microphone-mode mutex is not reentrant, and its guard
  was released on only one of three paths. Switching from always-on to on-demand _while recording_,
  or any call that left the mode unchanged, deadlocked the calling thread.

- **Always-on recording never noticed a broken audio stream.** The fault check added in 1.5.0 lives
  in the on-demand open path, which always-on mode does not use. After a device was unplugged or a
  driver reset, every subsequent take recorded silence while the interface showed it recording
  normally. The stream is now re-validated before each take in both modes.

- **An audio stream that failed to start was indistinguishable from a healthy one.** The existing
  fault flag is set by the stream's error callback, which cannot fire if no stream was ever built —
  so a failed open left the recorder marked healthy forever. Arm failures are now tracked
  separately.

- **Config negotiation could kill the app silently.** It ran inside a worker thread under
  `panic = "abort"`, so any failure terminated the process with nothing in the log. A playback
  endpoint triggers it immediately, since it advertises no input configurations at all.

- **The updater manifest generator could publish the wrong binary.** It defaulted to a build
  directory this project does not use, and never checked that the installer it was given matched
  the version being released. Because signatures cover file _bytes_ rather than names, a stale
  installer would have shipped under a valid signature — every client "updating" to an older build
  and then being offered the same update forever. It now resolves the real directory and refuses a
  version mismatch.

- **The test suite reported failure on a green run.** The doctest phase runs `rustdoc`, which Smart
  App Control blocks on some machines; this crate has no doctests, so that phase could only ever
  produce a false failure that hid a passing unit-test run.

## [1.5.0] - 2026-09-14

### Added

- **Custom text before and after a transcription.** A prefix and a suffix, each with its own
  on/off switch, its own text, and its own "add a newline" toggle (on by default). Configured
  separately for **Transcribe** and **Transcribe & Submit**, because a chat-submit signature is
  rarely what you want on ordinary dictation.

  The text is added to what is _delivered_. Your history keeps the plain transcript — otherwise
  Paste Last would re-apply the prefix and suffix to text that already had them, and you would get
  two copies. An enabled-but-empty affix adds nothing, not even its newline, and an empty
  transcription is left alone rather than delivering a bare signature into whatever has focus.

- **Lifetime totals that survive a purge.** When retention deletes a recording, its contribution is
  rolled into a carried-forward total before the row goes, so statistics stop shrinking as history
  is cleaned up. Deleting an entry by hand still removes it completely, totals included — pressing
  the trash can means _forget this_.

  Counts are stored as characters rather than words: `split_whitespace()` is permanently wrong for
  Chinese, Japanese and Korean, which Handy ships both locales and an ASR engine for, and once the
  detail is gone it cannot be recomputed.

### Changed

- **Retention no longer deletes your transcriptions — only the audio.** New setting, **on by
  default for existing installs as well as new ones**. This is a deliberate behaviour change on
  upgrade: it makes Handy delete _less_ of your data, and the alternative would have kept silently
  discarding transcripts while the setting that prevents it sat switched off.

  Note this changes what the history limit means. It now caps how many recordings keep their
  **audio**, not how many transcriptions you keep. The shipped default is 5.

### Fixed

- **A recording that never started was reported as successful.** `AudioRecorder::start` returned
  `Ok` without sending anything when the recorder was not open, so the overlay said "recording"
  over a take that could not exist.

- **A hang when stopping a recorder that was never opened.** `stop()` built its response channel
  before deciding whether to send a command, so the sender stayed alive in scope and the receiver
  blocked the calling thread forever.

- **A faulted audio stream was reused instead of reopened.** The CPAL error callback only logged;
  nothing cleared the "stream is open" flag. After a device was unplugged or a driver reset, every
  later take recorded silence while the interface showed it recording normally.

- **Capture latency is now measurable.** A single `capture-latency` line at INFO reports
  open→config, config→playing, playing→first-buffer and the total. The existing figures were
  `debug!`, which release builds never write, and one of them was measured from the wrong point.

## [1.4.0] - 2026-09-09

### Fixed

- **The Transcribe & Submit default no longer eats the space after an accented letter.** Windows
  reports AltGr as Ctrl+Alt, so the old `ctrl+alt+space` default was the same chord a Polish,
  German or French typist produces when AltGr is still held down for the space that follows a word
  like _mamą_. The new default is `ctrl+shift+f9`, which is identical on every keyboard layout.

  **Your saved chord is not touched.** If you already have the app installed, whatever you have
  bound keeps working — upgrading has never rewritten a binding and still does not. What does
  change is where _Reset to default_ lands: it now points at the new chord, so you can move onto it
  deliberately.

### Added

- **A warning next to any shortcut AltGr can type with.** Choosing something like `ctrl+alt+o`
  steals ó from anyone on a Polish (Programmers) layout — the shortcut fires and the character
  never arrives. An amber marker now appears beside such a chord and explains the conflict. The
  app cannot silently change a chord you chose, so it tells you instead.

- **A test over the whole default set** (`no_default_binding_collides_with_altgr`) fails the build
  if any future default lands on a chord AltGr types a character with. The Jumper's eighteen slot
  chords stay on `ctrl+alt+<digit>` and `ctrl+alt+shift+<digit>` as a documented exception: no
  common European layout puts a character on AltGr+digit, and no other free chord space of that
  size exists.

### Changed

- **Every trigger shortcut now lives on the General page.** Transcribe & Submit was reachable only
  from `Advanced › Transcription`, and Paste Last Transcription was buried in its own group further
  down General. Both chords now sit with Transcribe and Push-to-Talk at the top of General.

  The options behind them — paste method, submit key, clipboard handling, jump timings — moved the
  other way, into `Advanced › Transcription`, next to the equivalent global settings. Shortcuts in
  General, tuning in Advanced.

- **The shortcut documentation matched a release from several versions ago.** The reference tables
  still advertised `ctrl+alt+k`, `ctrl+alt+j`, `ctrl+alt+s`, `ctrl+alt+p` and `ctrl+alt+t` — nine
  AltGr-colliding chords that the application itself had already moved onto Ctrl+Shift function
  keys. Every default chord in `docs/` was regenerated from the source of truth in `settings.rs`.

## [1.3.2] - 2026-08-20

### Added

- **macOS builds for Intel and Apple Silicon**, as `.dmg` disk images and zipped `.app` bundles.
  1.3.0 shipped Windows only, and 1.3.1 was never released.

  Both are **unsigned and unnotarized**, so Gatekeeper refuses them on first open — right-click the
  app and choose _Open_, or clear the quarantine attribute. The Apple Silicon build is
  **cross-compiled from an Intel host and has not been run on Apple Silicon hardware**; it is
  published so it can be tested, not because it has been verified.

### Fixed

- **The Apple Silicon build could not be produced at all.** `build.rs` decided whether to compile
  the Apple Intelligence Swift bridge using `cfg!(target_arch)`, which in a build script describes
  the machine doing the building rather than the machine being built for. Cross-compiling from an
  Intel Mac therefore skipped the bridge while the application itself — gated on the real target —
  still referenced its symbols, and the link failed with `Undefined symbols for architecture
arm64`. The decision now reads `CARGO_CFG_TARGET_OS` / `CARGO_CFG_TARGET_ARCH`, which are the
  target's values. This only ever affected cross-compilation, which is why it went unnoticed: an
  Apple Silicon build had never been attempted.

### Changed

- No functional change to the application on Windows. The Windows build is identical to 1.3.0 apart
  from the version string, so there is nothing to gain by updating if you are already on 1.3.0.

## [1.3.0] - 2026-08-18

### Changed

- **The automatic "type into remote desktops" behaviour from 1.2.0 has been withdrawn.** It made
  things worse, not better. Dictating into an RDP or Citrix session produced text with characters
  missing, characters jumbled, and line breaks in places you did not put them. If you were affected,
  this release ends it — there is nothing to turn off, because the switch is gone.

  Why it failed, precisely, because it is not what the 1.2.0 notes assumed. Typing sent each batch
  of 40 characters as a _single instantaneous burst_ of input events into the remote session's
  virtual channel; pausing between bursts does not make any one burst gentler. Worse, when the take
  used Transcribe & Submit, the Enter key fired about 50 ms after the last batch — while the text
  was still crossing the wire. The message got submitted in pieces. That is where the phantom "line
  breaks" came from: not from your dictation, but from Enter arriving too early, repeatedly.

  Keystroke delivery has not been removed, only the _automatic_ targeting of it. If you want it,
  choose it deliberately: `Advanced › Transcription › Transcribe › Paste method = Direct`. It
  carries the caveats it always did, and the catalog has said so all along — it can drop characters
  in RDP, Citrix and VM consoles.

  **What this means for privacy.** Remote targets now use your configured paste method again, so the
  transcript does reach the remote machine's clipboard and its clipboard history, and nothing on
  this side can retract it. That is the trade this release makes deliberately, in favour of your
  dictation arriving intact. `Direct` remains the only delivery path that touches no clipboard.

### Fixed

- **The submit key no longer fires before your text has landed in a remote window.** This one is
  older than 1.2.0 and worth understanding, because it silently affected clipboard pastes too.

  The remote-specific delays — the ones you configure per Local / Remote desktop — were only ever
  applied when Handy had _jumped_ to the target window. Dictating into a remote window that already
  had focus, which is the ordinary way most people work, took neither branch and got **no wait at
  all**. A `Submit delay before Enter = 500 ms` setting could sit there for months and never once
  apply. Remote targets now get their remote timing whether Handy activated the window or not.

  An already-focused **local** target still submits instantly. That is deliberate and there is a
  test named after it: the fix must not add a quarter of a second to every ordinary dictation.

  The setting was called _Submit delay after jump_ — which was true before and is not now. It is
  now **Submit delay before Enter**. Your configured value is preserved.

- **A failed submit no longer tells you the text was not delivered.** When focus moved during the
  settle before Enter, the message said the delivery had failed — so you would press Paste Last and
  end up with the transcript inserted twice. It now says plainly that the text arrived and only the
  submit key was withheld. This matters more than it used to, because the longer remote settle makes
  that window wider.

- **`Paste method = Direct` no longer rushes line breaks.** Return and Tab were injected with no
  pause at all and did not count toward the pacing, so a blank line fired two Enter presses
  back-to-back at machine speed. They are now paced like any other injected key, and the safety
  bound that caps total typing time counts them, so a transcript that is mostly blank lines can no
  longer stall a delivery.

- **The documented defaults for the jump delays were wrong in three places.** `settings.rs` has
  always defaulted both delays to 300 ms local / 600 ms remote, but the settings UI fell back to
  250 ms / 1 s, and every tooltip in all 17 languages quoted those wrong numbers. If you never
  changed the delays, your actual behaviour did not change - only the numbers you were shown. All
  sources now agree with the code. Pre-existing, unrelated to the RDP work.

### Added

- **`Clipboard restore delay for remote desktops`** (`Advanced › Transcription › Transcribe`).
  An optional, separate restore delay for remote targets, so a local paste can stay fast while a
  remote one gets the time RDP needs to fetch the clipboard before the original is put back.
  Unset by default — it changes nothing until you pick a value. Stated plainly in the setting: a
  longer restore leaves the transcript on your clipboard for longer.

## [1.2.0] - 2026-08-18

### Fixed

- **Your dictation no longer ends up on the remote machine's clipboard.** With "Don't Modify
  Clipboard" set, transcriptions still turned up on the clipboard of the RDP or Citrix host you
  were dictating into. The local fixes in 1.1.0 were working exactly as intended — this is a
  different leak, one floor down. A clipboard paste method _has_ to put the transcript on your
  local clipboard, and clipboard redirection then copies it across to the remote computer's own
  clipboard and its clipboard history. That is a separate clipboard on a separate operating
  system. Restoring yours afterwards cannot reach it; Handy has no handle on it at all.

  Handy now **types** into remote desktops instead of pasting, which touches no clipboard on
  either machine. Three parts:
  - **Remote targets are recognised even when you did not jump to them.** Previously a window only
    counted as remote if Handy had jumped to it, which missed the ordinary case of dictating into
    a remote window that already had focus. Which windows count as remote still comes from your
    own match list on the Jumper page (`msrdc`, `mstsc`, `Citrix` by default).

  - **Typed text is paced so a remote session can keep up.** One instant burst is fine locally but
    drops characters over RDP. Text now goes out in batches of 40 characters with 15 ms between
    them — about 200 ms for a 500-character transcript, and both numbers are adjustable.

  - **Focus is re-checked between batches.** Typing takes hundreds of milliseconds where a paste
    took one keystroke, so Handy verifies the target is still the right one before each batch and
    stops rather than typing the rest of your transcript into whatever stole focus.

  **This is a behaviour change for remote desktop users** and one switch turns it off:
  `Jumper › Remote desktop detection › Type into remote desktops instead of pasting`. Two honest
  limits. It prevents _future_ leaks only — it cannot remove transcripts already sitting in the
  remote machine's clipboard history, and nothing Handy can do reaches those. And typed text can
  trigger autocomplete, bracket auto-closing and IME behaviour that a paste does not, and arrives
  as many undo steps rather than one.

  **It deliberately does nothing to a transcript containing line breaks.** A paste inserts a
  line break as inert text. Typing cannot -- it has to send each one as an Enter key press,
  which is a command: in a chat box that sends the message, and in a form it submits. Your
  first line would be posted and the rest sent as follow-up messages. Post-processed
  transcripts are multi-line by construction (the default prompt asks for a summary and a
  bulleted body), so this exemption is common, and for those the transcript still reaches
  the remote clipboard. Posting half a dictation into a colleague's chat is a worse outcome
  than the leak, and unlike the leak it cannot be undone.

  It also deliberately does nothing when `Advanced › Clipboard Handling` is set to _Copy to Clipboard_.
  That mode is defined by leaving the transcript on your clipboard after delivery, so redirection
  carries it across however Handy delivered it — switching to typing would cost you its downsides
  and buy no privacy. Use _Don't Modify Clipboard_ if you want the remote clipboard left clean.

## [1.1.0] - 2026-08-17

### Fixed

- **"Don't Modify Clipboard" now actually leaves your clipboard alone.** If you had this set — it
  is the default — old transcriptions could still end up on your clipboard and stay there,
  displacing what you had copied. Several separate faults combined to cause it, and all of them
  are fixed:
  - **Handy could hand you back one of its own transcripts as if it were your clipboard.** When a
    transcript was already sitting on the clipboard, the next dictation captured _that_ as "your
    previous clipboard" and faithfully restored it afterwards — so one leak became permanent, and
    every later dictation re-restored the same old text. Handy now recognises its own writing and
    restores what was really yours.

  - **A failed delivery wrote your clipboard anyway.** When an anchored jump or a paste could not
    be verified, the transcript was parked on the clipboard to avoid losing it — ignoring your
    setting entirely. It no longer does. The take is not lost: it is saved to History _before_
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
  crash, which would have left the shortcut re-pasting an _older_ take. It now recovers.

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
    `portable.marker` copy would create a _second_, installed copy rather than replace the
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
  (the shortcut you pressed is set to Live _and_ you are on a local model) and when the
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
- **Log files no longer accumulate forever.** Rotation kept _every_ rotated 10 MB file, with
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
