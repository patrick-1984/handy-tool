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
use tauri::{AppHandle, Emitter, Manager};

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
];

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
}

pub struct HistoryManager {
    app_handle: AppHandle,
    recordings_dir: PathBuf,
    db_path: PathBuf,
}

impl HistoryManager {
    pub fn new(app_handle: &AppHandle) -> Result<Self> {
        // Create recordings directory in app data dir
        let app_data_dir = app_handle.path().app_data_dir()?;
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

    fn delete_entries_and_files(&self, entries: &[(i64, String)]) -> Result<usize> {
        if entries.is_empty() {
            return Ok(0);
        }

        let conn = self.get_connection()?;
        let mut deleted_count = 0;

        for (id, file_name) in entries {
            // Delete database entry
            conn.execute(
                "DELETE FROM transcription_history WHERE id = ?1",
                params![id],
            )?;

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
            "SELECT id, file_name FROM transcription_history WHERE saved = 0 ORDER BY timestamp DESC"
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
            "SELECT id, file_name FROM transcription_history WHERE saved = 0 AND timestamp < ?1",
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
            "SELECT id, file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, cost_usd, duration_seconds, model_used FROM transcription_history ORDER BY timestamp DESC"
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
            "SELECT id, file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, cost_usd, duration_seconds, model_used
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
            "SELECT id, file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, cost_usd, duration_seconds, model_used
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
                model_used TEXT
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
