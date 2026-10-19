//! Rotated log archiving. The log plugin cuts `handy.log` at 10 MB into
//! `handy_<date>.log`; this compresses each such file into
//! `handy_<date>.log.gz` (about a tenth of the size), removes the plain file
//! and keeps the newest `KEEP` archives, none older than `MAX_AGE`. Runs at
//! startup and every few minutes. Only Handy's own rotated logs are touched:
//! never the live `handy.log`, never anything else in the folder.

use log::{info, warn};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Archives kept at most.
pub const KEEP: usize = 20;
/// Archives older than this are removed.
pub const MAX_AGE: Duration = Duration::from_secs(30 * 24 * 3600);
const EVERY: Duration = Duration::from_secs(5 * 60);

/// A rotated log: `handy_<something>.log` (not the live `handy.log`).
fn is_rotated_log(name: &str) -> bool {
    name.starts_with("handy_") && name.ends_with(".log") && name.len() > "handy_.log".len()
}

fn is_archive(name: &str) -> bool {
    name.starts_with("handy_") && name.ends_with(".log.gz")
}

/// gzip `path` into `<path>.gz` (via a temp name), then remove `path`.
fn compress(path: &Path) -> std::io::Result<PathBuf> {
    let gz = PathBuf::from(format!("{}.gz", path.display()));
    let tmp = PathBuf::from(format!("{}.gz.tmp", path.display()));
    {
        let mut input = fs::File::open(path)?;
        let out = fs::File::create(&tmp)?;
        let mut encoder = flate2::write::GzEncoder::new(out, flate2::Compression::default());
        let mut buf = vec![0u8; 1 << 16];
        loop {
            let n = input.read(&mut buf)?;
            if n == 0 {
                break;
            }
            encoder.write_all(&buf[..n])?;
        }
        encoder.finish()?.sync_all()?;
    }
    fs::rename(&tmp, &gz)?;
    fs::remove_file(path)?;
    Ok(gz)
}

/// One pass over `dir`: compress rotated logs, then prune old archives.
/// Returns (compressed, removed).
pub fn archive_once(dir: &Path, now: SystemTime) -> (usize, usize) {
    let Ok(entries) = fs::read_dir(dir) else {
        return (0, 0);
    };
    let mut compressed = 0;
    let mut archives: Vec<(SystemTime, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        if !path.is_file() {
            continue;
        }
        if is_rotated_log(&name) {
            match compress(&path) {
                Ok(gz) => {
                    compressed += 1;
                    let modified = fs::metadata(&gz).and_then(|m| m.modified()).unwrap_or(now);
                    archives.push((modified, gz));
                }
                Err(e) => warn!("Could not compress the log {}: {}", name, e),
            }
        } else if is_archive(&name) {
            let modified = entry.metadata().and_then(|m| m.modified()).unwrap_or(now);
            archives.push((modified, path));
        } else if name.starts_with("handy_") && name.ends_with(".log.gz.tmp") {
            // A compression cut short (the app closed): its plain log is still
            // there and is compressed again.
            let _ = fs::remove_file(&path);
        }
    }
    // Newest first; drop past KEEP or older than MAX_AGE.
    archives.sort_by(|a, b| b.0.cmp(&a.0));
    let mut removed = 0;
    for (i, (modified, path)) in archives.iter().enumerate() {
        let too_old = now.duration_since(*modified).unwrap_or_default() > MAX_AGE;
        if i >= KEEP || too_old {
            if fs::remove_file(path).is_ok() {
                removed += 1;
            }
        }
    }
    (compressed, removed)
}

/// Compress and prune now, then every few minutes, in the background.
pub fn start(dir: PathBuf) {
    let _ = std::thread::Builder::new()
        .name("log-archive".into())
        .spawn(move || loop {
            let (compressed, removed) = archive_once(&dir, SystemTime::now());
            if compressed > 0 || removed > 0 {
                info!("Logs: {compressed} rotated log(s) compressed, {removed} old archive(s) removed");
            }
            std::thread::sleep(EVERY);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotated_logs_are_compressed_and_the_live_log_and_other_files_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        fs::write(d.join("handy.log"), "live").unwrap();
        fs::write(
            d.join("handy_2026-10-01_10-00-00.log"),
            "old log ".repeat(1000),
        )
        .unwrap();
        fs::write(d.join("installer.log"), "keep").unwrap();
        fs::write(d.join("notes.txt"), "keep").unwrap();
        let (compressed, removed) = archive_once(d, SystemTime::now());
        assert_eq!((compressed, removed), (1, 0));
        assert!(d.join("handy.log").exists());
        assert!(d.join("installer.log").exists() && d.join("notes.txt").exists());
        assert!(!d.join("handy_2026-10-01_10-00-00.log").exists());
        let gz = d.join("handy_2026-10-01_10-00-00.log.gz");
        let mut text = String::new();
        flate2::read::GzDecoder::new(fs::File::open(&gz).unwrap())
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "old log ".repeat(1000));
    }

    #[test]
    fn only_the_newest_archives_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        for i in 0..(KEEP + 3) {
            fs::write(d.join(format!("handy_{i:03}.log.gz")), "x").unwrap();
        }
        let (_, removed) = archive_once(d, SystemTime::now());
        assert_eq!(removed, 3);
        // Far in the future: everything is too old.
        let (_, removed) = archive_once(d, SystemTime::now() + MAX_AGE * 2);
        assert_eq!(removed, KEEP);
    }

    #[test]
    fn names_are_recognised_strictly() {
        assert!(is_rotated_log("handy_2026-10-14_12-36-12.log"));
        assert!(!is_rotated_log("handy.log"));
        assert!(!is_rotated_log("handy_.log"));
        assert!(!is_rotated_log("installer.log"));
        assert!(is_archive("handy_x.log.gz") && !is_archive("other.log.gz"));
    }
}
