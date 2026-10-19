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

/// Suffix of a rotated log claimed for compression: renamed away first, so
/// a log the plugin rotates to the same name meanwhile is a different file
/// and is never removed by mistake.
const CLAIMED: &str = ".archiving";

/// gzip the claimed file `claimed` (`handy_x.log.archiving`) into
/// `handy_x.log.gz` (via a temp name; an existing archive of that name gets a
/// numbered one), give the archive the log's own modification time, then
/// remove `claimed`.
fn compress(claimed: &Path, modified: SystemTime) -> std::io::Result<PathBuf> {
    let log_name = claimed
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_suffix(CLAIMED))
        .unwrap_or("handy_log.log")
        .to_string();
    let dir = claimed.parent().unwrap_or(Path::new("."));
    let mut gz = dir.join(format!("{log_name}.gz"));
    let mut n = 1;
    while gz.exists() {
        let stem = log_name.strip_suffix(".log").unwrap_or(&log_name);
        gz = dir.join(format!("{stem}-{n}.log.gz"));
        n += 1;
    }
    let tmp = PathBuf::from(format!("{}.tmp", gz.display()));
    {
        let mut input = fs::File::open(claimed)?;
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
        let out = encoder.finish()?;
        out.sync_all()?;
        // The archive keeps the log's age (retention and order go by it).
        let _ = out.set_modified(modified);
    }
    fs::rename(&tmp, &gz)?;
    fs::remove_file(claimed)?;
    Ok(gz)
}

/// One pass over `dir`: compress rotated logs, then prune old archives.
/// Returns (compressed, removed).
pub fn archive_once(dir: &Path, now: SystemTime) -> (usize, usize) {
    let Ok(entries) = fs::read_dir(dir) else {
        return (0, 0);
    };
    let mut to_compress: Vec<PathBuf> = Vec::new();
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
            // Claim it under a name of its own before touching it.
            let claimed = dir.join(format!("{name}{CLAIMED}"));
            if !claimed.exists() && fs::rename(&path, &claimed).is_ok() {
                to_compress.push(claimed);
            }
        } else if name.starts_with("handy_") && name.ends_with(&format!(".log{CLAIMED}")) {
            // Claimed by a pass the app's exit cut short: finish it.
            to_compress.push(path);
        } else if name.starts_with("handy_") && name.ends_with(".log.gz.tmp") {
            let _ = fs::remove_file(&path);
        }
    }
    let mut compressed = 0;
    for claimed in to_compress {
        let modified = fs::metadata(&claimed)
            .and_then(|m| m.modified())
            .unwrap_or(now);
        match compress(&claimed, modified) {
            Ok(_) => compressed += 1,
            Err(e) => warn!("Could not compress the log {}: {}", claimed.display(), e),
        }
    }
    // Archives, newest log first; drop past KEEP or older than MAX_AGE.
    let mut archives: Vec<(SystemTime, PathBuf)> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.file_name().to_str().is_some_and(is_archive))
                .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
                .collect()
        })
        .unwrap_or_default();
    archives.sort_by(|a, b| b.0.cmp(&a.0));
    let mut removed = 0;
    for (i, (modified, path)) in archives.iter().enumerate() {
        let too_old = now.duration_since(*modified).unwrap_or_default() > MAX_AGE;
        if (i >= KEEP || too_old) && fs::remove_file(path).is_ok() {
            removed += 1;
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
    fn an_archive_keeps_the_age_of_its_log_and_a_name_clash_is_numbered() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let log = d.join("handy_2026-01-01_00-00-00.log");
        fs::write(&log, "january").unwrap();
        let january = SystemTime::now() - Duration::from_secs(60 * 24 * 3600);
        fs::File::options()
            .write(true)
            .open(&log)
            .unwrap()
            .set_modified(january)
            .unwrap();
        fs::write(d.join("handy_2026-01-01_00-00-00.log.gz"), "older archive").unwrap();
        let (compressed, _) = archive_once(d, SystemTime::now());
        assert_eq!(compressed, 1);
        let gz = d.join("handy_2026-01-01_00-00-00-1.log.gz");
        // Its age is the log's, so it is past MAX_AGE and pruned straight away.
        assert!(!gz.exists());
        assert!(!log.exists());
        assert!(!d.join("handy_2026-01-01_00-00-00.log.archiving").exists());
    }

    #[test]
    fn a_claimed_log_left_by_an_interrupted_pass_is_finished() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        fs::write(d.join("handy_x.log.archiving"), "half done").unwrap();
        let (compressed, _) = archive_once(d, SystemTime::now());
        assert_eq!(compressed, 1);
        assert!(d.join("handy_x.log.gz").exists());
        assert!(!d.join("handy_x.log.archiving").exists());
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
