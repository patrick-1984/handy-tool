//! Translator — folder-watch batch transcription (the FLMTray replacement).
//!
//! Watches user-chosen folders (the default entry is Handy's own
//! `{app_data}/recordings` queue folder) and transcribes NEW audio files into
//! a `.txt` next to the source (`take.opus` → `take.txt`), using whatever
//! engine/model/language/translate settings the app currently has.
//!
//! Scanning rules (FLMTray parity):
//! - Only files that APPEAR after watching starts are queued; the pre-existing
//!   backlog is snapshotted into a baseline and left alone.
//! - A file must be stable (same size across two scans, mtime a few seconds
//!   old) before it is queued — never read a file mid-write.
//! - A `.txt` sidecar marks a file as done forever (survives restarts).
//! - Recorder-internal files are never touched: `*-chunk-N.*`, `*-temp.*`,
//!   `*.partial` (the glued main file is the one that gets transcribed).
//! - The pending queue is persisted to `{app_data}/translator_queue.json` so
//!   queued-but-unfinished work survives a restart/crash.
//!
//! Engine sharing: the local engine is single-tenant (`transcribe()` fails
//! fast when busy), so every batch engine call serializes through
//! `actions::CHUNK_TRANSCRIBE_LOCK` — a live chunk worker waits at most ONE
//! batch segment. Files are decoded and split into speech segments of at most
//! ~40 s (energy VAD), which is also the pause granularity: "pausing" the
//! batch simply means not starting the next segment, so batch progress is
//! never dropped. The `TranslatorPriority` policies only decide WHEN the
//! worker yields:
//! - `LiveFirst`: yield while the pipeline is recording or processing.
//! - `FolderFirst`: yield only during stop-processing (the final pass must
//!   never starve); live segment texts queue behind batch segments.
//! - `Fifo`: the file in progress keeps going (yielding only during
//!   processing), but the next file waits until the pipeline is idle.

use crate::audio_toolkit::audio::decode_audio_file;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::{TranslatorFolder, TranslatorPriority, get_settings, write_settings};
use crate::transcription_coordinator::{STAGE_IDLE, STAGE_PROCESSING, pipeline_stage};
use log::{debug, info, warn};
use serde::Serialize;
use specta::Type;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Worker tick. Scanning happens every `translator_poll_secs`; the tick only
/// bounds how fast the worker reacts to pause/resume and shutdown.
const TICK: Duration = Duration::from_millis(250);
/// A candidate file's mtime must be at least this old (and its size stable
/// across two consecutive scans) before it is queued.
const MIN_FILE_AGE: Duration = Duration::from_secs(3);
/// Decoded-length cap: refuse absurdly long inputs (2 h at 16 kHz mono f32 is
/// already a ~460 MB buffer).
const MAX_DECODED_SAMPLES: usize = 2 * 3600 * 16_000;

// Segmenter tuning (16 kHz samples).
const WIN_SAMPLES: usize = 800; // 50 ms energy window
const SEG_SOFT_SAMPLES: usize = 25 * 16_000; // start looking for a cut
const SEG_HARD_SAMPLES: usize = 40 * 16_000; // force a cut at the quietest point
/// Speech runs separated by less than this much silence are merged.
const MERGE_GAP_WINDOWS: usize = 8; // 400 ms
/// Context kept around each speech run.
const PAD_WINDOWS: usize = 2; // 100 ms

#[derive(Clone, Serialize, Type, Default)]
pub struct TranslatorStatus {
    pub enabled: bool,
    /// Basenames of queued files (capped for the UI).
    pub queue: Vec<String>,
    pub queue_len: u32,
    pub current_file: Option<String>,
    pub current_segment: u32,
    pub current_total_segments: u32,
    /// Why the worker is holding off: "live" | "processing".
    pub paused_reason: Option<String>,
    pub done_count: u32,
    pub failed_count: u32,
}

struct Job {
    path: PathBuf,
    samples: Vec<f32>,
    segments: Vec<std::ops::Range<usize>>,
    next_segment: usize,
    parts: Vec<String>,
    segment_errors: usize,
}

