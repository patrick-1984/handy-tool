//! Speakers on this PC: NVIDIA Streaming Sortformer 4spk v2.1 (the model the
//! Custom STT server uses) as an optional download, run on the CPU. Who speaks
//! when comes first; then each speaker's turn is transcribed with the
//! dictation model, so it works with every engine (no word timestamps needed)
//! and gives the same `A: ...` paragraphs as the STT server.
//!
//! Weights: NVIDIA Open Model License (shown where it is downloaded).

pub mod mel;
pub mod sortformer;

use crate::managers::hf_parakeet::{HfFile, file_matches};
use anyhow::{Result, anyhow};
use futures_util::StreamExt;
use log::{info, warn};
use serde::Serialize;
use specta::Type;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// The full-precision (fp32) export (492 MB) of NVIDIA's model, pinned to a
/// commit and checked by SHA-256 (christopherthompson81/sortformer_parakeet_onnx).
/// Not the int8 export (2.5.0/2.5.1): its convolutions are `ConvInteger` with
/// int8 weights, which the ONNX Runtime bundled with ort 2.0.0-rc.10 cannot
/// run ("Could not find an implementation for ConvInteger(10)"); fp32 runs
/// everywhere and is ~3x faster on a CPU (RTF ~0.03).
const REPO: &str = "christopherthompson81/sortformer_parakeet_onnx";
const REVISION: &str = "434632d5eefa7ae83743cd02a8a26e64ce9fc1dd";
const FILE: &str = "diar_streaming_sortformer_4spk-v2.1.onnx";
const SHA256: &str = "d853932ae552342c48693b8d9dc02709f2d850c80a37cb5b422d14f599b7b8bc";
pub const SIZE: u64 = 492_268_840;
/// The int8 file 2.5.0/2.5.1 downloaded: unusable, deleted when seen.
const UNUSABLE_FILES: [&str; 2] = [
    "diar_streaming_sortformer_4spk-v2.1_int8.onnx",
    "diar_streaming_sortformer_4spk-v2.1_int8.onnx.part",
];
const DIR: &str = "sortformer-4spk-v2.1";
pub const LICENSE_URL: &str =
    "https://www.nvidia.com/en-us/agreements/enterprise-software/nvidia-open-model-license/";

static DOWNLOADING: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);
static DOWNLOADED_BYTES: AtomicU64 = AtomicU64::new(0);
/// One speakers job at a time (the model takes ~1 GB while it runs).
static JOB: Mutex<()> = Mutex::new(());

fn spec() -> HfFile {
    HfFile {
        path: FILE.to_string(),
        size: SIZE,
        sha256: Some(SHA256.to_string()),
        source: None,
    }
}

fn model_dir(app: &AppHandle) -> Result<PathBuf> {
    Ok(crate::portable::resolve_app_data_dir(app)
        .map_err(|e| anyhow!("{e}"))?
        .join("models")
        .join(DIR))
}

fn model_path(app: &AppHandle) -> Result<PathBuf> {
    Ok(model_dir(app)?.join(FILE))
}

/// Downloaded and complete (its checksum was verified when it was saved).
pub fn is_downloaded(app: &AppHandle) -> bool {
    model_path(app)
        .ok()
        .and_then(|p| std::fs::metadata(p).ok())
        .is_some_and(|m| m.len() == SIZE)
}

#[derive(Serialize, Clone, Debug, Type)]
pub struct LocalSpeakersStatus {
    pub downloaded: bool,
    pub downloading: bool,
    /// Bytes so far of a download in progress.
    pub downloaded_bytes: f64,
    pub size_bytes: f64,
    pub license_url: String,
}

/// Removes the unusable int8 model an earlier version downloaded (only those
/// two exact names, in the speakers model's own folder).
fn remove_unusable(app: &AppHandle) {
    let Ok(dir) = model_dir(app) else {
        return;
    };
    for name in UNUSABLE_FILES {
        let path = dir.join(name);
        if path.exists() {
            match std::fs::remove_file(&path) {
                Ok(()) => info!("Speakers model: removed the unusable {name}"),
                Err(e) => warn!("Speakers model: could not remove {name}: {e}"),
            }
        }
    }
}

