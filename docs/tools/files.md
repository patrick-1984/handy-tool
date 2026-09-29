# Files

## The moment

A phone recording, a lecture or a meeting should not occupy your hands all evening. Pick the file, or point Handy Tool at a folder and leave it running, and come back to text.

## How it fits your day

Use Transcribe a file for one recording you have now: a voice note from your phone, an interview, a talk. Use the watched folders for folders that accumulate audio outside your live dictation flow, and decide which engine carries the backlog and what happens when you start dictating in the middle of it.

## What it can do

- [One file, transcribed as accurately as the model can](../features.md#one-file-transcribed-as-accurately-as-the-model-can)
- [A folder of recordings, transcribed while you sleep](../features.md#a-folder-of-recordings-transcribed-while-you-sleep)
- [Your existing files are left alone](../features.md#your-existing-files-are-left-alone)
- [A file is never transcribed twice](../features.md#a-file-is-never-transcribed-twice)
- [You can see what it is working on](../features.md#you-can-see-what-it-is-working-on)
- [Live dictation always wins](../features.md#live-dictation-always-wins)
- [Batch on one accelerator, dictation on another](../features.md#batch-on-one-accelerator-dictation-on-another)
- [Pick the engine that fits the machine](../features.md#pick-the-engine-that-fits-the-machine)

## Settings that matter

- [Files settings](../reference/settings/files.md)
- [Models settings](../reference/settings/models.md)

## When it goes wrong

- [Never reads a file that is still being written](../features.md#never-reads-a-file-that-is-still-being-written)
- [A take that produced no text says so](../features.md#a-take-that-produced-no-text-says-so)

## Set it up

For one file:

1. Pick the model at `Files › Transcribe a file › Model` (your dictation model unless you change it).
2. Choose the file at `Files › Transcribe a file › Choose an audio file`, and say where the text goes: Handy's folder, next to the recording, or nowhere.
3. Copy the text when it appears.

For a folder:

1. Add the source from `Files › Folders › Add a folder`.
2. Keep interactive work ahead of the queue at `Files › Watched folders › Priority = Live dictation first`.
3. Reuse the active dictation engine at `Files › Watched folders › Batch model = Same as dictation (default)`.
4. Start folder watching at `Files › Watched folders › Watch folders = On`.
5. Confirm the current item at `Files › Watched folders › Status`, then leave the first file in place until its text sidecar appears.