pub struct TranslatorManager {
    shutdown: Arc<AtomicBool>,
    status: Arc<Mutex<TranslatorStatus>>,
}

impl TranslatorManager {
    pub fn new(app: &AppHandle) -> Arc<Self> {
        let shutdown = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(TranslatorStatus::default()));
        let manager = Arc::new(Self {
            shutdown: shutdown.clone(),
            status: status.clone(),
        });
        let app = app.clone();
        std::thread::Builder::new()
            .name("translator-worker".into())
            .spawn(move || worker(app, shutdown, status))
            .expect("spawn translator worker");
        manager
    }

    pub fn status(&self) -> TranslatorStatus {
        self.status
            .lock()
            .map(|s| s.clone())
            .unwrap_or_else(|p| p.into_inner().clone())
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }
}

fn queue_store_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("translator_queue.json"))
}

fn load_persisted_queue(app: &AppHandle) -> VecDeque<PathBuf> {
    let Some(path) = queue_store_path(app) else {
        return VecDeque::new();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return VecDeque::new();
    };
    let Ok(paths) = serde_json::from_slice::<Vec<String>>(&bytes) else {
        warn!("translator_queue.json is unreadable; starting with an empty queue");
        return VecDeque::new();
    };
    paths
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| p.exists() && !sidecar_path(p).exists())
        .collect()
}

fn persist_queue(app: &AppHandle, queue: &VecDeque<PathBuf>, current: Option<&Path>) {
    let Some(path) = queue_store_path(app) else {
        return;
    };
    // The in-flight file is persisted too — restart re-does it from scratch.
    let all: Vec<String> = current
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .chain(queue.iter().map(|p| p.to_string_lossy().to_string()))
        .collect();
    match serde_json::to_vec(&all) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(&path, bytes) {
                debug!("could not persist translator queue: {e}");
            }
        }
        Err(e) => debug!("could not serialize translator queue: {e}"),
    }
}

/// `take.opus` → `take.txt` (extension replaced, FLMTray-compatible).
fn sidecar_path(file: &Path) -> PathBuf {
    file.with_extension("txt")
}

/// Recorder-internal and non-final artifacts that must never be transcribed.
fn is_internal_artifact(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".partial") {
        return true;
    }
    let stem = lower.rsplit_once('.').map(|(s, _)| s).unwrap_or(&lower);
    if stem.ends_with("-temp") {
        return true;
    }
    // "…-chunk-<n>" suffix = in-progress recorder chunk; the glued main file
    // is the one that gets transcribed (Handy's crash recovery re-glues
    // orphans at startup, so skipping chunks here loses nothing).
    if let Some((_, tail)) = stem.rsplit_once("-chunk-") {
        if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
    }
    false
}