pub fn status(app: &AppHandle) -> LocalSpeakersStatus {
    remove_unusable(app);
    LocalSpeakersStatus {
        downloaded: is_downloaded(app),
        downloading: DOWNLOADING.load(Ordering::SeqCst),
        downloaded_bytes: DOWNLOADED_BYTES.load(Ordering::SeqCst) as f64,
        size_bytes: SIZE as f64,
        license_url: LICENSE_URL.to_string(),
    }
}

fn emit_status(app: &AppHandle) {
    let _ = app.emit("local-speakers-status", status(app));
}

/// Downloads the model (resuming a partial file), checks its SHA-256 and puts
/// it in place. One download at a time.
pub async fn download(app: &AppHandle) -> Result<()> {
    if DOWNLOADING.swap(true, Ordering::SeqCst) {
        return Err(anyhow!("the speakers model is already downloading"));
    }
    struct Done<'a>(&'a AppHandle);
    impl Drop for Done<'_> {
        fn drop(&mut self) {
            DOWNLOADING.store(false, Ordering::SeqCst);
            DOWNLOADED_BYTES.store(0, Ordering::SeqCst);
            emit_status(self.0);
        }
    }
    let _done = Done(app);
    CANCEL.store(false, Ordering::SeqCst);
    let dir = model_dir(app)?;
    std::fs::create_dir_all(&dir)?;
    let target = dir.join(FILE);
    let part = dir.join(format!("{FILE}.part"));
    let url = format!("https://huggingface.co/{REPO}/resolve/{REVISION}/{FILE}");
    let client = crate::net::client_builder()
        .connect_timeout(Duration::from_secs(15))
        .build()?;

    let mut have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if have >= SIZE {
        let _ = std::fs::remove_file(&part);
        have = 0;
    }
    let mut request = client.get(&url);
    if have > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let response = tokio::select! {
        r = tokio::time::timeout(Duration::from_secs(30), request.send()) => r
            .map_err(|_| anyhow!("the server did not answer for 30 s"))??
            .error_for_status()?,
        _ = cancelled() => return Err(anyhow!("cancelled")),
    };
    let resumed = have > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resumed)
        .truncate(!resumed)
        .open(&part)?;
    let mut done = if resumed { have } else { 0 };
    info!("Speakers model: downloading from {} bytes", done);
    let mut stream = response.bytes_stream();
    let mut last_emit = Instant::now();
    loop {
        let next = tokio::select! {
            n = tokio::time::timeout(Duration::from_secs(60), stream.next()) => n
                .map_err(|_| anyhow!("the download stalled: no data for 60 s"))?,
            _ = cancelled() => return Err(anyhow!("cancelled")),
        };
        let Some(chunk) = next else { break };
        let chunk = chunk?;
        file.write_all(&chunk)?;
        done += chunk.len() as u64;
        DOWNLOADED_BYTES.store(done, Ordering::SeqCst);
        if last_emit.elapsed() >= Duration::from_millis(200) {
            last_emit = Instant::now();
            emit_status(app);
        }
    }
    file.flush()?;
    drop(file);
    let check = part.clone();
    let ok = tauri::async_runtime::spawn_blocking(move || file_matches(&check, &spec()))
        .await
        .map_err(|e| anyhow!("{e}"))??;
    if !ok {
        let _ = std::fs::remove_file(&part);
        return Err(anyhow!(
            "the speakers model does not match its checksum; try again"
        ));
    }
    if CANCEL.load(Ordering::SeqCst) {
        return Err(anyhow!("cancelled"));
    }
    std::fs::rename(&part, &target)?;
    info!("Speakers model: downloaded and verified");
    Ok(())
}

