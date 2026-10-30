use anyhow::Result;
use chrono::{DateTime, Local, Utc};
use log::{debug, error, info};
use rusqlite::{Connection, OptionalExtension, params};
use rusqlite_migration::{M, Migrations};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};

use crate::audio_toolkit::save_wav_file;

/// True only for files Handy itself created in the recordings folder. Used as a
/// safety guard so cleanup never deletes user-provided files (e.g. `.txt` notes
/// the user keeps alongside recordings for historical purposes).
fn is_handy_recording_file(file_name: &str) -> bool {
    file_name.starts_with("handy-")
        && (file_name.ends_with(".wav")
            || file_name.ends_with(".opus")
            || file_name.ends_with(".ogg"))
}

/// Where a recording's transcript goes: `handy-{ts}.txt` in the recordings
/// folder. `None` for anything but a plain Handy recording name.
fn text_file_path(recordings_dir: &std::path::Path, file_name: &str) -> Option<PathBuf> {
    let plain = !file_name.contains(['/', '\\', ':'])
        && !file_name.contains("..")
        && is_handy_recording_file(file_name);
    plain.then(|| recordings_dir.join(std::path::Path::new(file_name).with_extension("txt")))
}

/// A take's text file that Handy wrote: it holds exactly the row's text. A
/// `.txt` with anything else in it (a note of yours, an edited copy, the
/// watched-folder transcript of older versions) is never replaced or deleted.
/// `Ok(None)` = missing or not Handy's; an error = it could not be read (e.g.
/// open in another program), so nobody can tell - callers must not go on as if
/// it were not Handy's.
fn owned_text_file(
    recordings_dir: &std::path::Path,
    file_name: &str,
    row_text: &str,
) -> std::io::Result<Option<PathBuf>> {
    let Some(path) = text_file_path(recordings_dir, file_name) else {
        return Ok(None);
    };
    let row_text = row_text.trim();
    let on_disk = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        // Not UTF-8: not a file Handy wrote.
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => return Ok(None),
        Err(e) => return Err(e),
    };
    Ok((!row_text.is_empty() && on_disk.trim() == row_text).then_some(path))
}

/// A new take's text file, never over an existing file: written to a temp
/// name, then linked into place (fails if the name is taken). `Ok(true)` =
/// written.
fn create_text_file_in(
    recordings_dir: &std::path::Path,
    file_name: &str,
    text: &str,
) -> Result<bool> {
    let text = text.trim();
    let Some(path) = text_file_path(recordings_dir, file_name) else {
        return Ok(false);
    };
    if text.is_empty() || path.exists() {
        return Ok(false);
    }
    let tmp = path.with_extension("txt.tmp");
    fs::write(&tmp, text.as_bytes())?;
    let linked = fs::hard_link(&tmp, &path);
    let _ = fs::remove_file(&tmp);
    match linked {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        // No hard links on this file system: create it, still never over one.
        Err(_) => {
            use std::io::Write;
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut f) => {
                    f.write_all(text.as_bytes())?;
                    Ok(true)
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
                Err(e) => Err(e.into()),
            }
        }
    }
}

/// History's "Transcribe again": the take's text file follows the new text -
/// replaced when Handy wrote it (it still holds `previous_text`), written when
/// missing and `create_missing`, removed when the new text is empty. Anything
/// else is left alone.
fn rewrite_text_file_in(
    recordings_dir: &std::path::Path,
    file_name: &str,
    previous_text: &str,
    new_text: &str,
    create_missing: bool,
) -> Result<bool> {
    let Some(path) = text_file_path(recordings_dir, file_name) else {
        return Ok(false);
    };
    let owned = owned_text_file(recordings_dir, file_name, previous_text)?.is_some();
    if path.exists() && !owned {
        return Ok(false);
    }
    if !path.exists() && !create_missing {
        return Ok(false);
    }
    let new_text = new_text.trim();
    if new_text.is_empty() {
        if owned {
            fs::remove_file(&path)?;
            return Ok(true);
        }
        return Ok(false);
    }
    let tmp = path.with_extension("txt.tmp");
    fs::write(&tmp, new_text.as_bytes())?;
    fs::rename(&tmp, &path)?;
    Ok(true)
}

/// Removes the take's text file with its row, when Handy wrote it.
fn remove_owned_text_file(recordings_dir: &std::path::Path, file_name: &str, row_text: &str) {
    match owned_text_file(recordings_dir, file_name, row_text) {
        Ok(Some(path)) => {
            if let Err(e) = fs::remove_file(&path) {
                error!("Failed to delete text file {}: {}", path.display(), e);
            }
        }
        Ok(None) => {}
        // Unreadable now: keep it rather than guess (the row still goes).
        Err(e) => error!("Could not check the text file of {}: {}", file_name, e),
    }
}

/// Best-effort duration (seconds) of a recording file: WAV via hound, Ogg/Opus
/// via the Ogg granule. `None` for unknown/unreadable files.
fn audio_file_duration_seconds(path: &std::path::Path) -> Option<f64> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase());
    match ext.as_deref() {
        Some("wav") => {
            let reader = hound::WavReader::open(path).ok()?;
            let rate = reader.spec().sample_rate;
            if rate == 0 {
                return None;
            }
            Some(reader.duration() as f64 / rate as f64)
        }
        Some("opus") | Some("ogg") => crate::audio_toolkit::audio::opus_duration_seconds(path).ok(),
        _ => None,
    }
}

/// Parse the recording timestamp from an in-progress chunk name
/// `handy-{ts}-chunk-{N}-temp.opus`.
fn parse_temp_chunk_ts(name: &str) -> Option<u64> {
    let rest = name.strip_prefix("handy-")?.strip_suffix("-temp.opus")?;
    rest.split("-chunk-").next()?.parse::<u64>().ok()
}

/// Parse the chunk index from a finalized chunk name `handy-{ts}-chunk-{N}.opus`
/// (returns `None` for `-temp` files or a different timestamp).
fn parse_chunk_index(name: &str, ts: u64) -> Option<usize> {
    if name.ends_with("-temp.opus") {
        return None;
    }
    let prefix = format!("handy-{}-chunk-", ts);
    name.strip_prefix(&prefix)?
        .strip_suffix(".opus")?
        .parse::<usize>()
        .ok()
}