fn is_candidate(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if name.starts_with('.') || is_internal_artifact(name) {
        return false;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    crate::audio_toolkit::audio::SUPPORTED_EXTENSIONS.contains(&ext.as_str())
}

/// Seed the default watch folder ({app_data}/recordings) exactly once.
fn seed_default_folder(app: &AppHandle) {
    let mut settings = get_settings(app);
    if settings.translator_seeded {
        return;
    }
    if let Ok(dir) = app.path().app_data_dir() {
        let rec = dir.join("recordings");
        settings.translator_folders.push(TranslatorFolder {
            path: rec.to_string_lossy().to_string(),
            enabled: true,
        });
    }
    settings.translator_seeded = true;
    write_settings(app, settings);
}

/// Energy-VAD segmentation: speech-only ranges, merged across short gaps and
/// hard-capped at ~40 s (cut at the quietest window past the ~25 s mark).
/// Long silence is skipped entirely — the decoder never sees it, which both
/// speeds batch work up and starves Whisper's silence-hallucination loops.
fn split_speech_segments(samples: &[f32]) -> Vec<std::ops::Range<usize>> {
    if samples.is_empty() {
        return Vec::new();
    }
    let n_windows = samples.len().div_ceil(WIN_SAMPLES);
    let mut rms = Vec::with_capacity(n_windows);
    let mut max_rms = 0f32;
    for w in 0..n_windows {
        let start = w * WIN_SAMPLES;
        let end = (start + WIN_SAMPLES).min(samples.len());
        let sum_sq: f32 = samples[start..end].iter().map(|s| s * s).sum();
        let r = (sum_sq / (end - start) as f32).sqrt();
        max_rms = max_rms.max(r);
        rms.push(r);
    }
    let threshold = (0.05 * max_rms).max(1.5e-3);

    // Speech runs in window space, padded and gap-merged.
    let mut runs: Vec<(usize, usize)> = Vec::new(); // [start_win, end_win)
    let mut cur: Option<(usize, usize)> = None;
    for (w, &r) in rms.iter().enumerate() {
        if r >= threshold {
            cur = Some(match cur {
                Some((s, _)) => (s, w + 1),
                None => (w, w + 1),
            });
        } else if let Some((s, e)) = cur {
            if w >= e + MERGE_GAP_WINDOWS {
                runs.push((s, e));
                cur = None;
            }
        }
    }
    if let Some(run) = cur {
        runs.push(run);
    }

    // Pad, convert to samples, and hard-split overlong runs at quiet windows.
    let mut segments = Vec::new();
    for (s, e) in runs {
        let s = s.saturating_sub(PAD_WINDOWS);
        let e = (e + PAD_WINDOWS).min(n_windows);
        let mut start = s * WIN_SAMPLES;
        let end = (e * WIN_SAMPLES).min(samples.len());
        while end - start > SEG_HARD_SAMPLES {
            // Quietest window between the soft and hard marks.
            let win_lo = (start + SEG_SOFT_SAMPLES) / WIN_SAMPLES;
            let win_hi = ((start + SEG_HARD_SAMPLES) / WIN_SAMPLES).min(n_windows - 1);
            let cut_win = (win_lo..=win_hi)
                .min_by(|a, b| rms[*a].total_cmp(&rms[*b]))
                .unwrap_or(win_hi);
            let cut = ((cut_win + 1) * WIN_SAMPLES).min(end);
            if cut <= start {
                break; // defensive: cannot make progress
            }
            segments.push(start..cut);
            start = cut;
        }
        if end > start {
            segments.push(start..end);
        }
    }
    segments
}

fn prepare_job(path: PathBuf) -> Result<Job, String> {
    let samples = decode_audio_file(&path).map_err(|e| format!("decode failed: {e}"))?;
    if samples.len() > MAX_DECODED_SAMPLES {
        return Err(format!(
            "file too long ({} min audio; cap is {} min)",
            samples.len() / 16_000 / 60,
            MAX_DECODED_SAMPLES / 16_000 / 60
        ));
    }
    let segments = split_speech_segments(&samples);
    debug!(
        "translator: {} decoded ({:.1}s audio, {} speech segments)",
        path.display(),
        samples.len() as f32 / 16_000.0,
        segments.len()
    );
    Ok(Job {
        path,
        samples,
        segments,
        next_segment: 0,
        parts: Vec::new(),
        segment_errors: 0,
    })
}

/// Atomic sidecar write: `.txt.tmp` then rename over.
fn write_sidecar(file: &Path, text: &str) -> std::io::Result<()> {
    let out = sidecar_path(file);
    let tmp = out.with_extension("txt.tmp");
    std::fs::write(&tmp, text.as_bytes())?;
    std::fs::rename(&tmp, &out)
}

struct Scanner {
    /// Per-folder snapshot of pre-existing files (never queued).
    baselines: HashMap<PathBuf, HashSet<PathBuf>>,
    /// Stability tracking for new candidates: size + when first seen at it.
    pending: HashMap<PathBuf, (u64, Instant)>,
    /// Folders already warned about (missing/unreadable) — warn once.
    warned: HashSet<PathBuf>,
}

impl Scanner {
    fn new() -> Self {
        Self {
            baselines: HashMap::new(),
            pending: HashMap::new(),
            warned: HashSet::new(),
        }
    }

    /// Scan enabled folders; returns newly stable files, oldest first.
    fn scan(
        &mut self,
        folders: &[TranslatorFolder],
        already_queued: &dyn Fn(&Path) -> bool,
    ) -> Vec<PathBuf> {
        let mut promoted: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
        for folder in folders.iter().filter(|f| f.enabled) {
            let dir = PathBuf::from(&folder.path);
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(e) => {
                    if self.warned.insert(dir.clone()) {
                        warn!("translator: cannot read folder {}: {e}", dir.display());
                    }
                    continue;
                }
            };
            self.warned.remove(&dir);
            let baseline_is_new = !self.baselines.contains_key(&dir);
            let baseline = self.baselines.entry(dir.clone()).or_default();

            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() || !is_candidate(&path) {
                    continue;
                }
                if baseline_is_new {
                    // First look at this folder: everything present is
                    // backlog, not queue input.
                    baseline.insert(path);
                    continue;
                }
                if baseline.contains(&path) || sidecar_path(&path).exists() || already_queued(&path)
                {
                    continue;
                }
                let Ok(meta) = entry.metadata() else { continue };
                let mtime = meta.modified().unwrap_or(std::time::SystemTime::now());
                let age_ok = mtime.elapsed().map(|a| a >= MIN_FILE_AGE).unwrap_or(false);
                let size = meta.len();
                match self.pending.get(&path) {
                    Some((seen_size, _first)) if *seen_size == size && age_ok && size > 0 => {
                        self.pending.remove(&path);
                        baseline.insert(path.clone());
                        promoted.push((mtime, path));
                    }
                    Some((seen_size, _)) if *seen_size != size => {
                        self.pending.insert(path, (size, Instant::now()));
                    }
                    Some(_) => {} // stable but too young — wait another scan
                    None => {
                        self.pending.insert(path, (size, Instant::now()));
                    }
                }
            }
        }
        promoted.sort_by_key(|(mtime, _)| *mtime);
        promoted.into_iter().map(|(_, p)| p).collect()
    }
}