/// Resolves once a cancel has been asked for.
async fn cancelled() {
    while !CANCEL.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

pub fn cancel_download() {
    CANCEL.store(true, Ordering::SeqCst);
}

pub fn delete(app: &AppHandle) -> Result<()> {
    if DOWNLOADING.load(Ordering::SeqCst) {
        return Err(anyhow!("the speakers model is downloading"));
    }
    // Never wait for a running job (this is called from the UI).
    let _job = match JOB.try_lock() {
        Ok(guard) => guard,
        Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => {
            return Err(anyhow!(
                "the speakers model is in use; try again when it is done"
            ));
        }
    };
    let dir = model_dir(app)?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    emit_status(app);
    Ok(())
}

/// A recording's text and, with at least two speakers, its `A: ...` text.
pub struct Speakers {
    pub text: String,
    pub diarized: Option<String>,
}

/// Who said what in `samples` (16 kHz mono). `transcribe` gets each speaker
/// turn's audio and returns its text. Blocking: run off the async runtime.
pub fn speakers_for(
    app: &AppHandle,
    samples: &[f32],
    mut transcribe: impl FnMut(Vec<f32>) -> std::result::Result<String, String>,
) -> std::result::Result<Speakers, String> {
    if !is_downloaded(app) {
        return Err("LOCAL_SPEAKERS_NOT_DOWNLOADED".into());
    }
    let path = model_path(app).map_err(|e| e.to_string())?;
    let total_secs = samples.len() as f32 / 16_000.0;
    let started = Instant::now();
    let probs = {
        let _job = JOB.lock().unwrap_or_else(|p| p.into_inner());
        // Not every core: dictation and the engine keep running beside it.
        let threads = std::thread::available_parallelism()
            .map(|n| (n.get() / 2).clamp(1, 4))
            .unwrap_or(2);
        let mut model = sortformer::Sortformer::load(&path, threads)
            .map_err(|e| format!("could not load the speakers model: {e}"))?;
        model
            .probabilities(samples, &|| false)
            .map_err(|e| format!("the speakers model failed: {e}"))?
        // The model is dropped here, before the turns are transcribed.
    };
    let smoothed = sortformer::smooth(&probs);
    let segments = sortformer::segments(&smoothed);
    let turns = sortformer::turns(&smoothed, &segments, total_secs);
    info!(
        "Speakers: {} turn(s) in {:.0} s of audio, found in {:.1} s",
        turns.len(),
        total_secs,
        started.elapsed().as_secs_f32()
    );

    let mut said: Vec<(usize, String)> = Vec::new();
    for turn in &turns {
        let a = ((turn.start * 16_000.0) as usize).min(samples.len());
        let b = ((turn.end * 16_000.0).ceil() as usize).clamp(a, samples.len());
        if b <= a {
            continue;
        }
        let text = transcribe(samples[a..b].to_vec())?;
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        match said.last_mut() {
            Some(last) if last.0 == turn.speaker => {
                last.1.push(' ');
                last.1.push_str(text);
            }
            _ => said.push((turn.speaker, text.to_string())),
        }
    }
    Ok(format_speakers(&said))
}

/// Speakers lettered A, B, ... in the order they first speak; the speaker
/// text only with two or more of them (as the STT server gives it).
pub fn format_speakers(said: &[(usize, String)]) -> Speakers {
    let mut order: Vec<usize> = Vec::new();
    for (s, _) in said {
        if !order.contains(s) {
            order.push(*s);
        }
    }
    let text = said
        .iter()
        .map(|(_, t)| t.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let diarized = (order.len() >= 2).then(|| {
        said.iter()
            .map(|(s, t)| {
                let letter = (b'A' + order.iter().position(|o| o == s).unwrap_or(0) as u8) as char;
                format!("{letter}: {t}")
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    });
    if diarized.is_none() && !said.is_empty() {
        warn!("Speakers: only one voice found");
    }
    Speakers { text, diarized }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speakers_are_lettered_in_order_of_first_words() {
        let said = vec![
            (2, "Hello there.".to_string()),
            (0, "Hi.".to_string()),
            (2, "How are you?".to_string()),
        ];
        let s = format_speakers(&said);
        assert_eq!(s.text, "Hello there. Hi. How are you?");
        assert_eq!(
            s.diarized.as_deref(),
            Some("A: Hello there.\n\nB: Hi.\n\nA: How are you?")
        );
    }

    #[test]
    fn one_voice_has_no_speaker_text() {
        let s = format_speakers(&[(1, "Just me.".to_string())]);
        assert_eq!(s.text, "Just me.");
        assert!(s.diarized.is_none());
    }

    #[test]
    fn the_pinned_file_is_the_fp32_export() {
        assert_eq!(spec().size, SIZE);
        assert!(!FILE.contains("int8"));
        assert!(UNUSABLE_FILES.iter().all(|f| *f != FILE));
        assert_eq!(REVISION.len(), 40);
        assert_eq!(SHA256.len(), 64);
    }
}