/// Database migrations for transcription history.
/// Each migration is applied in order. The library tracks which migrations
/// have been applied using SQLite's user_version pragma.
///
/// Note: For users upgrading from tauri-plugin-sql, migrate_from_tauri_plugin_sql()
/// converts the old _sqlx_migrations table tracking to the user_version pragma,
/// ensuring migrations don't re-run on existing databases.
static MIGRATIONS: &[M] = &[
    M::up(
        "CREATE TABLE IF NOT EXISTS transcription_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_name TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            saved BOOLEAN NOT NULL DEFAULT 0,
            title TEXT NOT NULL,
            transcription_text TEXT NOT NULL
        );",
    ),
    M::up("ALTER TABLE transcription_history ADD COLUMN post_processed_text TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN post_process_prompt TEXT;"),
    // Real per-transcription cost in USD (OpenRouter usage.cost; NULL for local
    // engines) and the recording length in seconds, for the cost report.
    M::up("ALTER TABLE transcription_history ADD COLUMN cost_usd REAL;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN duration_seconds REAL;"),
    // Human label of the transcription engine/model used (e.g. "Whisper Large —
    // local" or "openai/whisper-large-v3 — OpenRouter"), for the history title.
    M::up("ALTER TABLE transcription_history ADD COLUMN model_used TEXT;"),
    // Unix timestamp at which retention deliberately deleted this row's audio while
    // keeping the transcript (settings.preserve_transcriptions). NULL means the audio
    // was never purged on purpose - so a missing file is a genuine fault, not policy.
    M::up("ALTER TABLE transcription_history ADD COLUMN audio_purged_at INTEGER;"),
    // Carry-forward rollup ("balance brought forward"): when retention DELETES a
    // row, its contribution is accrued here first, so lifetime statistics do not
    // shrink just because old detail was discarded. Reported totals are
    // `purged_totals + fold(live rows)`.
    //
    // Integer units on purpose: f64 sums drift and cannot be merged. Cost is in
    // NANOdollars because per-take OpenRouter costs are routinely sub-microdollar -
    // at microdollar resolution thousands of takes round to $0.0000 while the live
    // rows beside them visibly sum to more.
    M::up(
        "CREATE TABLE IF NOT EXISTS purged_totals (
            key TEXT PRIMARY KEY,
            takes INTEGER NOT NULL DEFAULT 0,
            seconds_milli INTEGER NOT NULL DEFAULT 0,
            cost_nanos INTEGER NOT NULL DEFAULT 0,
            chars INTEGER NOT NULL DEFAULT 0,
            first_ts INTEGER,
            last_ts INTEGER
        );",
    ),
    // The take's text with speakers (a Custom STT server with speakers on):
    // one "A: ..." paragraph per speaker turn. NULL = no speaker text.
    M::up("ALTER TABLE transcription_history ADD COLUMN diarized_text TEXT;"),
    // A name the user gives a recording (History), searched with its text.
    M::up("ALTER TABLE transcription_history ADD COLUMN name TEXT;"),
];

/// Totals carried forward from history rows that retention has deleted, so
/// lifetime statistics do not shrink when old detail is purged.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
pub struct PurgedTotals {
    pub takes: i64,
    pub seconds: f64,
    pub cost_usd: f64,
    pub chars: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct HistoryEntry {
    pub id: i64,
    pub file_name: String,
    pub timestamp: i64,
    pub saved: bool,
    pub title: String,
    pub transcription_text: String,
    pub post_processed_text: Option<String>,
    pub post_process_prompt: Option<String>,
    /// Real USD cost of this transcription (OpenRouter usage.cost); `None` for
    /// local/free engines or when the provider didn't report a cost.
    #[serde(default)]
    pub cost_usd: Option<f64>,
    /// Recording length in seconds.
    #[serde(default)]
    pub duration_seconds: Option<f64>,
    /// Human label of the engine/model that produced this transcription.
    #[serde(default)]
    pub model_used: Option<String>,
    /// Set when retention deliberately deleted this row's audio while keeping the
    /// transcript. `None` means the audio was never purged on purpose, so a missing
    /// file is a fault rather than policy - the UI must distinguish the two.
    #[serde(default)]
    pub audio_purged_at: Option<i64>,
    /// The text with speakers, one `A: ...` paragraph per turn, when the
    /// engine told the speakers apart (Custom STT server). `None` otherwise.
    #[serde(default)]
    pub diarized_text: Option<String>,
    /// A name the user gave the recording; `None` = unnamed.
    #[serde(default)]
    pub name: Option<String>,
}

pub struct HistoryManager {
    app_handle: AppHandle,
    recordings_dir: PathBuf,
    db_path: PathBuf,
    /// Serialises what changes a row together with its text file: "Transcribe
    /// again" (file + row), deleting an entry and retention. Without it a
    /// deletion could read the row's old text, miss the file just rewritten
    /// and leave it behind. Never held across an `.await`.
    text_files: std::sync::Mutex<()>,
}

impl HistoryManager {
    pub fn new(app_handle: &AppHandle) -> Result<Self> {
        // Create recordings directory in app data dir
        let app_data_dir =
            crate::portable::resolve_app_data_dir(app_handle).map_err(|e| anyhow::anyhow!(e))?;
        let recordings_dir = app_data_dir.join("recordings");
        let db_path = app_data_dir.join("history.db");

        // Ensure recordings directory exists
        if !recordings_dir.exists() {
            fs::create_dir_all(&recordings_dir)?;
            debug!("Created recordings directory: {:?}", recordings_dir);
        }

        let manager = Self {
            app_handle: app_handle.clone(),
            recordings_dir,
            db_path,
            text_files: std::sync::Mutex::new(()),
        };

        // Initialize database and run migrations synchronously
        manager.init_database()?;

        Ok(manager)
    }

    fn init_database(&self) -> Result<()> {
        info!("Initializing database at {:?}", self.db_path);

        let mut conn = Connection::open(&self.db_path)?;

        // Handle migration from tauri-plugin-sql to rusqlite_migration
        // tauri-plugin-sql used _sqlx_migrations table, rusqlite_migration uses user_version pragma
        self.migrate_from_tauri_plugin_sql(&conn)?;

        // Create migrations object and run to latest version
        let migrations = Migrations::new(MIGRATIONS.to_vec());

        // Validate migrations in debug builds
        #[cfg(debug_assertions)]
        migrations.validate().expect("Invalid migrations");

        // Get current version before migration
        let version_before: i32 =
            conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        debug!("Database version before migration: {}", version_before);

        // Apply any pending migrations
        migrations.to_latest(&mut conn)?;

        // Get version after migration
        let version_after: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

        if version_after > version_before {
            info!(
                "Database migrated from version {} to {}",
                version_before, version_after
            );
        } else {
            debug!("Database already at latest version {}", version_after);
        }

        Ok(())
    }

