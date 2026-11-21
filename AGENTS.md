# Agent instructions for handy-tool

## Software feedback loop

At the start of a repository task, read any saved software lessons for this project. Use the private local
`handy-tool/software-lessons.md` file under `XDG_STATE_HOME` or `~/.local/state` on Unix. On Windows use
the same relative path under `LOCALAPPDATA`. A missing file means there are no saved lessons yet.

Record improvements to the app, accessibility, packaging, or contributor workflow. Keep audio, transcripts, microphone or account data, and credentials out of software lessons. Follow the upstream rules inside vendored trees.

After a major change and at close-out, save generalized lessons that will help on a future task. Record
the verified symptom or gap, the useful change, and how it was checked. Deduplicate existing lessons.
Keep the file local and outside the repository. Share any relevant core-project fix or feature suggestion
with the user, including its expected benefit. A suggestion stays a proposal until the user asks to implement it.
