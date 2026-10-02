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
    // Notes made from transcripts ("Make note" on a History entry, or pasted
    // text on Notes › Manual note). `history_id` is NULL for pasted text and is
    // deliberately not a foreign key: the note outlives its source entry, and
    // the UI shows "Source transcript was deleted" instead. `with_speakers`
    // marks a note made from a speaker-labelled transcript; `cost_usd` is the
    // LLM call's cost when the provider reported or priced it.
    M::up(
        "CREATE TABLE IF NOT EXISTS notes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            history_id INTEGER NULL,
            timestamp INTEGER NOT NULL,
            source_text TEXT NOT NULL,
            note_text TEXT NOT NULL,
            skill_name TEXT NULL,
            model TEXT NOT NULL,
            with_speakers BOOLEAN NOT NULL DEFAULT 0,
            cost_usd REAL NULL
        );
        CREATE INDEX IF NOT EXISTS idx_notes_history_id ON notes(history_id);",
    ),
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
}

/// A Markdown note generated from a transcript.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct Note {
    pub id: i64,
    /// The history entry the transcript came from; `None` for pasted text.
    pub history_id: Option<i64>,
    pub timestamp: i64,
    pub source_text: String,
    pub note_text: String,
    /// The skill the note was written with; `None` = the built-in instructions.
    pub skill_name: Option<String>,
    pub model: String,
    /// Made from a speaker-labelled transcript ("Make note with speakers").
    pub with_speakers: bool,
    /// USD cost of the LLM call, when known.
    pub cost_usd: Option<f64>,
    /// Whether the source history entry still exists.
    pub source_exists: bool,
}

/// A note to save (see [`HistoryManager::save_note`]).
pub struct NewNote<'a> {
    pub history_id: Option<i64>,
    pub source_text: &'a str,
    pub note_text: &'a str,
    pub skill_name: Option<&'a str>,
    pub model: &'a str,
    pub with_speakers: bool,
    pub cost_usd: Option<f64>,
}

const NOTE_COLUMNS: &str =
    "n.id, n.history_id, n.timestamp, n.source_text, n.note_text, n.skill_name, n.model,
     n.with_speakers, n.cost_usd,
     EXISTS(SELECT 1 FROM transcription_history h WHERE h.id = n.history_id) AS source_exists";

