# Backup settings

Open `Backup`. Export archives are gzip-compressed tar files and are not encrypted.

## Configuration + history

### Export config + history

`Backup › Configuration + history › Export config + history`

Exports `settings_store.json` and `history.db` without audio or downloaded models. Stored API keys and the MCP token are included. **Default:** not applicable; this is an action.

Catalog: [One file that carries your whole setup](../../features.md#one-file-that-carries-your-whole-setup).

## Full data (with compressed audio)

### Export full backup

`Backup › Full data (with compressed audio) › Export full backup`

Exports configuration and history plus Opus and Ogg recordings. WAV, FLAC, temporary chunks, and downloaded models remain excluded. **Default:** not applicable; this is an action.

Catalog: [What a backup deliberately leaves out](../../features.md#what-a-backup-deliberately-leaves-out).

## Restore from backup

<a id="configuration--history-settings-history-db"></a>

### Configuration & history (settings, history DB)

`Backup › Restore from backup › Configuration & history (settings, history DB)`

Chooses whether restore replaces the settings file and history database. A successful restore of either requires a restart. **Default:** selected.

Catalog: [Move machines, or undo a bad week](../../features.md#move-machines-or-undo-a-bad-week).

### Recordings (audio files)

`Backup › Restore from backup › Recordings (audio files)`

Chooses whether the archive's eligible audio files are restored. It can be used independently of [Configuration & history (settings, history DB)](#configuration--history-settings-history-db). **Default:** selected.

Catalog: [Move machines, or undo a bad week](../../features.md#move-machines-or-undo-a-bad-week).

### Restore from backup…

`Backup › Restore from backup › Restore from backup…`

Opens an archive and restores the selected categories. A `Restart Handy Tool now` action appears when the restored data requires it. **Default:** not applicable; this is an action.

Catalog: [Why a restore asks for a restart](../../features.md#why-a-restore-asks-for-a-restart).

