//! App-data backup and restore as a `.tar.gz`. Two backup profiles:
//! - `config`: settings + history DB only (metadata; small).
//! - `full`:   settings + history DB + the small compressed recordings
//!             (`.opus`/`.ogg`), EXCLUDING downloaded models and large
//!             uncompressed audio (`.wav`/`.flac`) and in-progress temp chunks.
//!
//! Downloaded models are always excluded (re-downloadable) and large audio is
//! excluded for size.
//!
//! Restore reads the same archives back, with independent switches for
//! config+metadata vs recordings, extracting ONLY whitelisted entry names
//! (no path traversal). Restoring settings/history requires an app restart —
//! the running app holds settings in memory and would clobber the restored
//! file on its next write.

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use std::fs::File;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

fn append_if_exists<W: std::io::Write>(
    tar: &mut tar::Builder<W>,
    disk: &Path,
    name: &str,
) -> Result<(), String> {
    if disk.exists() {
        tar.append_path_with_name(disk, name)
            .map_err(|e| format!("archive {}: {}", name, e))?;
    }
    Ok(())
}

/// Create a `.tar.gz` backup at `dest_path`. `profile` is "config" or "full".
#[tauri::command]
#[specta::specta]
pub fn create_backup(app: AppHandle, profile: String, dest_path: String) -> Result<String, String> {
    let data_dir = crate::portable::resolve_app_data_dir(&app)?;
    // In portable mode data_dir and config_dir are the same folder (there is
    // only one "beside the exe" location), so short-circuit to it without a
    // second app_config_dir() lookup. Only fall back to the real
    // app_config_dir() — which can differ from app_data_dir() on
    // non-Windows — when portable mode isn't active, preserving the exact
    // pre-existing cross-platform settings-file search behavior below.
    let config_dir = match crate::portable::portable_data_dir() {
        Some(dir) => Some(dir),
        None => app.path().app_config_dir().ok(),
    };

    let file = File::create(&dest_path).map_err(|e| format!("create {}: {}", dest_path, e))?;
    let enc = GzEncoder::new(file, Compression::default());
    let mut tar = tar::Builder::new(enc);

    // Settings: tauri-plugin-store writes under the app config dir (on Windows
    // that equals the data dir). Include the first copy found.
    let mut settings_added = false;
    for base in [Some(data_dir.clone()), config_dir].into_iter().flatten() {
        let p = base.join(crate::settings::SETTINGS_STORE_PATH);
        if !settings_added && p.exists() {
            tar.append_path_with_name(&p, crate::settings::SETTINGS_STORE_PATH)
                .map_err(|e| format!("archive settings: {}", e))?;
            settings_added = true;
        }
    }

    // History DB (timestamps, transcriptions, cost, duration, model — the data).
    append_if_exists(&mut tar, &data_dir.join("history.db"), "history.db")?;

    // Full profile: include only small compressed recordings.
    if profile == "full" {
        let rec = data_dir.join("recordings");
        if let Ok(entries) = std::fs::read_dir(&rec) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                let lower = name.to_lowercase();
                let is_compressed = lower.ends_with(".opus") || lower.ends_with(".ogg");
                let is_temp = lower.ends_with("-temp.opus");
                if is_compressed && !is_temp {
                    if let Err(e) = tar.append_path_with_name(&path, format!("recordings/{}", name))
                    {
                        // Don't fail the whole backup for one locked/removed file,
                        // but don't silently drop it either.
                        log::warn!("Backup: skipped recording {}: {}", name, e);
                    }
                }
            }
        }
    }

    let enc = tar
        .into_inner()
        .map_err(|e| format!("finalize archive: {}", e))?;
    enc.finish().map_err(|e| format!("gzip finish: {}", e))?;
    Ok(dest_path)
}

/// What a restore actually applied, so the UI can report it and decide whether
/// a restart is needed (settings/history changes only take effect on restart).
/// Per-target failures land in `errors` instead of aborting the whole restore,
/// so a partial restore is always visible (and the restart prompt still shows
/// for whatever DID get replaced).
#[derive(serde::Serialize, specta::Type)]
pub struct RestoreReport {
    pub settings_restored: bool,
    pub history_restored: bool,
    pub recordings_restored: u32,
    pub restart_required: bool,
    pub errors: Vec<String>,
}

/// Decompression-bomb guard: tar headers state each entry's exact size, and
/// unpack writes exactly that many bytes, so capping on header size bounds all
/// disk writes from a hostile archive.
const MAX_ENTRY_BYTES: u64 = 4 * 1024 * 1024 * 1024; // 4 GiB per entry
const MAX_TOTAL_BYTES: u64 = 16 * 1024 * 1024 * 1024; // 16 GiB per restore

/// Where a whitelisted archive entry may be written. Anything else is skipped.
enum RestoreTarget {
    Settings,
    History,
    Recording(String),
}