    /// Migrate from tauri-plugin-sql's migration tracking to rusqlite_migration's.
    /// tauri-plugin-sql used a _sqlx_migrations table, while rusqlite_migration uses
    /// SQLite's user_version pragma. This function checks if the old system was in use
    /// and sets the user_version accordingly so migrations don't re-run.
    fn migrate_from_tauri_plugin_sql(&self, conn: &Connection) -> Result<()> {
        // Check if the old _sqlx_migrations table exists
        let has_sqlx_migrations: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='_sqlx_migrations'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !has_sqlx_migrations {
            return Ok(());
        }

        // Check current user_version
        let current_version: i32 =
            conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

        if current_version > 0 {
            // Already migrated to rusqlite_migration system
            return Ok(());
        }

        // Get the highest version from the old migrations table
        let old_version: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations WHERE success = 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if old_version > 0 {
            info!(
                "Migrating from tauri-plugin-sql (version {}) to rusqlite_migration",
                old_version
            );

            // Set user_version to match the old migration state
            conn.pragma_update(None, "user_version", old_version)?;

            // Optionally drop the old migrations table (keeping it doesn't hurt)
            // conn.execute("DROP TABLE IF EXISTS _sqlx_migrations", [])?;

            info!(
                "Migration tracking converted: user_version set to {}",
                old_version
            );
        }

        Ok(())
    }

    fn get_connection(&self) -> Result<Connection> {
        Ok(Connection::open(&self.db_path)?)
    }

    /// Recover any in-progress recordings left behind by a crash and add them to
    /// history (marked "Recovered"). Handles both the current chunked-Opus format
    /// (`handy-{ts}-chunk-N-temp.opus`, see `reconcile_orphan_opus_chunks`) and
    /// the legacy single-WAV crash-safety format (`handy-{ts}.recording.wav`).
    /// Only ever touches files Handy created; user files are left alone.
    pub fn reconcile_orphan_recordings(&self) -> Result<()> {
        if !self.recordings_dir.exists() {
            return Ok(());
        }

        let mut recovered = 0usize;
        for entry in fs::read_dir(&self.recordings_dir)? {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let file_name = match path.file_name().and_then(|s| s.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            // Only ever touch files Handy created itself. The user may keep
            // their own files (e.g. `.txt` notes) in this folder, and we must
            // never rename or delete those.
            if !(file_name.starts_with("handy-") && file_name.ends_with(".recording.wav")) {
                continue;
            }

            match crate::audio_toolkit::repair_wav_header(&path) {
                Ok(samples) if samples > 0 => {
                    // Derive the original timestamp from the file name.
                    let stem = file_name
                        .strip_suffix(".recording.wav")
                        .unwrap_or(&file_name);
                    let ts = stem
                        .strip_prefix("handy-")
                        .and_then(|s| s.parse::<i64>().ok())
                        .unwrap_or_else(|| Utc::now().timestamp());

                    // Choose a non-colliding finalized name.
                    let mut target_name = format!("handy-{}.wav", ts);
                    let mut target_path = self.recordings_dir.join(&target_name);
                    let mut n = 1;
                    while target_path.exists() {
                        target_name = format!("handy-{}-recovered-{}.wav", ts, n);
                        target_path = self.recordings_dir.join(&target_name);
                        n += 1;
                    }

                    if let Err(e) = fs::rename(&path, &target_path) {
                        error!(
                            "Failed to finalize recovered recording {}: {}",
                            file_name, e
                        );
                        continue;
                    }

                    let title = format!("{} (Recovered)", self.format_timestamp_title(ts));
                    if let Err(e) = self.save_to_database(
                        target_name.clone(),
                        ts,
                        title,
                        String::new(),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                    ) {
                        error!("Failed to add recovered recording to history: {}", e);
                        continue;
                    }

                    info!(
                        "Recovered interrupted recording: {} -> {}",
                        file_name, target_name
                    );
                    recovered += 1;
                }
                _ => {
                    // Nothing recoverable (header-only or unreadable) — clean up.
                    let _ = fs::remove_file(&path);
                    debug!("Removed unrecoverable recording file: {}", file_name);
                }
            }
        }

        // Also recover crashed chunked-Opus recordings (the current format).
        recovered += self.reconcile_orphan_opus_chunks().unwrap_or_else(|e| {
            error!("Opus chunk recovery failed: {}", e);
            0
        });

        if recovered > 0 {
            info!(
                "Recovered {} interrupted recording(s) into history",
                recovered
            );
            if let Err(e) = self.app_handle.emit("history-updated", ()) {
                error!("Failed to emit history-updated event: {}", e);
            }
        }

        Ok(())
    }

    /// Recover crashed chunked-Opus recordings: repair the in-progress
    /// `handy-{ts}-chunk-N-temp.opus` chunk(s), glue all chunks for that
    /// timestamp into `handy-{ts}.opus`, and add a "(Recovered)" history row.
    /// Returns the number of recordings recovered.
    fn reconcile_orphan_opus_chunks(&self) -> Result<usize> {
        // Timestamps that have an in-progress (`-temp.opus`) chunk = crashed.
        let mut crashed_ts: BTreeSet<u64> = BTreeSet::new();
        for entry in fs::read_dir(&self.recordings_dir)?.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(ts) = parse_temp_chunk_ts(&name) {
                crashed_ts.insert(ts);
            }
        }

        let mut recovered = 0usize;
        for ts in crashed_ts {
            // 1. Repair/finalize each in-progress chunk for this timestamp.
            for entry in fs::read_dir(&self.recordings_dir)?.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if parse_temp_chunk_ts(&name) != Some(ts) {
                    continue;
                }
                let temp_path = entry.path();
                match crate::audio_toolkit::repair_truncated_opus(&temp_path) {
                    Ok(samples) if samples > 0 => {
                        let final_name = name.replace("-temp.opus", ".opus");
                        let final_path = self.recordings_dir.join(&final_name);
                        if let Err(e) = fs::rename(&temp_path, &final_path) {
                            error!("Failed to finalize recovered chunk {}: {}", name, e);
                        }
                    }
                    _ => {
                        let _ = fs::remove_file(&temp_path);
                        debug!("Removed unrecoverable chunk: {}", name);
                    }
                }
            }

            // 2. Collect all finalized chunks for this timestamp, in index order.
            let mut chunks: Vec<(usize, PathBuf)> = Vec::new();
            for entry in fs::read_dir(&self.recordings_dir)?.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(idx) = parse_chunk_index(&name, ts) {
                    chunks.push((idx, entry.path()));
                }
            }
            if chunks.is_empty() {
                continue;
            }
            chunks.sort_by_key(|(i, _)| *i);
            let chunk_paths: Vec<PathBuf> = chunks.into_iter().map(|(_, p)| p).collect();

            // 3. Produce handy-{ts}.opus if not already present. A single chunk
            //    (recording under ~10 min) is renamed to the full name with no
            //    redundant chunk file; multiple chunks are glued + kept.
            let full_name = format!("handy-{}.opus", ts);
            let full_path = self.recordings_dir.join(&full_name);
            if !full_path.exists() {
                if chunk_paths.len() == 1 {
                    if let Err(e) = fs::rename(&chunk_paths[0], &full_path) {
                        error!(
                            "Failed to finalize single recovered chunk for ts {}: {}",
                            ts, e
                        );
                        continue;
                    }
                } else if let Err(e) = crate::audio_toolkit::glue_chunks(&chunk_paths, &full_path) {
                    error!("Failed to glue recovered chunks for ts {}: {}", ts, e);
                    continue;
                }
            }