fn worker(app: AppHandle, shutdown: Arc<AtomicBool>, status: Arc<Mutex<TranslatorStatus>>) {
    info!("Translator worker started");
    seed_default_folder(&app);

    let mut scanner = Scanner::new();
    let mut queue = load_persisted_queue(&app);
    if !queue.is_empty() {
        info!("Translator: restored {} queued file(s)", queue.len());
    }
    let mut current: Option<Job> = None;
    let mut done_count = 0u32;
    let mut failed_count = 0u32;
    // Force an immediate first scan.
    let mut last_scan: Option<Instant> = None;
    let mut was_enabled = false;

    let publish = |status: &Arc<Mutex<TranslatorStatus>>,
                   app: &AppHandle,
                   enabled: bool,
                   queue: &VecDeque<PathBuf>,
                   current: &Option<Job>,
                   paused: Option<&str>,
                   done: u32,
                   failed: u32| {
        let snapshot = TranslatorStatus {
            enabled,
            queue: queue
                .iter()
                .take(20)
                .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .collect(),
            queue_len: queue.len() as u32,
            current_file: current
                .as_ref()
                .and_then(|j| j.path.file_name())
                .map(|n| n.to_string_lossy().to_string()),
            current_segment: current.as_ref().map(|j| j.next_segment as u32).unwrap_or(0),
            current_total_segments: current
                .as_ref()
                .map(|j| j.segments.len() as u32)
                .unwrap_or(0),
            paused_reason: paused.map(|s| s.to_string()),
            done_count: done,
            failed_count: failed,
        };
        if let Ok(mut guard) = status.lock() {
            *guard = snapshot.clone();
        }
        let _ = app.emit("translator-status", snapshot);
    };

    while !shutdown.load(Ordering::SeqCst) {
        let settings = get_settings(&app);

        if !settings.translator_enabled {
            if was_enabled || current.is_some() || !queue.is_empty() {
                // Disabling drops the in-flight job's progress (segments are
                // cheap to redo) but keeps queued files queued for next time.
                if let Some(job) = current.take() {
                    queue.push_front(job.path);
                }
                persist_queue(&app, &queue, None);
                publish(
                    &status,
                    &app,
                    false,
                    &queue,
                    &current,
                    None,
                    done_count,
                    failed_count,
                );
                was_enabled = false;
            }
            std::thread::sleep(TICK);
            continue;
        }
        if !was_enabled {
            was_enabled = true;
            // Re-arm the first-scan-now behavior on every enable.
            last_scan = None;
            publish(
                &status,
                &app,
                true,
                &queue,
                &current,
                None,
                done_count,
                failed_count,
            );
        }

        // Periodic folder scan.
        let poll = Duration::from_secs(settings.translator_poll_secs.clamp(2, 3600));
        if last_scan.map(|t| t.elapsed() >= poll).unwrap_or(true) {
            last_scan = Some(Instant::now());
            let queued_now: HashSet<PathBuf> = queue.iter().cloned().collect();
            let current_path = current.as_ref().map(|j| j.path.clone());
            let fresh = scanner.scan(&settings.translator_folders, &|p: &Path| {
                queued_now.contains(p) || current_path.as_deref() == Some(p)
            });
            if !fresh.is_empty() {
                info!("Translator: queued {} new file(s)", fresh.len());
                queue.extend(fresh);
                persist_queue(&app, &queue, current.as_ref().map(|j| j.path.as_path()));
                publish(
                    &status,
                    &app,
                    true,
                    &queue,
                    &current,
                    None,
                    done_count,
                    failed_count,
                );
            }
        }

        // Priority gate.
        let stage = pipeline_stage();
        let yield_now = match settings.translator_priority {
            TranslatorPriority::LiveFirst => stage != STAGE_IDLE,
            TranslatorPriority::FolderFirst => stage == STAGE_PROCESSING,
            TranslatorPriority::Fifo => {
                if current.is_some() {
                    stage == STAGE_PROCESSING
                } else {
                    stage != STAGE_IDLE
                }
            }
        };
        if yield_now {
            let reason = if stage == STAGE_PROCESSING {
                "processing"
            } else {
                "live"
            };
            publish(
                &status,
                &app,
                true,
                &queue,
                &current,
                Some(reason),
                done_count,
                failed_count,
            );
            std::thread::sleep(TICK);
            continue;
        }

        // Pick up the next file.
        if current.is_none() {
            let Some(path) = queue.pop_front() else {
                publish(
                    &status,
                    &app,
                    true,
                    &queue,
                    &current,
                    None,
                    done_count,
                    failed_count,
                );
                std::thread::sleep(TICK);
                continue;
            };
            match prepare_job(path.clone()) {
                Ok(job) => {
                    info!(
                        "Translator: starting {} ({} segments)",
                        path.display(),
                        job.segments.len()
                    );
                    current = Some(job);
                }
                Err(e) => {
                    warn!("Translator: skipping {}: {e}", path.display());
                    failed_count += 1;
                }
            }
            persist_queue(&app, &queue, current.as_ref().map(|j| j.path.as_path()));
            publish(
                &status,
                &app,
                true,
                &queue,
                &current,
                None,
                done_count,
                failed_count,
            );
            continue; // re-check the gate before the first segment
        }

        // Transcribe exactly one segment, then loop (gate re-checked between
        // segments — that IS the pause mechanism).
        if let Some(job) = current.as_mut() {
            if job.next_segment < job.segments.len() {
                let range = job.segments[job.next_segment].clone();
                let segment = job.samples[range].to_vec();
                let result = {
                    let tm = app.state::<Arc<TranscriptionManager>>();
                    // Single-tenant engine: serialize with live chunk workers
                    // and the stop-path final pass.
                    let _serial = crate::actions::CHUNK_TRANSCRIBE_LOCK
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    tm.transcribe(segment)
                };
                match result {
                    Ok(text) => {
                        let t = text.trim();
                        if !t.is_empty() {
                            job.parts.push(t.to_string());
                        }
                    }
                    Err(e) => {
                        job.segment_errors += 1;
                        warn!(
                            "Translator: segment {}/{} of {} failed: {e}",
                            job.next_segment + 1,
                            job.segments.len(),
                            job.path.display()
                        );
                    }
                }
                job.next_segment += 1;
            }

            if job.next_segment >= job.segments.len() {
                let job = current.take().expect("job present");
                let text = job.parts.join(" ");
                let all_failed =
                    !job.segments.is_empty() && job.segment_errors == job.segments.len();
                if all_failed {
                    failed_count += 1;
                    warn!(
                        "Translator: {} failed (every segment errored); no sidecar written",
                        job.path.display()
                    );
                } else {
                    match write_sidecar(&job.path, &text) {
                        Ok(()) => {
                            done_count += 1;
                            info!(
                                "Translator: finished {} ({} chars)",
                                job.path.display(),
                                text.len()
                            );
                        }
                        Err(e) => {
                            failed_count += 1;
                            warn!(
                                "Translator: could not write sidecar for {}: {e}",
                                job.path.display()
                            );
                        }
                    }
                }
                persist_queue(&app, &queue, None);
            }
            publish(
                &status,
                &app,
                true,
                &queue,
                &current,
                None,
                done_count,
                failed_count,
            );
        }
    }
    info!("Translator worker stopped");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_artifacts_are_skipped() {
        assert!(is_internal_artifact("handy-123-chunk-1.opus"));
        assert!(is_internal_artifact("handy-123-chunk-12.opus"));
        assert!(is_internal_artifact("handy-123-chunk-1-temp.opus"));
        assert!(is_internal_artifact("something-temp.wav"));
        assert!(is_internal_artifact("x.opus.partial"));
        assert!(!is_internal_artifact("handy-123.opus"));
        assert!(!is_internal_artifact("meeting-chunk-notes.opus")); // non-digit tail
        assert!(!is_internal_artifact("take.wav"));
    }

    #[test]
    fn candidates_filter_by_extension() {
        assert!(is_candidate(Path::new("C:/x/a.wav")));
        assert!(is_candidate(Path::new("C:/x/a.OPUS")));
        assert!(is_candidate(Path::new("C:/x/a.ogg")));
        assert!(!is_candidate(Path::new("C:/x/a.mp3")));
        assert!(!is_candidate(Path::new("C:/x/a.txt")));
        assert!(!is_candidate(Path::new("C:/x/.hidden.wav")));
    }

    #[test]
    fn sidecar_swaps_extension() {
        assert_eq!(
            sidecar_path(Path::new("C:/rec/take.opus")),
            PathBuf::from("C:/rec/take.txt")
        );
    }

    #[test]
    fn segmenter_skips_silence_and_caps_length() {
        // 1 s speech, 5 s silence, 1 s speech.
        let mut samples = Vec::new();
        let tone: Vec<f32> = (0..16_000)
            .map(|i| (i as f32 * 300.0 * 2.0 * std::f32::consts::PI / 16_000.0).sin() * 0.4)
            .collect();
        samples.extend_from_slice(&tone);
        samples.extend(std::iter::repeat(0.0f32).take(5 * 16_000));
        samples.extend_from_slice(&tone);

        let segs = split_speech_segments(&samples);
        assert_eq!(segs.len(), 2, "expected two speech islands, got {segs:?}");
        // The 5 s silence gap is not inside any segment.
        for seg in &segs {
            assert!(seg.end - seg.start < 2 * 16_000);
        }

        // A continuous 100 s tone must be split under the hard cap.
        let long: Vec<f32> = (0..100 * 16_000)
            .map(|i| (i as f32 * 300.0 * 2.0 * std::f32::consts::PI / 16_000.0).sin() * 0.4)
            .collect();
        let segs = split_speech_segments(&long);
        assert!(
            segs.len() >= 3,
            "100s tone should split, got {}",
            segs.len()
        );
        for seg in &segs {
            assert!(
                seg.end - seg.start <= SEG_HARD_SAMPLES + WIN_SAMPLES,
                "segment exceeds hard cap: {}",
                seg.end - seg.start
            );
        }
        // Segments cover the run contiguously (no audio lost inside a run).
        for pair in segs.windows(2) {
            assert_eq!(pair[0].end, pair[1].start);
        }
    }

    #[test]
    fn segmenter_returns_nothing_for_silence() {
        let silence = vec![0.0f32; 10 * 16_000];
        assert!(split_speech_segments(&silence).is_empty());
    }
}
