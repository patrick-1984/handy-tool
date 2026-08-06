# Backup and portable mode

## The moment

You move to a new machine on Monday and do not want to rebuild every shortcut, provider, prompt, and history choice by hand. One export on the old machine, one restore on the new one, and the afternoon stays yours.

## How it fits your day

Export before moving machines or making broad configuration changes. Use portable mode when the executable and its data should travel together on a stick. Before you send an archive anywhere, read what it contains and what it leaves behind.

## What it can do

- [One file that carries your whole setup](../features.md#one-file-that-carries-your-whole-setup)
- [Move machines, or undo a bad week](../features.md#move-machines-or-undo-a-bad-week)
- [Run it from a USB stick and leave no trace](../features.md#run-it-from-a-usb-stick)
- [What a backup deliberately leaves out](../features.md#what-a-backup-deliberately-leaves-out)
- [Why a restore asks for a restart](../features.md#why-a-restore-asks-for-a-restart)

## Settings that matter

- [Backup settings](../reference/settings/backup.md)

## When it goes wrong

- [A crafted archive can't write outside the app](../features.md#a-crafted-archive-cant-write-outside-the-app)
- [Where your data lives on disk](../features.md#where-your-data-lives-on-disk)

## Set it up

1. For settings and History only, use `Backup › Configuration + history › Export config + history`.
2. To add eligible compressed recordings, use `Backup › Full data (with compressed audio) › Export full backup`.
3. On restore, choose settings and History at `Backup › Restore from backup › Configuration & history (settings, history DB) = On`.
4. Choose eligible audio separately at `Backup › Restore from backup › Recordings (audio files) = On`.
5. Apply the archive from `Backup › Restore from backup › Restore from backup…`, then restart when settings or History were restored.
6. Treat every exported archive as a secret-bearing file. Store and transfer it with the same care as the API keys inside it.