/// Map an archive entry path to a whitelisted restore target. Rejects absolute
/// paths, `..`, separators inside recording names, and unknown names — the
/// archive is user-supplied, so nothing outside the known layout is written.
fn classify_entry(path: &Path) -> Option<RestoreTarget> {
    let mut components = path.components();
    let first = components.next()?;
    let first = match first {
        std::path::Component::Normal(s) => s.to_str()?,
        _ => return None, // absolute / .. / prefix components
    };
    match (first, components.next()) {
        (s, None) if s == crate::settings::SETTINGS_STORE_PATH => Some(RestoreTarget::Settings),
        ("history.db", None) => Some(RestoreTarget::History),
        ("recordings", Some(std::path::Component::Normal(name))) => {
            // Exactly recordings/<basename>, nothing deeper.
            if components.next().is_some() {
                return None;
            }
            let name = name.to_str()?;
            let lower = name.to_lowercase();
            let ok = (lower.ends_with(".opus") || lower.ends_with(".ogg"))
                && !name.contains(['/', '\\'])
                && !name.contains("..");
            if ok {
                Some(RestoreTarget::Recording(name.to_string()))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Extract one entry to `dest` by writing a temp sibling then swapping it in,
/// so a locked/half-written destination never ends up corrupted.
fn unpack_replace<R: std::io::Read>(entry: &mut tar::Entry<R>, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create dir: {}", e))?;
    }
    let tmp = dest.with_extension("restore-tmp");
    entry.unpack(&tmp).map_err(|e| {
        // Don't leave a partially-written temp file behind.
        let _ = std::fs::remove_file(&tmp);
        format!("extract {}: {}", dest.display(), e)
    })?;
    if dest.exists() {
        std::fs::remove_file(dest).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("replace {} (is it in use?): {}", dest.display(), e)
        })?;
    }
    std::fs::rename(&tmp, dest).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("move into place {}: {}", dest.display(), e)
    })?;
    Ok(())
}

/// Restore from a Handy backup `.tar.gz`. `include_config` restores settings +
/// the history DB (metadata); `include_recordings` restores the compressed
/// audio files. Only whitelisted entries are written; everything else in the
/// archive is ignored.
#[tauri::command]
#[specta::specta]
pub fn restore_backup(
    app: AppHandle,
    archive_path: String,
    include_config: bool,
    include_recordings: bool,
) -> Result<RestoreReport, String> {
    let data_dir = crate::portable::resolve_app_data_dir(&app)?;

    let file = File::open(&archive_path).map_err(|e| format!("open {}: {}", archive_path, e))?;
    let mut archive = tar::Archive::new(GzDecoder::new(file));

    let mut report = RestoreReport {
        settings_restored: false,
        history_restored: false,
        recordings_restored: 0,
        restart_required: false,
        errors: Vec::new(),
    };
    let mut total_bytes: u64 = 0;

    for entry in archive
        .entries()
        .map_err(|e| format!("read archive: {}", e))?
    {
        let mut entry = entry.map_err(|e| format!("read archive entry: {}", e))?;
        // Only plain files: a symlink/hardlink/dir entry named like a known file
        // would otherwise be swapped into place and later written through.
        if entry.header().entry_type() != tar::EntryType::Regular {
            continue;
        }
        let entry_path: PathBuf = entry
            .path()
            .map_err(|e| format!("entry path: {}", e))?
            .into_owned();
        let Some(target) = classify_entry(&entry_path) else {
            log::debug!("Restore: skipping archive entry {}", entry_path.display());
            continue;
        };

        // Bomb guard: bound every write by the tar-declared entry size.
        let size = entry.header().size().unwrap_or(u64::MAX);
        if size > MAX_ENTRY_BYTES || total_bytes.saturating_add(size) > MAX_TOTAL_BYTES {
            report.errors.push(format!(
                "skipped {}: exceeds restore size limits",
                entry_path.display()
            ));
            continue;
        }
        total_bytes = total_bytes.saturating_add(size);

        // Per-target failures are recorded, not fatal: the user must see what
        // WAS applied (and get the restart prompt for it) even when another
        // target failed — e.g. history.db locked by an in-flight query.
        match target {
            RestoreTarget::Settings if include_config => {
                match unpack_replace(
                    &mut entry,
                    &data_dir.join(crate::settings::SETTINGS_STORE_PATH),
                ) {
                    Ok(()) => report.settings_restored = true,
                    Err(e) => report.errors.push(format!("settings: {}", e)),
                }
            }
            RestoreTarget::History if include_config => {
                match unpack_replace(&mut entry, &data_dir.join("history.db")) {
                    Ok(()) => report.history_restored = true,
                    Err(e) => report.errors.push(format!("history: {}", e)),
                }
            }
            RestoreTarget::Recording(name) if include_recordings => {
                let dest = data_dir.join("recordings").join(&name);
                match unpack_replace(&mut entry, &dest) {
                    Ok(()) => report.recordings_restored += 1,
                    Err(e) => {
                        log::warn!("Restore: skipped recording {}: {}", name, e);
                        report.errors.push(format!("recording {}: {}", name, e));
                    }
                }
            }
            _ => {}
        }
    }

    report.restart_required = report.settings_restored || report.history_restored;
    log::info!(
        "Restore complete: settings={}, history={}, recordings={}",
        report.settings_restored,
        report.history_restored,
        report.recordings_restored
    );
    Ok(report)
}

/// Restart the app (used after a restore so the restored settings/history are
/// actually loaded instead of being clobbered by in-memory state).
#[tauri::command]
#[specta::specta]
pub fn restart_app(app: AppHandle) {
    app.restart();
}