pub struct HistoryManager {
    app_handle: AppHandle,
    recordings_dir: PathBuf,
    db_path: PathBuf,
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
    /// off (or no glued Opus file exists).
    #[allow(clippy::too_many_arguments)]
    pub async fn save_transcription(
        &self,
        audio_samples: Vec<f32>,
        transcription_text: String,
        post_processed_text: Option<String>,
        post_process_prompt: Option<String>,
        cost_usd: Option<f64>,
        duration_seconds: Option<f64>,
        model_used: Option<String>,
    ) -> Result<()> {
        let timestamp = Utc::now().timestamp();
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
    ) -> Result<()> {
        let title = self.format_timestamp_title(timestamp);

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
        )?;

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
    ) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute(
            // audio_purged_at is deliberately NOT set here: a new row's audio exists,
            // and NULL is what "never purged on purpose" means.
            "INSERT INTO transcription_history (file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, cost_usd, duration_seconds, model_used) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![file_name, timestamp, false, title, transcription_text, post_processed_text, post_process_prompt, cost_usd, duration_seconds, model_used],
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
            "SELECT id, file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, cost_usd, duration_seconds, model_used, audio_purged_at FROM transcription_history ORDER BY timestamp DESC"
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
            "SELECT id, file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, cost_usd, duration_seconds, model_used, audio_purged_at
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
                })
            })
            .optional()?;

        Ok(entry)
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

    pub async fn get_entry_by_id(&self, id: i64) -> Result<Option<HistoryEntry>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, cost_usd, duration_seconds, model_used, audio_purged_at
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
                })
            })
            .optional()?;

        Ok(entry)
    }

    pub async fn delete_entry(&self, id: i64) -> Result<()> {
        let conn = self.get_connection()?;

        // Get the entry to find the file name
        if let Some(entry) = self.get_entry_by_id(id).await? {
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
            }
        }

        // Delete from database
        conn.execute(
            "DELETE FROM transcription_history WHERE id = ?1",
            params![id],
        )?;

        debug!("Deleted history entry with id: {}", id);

        // Emit history updated event
        if let Err(e) = self.app_handle.emit("history-updated", ()) {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(())
    }

    /// Star a history entry so retention never deletes it (or its audio), and
    /// emit `history-updated` when that changed it. Set-only, unlike the Star
    /// button's toggle. Returns false when the entry no longer exists.
    pub fn mark_entry_saved(&self, id: i64) -> Result<bool> {
        let conn = self.get_connection()?;
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM transcription_history WHERE id = ?1)",
            params![id],
            |row| row.get(0),
        )?;
        if !exists {
            return Ok(false);
        }
        if Self::mark_entry_saved_with_conn(&conn, id)? {
            debug!("Starred history entry {} for a note", id);
            if let Err(e) = self.app_handle.emit("history-updated", ()) {
                error!("Failed to emit history-updated event: {}", e);
            }
        }
        Ok(true)
    }

    /// Returns whether the entry was newly starred.
    fn mark_entry_saved_with_conn(conn: &Connection, id: i64) -> Result<bool> {
        Ok(conn.execute(
            "UPDATE transcription_history SET saved = 1 WHERE id = ?1 AND saved = 0",
            params![id],
        )? > 0)
    }

    fn map_note(row: &rusqlite::Row<'_>) -> rusqlite::Result<Note> {
        Ok(Note {
            id: row.get("id")?,
            history_id: row.get("history_id")?,
            timestamp: row.get("timestamp")?,
            source_text: row.get("source_text")?,
            note_text: row.get("note_text")?,
            skill_name: row.get("skill_name")?,
            model: row.get("model")?,
            with_speakers: row.get("with_speakers")?,
            cost_usd: row.get("cost_usd")?,
            source_exists: row.get("source_exists")?,
        })
    }

    /// Save a generated note. Its source entry (if any) is starred too, so a
    /// transcript behind a note is never removed by retention.
    pub fn save_note(&self, new_note: &NewNote<'_>) -> Result<Note> {
        let conn = self.get_connection()?;
        let (note, starred) = Self::save_note_with_conn(&conn, new_note)?;
        debug!("Saved note with id {}", note.id);
        if starred {
            if let Err(e) = self.app_handle.emit("history-updated", ()) {
                error!("Failed to emit history-updated event: {}", e);
            }
        }
        Ok(note)
    }

    /// Returns the saved note and whether its source entry was newly starred.
    fn save_note_with_conn(conn: &Connection, note: &NewNote<'_>) -> Result<(Note, bool)> {
        conn.execute(
            "INSERT INTO notes (history_id, timestamp, source_text, note_text, skill_name, model, with_speakers, cost_usd)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                note.history_id,
                Utc::now().timestamp(),
                note.source_text,
                note.note_text,
                note.skill_name,
                note.model,
                note.with_speakers,
                note.cost_usd
            ],
        )?;
        let note_id = conn.last_insert_rowid();
        let starred = match note.history_id {
            Some(id) => Self::mark_entry_saved_with_conn(conn, id)?,
            None => false,
        };
        let saved = conn.query_row(
            &format!("SELECT {NOTE_COLUMNS} FROM notes n WHERE n.id = ?1"),
            params![note_id],
            Self::map_note,
        )?;
        Ok((saved, starred))
    }

    /// All notes, newest first.
    pub fn get_notes(&self) -> Result<Vec<Note>> {
        let conn = self.get_connection()?;
        Self::get_notes_with_conn(&conn)
    }

    fn get_notes_with_conn(conn: &Connection) -> Result<Vec<Note>> {
        let mut stmt = conn.prepare(&format!(
            "SELECT {NOTE_COLUMNS} FROM notes n ORDER BY n.id DESC"
        ))?;
        let notes = stmt
            .query_map([], Self::map_note)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(notes)
    }

    /// Notes made from the given history entries, newest first, so History
    /// can show each entry's notes under it.
    pub fn get_notes_for_history_ids(&self, history_ids: &[i64]) -> Result<Vec<Note>> {
        let conn = self.get_connection()?;
        Self::get_notes_for_history_ids_with_conn(&conn, history_ids)
    }

    fn get_notes_for_history_ids_with_conn(
        conn: &Connection,
        history_ids: &[i64],
    ) -> Result<Vec<Note>> {
        // Stay well below SQLite's limit on bound parameters.
        const CHUNK_SIZE: usize = 500;

        let mut notes = Vec::new();
        for chunk in history_ids.chunks(CHUNK_SIZE) {
            let placeholders = vec!["?"; chunk.len()].join(", ");
            let mut stmt = conn.prepare(&format!(
                "SELECT {NOTE_COLUMNS} FROM notes n WHERE n.history_id IN ({placeholders})"
            ))?;
            let rows = stmt.query_map(rusqlite::params_from_iter(chunk), Self::map_note)?;
            for note in rows {
                notes.push(note?);
            }
        }
        notes.sort_by_key(|note| std::cmp::Reverse(note.id));
        Ok(notes)
    }

    pub fn delete_note(&self, id: i64) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute("DELETE FROM notes WHERE id = ?1", params![id])?;
        debug!("Deleted note with id: {}", id);
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
                audio_purged_at INTEGER
            );",
        )
        .expect("create transcription_history table");
        conn
    }

    /// A database built by the real migrations, including the notes table.
    fn setup_conn_with_notes() -> Connection {
        let mut conn = Connection::open_in_memory().expect("open in-memory db");
        let migrations = Migrations::new(MIGRATIONS.to_vec());
        migrations.validate().expect("valid migrations");
        migrations.to_latest(&mut conn).expect("apply migrations");
        conn
    }

    fn is_saved(conn: &Connection, id: i64) -> bool {
        conn.query_row(
            "SELECT saved FROM transcription_history WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .expect("read saved flag")
    }

    fn new_note<'a>(
        history_id: Option<i64>,
        source_text: &'a str,
        note_text: &'a str,
    ) -> NewNote<'a> {
        NewNote {
            history_id,
            source_text,
            note_text,
            skill_name: None,
            model: "m",
            with_speakers: false,
            cost_usd: None,
        }
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

    #[test]
    fn save_note_stores_fields_and_stars_the_source() {
        let conn = setup_conn_with_notes();
        insert_entry(&conn, 100, "source transcript", None);
        assert!(!is_saved(&conn, 1));

        let (note, starred) = HistoryManager::save_note_with_conn(
            &conn,
            &NewNote {
                skill_name: Some("Meeting notes"),
                model: "google/gemini-2.5-flash",
                cost_usd: Some(0.0012),
                ..new_note(Some(1), "source transcript", "# Note")
            },
        )
        .expect("save note");

        assert!(starred);
        assert!(is_saved(&conn, 1));
        assert_eq!(note.history_id, Some(1));
        assert_eq!(note.source_text, "source transcript");
        assert_eq!(note.note_text, "# Note");
        assert_eq!(note.skill_name.as_deref(), Some("Meeting notes"));
        assert_eq!(note.model, "google/gemini-2.5-flash");
        assert_eq!(note.cost_usd, Some(0.0012));
        assert!(!note.with_speakers);
        assert!(note.source_exists);

        // An already-starred source is not reported as newly starred.
        let (_, starred_again) =
            HistoryManager::save_note_with_conn(&conn, &new_note(Some(1), "x", "# Again"))
                .expect("save second note");
        assert!(!starred_again);
    }

    #[test]
    fn save_note_keeps_the_speakers_flag() {
        let conn = setup_conn_with_notes();
        HistoryManager::save_note_with_conn(&conn, &new_note(None, "a", "plain"))
            .expect("save plain note");
        let (speakers, _) = HistoryManager::save_note_with_conn(
            &conn,
            &NewNote {
                with_speakers: true,
                ..new_note(None, "[Person 1]: Hi.", "speakers")
            },
        )
        .expect("save note with speakers");
        assert!(speakers.with_speakers);
        let flags: Vec<bool> = HistoryManager::get_notes_with_conn(&conn)
            .expect("list notes")
            .iter()
            .map(|n| n.with_speakers)
            .collect();
        assert_eq!(flags, vec![true, false]);
    }

    #[test]
    fn mark_entry_saved_is_set_only() {
        let conn = setup_conn_with_notes();
        insert_entry(&conn, 100, "source", None);

        assert!(HistoryManager::mark_entry_saved_with_conn(&conn, 1).expect("mark"));
        assert!(is_saved(&conn, 1));
        assert!(!HistoryManager::mark_entry_saved_with_conn(&conn, 1).expect("mark again"));
        assert!(is_saved(&conn, 1));
        assert!(!HistoryManager::mark_entry_saved_with_conn(&conn, 99).expect("mark missing"));
    }

    #[test]
    fn manual_note_has_no_source() {
        let conn = setup_conn_with_notes();
        let (note, starred) =
            HistoryManager::save_note_with_conn(&conn, &new_note(None, "pasted", "note"))
                .expect("save note");
        assert!(!starred);
        assert_eq!(note.history_id, None);
        assert!(!note.source_exists);
    }

    #[test]
    fn notes_list_newest_first_and_track_a_deleted_source() {
        let conn = setup_conn_with_notes();
        insert_entry(&conn, 100, "first", None);
        insert_entry(&conn, 200, "second", None);
        HistoryManager::save_note_with_conn(&conn, &new_note(Some(1), "first", "note 1"))
            .expect("save note 1");
        HistoryManager::save_note_with_conn(&conn, &new_note(Some(2), "second", "note 2"))
            .expect("save note 2");

        conn.execute("DELETE FROM transcription_history WHERE id = 1", [])
            .expect("delete source entry");

        let notes = HistoryManager::get_notes_with_conn(&conn).expect("list notes");
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].note_text, "note 2");
        assert!(notes[0].source_exists);
        assert_eq!(notes[1].note_text, "note 1");
        assert!(!notes[1].source_exists);
    }

    #[test]
    fn notes_for_history_ids_are_filtered_and_newest_first() {
        let conn = setup_conn_with_notes();
        for ts in [100, 200, 300] {
            insert_entry(&conn, ts, "t", None);
        }
        for (id, text) in [(1, "a"), (2, "b"), (1, "c"), (3, "d")] {
            HistoryManager::save_note_with_conn(&conn, &new_note(Some(id), "t", text))
                .expect("save note");
        }
        HistoryManager::save_note_with_conn(&conn, &new_note(None, "t", "manual"))
            .expect("save manual note");

        let notes =
            HistoryManager::get_notes_for_history_ids_with_conn(&conn, &[1, 3]).expect("query");
        let texts: Vec<&str> = notes.iter().map(|n| n.note_text.as_str()).collect();
        assert_eq!(texts, vec!["d", "c", "a"]);
        assert!(
            HistoryManager::get_notes_for_history_ids_with_conn(&conn, &[])
                .expect("empty query")
                .is_empty()
        );
    }

    #[test]
    fn notes_migration_applies_on_top_of_an_existing_database() {
        // A database at the previous version (before the notes table) keeps its
        // rows and gains the notes table.
        let mut conn = Connection::open_in_memory().expect("open in-memory db");
        let before = Migrations::new(MIGRATIONS[..MIGRATIONS.len() - 1].to_vec());
        before.to_latest(&mut conn).expect("apply older migrations");
        insert_entry(&conn, 100, "kept", None);

        Migrations::new(MIGRATIONS.to_vec())
            .to_latest(&mut conn)
            .expect("apply notes migration");
        let (note, _) =
            HistoryManager::save_note_with_conn(&conn, &new_note(Some(1), "kept", "note"))
                .expect("save note");
        assert!(note.source_exists);
    }
}