            // 4. Add a "(Recovered)" history row if one doesn't already exist.
            if !self.history_has_file(&full_name).unwrap_or(false) {
                let title = format!("{} (Recovered)", self.format_timestamp_title(ts as i64));
                if let Err(e) = self.save_to_database(
                    full_name.clone(),
                    ts as i64,
                    title,
                    String::new(),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                ) {
                    error!("Failed to add recovered recording to history: {}", e);
                    continue;
                }
                info!("Recovered interrupted chunked recording: {}", full_name);
                recovered += 1;
            }
        }

        Ok(recovered)
    }

    /// Whether a history row already references the given file name.
    fn history_has_file(&self, file_name: &str) -> Result<bool> {
        let conn = self.get_connection()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM transcription_history WHERE file_name = ?1",
            params![file_name],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Save a transcription to history, writing a WAV from the in-memory samples.
    /// Used by the legacy / live / API paths when crash-safe Opus recording is
    /// off (or no glued Opus file exists). `timestamp` is the recording's start
    /// (the name its chunks carry, so a take whose chunks could not be glued
    /// can still be found from its row); None = now. An existing file of that
    /// name is never overwritten - the row then gets a fresh name.
    #[allow(clippy::too_many_arguments)]
    pub async fn save_transcription(
        &self,
        timestamp: Option<i64>,
        audio_samples: Vec<f32>,
        transcription_text: String,
        post_processed_text: Option<String>,
        post_process_prompt: Option<String>,
        cost_usd: Option<f64>,
        duration_seconds: Option<f64>,
        model_used: Option<String>,
        diarized_text: Option<String>,
    ) -> Result<()> {
        // The first free second from there: never write over another take's WAV.
        let mut timestamp = timestamp.unwrap_or_else(|| Utc::now().timestamp());
        for _ in 0..1000 {
            if !self
                .recordings_dir
                .join(format!("handy-{timestamp}.wav"))
                .exists()
            {
                break;
            }
            timestamp += 1;
        }
        if self
            .recordings_dir
            .join(format!("handy-{timestamp}.wav"))
            .exists()
        {
            return Err(anyhow::anyhow!("no free name for the recording"));
        }
        let file_name = format!("handy-{}.wav", timestamp);

        // Save WAV file
        let file_path = self.recordings_dir.join(&file_name);
        save_wav_file(file_path, &audio_samples).await?;

        self.save_transcription_with_file(
            file_name,
            timestamp,
            transcription_text,
            post_processed_text,
            post_process_prompt,
            cost_usd,
            duration_seconds,
            model_used,
            diarized_text,
        )
        .await
    }

    /// Save a history row for an audio file that ALREADY exists on disk (e.g. the
    /// glued `handy-{ts}.opus` produced by the chunked recorder). Skips writing a
    /// WAV. `timestamp` MUST be the recording-start ts used to build `file_name`
    /// so the row's title and the file name agree.
    #[allow(clippy::too_many_arguments)]
    pub async fn save_transcription_with_file(
        &self,
        file_name: String,
        timestamp: i64,
        transcription_text: String,
        post_processed_text: Option<String>,
        post_process_prompt: Option<String>,
        cost_usd: Option<f64>,
        duration_seconds: Option<f64>,
        model_used: Option<String>,
        diarized_text: Option<String>,
    ) -> Result<()> {
        let title = self.format_timestamp_title(timestamp);
        // The take's transcript beside its recording (`handy-{ts}.txt`), from
        // the text it already has: nothing transcribes the recording again.
        let text_file = crate::settings::get_settings(&self.app_handle)
            .recording_text_file
            .then(|| (file_name.clone(), transcription_text.clone()));

        {
            // Row and text file together (see `text_files`); released before
            // the cleanup below, which takes the lock itself.
            let _files = self.text_files.lock().unwrap_or_else(|p| p.into_inner());
            self.save_to_database(
                file_name,
                timestamp,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                cost_usd,
                duration_seconds,
                model_used,
                diarized_text,
            )?;
            if let Some((name, text)) = text_file {
                if let Err(e) = create_text_file_in(&self.recordings_dir, &name, &text) {
                    error!("Failed to write the text file of {}: {}", name, e);
                }
            }
        }

        // Clean up old entries
        self.cleanup_old_entries()?;

        // Emit history updated event
        if let Err(e) = self.app_handle.emit("history-updated", ()) {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn save_to_database(
        &self,
        file_name: String,
        timestamp: i64,
        title: String,
        transcription_text: String,
        post_processed_text: Option<String>,
        post_process_prompt: Option<String>,
        cost_usd: Option<f64>,
        duration_seconds: Option<f64>,
        model_used: Option<String>,
        diarized_text: Option<String>,
    ) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute(
            // audio_purged_at is deliberately NOT set here: a new row's audio exists,
            // and NULL is what "never purged on purpose" means.
            "INSERT INTO transcription_history (file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, cost_usd, duration_seconds, model_used, diarized_text) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![file_name, timestamp, false, title, transcription_text, post_processed_text, post_process_prompt, cost_usd, duration_seconds, model_used, diarized_text],
        )?;

        debug!("Saved transcription to database");
        Ok(())
    }

    pub fn cleanup_old_entries(&self) -> Result<()> {
        let retention_period = crate::settings::get_recording_retention_period(&self.app_handle);

        match retention_period {
            crate::settings::RecordingRetentionPeriod::Never => {
                // Don't delete anything
                return Ok(());
            }
            crate::settings::RecordingRetentionPeriod::PreserveLimit => {
                // Use the old count-based logic with history_limit
                let limit = crate::settings::get_history_limit(&self.app_handle);
                return self.cleanup_by_count(limit);
            }
            _ => {
                // Use time-based logic
                return self.cleanup_by_time(retention_period);
            }
        }
    }

    /// Delete the chunk sibling files (`handy-{ts}-chunk-*.opus`) for a finalized
    /// `handy-{ts}.opus` recording, so chunks share their parent's lifecycle. A
    /// no-op for non-opus or non-Handy names.
    fn delete_chunk_siblings(&self, full_file_name: &str) {
        let ts = match full_file_name
            .strip_prefix("handy-")
            .and_then(|s| s.strip_suffix(".opus"))
        {
            Some(ts) => ts.to_string(),
            None => return,
        };
        let prefix = format!("handy-{}-chunk-", ts);
        let entries = match fs::read_dir(&self.recordings_dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&prefix) && (name.ends_with(".opus") || name.ends_with(".ogg")) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    /// Fold one about-to-be-deleted row into the carry-forward totals.
    ///
    /// `chars` counts CHARACTERS, not words: `split_whitespace()` is permanently
    /// wrong for Chinese, Japanese and Korean - all of which Handy ships locales and
    /// an ASR engine for - and a 600-character Japanese dictation would score 1 word
    /// forever, with the detail gone and no way to recompute it.
    fn accrue_purged_row(conn: &rusqlite::Connection, id: i64) -> Result<()> {
        let (ts, secs, cost, chars): (i64, Option<f64>, Option<f64>, i64) = conn.query_row(
            "SELECT timestamp, duration_seconds, cost_usd, LENGTH(transcription_text)
             FROM transcription_history WHERE id = ?1",
            params![id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get::<_, i64>(3).unwrap_or(0),
                ))
            },
        )?;

        let secs_milli = (secs.unwrap_or(0.0) * 1000.0).round().max(0.0) as i64;
        let cost_nanos = (cost.unwrap_or(0.0) * 1_000_000_000.0).round().max(0.0) as i64;

        conn.execute(
            "INSERT INTO purged_totals (key, takes, seconds_milli, cost_nanos, chars, first_ts, last_ts)
             VALUES ('lifetime', 1, ?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(key) DO UPDATE SET
               takes         = takes + 1,
               seconds_milli = seconds_milli + excluded.seconds_milli,
               cost_nanos    = cost_nanos + excluded.cost_nanos,
               chars         = chars + excluded.chars,
               first_ts      = MIN(COALESCE(first_ts, excluded.first_ts), excluded.first_ts),
               last_ts       = MAX(COALESCE(last_ts, excluded.last_ts), excluded.last_ts)",
            params![secs_milli, cost_nanos, chars, ts],
        )?;
        Ok(())
    }

    /// Totals carried forward from rows retention has deleted. Added to the live-row
    /// fold to produce a lifetime figure that does not shrink when history is purged.
    pub fn get_purged_totals(&self) -> Result<PurgedTotals> {
        let conn = self.get_connection()?;
        let t = conn
            .query_row(
                "SELECT takes, seconds_milli, cost_nanos, chars FROM purged_totals WHERE key = 'lifetime'",
                [],
                |r| {
                    Ok(PurgedTotals {
                        takes: r.get(0)?,
                        seconds: r.get::<_, i64>(1)? as f64 / 1000.0,
                        cost_usd: r.get::<_, i64>(2)? as f64 / 1_000_000_000.0,
                        chars: r.get(3)?,
                    })
                },
            )
            .unwrap_or_default();
        Ok(t)
    }

    /// Discard the carried-forward totals. The escape hatch that makes retaining
    /// any derived data acceptable.
    pub fn reset_purged_totals(&self) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute("DELETE FROM purged_totals", [])?;
        Ok(())
    }

    fn delete_entries_and_files(&self, entries: &[(i64, String)]) -> Result<usize> {
        if entries.is_empty() {
            return Ok(0);
        }
        let _files = self.text_files.lock().unwrap_or_else(|p| p.into_inner());

        // When the user has asked to keep transcriptions, retention becomes
        // audio-only: the row survives with `audio_purged_at` stamped, so the
        // History page can say "audio removed" instead of silently offering a
        // play button for a file that is gone. Explicit per-entry deletion is a
        // different path and still removes everything.
        let preserve = crate::settings::get_preserve_transcriptions(&self.app_handle);

        let conn = self.get_connection()?;
        let mut deleted_count = 0;
        let now = Utc::now().timestamp();

        for (id, file_name) in entries {
            if preserve {
                conn.execute(
                    "UPDATE transcription_history SET audio_purged_at = ?1 WHERE id = ?2",
                    params![now, id],
                )?;
            } else {
                // The row's own text file goes with the row (only when Handy
                // wrote it; with transcriptions kept, above, it stays too).
                let row_text: Option<String> = conn
                    .query_row(
                        "SELECT transcription_text FROM transcription_history WHERE id = ?1",
                        params![id],
                        |row| row.get(0),
                    )
                    .optional()?;
                if let Some(text) = row_text {
                    remove_owned_text_file(&self.recordings_dir, file_name, &text);
                }
                // Accrue BEFORE the delete, in the same loop that removes the row, so
                // a row cannot be deleted without being counted. Explicit per-entry
                // deletion does not come through here: pressing the trash can means
                // "forget this", and it should leave no residue in the totals.
                if let Err(e) = Self::accrue_purged_row(&conn, *id) {
                    error!("Failed to accrue lifetime totals for entry {}: {}", id, e);
                }
                conn.execute(
                    "DELETE FROM transcription_history WHERE id = ?1",
                    params![id],
                )?;
            }

            // Only ever delete files Handy created. Never touch user files.
            if !is_handy_recording_file(file_name) {
                debug!(
                    "Skipping deletion of non-Handy file referenced in history: {}",
                    file_name
                );
                continue;
            }

            // Delete the recording file
            let file_path = self.recordings_dir.join(file_name);
            if file_path.exists() {
                if let Err(e) = fs::remove_file(&file_path) {
                    error!("Failed to delete recording file {}: {}", file_name, e);
                } else {
                    debug!("Deleted old recording file: {}", file_name);
                    deleted_count += 1;
                }
            }
            // Remove the chunk siblings of a glued opus recording.
            self.delete_chunk_siblings(file_name);
        }

        Ok(deleted_count)
    }

    fn cleanup_by_count(&self, limit: usize) -> Result<()> {
        let conn = self.get_connection()?;

        // Get all entries that are not saved, ordered by timestamp desc
        let mut stmt = conn.prepare(
            // audio_purged_at IS NULL: a row whose audio retention already removed is
            // not a candidate again. Preserved rows survive, so without this filter
            // they would be re-selected on every run forever.
            "SELECT id, file_name FROM transcription_history              WHERE saved = 0 AND audio_purged_at IS NULL ORDER BY timestamp DESC"
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>("id")?, row.get::<_, String>("file_name")?))
        })?;

        let mut entries: Vec<(i64, String)> = Vec::new();
        for row in rows {
            entries.push(row?);
        }

        if entries.len() > limit {
            let entries_to_delete = &entries[limit..];
            let deleted_count = self.delete_entries_and_files(entries_to_delete)?;

            if deleted_count > 0 {
                debug!("Cleaned up {} old history entries by count", deleted_count);
            }
        }

        Ok(())
    }

    fn cleanup_by_time(
        &self,
        retention_period: crate::settings::RecordingRetentionPeriod,
    ) -> Result<()> {
        let conn = self.get_connection()?;

        // Calculate cutoff timestamp (current time minus retention period)
        let now = Utc::now().timestamp();
        let cutoff_timestamp = match retention_period {
            crate::settings::RecordingRetentionPeriod::Days3 => now - (3 * 24 * 60 * 60), // 3 days in seconds
            crate::settings::RecordingRetentionPeriod::Weeks2 => now - (2 * 7 * 24 * 60 * 60), // 2 weeks in seconds
            crate::settings::RecordingRetentionPeriod::Months3 => now - (3 * 30 * 24 * 60 * 60), // 3 months in seconds (approximate)
            _ => unreachable!("Should not reach here"),
        };

        // Get all unsaved entries older than the cutoff timestamp
        let mut stmt = conn.prepare(
            "SELECT id, file_name FROM transcription_history              WHERE saved = 0 AND audio_purged_at IS NULL AND timestamp < ?1",
        )?;

        let rows = stmt.query_map(params![cutoff_timestamp], |row| {
            Ok((row.get::<_, i64>("id")?, row.get::<_, String>("file_name")?))
        })?;

        let mut entries_to_delete: Vec<(i64, String)> = Vec::new();
        for row in rows {
            entries_to_delete.push(row?);
        }

        let deleted_count = self.delete_entries_and_files(&entries_to_delete)?;

        if deleted_count > 0 {
            debug!(
                "Cleaned up {} old history entries based on retention period",
                deleted_count
            );
        }

        Ok(())
    }

    pub async fn get_history_entries(&self) -> Result<Vec<HistoryEntry>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, cost_usd, duration_seconds, model_used, audio_purged_at, diarized_text, name FROM transcription_history ORDER BY timestamp DESC"
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(HistoryEntry {
                id: row.get("id")?,
                file_name: row.get("file_name")?,
                timestamp: row.get("timestamp")?,
                saved: row.get("saved")?,
                title: row.get("title")?,
                transcription_text: row.get("transcription_text")?,
                post_processed_text: row.get("post_processed_text")?,
                post_process_prompt: row.get("post_process_prompt")?,
                cost_usd: row.get("cost_usd")?,
                duration_seconds: row.get("duration_seconds")?,
                model_used: row.get("model_used")?,
                audio_purged_at: row.get("audio_purged_at")?,
                diarized_text: row.get("diarized_text")?,
                name: row.get("name")?,
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }

        // Breadcrumb for diagnosing "history page is empty" reports: proves the
        // frontend asked and what it got.
        log::info!("get_history_entries → {} rows", entries.len());
        Ok(entries)
    }

    /// Backfill `duration_seconds` for rows missing it, by reading each existing
    /// audio file (WAV via hound; Ogg/Opus via the granule). Idempotent — only
    /// touches NULL rows whose file still exists. Returns the number updated.
    pub fn backfill_missing_durations(&self) -> Result<usize> {
        let conn = self.get_connection()?;
        let rows: Vec<(i64, String)> = {
            let mut stmt = conn.prepare(
                "SELECT id, file_name FROM transcription_history WHERE duration_seconds IS NULL",
            )?;
            let mapped = stmt.query_map([], |row| {
                Ok((row.get::<_, i64>("id")?, row.get::<_, String>("file_name")?))
            })?;
            mapped.collect::<std::result::Result<Vec<_>, _>>()?
        };
        let mut updated = 0usize;
        for (id, file_name) in rows {
            let path = self.recordings_dir.join(&file_name);
            if !path.exists() {
                continue;
            }
            if let Some(secs) = audio_file_duration_seconds(&path) {
                if conn
                    .execute(
                        "UPDATE transcription_history SET duration_seconds = ?1 WHERE id = ?2",
                        params![secs, id],
                    )
                    .is_ok()
                {
                    updated += 1;
                }
            }
        }
        if updated > 0 {
            info!("Backfilled duration for {} history entries", updated);
        }
        Ok(updated)
    }

    pub fn get_latest_entry(&self) -> Result<Option<HistoryEntry>> {
        let conn = self.get_connection()?;
        Self::get_latest_entry_with_conn(&conn)
    }

    fn get_latest_entry_with_conn(conn: &Connection) -> Result<Option<HistoryEntry>> {
        let mut stmt = conn.prepare(
            "SELECT id, file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, cost_usd, duration_seconds, model_used, audio_purged_at, diarized_text, name
             FROM transcription_history
             ORDER BY timestamp DESC
             LIMIT 1",
        )?;

        let entry = stmt
            .query_row([], |row| {
                Ok(HistoryEntry {
                    id: row.get("id")?,
                    file_name: row.get("file_name")?,
                    timestamp: row.get("timestamp")?,
                    saved: row.get("saved")?,
                    title: row.get("title")?,
                    transcription_text: row.get("transcription_text")?,
                    post_processed_text: row.get("post_processed_text")?,
                    post_process_prompt: row.get("post_process_prompt")?,
                    cost_usd: row.get("cost_usd")?,
                    duration_seconds: row.get("duration_seconds")?,
                    model_used: row.get("model_used")?,
                    audio_purged_at: row.get("audio_purged_at")?,
                    diarized_text: row.get("diarized_text")?,
                    name: row.get("name")?,
                })
            })
            .optional()?;

        Ok(entry)
    }

    /// Names a recording (`None` or blank = no name). At most 120 characters.
    pub async fn set_entry_name(&self, id: i64, name: Option<String>) -> Result<()> {
        let name = name
            .map(|n| n.trim().chars().take(120).collect::<String>())
            .filter(|n| !n.is_empty());
        let conn = self.get_connection()?;
        let changed = conn.execute(
            "UPDATE transcription_history SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        if changed != 1 {
            return Err(anyhow::anyhow!("the history entry no longer exists"));
        }
        if let Err(e) = self.app_handle.emit("history-updated", ()) {
            error!("Failed to emit history-updated event: {}", e);
        }
        Ok(())
    }

    pub async fn toggle_saved_status(&self, id: i64) -> Result<()> {
        let conn = self.get_connection()?;

        // Get current saved status
        let current_saved: bool = conn.query_row(
            "SELECT saved FROM transcription_history WHERE id = ?1",
            params![id],
            |row| row.get("saved"),
        )?;

        let new_saved = !current_saved;

        conn.execute(
            "UPDATE transcription_history SET saved = ?1 WHERE id = ?2",
            params![new_saved, id],
        )?;

        debug!("Toggled saved status for entry {}: {}", id, new_saved);

        // Emit history updated event
        if let Err(e) = self.app_handle.emit("history-updated", ()) {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(())
    }

    pub fn get_audio_file_path(&self, file_name: &str) -> PathBuf {
        self.recordings_dir.join(file_name)
    }

    /// History's "Transcribe again": the take's text file follows the new text
    /// (see `rewrite_text_file_in`). A file Handy wrote is always kept in step
    /// with its row, so it never turns into an orphan Handy no longer
    /// recognises; a missing one is created only when `recording_text_file` is
    /// on, or when `restore` puts back what a failed update replaced.
    pub fn rewrite_text_file(
        &self,
        file_name: &str,
        previous_text: &str,
        new_text: &str,
        restore: bool,
    ) -> Result<bool> {
        let create_missing =
            restore || crate::settings::get_settings(&self.app_handle).recording_text_file;
        rewrite_text_file_in(
            &self.recordings_dir,
            file_name,
            previous_text,
            new_text,
            create_missing,
        )
    }

    /// The audio of a history row, to read it again: the row's own file, or -
    /// when a long take's glued file is missing - its chunks in order. Only a
    /// plain Handy recording name inside the recordings folder is accepted
    /// (no paths, no `..`), and nothing is ever written or deleted.
    pub fn recording_files_for(&self, file_name: &str) -> Result<Vec<PathBuf>> {
        let plain = !file_name.is_empty()
            && !file_name.contains(['/', '\\', ':'])
            && !file_name.contains("..")
            && is_handy_recording_file(file_name);
        if !plain {
            return Err(anyhow::anyhow!("not a Handy recording: {file_name}"));
        }
        let path = self.recordings_dir.join(file_name);
        // A row's own file with real audio in it (an empty WAV is 44 bytes:
        // what a take whose chunks could not be glued saves under its row).
        let own_has_audio = fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() > 1024);
        let files = if own_has_audio {
            vec![path]
        } else {
            match self.chunk_files_for(file_name)? {
                Some(chunks) => chunks,
                None if path.is_file() => vec![path],
                None => return Err(anyhow::anyhow!("the recording file is missing")),
            }
        };
        // Inside the recordings folder for real (no link pointing elsewhere).
        let dir = fs::canonicalize(&self.recordings_dir)?;
        for file in &files {
            let linked = fs::symlink_metadata(file).is_ok_and(|m| m.file_type().is_symlink());
            if linked || !fs::canonicalize(file)?.starts_with(&dir) {
                return Err(anyhow::anyhow!("not a Handy recording: {}", file.display()));
            }
        }
        Ok(files)
    }

    /// A chunked take's pieces, handy-{ts}-chunk-N.opus in order, when they
    /// are all there (1..=n, no gap); None otherwise.
    fn chunk_files_for(&self, file_name: &str) -> Result<Option<Vec<PathBuf>>> {
        let Some(stem) = [".opus", ".ogg", ".wav"]
            .iter()
            .find_map(|ext| file_name.strip_suffix(ext))
        else {
            return Ok(None);
        };
        let prefix = format!("{stem}-chunk-");
        let mut chunks: Vec<(usize, PathBuf)> = fs::read_dir(&self.recordings_dir)?
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().to_string();
                let n = name
                    .strip_prefix(&prefix)?
                    .strip_suffix(".opus")?
                    .parse::<usize>()
                    .ok()?;
                Some((n, entry.path()))
            })
            .collect();
        chunks.sort_by_key(|(n, _)| *n);
        let complete =
            !chunks.is_empty() && chunks.iter().enumerate().all(|(i, (n, _))| *n == i + 1);
        Ok(complete.then(|| chunks.into_iter().map(|(_, p)| p).collect()))
    }

    /// Replaces a row's transcript with a new one (History's "Transcribe
    /// again"). The post-processed text came from the old transcript, so it
    /// is cleared with it.
    /// History's "Transcribe again": the row and its text file change together,
    /// under the same lock as deletion and retention. The file follows first
    /// (only a file Handy wrote, see `rewrite_text_file`) - a failed write
    /// leaves both as they were - and is put back if the row cannot be updated.
    pub fn replace_transcription(
        &self,
        id: i64,
        text: &str,
        model: Option<String>,
        diarized: Option<&str>,
    ) -> Result<()> {
        let _files = self.text_files.lock().unwrap_or_else(|p| p.into_inner());
        let entry = self
            .entry_by_id(id)?
            .ok_or_else(|| anyhow::anyhow!("the history entry no longer exists"))?;
        let file_changed =
            self.rewrite_text_file(&entry.file_name, &entry.transcription_text, text, false)?;
        if let Err(e) = self.update_transcription_row(id, text, model, diarized) {
            if file_changed {
                if let Err(e) =
                    self.rewrite_text_file(&entry.file_name, text, &entry.transcription_text, true)
                {
                    error!("Failed to put back the text file of entry {}: {}", id, e);
                }
            }
            return Err(e);
        }
        Ok(())
    }

    /// A take's speaker text that arrived after the take was saved (the STT
    /// server's whole-take request runs after delivery). Only fills an empty
    /// one: a row transcribed again meanwhile keeps its newer speaker text.
    pub fn set_diarized_for_file(&self, file_name: &str, diarized: &str) -> Result<()> {
        let conn = self.get_connection()?;
        let changed = conn.execute(
            "UPDATE transcription_history SET diarized_text = ?1 WHERE file_name = ?2 AND diarized_text IS NULL",
            params![diarized, file_name],
        )?;
        if changed == 0 {
            return Err(anyhow::anyhow!(
                "no row without speaker text for this recording"
            ));
        }
        if let Err(e) = self.app_handle.emit("history-updated", ()) {
            error!("Failed to emit history-updated event: {}", e);
        }
        Ok(())
    }

    /// The row's new text; its speaker text is replaced too (`None` clears a
    /// stale one from an earlier transcription).
    fn update_transcription_row(
        &self,
        id: i64,
        text: &str,
        model: Option<String>,
        diarized: Option<&str>,
    ) -> Result<()> {
        let conn = self.get_connection()?;
        let changed = conn.execute(
            "UPDATE transcription_history SET transcription_text = ?1, post_processed_text = NULL, post_process_prompt = NULL, model_used = COALESCE(?2, model_used), diarized_text = ?4 WHERE id = ?3",
            params![text, model, id, diarized],
        )?;
        if changed != 1 {
            return Err(anyhow::anyhow!("the history entry no longer exists"));
        }
        if let Err(e) = self.app_handle.emit("history-updated", ()) {
            error!("Failed to emit history-updated event: {}", e);
        }
        Ok(())
    }

    pub async fn get_entry_by_id(&self, id: i64) -> Result<Option<HistoryEntry>> {
        self.entry_by_id(id)
    }

    fn entry_by_id(&self, id: i64) -> Result<Option<HistoryEntry>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, cost_usd, duration_seconds, model_used, audio_purged_at, diarized_text, name
             FROM transcription_history WHERE id = ?1",
        )?;

        let entry = stmt
            .query_row([id], |row| {
                Ok(HistoryEntry {
                    id: row.get("id")?,
                    file_name: row.get("file_name")?,
                    timestamp: row.get("timestamp")?,
                    saved: row.get("saved")?,
                    title: row.get("title")?,
                    transcription_text: row.get("transcription_text")?,
                    post_processed_text: row.get("post_processed_text")?,
                    post_process_prompt: row.get("post_process_prompt")?,
                    cost_usd: row.get("cost_usd")?,
                    duration_seconds: row.get("duration_seconds")?,
                    model_used: row.get("model_used")?,
                    audio_purged_at: row.get("audio_purged_at")?,
                    diarized_text: row.get("diarized_text")?,
                    name: row.get("name")?,
                })
            })
            .optional()?;

        Ok(entry)
    }

    pub async fn delete_entry(&self, id: i64) -> Result<()> {
        self.delete_entry_files_and_row(id)?;

        debug!("Deleted history entry with id: {}", id);

        // Emit history updated event
        if let Err(e) = self.app_handle.emit("history-updated", ()) {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(())
    }

    fn delete_entry_files_and_row(&self, id: i64) -> Result<()> {
        let _files = self.text_files.lock().unwrap_or_else(|p| p.into_inner());
        let conn = self.get_connection()?;

        // Get the entry to find the file name
        if let Some(entry) = self.entry_by_id(id)? {
            // Delete the audio file first — but only if Handy created it, so we
            // never remove a user's own file that may share the folder.
            if is_handy_recording_file(&entry.file_name) {
                let file_path = self.get_audio_file_path(&entry.file_name);
                if file_path.exists() {
                    if let Err(e) = fs::remove_file(&file_path) {
                        error!("Failed to delete audio file {}: {}", entry.file_name, e);
                        // Continue with database deletion even if file deletion fails
                    }
                }
                // Remove the chunk siblings of a glued opus recording.
                self.delete_chunk_siblings(&entry.file_name);
                remove_owned_text_file(
                    &self.recordings_dir,
                    &entry.file_name,
                    &entry.transcription_text,
                );
            }
        }

        // Delete from database
        conn.execute(
            "DELETE FROM transcription_history WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    fn format_timestamp_title(&self, timestamp: i64) -> String {
        if let Some(utc_datetime) = DateTime::from_timestamp(timestamp, 0) {
            // Convert UTC to local timezone
            let local_datetime = utc_datetime.with_timezone(&Local);
            local_datetime.format("%B %e, %Y - %l:%M%p").to_string()
        } else {
            format!("Recording {}", timestamp)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{Connection, params};

    #[test]
    fn a_takes_text_goes_beside_its_recording_and_never_over_a_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let rec = dir.path();
        assert!(create_text_file_in(rec, "handy-17.opus", "  Hello there. \n").unwrap());
        let txt = rec.join("handy-17.txt");
        assert_eq!(fs::read_to_string(&txt).unwrap(), "Hello there.");
        assert!(!rec.join("handy-17.txt.tmp").exists());
        // A file already there is never replaced at take time.
        assert!(!create_text_file_in(rec, "handy-17.opus", "Other").unwrap());
        assert_eq!(fs::read_to_string(&txt).unwrap(), "Hello there.");
        // A WAV take gets the same kind of name.
        assert!(create_text_file_in(rec, "handy-18.wav", "Wav take").unwrap());
        assert!(rec.join("handy-18.txt").exists());
        // Nothing for empty text or a name that is not a Handy recording.
        assert!(!create_text_file_in(rec, "handy-19.opus", "   ").unwrap());
        for name in [
            "notes.opus",
            "../handy-20.opus",
            "sub/handy-20.opus",
            "handy-20.txt",
        ] {
            assert!(!create_text_file_in(rec, name, "text").unwrap(), "{name}");
        }
        assert!(!rec.join("handy-19.txt").exists());
    }

    #[test]
    fn transcribe_again_replaces_only_a_file_handy_wrote() {
        let dir = tempfile::tempdir().expect("temp dir");
        let rec = dir.path();
        let txt = rec.join("handy-21.txt");
        // Handy's file (it holds the row's text): replaced.
        fs::write(&txt, "old text").unwrap();
        assert!(rewrite_text_file_in(rec, "handy-21.opus", "old text", "new text", true).unwrap());
        assert_eq!(fs::read_to_string(&txt).unwrap(), "new text");
        // Your own note under that name: left alone.
        fs::write(&txt, "my note").unwrap();
        assert!(!rewrite_text_file_in(rec, "handy-21.opus", "new text", "newer", true).unwrap());
        assert_eq!(fs::read_to_string(&txt).unwrap(), "my note");
        // Missing: written only when asked to create it.
        fs::remove_file(&txt).unwrap();
        assert!(!rewrite_text_file_in(rec, "handy-21.opus", "", "fresh", false).unwrap());
        assert!(!txt.exists());
        assert!(rewrite_text_file_in(rec, "handy-21.opus", "", "fresh", true).unwrap());
        assert_eq!(fs::read_to_string(&txt).unwrap(), "fresh");
        // New text empty: Handy's file is removed, a note is not.
        assert!(rewrite_text_file_in(rec, "handy-21.opus", "fresh", "  ", false).unwrap());
        assert!(!txt.exists());
        fs::write(&txt, "my note").unwrap();
        assert!(!rewrite_text_file_in(rec, "handy-21.opus", "fresh", "", false).unwrap());
        assert!(txt.exists());
    }

    #[test]
    fn a_rows_text_file_is_removed_only_when_handy_wrote_it() {
        let dir = tempfile::tempdir().expect("temp dir");
        let rec = dir.path();
        let txt = rec.join("handy-22.txt");
        fs::write(&txt, "my note").unwrap();
        remove_owned_text_file(rec, "handy-22.opus", "the take");
        assert!(txt.exists());
        fs::write(&txt, "the take\n").unwrap();
        remove_owned_text_file(rec, "handy-22.opus", "the take");
        assert!(!txt.exists());
        // An empty row owns nothing.
        fs::write(&txt, "").unwrap();
        remove_owned_text_file(rec, "handy-22.opus", "");
        assert!(txt.exists());
    }

    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(
            "CREATE TABLE transcription_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_name TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                saved BOOLEAN NOT NULL DEFAULT 0,
                title TEXT NOT NULL,
                transcription_text TEXT NOT NULL,
                post_processed_text TEXT,
                post_process_prompt TEXT,
                cost_usd REAL,
                duration_seconds REAL,
                model_used TEXT,
                audio_purged_at INTEGER,
                diarized_text TEXT,
                name TEXT
            );",
        )
        .expect("create transcription_history table");
        conn
    }

    fn insert_entry(conn: &Connection, timestamp: i64, text: &str, post_processed: Option<&str>) {
        conn.execute(
            "INSERT INTO transcription_history (file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                format!("handy-{}.wav", timestamp),
                timestamp,
                false,
                format!("Recording {}", timestamp),
                text,
                post_processed,
                Option::<String>::None
            ],
        )
        .expect("insert history entry");
    }

    #[test]
    fn get_latest_entry_returns_none_when_empty() {
        let conn = setup_conn();
        let entry = HistoryManager::get_latest_entry_with_conn(&conn).expect("fetch latest entry");
        assert!(entry.is_none());
    }

    #[test]
    fn get_latest_entry_returns_newest_entry() {
        let conn = setup_conn();
        insert_entry(&conn, 100, "first", None);
        insert_entry(&conn, 200, "second", Some("processed"));

        let entry = HistoryManager::get_latest_entry_with_conn(&conn)
            .expect("fetch latest entry")
            .expect("entry exists");

        assert_eq!(entry.timestamp, 200);
        assert_eq!(entry.transcription_text, "second");
        assert_eq!(entry.post_processed_text.as_deref(), Some("processed"));
    }
}
