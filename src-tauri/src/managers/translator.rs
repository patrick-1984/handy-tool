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
//! - Disabling a folder FREEZES it: `Scanner` forgets the folder's baseline
//!   (and any of its pending candidates) the moment it drops out of the
//!   enabled list. Re-enabling behaves exactly like a first-ever enable —
//!   the folder's current contents are re-snapshotted as backlog and left
//!   alone. This means files that arrived while a folder was disabled are
//!   NOT retroactively queued on re-enable (same rule as the initial
//!   backlog), which keeps "only files that appear after watching starts"
//!   a single consistent contract no matter how many times a folder is
//!   toggled (T-107).
//! - A file that disappears and reappears under the SAME name (deleted, then
//!   a new take recorded with that name) is identified by mtime, not just
//!   name: `Scanner` records the mtime it last saw a path at, and a newer
//!   mtime on a later scan means the file was recreated and is queued again
//!   as if it were brand new (T-107).
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
    crate::portable::resolve_app_data_dir(app)
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
            // Atomic: a crash mid-write must never leave malformed JSON (the
            // loader would silently restore an empty queue).
            let tmp = path.with_extension("json.tmp");
            let write_ok = std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, &path));
            if let Err(e) = write_ok {
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
    if let Ok(dir) = crate::portable::resolve_app_data_dir(app) {
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
    /// Per-folder snapshot of known files: the mtime recorded the last time
    /// each path was placed in the baseline (either as pre-existing backlog
    /// or after being promoted to the queue). A path missing from disk is
    /// pruned; a path whose current mtime is newer than the recorded one was
    /// deleted and recreated under the same name and is treated as a brand
    /// new candidate (T-107).
    baselines: HashMap<PathBuf, HashMap<PathBuf, std::time::SystemTime>>,
    /// Stability tracking for new candidates: size + mtime + when first seen
    /// at that (size, mtime) pair. T-107(a): mtime is part of the identity —
    /// a same-size REPLACEMENT of the file between two scans (same byte
    /// count, different content — e.g. two same-duration recordings sharing
    /// a name) bumps the mtime, which must reset the stability window rather
    /// than being silently treated as "the same candidate, now stable" just
    /// because the size happened to match.
    pending: HashMap<PathBuf, (u64, std::time::SystemTime, Instant)>,
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
    ///
    /// Disabled/removed folders are frozen: their baseline and any pending
    /// candidates are dropped up front, so a re-enable rebuilds the baseline
    /// from scratch (files present at that moment become backlog again,
    /// exactly like a first-ever enable — see the module-level doc). This
    /// also bounds `baselines`/`pending` growth: entries for files that
    /// vanish (or whose folder is no longer enabled) don't linger forever.
    fn scan(
        &mut self,
        folders: &[TranslatorFolder],
        already_queued: &dyn Fn(&Path) -> bool,
    ) -> Vec<PathBuf> {
        let enabled_dirs: HashSet<PathBuf> = folders
            .iter()
            .filter(|f| f.enabled)
            .map(|f| PathBuf::from(&f.path))
            .collect();

        // Freeze: forget everything belonging to a folder that isn't
        // currently enabled (disabled, or removed from the watch list).
        self.baselines.retain(|dir, _| enabled_dirs.contains(dir));
        self.pending.retain(|path, _| {
            path.parent()
                .map(|parent| enabled_dirs.contains(parent))
                .unwrap_or(false)
        });
        self.warned.retain(|dir| enabled_dirs.contains(dir));

        let mut promoted: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
        for dir in &enabled_dirs {
            let entries = match std::fs::read_dir(dir) {
                Ok(e) => e,
                Err(e) => {
                    if self.warned.insert(dir.clone()) {
                        warn!("translator: cannot read folder {}: {e}", dir.display());
                    }
                    // T-107(b): an unreadable/missing enabled folder must not
                    // retain its baseline/pending forever. The per-dir prune
                    // below (after the entries loop) never runs on this
                    // early `continue`, and the top-level freeze at the top
                    // of `scan()` only drops folders that are DISABLED or
                    // removed from the watch list — a folder that's still
                    // enabled but transiently unreadable (network drive
                    // hiccup, permission blip) would otherwise keep growing
                    // these maps forever. Treat it exactly like a disabled
                    // folder: forget its state now, so a later successful
                    // scan re-snapshots current contents as backlog again
                    // (first-ever-enable semantics), matching the existing
                    // disable/re-enable freeze behavior.
                    self.baselines.remove(dir);
                    self.pending.retain(|p, _| {
                        p.parent()
                            .map(|parent| parent != dir.as_path())
                            .unwrap_or(true)
                    });
                    continue;
                }
            };
            self.warned.remove(dir);
            let baseline_is_new = !self.baselines.contains_key(dir);
            let baseline = self.baselines.entry(dir.clone()).or_default();

            let mut present: HashSet<PathBuf> = HashSet::new();
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() || !is_candidate(&path) {
                    continue;
                }
                present.insert(path.clone());
                let Ok(meta) = entry.metadata() else { continue };
                let mtime = meta.modified().unwrap_or(std::time::SystemTime::now());

                if baseline_is_new {
                    // First look at this folder (or first look since it was
                    // last frozen): everything present is backlog, not queue
                    // input.
                    baseline.insert(path, mtime);
                    continue;
                }
                if let Some(known_mtime) = baseline.get(&path) {
                    if mtime <= *known_mtime {
                        continue; // unchanged: still backlog-ignored, or already handled
                    }
                    // else: mtime advanced since we last recorded this path —
                    // deleted-and-recreated (or modified) — fall through and
                    // treat it as a fresh candidate.
                }
                if sidecar_path(&path).exists() || already_queued(&path) {
                    continue;
                }
                let age_ok = mtime.elapsed().map(|a| a >= MIN_FILE_AGE).unwrap_or(false);
                let size = meta.len();
                // T-107(a): identity/stability requires size AND mtime to
                // match the previous observation — size alone let a
                // same-size REPLACEMENT between two scans (same byte count,
                // different content) slip through as "the same candidate,
                // now stable" and get promoted immediately, since only the
                // byte count was ever compared across scans.
                match self.pending.get(&path) {
                    Some((seen_size, seen_mtime, _first))
                        if *seen_size == size && *seen_mtime == mtime && age_ok && size > 0 =>
                    {
                        self.pending.remove(&path);
                        baseline.insert(path.clone(), mtime);
                        promoted.push((mtime, path));
                    }
                    Some((seen_size, seen_mtime, _))
                        if *seen_size != size || *seen_mtime != mtime =>
                    {
                        // Size changed, OR the size coincidentally matches
                        // but the mtime moved (a same-size replacement) —
                        // either way this is NOT confirmed-stable yet;
                        // restart the stability window against the CURRENT
                        // (size, mtime) pair.
                        self.pending.insert(path, (size, mtime, Instant::now()));
                    }
                    Some(_) => {} // stable but too young — wait another scan
                    None => {
                        self.pending.insert(path, (size, mtime, Instant::now()));
                    }
                }
            }
            // Prune: drop baseline/pending entries for files that vanished
            // from this folder so both maps stay bounded across days of
            // uptime, not just over each file's own lifecycle.
            baseline.retain(|p, _| present.contains(p));
            self.pending.retain(|p, _| {
                p.parent()
                    .map(|parent| parent != dir.as_path())
                    .unwrap_or(true)
                    || present.contains(p)
            });
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
    // Transient-failure retries per file (model hiccups, sidecar write
    // failures). Decode failures are permanent and never retried.
    let mut retry_counts: HashMap<PathBuf, u8> = HashMap::new();
    const MAX_RETRIES: u8 = 2;

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
                    app.state::<Arc<TranscriptionManager>>()
                        .set_batch_transcribing(false);
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

        // Batch model: the per-Translator override when set and still valid,
        // otherwise the dictation model. Only transcription-registry ids are
        // accepted (every entry is ASR-capable — LLM chat providers aren't).
        let model_manager = app.state::<Arc<crate::managers::model::ModelManager>>();
        let effective_model = {
            let m = settings.translator_model.trim();
            if !m.is_empty() && model_manager.get_model_info(m).is_some() {
                m.to_string()
            } else {
                settings.selected_model.clone()
            }
        };

        // Engine class decides two special cases below: external engines
        // (FLM/API/OpenRouter) don't need the single-tenant engine lock, and
        // OpenRouter batch work must never overlap a live take (its per-take
        // cost accumulator is process-global — overlap would bill batch
        // segments to the live recording's history row).
        let engine_info = model_manager.get_model_info(&effective_model);
        let engine_external = engine_info
            .as_ref()
            .map(|m| m.engine_type.is_external())
            .unwrap_or(false);
        let engine_openrouter = matches!(
            engine_info.as_ref().map(|m| &m.engine_type),
            Some(crate::managers::model::EngineType::OpenRouterWhisper)
        );

        // Priority gate.
        let stage = pipeline_stage();
        let mut yield_now = match settings.translator_priority {
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
        if engine_openrouter && stage != STAGE_IDLE {
            yield_now = true;
        }
        if yield_now {
            // Nothing to hand back: a LOCAL override model lives in its own
            // dedicated engine slot (never displacing the dictation model), and
            // an EXTERNAL override reloads through the normal load path when
            // dictation next needs it. Removing the old yield-time dictation
            // reload here (which borrowed the SHARED slot and thrashed the live
            // engine) is what fixes the recording-stop hang (T-300).
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
            // A transcript may have appeared while the file sat in the queue
            // (another tool, a restart, or a same-stem sibling finishing) —
            // never overwrite an existing sidecar.
            if sidecar_path(&path).exists() {
                debug!(
                    "Translator: {} already has a transcript; skipping",
                    path.display()
                );
                persist_queue(&app, &queue, None);
                continue;
            }
            match prepare_job(path.clone()) {
                Ok(job) => {
                    info!(
                        "Translator: starting {} ({} segments)",
                        path.display(),
                        job.segments.len()
                    );
                    app.state::<Arc<TranscriptionManager>>()
                        .set_batch_transcribing(true);
                    current = Some(job);
                }
                Err(e) => {
                    // Decode failures are permanent — no retry.
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
                let tm = app.state::<Arc<TranscriptionManager>>();
                // Route decision (T-300): a LOCAL override model (one that
                // differs from the dictation model) gets its own dedicated,
                // resident engine slot so both models stay loaded in PARALLEL
                // and never thrash the shared slot. The same-model case and
                // ALL external engines (FLM/API/OpenRouter) keep the shared
                // path — one NPU can't host two contexts and HTTP is
                // stateless.
                let use_parallel = !engine_external && effective_model != settings.selected_model;

                // The batch worker's expectation is ITS model (the override,
                // or the dictation model when unset) — `transcribe()` itself
                // would assert the SELECTED model and wrongly reject override
                // batches (pass-3 finding 8a revalidation).
                let result = if use_parallel {
                    // Load into the dedicated slot once (no-op if already
                    // resident), OUTSIDE the chunk lock so we never hold it
                    // while awaiting load_flight. Then transcribe UNDER the
                    // chunk lock: local inference serializes with the live
                    // pipeline (graceful iGPU turn-taking) while both models
                    // stay resident. FLM(NPU)+Whisper(iGPU) run truly in
                    // parallel because FLM is external and stays on the shared
                    // path (never taking this lock).
                    if tm.get_current_translator_model().as_deref()
                        != Some(effective_model.as_str())
                    {
                        if let Err(e) = tm.load_translator_model(&effective_model) {
                            warn!("Translator: parallel model load failed: {e}");
                        }
                    }
                    let _serial = crate::actions::CHUNK_TRANSCRIBE_LOCK
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    tm.transcribe_translator_expecting(&effective_model, segment)
                } else if engine_external {
                    // External engines (HTTP/subprocess) aren't single-tenant —
                    // and must not hold the engine lock across a network call.
                    // The Translator starts cold: transcribe() waits for an
                    // in-flight load but never initiates one, so load here.
                    if tm.get_current_model().as_deref() != Some(effective_model.as_str())
                        && !effective_model.is_empty()
                    {
                        if let Err(e) = tm.load_model(&effective_model) {
                            warn!("Translator: model load failed: {e}");
                        }
                    }
                    tm.transcribe_expecting(&effective_model, segment)
                } else {
                    // Same model as dictation: share the single dictation slot.
                    if tm.get_current_model().as_deref() != Some(effective_model.as_str())
                        && !effective_model.is_empty()
                    {
                        if let Err(e) = tm.load_model(&effective_model) {
                            warn!("Translator: model load failed: {e}");
                        }
                    }
                    // Single-tenant local engine: serialize with live chunk
                    // workers and the stop-path final pass.
                    let _serial = crate::actions::CHUNK_TRANSCRIBE_LOCK
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    tm.transcribe_expecting(&effective_model, segment)
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
                app.state::<Arc<TranscriptionManager>>()
                    .set_batch_transcribing(false);
                let text = job.parts.join(" ");
                // A sidecar means "complete transcript" — a file with ANY
                // errored segment is retried whole (bounded), never published
                // with silent holes.
                let mut transient_failure = job.segment_errors > 0;
                if !transient_failure {
                    match write_sidecar(&job.path, &text) {
                        Ok(()) => {
                            done_count += 1;
                            retry_counts.remove(&job.path);
                            info!(
                                "Translator: finished {} ({} chars)",
                                job.path.display(),
                                text.len()
                            );
                        }
                        Err(e) => {
                            warn!(
                                "Translator: could not write sidecar for {}: {e}",
                                job.path.display()
                            );
                            transient_failure = true;
                        }
                    }
                }
                if transient_failure {
                    let tries = retry_counts.entry(job.path.clone()).or_insert(0);
                    if *tries < MAX_RETRIES {
                        *tries += 1;
                        warn!(
                            "Translator: re-queueing {} (attempt {}/{})",
                            job.path.display(),
                            *tries,
                            MAX_RETRIES
                        );
                        queue.push_back(job.path);
                    } else {
                        failed_count += 1;
                        retry_counts.remove(&job.path);
                        warn!(
                            "Translator: giving up on {} after {} retries",
                            job.path.display(),
                            MAX_RETRIES
                        );
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
    // Never leave the unload-suppression flag armed past the worker's life.
    if let Some(tm) = app.try_state::<Arc<TranscriptionManager>>() {
        tm.set_batch_transcribing(false);
    }
    info!("Translator worker stopped");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Push a file's mtime into the past so it clears `MIN_FILE_AGE`
    /// immediately, without a real-time sleep in the test.
    fn backdate(path: &Path, secs_ago: u64) {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open for mtime backdate");
        let t = std::time::SystemTime::now() - Duration::from_secs(secs_ago);
        file.set_modified(t).expect("set_modified");
    }

    fn folder(dir: &TempDir, enabled: bool) -> Vec<TranslatorFolder> {
        vec![TranslatorFolder {
            path: dir.path().to_string_lossy().to_string(),
            enabled,
        }]
    }

    #[test]
    fn scanner_ignores_preexisting_backlog_then_queues_new_files() {
        let dir = TempDir::new().unwrap();
        let folders = folder(&dir, true);

        // Pre-existing file before watching starts: backlog, never queued.
        let old = dir.path().join("old.wav");
        std::fs::write(&old, b"backlog").unwrap();

        let mut scanner = Scanner::new();
        let promoted = scanner.scan(&folders, &|_| false);
        assert!(
            promoted.is_empty(),
            "pre-existing backlog must not be queued"
        );

        // A genuinely new file appears after watching started.
        let fresh = dir.path().join("fresh.wav");
        std::fs::write(&fresh, b"new").unwrap();
        backdate(&fresh, 10);

        let promoted = scanner.scan(&folders, &|_| false); // first sighting -> pending
        assert!(promoted.is_empty());
        let promoted = scanner.scan(&folders, &|_| false); // stable -> promoted
        assert_eq!(promoted, vec![fresh]);

        // Both still-present files remain baselined; nothing unbounded here.
        assert_eq!(
            scanner
                .baselines
                .get(&PathBuf::from(dir.path()))
                .map(|b| b.len()),
            Some(2)
        );
    }

    #[test]
    fn recreated_file_under_the_same_name_is_queued_again() {
        let dir = TempDir::new().unwrap();
        let folders = folder(&dir, true);
        let target = dir.path().join("take.wav");
        std::fs::write(&target, b"original").unwrap();
        backdate(&target, 200);

        let mut scanner = Scanner::new();
        let promoted = scanner.scan(&folders, &|_| false);
        assert!(promoted.is_empty(), "original backlog file is ignored");

        // Delete and re-record under the identical name — newer mtime.
        std::fs::remove_file(&target).unwrap();
        std::fs::write(&target, b"re-recorded").unwrap();
        backdate(&target, 10);

        let promoted = scanner.scan(&folders, &|_| false); // first sighting of the "new" file
        assert!(promoted.is_empty());
        let promoted = scanner.scan(&folders, &|_| false); // stable -> promoted
        assert_eq!(
            promoted,
            vec![target],
            "recreated file must be treated as new, not ignored forever"
        );
    }

    #[test]
    fn same_size_replacement_between_scans_resets_stability_tracking() {
        // T-107(a) regression: a same-size REPLACEMENT of the pending file
        // (identical byte count, different content — e.g. two same-duration
        // recordings sharing a name) must NOT be promoted on the very next
        // scan just because the byte count still matches; the mtime moved,
        // so stability tracking must restart against the new (size, mtime).
        let dir = TempDir::new().unwrap();
        let folders = folder(&dir, true);

        let mut scanner = Scanner::new();
        scanner.scan(&folders, &|_| false); // baseline established (empty folder)

        let target = dir.path().join("take.wav");
        std::fs::write(&target, b"AAAAAAAAAA").unwrap(); // 10 bytes
        backdate(&target, 10);
        scanner.scan(&folders, &|_| false); // first sighting -> pending
        assert_eq!(scanner.pending.len(), 1);

        // Same-size replacement: different bytes, identical length, newer mtime.
        std::fs::write(&target, b"BBBBBBBBBB").unwrap(); // still 10 bytes
        backdate(&target, 10);

        let promoted = scanner.scan(&folders, &|_| false);
        assert!(
            promoted.is_empty(),
            "a same-size replacement must not be promoted immediately just because the byte count matches"
        );

        // No further changes: now genuinely stable against the replacement's
        // own (size, mtime) -> promoted.
        let promoted = scanner.scan(&folders, &|_| false);
        assert_eq!(promoted, vec![target]);
    }

    #[test]
    fn missing_or_unreadable_folder_prunes_its_baseline_and_pending() {
        // T-107(b) regression: an enabled folder that becomes unreadable
        // (permission blip, disconnected network drive) or disappears
        // outright must not retain its baseline/pending entries forever —
        // the per-dir prune only runs after a successful `read_dir`, so the
        // error path needs its own pruning.
        let dir = TempDir::new().unwrap();
        let folders = folder(&dir, true);

        let backlog_file = dir.path().join("backlog.wav");
        std::fs::write(&backlog_file, b"x").unwrap();

        let mut scanner = Scanner::new();
        scanner.scan(&folders, &|_| false); // backlog_file baselined
        assert_eq!(
            scanner
                .baselines
                .get(&PathBuf::from(dir.path()))
                .map(|b| b.len()),
            Some(1)
        );

        let pending_file = dir.path().join("pending.wav");
        std::fs::write(&pending_file, b"partial").unwrap(); // fresh mtime: too young
        scanner.scan(&folders, &|_| false);
        assert_eq!(scanner.pending.len(), 1);

        // The folder itself disappears (simulates "unreadable" just as well
        // as a genuine permission error — `read_dir` fails either way).
        std::fs::remove_file(&backlog_file).unwrap();
        std::fs::remove_file(&pending_file).unwrap();
        std::fs::remove_dir(dir.path()).unwrap();

        scanner.scan(&folders, &|_| false);

        assert!(
            !scanner.baselines.contains_key(&PathBuf::from(dir.path())),
            "an unreadable/missing folder must not retain its baseline forever"
        );
        assert!(
            scanner.pending.is_empty(),
            "an unreadable/missing folder must not retain its pending entries forever"
        );
    }

    #[test]
    fn disabling_a_folder_freezes_it_and_reenable_rebuilds_the_baseline() {
        let dir = TempDir::new().unwrap();
        let mut folders = folder(&dir, true);

        let pre = dir.path().join("pre.wav");
        std::fs::write(&pre, b"backlog").unwrap();

        let mut scanner = Scanner::new();
        scanner.scan(&folders, &|_| false); // baseline established, pre.wav ignored

        // Disable: the folder's baseline is frozen/forgotten.
        folders[0].enabled = false;
        scanner.scan(&folders, &|_| false);
        assert!(
            !scanner.baselines.contains_key(&PathBuf::from(dir.path())),
            "disabling must drop the folder's baseline"
        );

        // A file appears while the folder is disabled.
        let while_disabled = dir.path().join("while_disabled.wav");
        std::fs::write(&while_disabled, b"during").unwrap();
        backdate(&while_disabled, 10);

        // Re-enable: per the documented rule, this rebuilds the baseline
        // fresh — the file that arrived while disabled becomes backlog too
        // and is NOT retroactively queued.
        folders[0].enabled = true;
        let promoted = scanner.scan(&folders, &|_| false);
        assert!(
            promoted.is_empty(),
            "re-enable must re-snapshot current contents as backlog"
        );

        // A genuinely new file after re-enable is queued normally.
        let fresh = dir.path().join("fresh.wav");
        std::fs::write(&fresh, b"z").unwrap();
        backdate(&fresh, 10);
        scanner.scan(&folders, &|_| false); // pending
        let promoted = scanner.scan(&folders, &|_| false); // stable -> promoted
        assert_eq!(promoted, vec![fresh]);
    }

    #[test]
    fn baseline_and_pending_entries_are_pruned_when_files_vanish() {
        let dir = TempDir::new().unwrap();
        let folders = folder(&dir, true);

        let backlog_file = dir.path().join("gone.wav");
        std::fs::write(&backlog_file, b"x").unwrap();

        let mut scanner = Scanner::new();
        scanner.scan(&folders, &|_| false); // baselined as backlog
        assert_eq!(
            scanner
                .baselines
                .get(&PathBuf::from(dir.path()))
                .map(|b| b.len()),
            Some(1)
        );

        // A second, not-yet-stable candidate sits in `pending`.
        let mid_write = dir.path().join("mid_write.wav");
        std::fs::write(&mid_write, b"partial").unwrap(); // fresh mtime: too young
        scanner.scan(&folders, &|_| false);
        assert_eq!(scanner.pending.len(), 1);

        std::fs::remove_file(&backlog_file).unwrap();
        std::fs::remove_file(&mid_write).unwrap();
        scanner.scan(&folders, &|_| false);

        assert_eq!(
            scanner
                .baselines
                .get(&PathBuf::from(dir.path()))
                .map(|b| b.len()),
            Some(0),
            "removed file must be pruned from the baseline"
        );
        assert!(
            scanner.pending.is_empty(),
            "pending entry for a deleted file must be pruned"
        );
    }

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
