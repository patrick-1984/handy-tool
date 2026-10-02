//! The two speaker models speaker detection needs, where they live and how
//! they are downloaded.
//!
//! Third-party models (downloaded on demand, not bundled):
//! - Segmentation: pyannote `segmentation-3.0` by Hervé Bredin / pyannote,
//!   MIT licence (<https://huggingface.co/pyannote/segmentation-3.0>), in the
//!   ONNX export published by the sherpa-onnx project (k2-fsa).
//! - Embedding: WeSpeaker `voxceleb_resnet34_LM` (ResNet34, VoxCeleb), CC BY
//!   4.0 (<https://github.com/wenet-e2e/wespeaker>), ONNX export by
//!   sherpa-onnx.

use crate::managers::model::{DownloadCancel, DownloadOutcome, stream_to_file_with_cancel};
use anyhow::{Result, anyhow, bail};
use log::{info, warn};
use serde::Serialize;
use sha2::{Digest, Sha256};
use specta::Type;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub struct ModelFile {
    pub file_name: &'static str,
    pub url: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

pub const SEGMENTATION: ModelFile = ModelFile {
    file_name: "pyannote-segmentation-3.0.onnx",
    url: "https://huggingface.co/csukuangfj/sherpa-onnx-pyannote-segmentation-3-0/resolve/main/model.onnx",
    size: 5_992_913,
    sha256: "220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079",
};

pub const EMBEDDING: ModelFile = ModelFile {
    file_name: "wespeaker_en_voxceleb_resnet34_LM.onnx",
    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/wespeaker_en_voxceleb_resnet34_LM.onnx",
    size: 26_530_550,
    sha256: "e9848563da86f263117134dfd7ad63c92355b37de492b55e325400c9d9c39012",
};

const FILES: [&ModelFile; 2] = [&SEGMENTATION, &EMBEDDING];

/// Event with the current [`SpeakerModelStatus`], emitted while a download
/// runs and after a delete.
pub const STATUS_EVENT: &str = "speaker-model-status";

/// Abort a download when the server sends nothing for this long; the partial
/// file is kept, so the next try resumes.
const STALL_TIMEOUT: Duration = Duration::from_secs(60);
/// At most ten progress events a second.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// `<app_data>/models/diarization/`
pub fn models_dir(app: &AppHandle) -> Result<PathBuf> {
    Ok(crate::portable::resolve_app_data_dir(app)
        .map_err(|e| anyhow!("Failed to get app data dir: {}", e))?
        .join("models")
        .join("diarization"))
}

#[derive(Debug, Clone)]
pub struct ModelPaths {
    pub segmentation: PathBuf,
    pub embedding: PathBuf,
}

fn paths_in(dir: &Path) -> Option<ModelPaths> {
    let paths = ModelPaths {
        segmentation: dir.join(SEGMENTATION.file_name),
        embedding: dir.join(EMBEDDING.file_name),
    };
    (paths.segmentation.is_file() && paths.embedding.is_file()).then_some(paths)
}

/// Paths of both models, if both are fully downloaded.
pub fn installed_paths(app: &AppHandle) -> Option<ModelPaths> {
    paths_in(&models_dir(app).ok()?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SpeakerModelState {
    Missing,
    Downloading,
    Verifying,
    Ready,
    Failed,
}

/// Download state of the speaker models; also emitted as
/// [`STATUS_EVENT`] while a download runs.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
pub struct SpeakerModelStatus {
    pub state: SpeakerModelState,
    pub downloaded: u64,
    pub total: u64,
    pub error: Option<String>,
}

fn total_size() -> u64 {
    FILES.iter().map(|f| f.size).sum()
}

static DOWNLOADING: AtomicBool = AtomicBool::new(false);
/// Held for a whole download, so a second caller waits for it to finish.
static DOWNLOAD_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
/// Held for a whole "Make note with speakers" transcription (see
/// [`lock_for_use`]): such jobs run one at a time, and the models are not
/// deleted under a running one.
static IN_USE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
/// Latest in-flight progress, or the error of the last failed attempt.
static LAST_STATUS: Mutex<Option<SpeakerModelStatus>> = Mutex::new(None);

fn publish(app: &AppHandle, status: SpeakerModelStatus) {
    if let Ok(mut last) = LAST_STATUS.lock() {
        *last = Some(status.clone());
    }
    let _ = app.emit(STATUS_EVENT, status);
}

pub fn status(app: &AppHandle) -> SpeakerModelStatus {
    let total = total_size();
    if DOWNLOADING.load(Ordering::Acquire)
        && let Some(status) = LAST_STATUS.lock().ok().and_then(|s| s.clone())
    {
        return status;
    }
    if installed_paths(app).is_some() {
        return SpeakerModelStatus {
            state: SpeakerModelState::Ready,
            downloaded: total,
            total,
            error: None,
        };
    }
    let error = LAST_STATUS
        .lock()
        .ok()
        .and_then(|s| s.clone())
        .filter(|s| s.state == SpeakerModelState::Failed)
        .and_then(|s| s.error);
    SpeakerModelStatus {
        state: if error.is_some() {
            SpeakerModelState::Failed
        } else {
            SpeakerModelState::Missing
        },
        downloaded: 0,
        total,
        error,
    }
}

/// Download whichever model files are missing (resuming partials) and verify
/// their sha256. A call made while a download runs waits for it and returns
/// once the models are in place (or downloads again if that one failed).
pub async fn download(app: &AppHandle) -> Result<()> {
    let _download = DOWNLOAD_LOCK.lock().await;
    if installed_paths(app).is_some() {
        return Ok(());
    }
    DOWNLOADING.store(true, Ordering::Release);
    let dir = models_dir(app)?;
    // Replace any stale status (e.g. the last failure) right away: the first
    // progress event only comes once the server answers.
    let finished: u64 = FILES
        .iter()
        .filter(|f| dir.join(f.file_name).is_file())
        .map(|f| f.size)
        .sum();
    publish(
        app,
        SpeakerModelStatus {
            state: SpeakerModelState::Downloading,
            downloaded: finished,
            total: total_size(),
            error: None,
        },
    );
    let progress_app = app.clone();
    let result = download_files(&dir, &mut |state, downloaded| {
        publish(
            &progress_app,
            SpeakerModelStatus {
                state,
                downloaded,
                total: total_size(),
                error: None,
            },
        )
    })
    .await;
    DOWNLOADING.store(false, Ordering::Release);
    let total = total_size();
    match &result {
        Ok(()) => publish(
            app,
            SpeakerModelStatus {
                state: SpeakerModelState::Ready,
                downloaded: total,
                total,
                error: None,
            },
        ),
        Err(e) => {
            warn!("Speaker model download failed: {:#}", e);
            publish(
                app,
                SpeakerModelStatus {
                    state: SpeakerModelState::Failed,
                    downloaded: 0,
                    total,
                    error: Some(e.to_string()),
                },
            );
        }
    }
    result
}

/// Wait until no other speaker transcription runs, then keep the models
/// in use (not deletable) until the guard is dropped.
pub async fn lock_for_use() -> tokio::sync::MutexGuard<'static, ()> {
    IN_USE.lock().await
}

/// Error from [`delete`] while a speaker transcription uses the models.
pub const ERR_IN_USE: &str = "speakers_models_in_use";
/// Error from [`delete`] while the models are being downloaded.
pub const ERR_DOWNLOADING: &str = "speakers_models_downloading";

/// Delete the downloaded models (and any partial downloads) to free space.
/// Refused while a download or a speaker transcription runs.
pub fn delete(app: &AppHandle) -> Result<()> {
    let Ok(_download) = DOWNLOAD_LOCK.try_lock() else {
        bail!(ERR_DOWNLOADING);
    };
    let Ok(_in_use) = IN_USE.try_lock() else {
        bail!(ERR_IN_USE);
    };
    let dir = models_dir(app)?;
    delete_files(&dir)?;
    info!("Deleted the speaker models in {:?}", dir);
    if let Ok(mut last) = LAST_STATUS.lock() {
        *last = None;
    }
    let _ = app.emit(STATUS_EVENT, status(app));
    Ok(())
}

fn delete_files(dir: &Path) -> Result<()> {
    for file in FILES {
        for name in [
            file.file_name.to_string(),
            format!("{}.partial", file.file_name),
        ] {
            let path = dir.join(name);
            if path.exists() {
                std::fs::remove_file(&path)?;
            }
        }
    }
    Ok(())
}

/// Download every missing file of [`FILES`] into `dir`. `progress` gets the
/// state and the bytes done over all files.
async fn download_files(
    dir: &Path,
    progress: &mut (dyn FnMut(SpeakerModelState, u64) + Send),
) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .build()?;
    let mut done = 0;
    for file in FILES {
        let final_path = dir.join(file.file_name);
        if !final_path.is_file() {
            download_file(&client, file.url, file, dir, done, progress).await?;
        }
        done += file.size;
    }
    Ok(())
}

/// One file: resume `<name>.partial` with a Range request, check its size and
/// sha256, then move it into place. A partial that fails the check is
/// deleted, so the next try starts over.
async fn download_file(
    client: &reqwest::Client,
    url: &str,
    file: &ModelFile,
    dir: &Path,
    base: u64,
    progress: &mut (dyn FnMut(SpeakerModelState, u64) + Send),
) -> Result<()> {
    let partial = dir.join(format!("{}.partial", file.file_name));
    let mut resume_from = std::fs::metadata(&partial).map_or(0, |m| m.len());
    if resume_from > file.size {
        std::fs::remove_file(&partial)?;
        resume_from = 0;
    }
    if resume_from < file.size {
        let mut request = client.get(url);
        if resume_from > 0 {
            request = request.header("Range", format!("bytes={}-", resume_from));
        }
        let response = request.send().await?;
        let status = response.status();
        if !status.is_success() {
            bail!("HTTP {} for {}", status, file.file_name);
        }
        // A server that ignores the Range header sends the whole file.
        let append = resume_from > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT;
        if !append {
            resume_from = 0;
        }
        let mut out = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(&partial)?;
        let mut stream = Box::pin(response.bytes_stream());
        let mut downloaded = resume_from;
        let mut last_emit = Instant::now();
        progress(SpeakerModelState::Downloading, base + downloaded);
        // Never cancelled: the download is small and the UI offers no way to
        // abort it. The stall timeout still applies.
        let cancel = DownloadCancel::new();
        let outcome =
            stream_to_file_with_cancel(&mut stream, &mut out, &cancel, STALL_TIMEOUT, |n| {
                downloaded += n as u64;
                if last_emit.elapsed() >= PROGRESS_INTERVAL {
                    progress(SpeakerModelState::Downloading, base + downloaded);
                    last_emit = Instant::now();
                }
            })
            .await?;
        if let DownloadOutcome::Cancelled = outcome {
            bail!("download of {} was cancelled", file.file_name);
        }
        out.sync_all()?;
    }

    progress(SpeakerModelState::Verifying, base + file.size);
    let path = partial.clone();
    let actual = tokio::task::spawn_blocking(move || sha256_file(&path)).await??;
    let size = std::fs::metadata(&partial)?.len();
    if size != file.size || actual != file.sha256 {
        let _ = std::fs::remove_file(&partial);
        bail!(
            "{} failed verification (size {} of {}, sha256 {})",
            file.file_name,
            size,
            file.size,
            actual
        );
    }
    std::fs::rename(&partial, dir.join(file.file_name))?;
    info!("Downloaded speaker model {}", file.file_name);
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    /// A tiny HTTP server for one file that honours `Range: bytes=N-`.
    /// Returns the URL and a log of the ranges asked for.
    fn serve(body: Vec<u8>, requests: usize) -> (String, std::sync::mpsc::Receiver<u64>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/model.onnx", listener.local_addr().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(requests) {
                let mut stream = stream.unwrap();
                let mut from = 0u64;
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                        from = v.trim().trim_end_matches('-').parse().unwrap();
                    }
                }
                tx.send(from).unwrap();
                let part = &body[from as usize..];
                let status = if from > 0 {
                    "206 Partial Content"
                } else {
                    "200 OK"
                };
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    part.len()
                )
                .unwrap();
                stream.write_all(part).unwrap();
            }
        });
        (url, rx)
    }

    fn file_for(body: &[u8]) -> ModelFile {
        let mut hasher = Sha256::new();
        hasher.update(body);
        let sha: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        ModelFile {
            file_name: "model.onnx",
            url: "",
            size: body.len() as u64,
            sha256: Box::leak(sha.into_boxed_str()),
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "handy-speaker-models-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn download_resumes_a_partial_and_verifies_it() {
        let body: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
        let file = file_for(&body);
        let dir = temp_dir("resume");
        std::fs::write(dir.join("model.onnx.partial"), &body[..40_000]).unwrap();
        let (url, ranges) = serve(body.clone(), 1);
        let client = reqwest::Client::new();
        let mut seen = Vec::new();
        download_file(&client, &url, &file, &dir, 7, &mut |state, done| {
            seen.push((state, done))
        })
        .await
        .unwrap();
        assert_eq!(ranges.recv().unwrap(), 40_000);
        assert_eq!(std::fs::read(dir.join("model.onnx")).unwrap(), body);
        assert!(!dir.join("model.onnx.partial").exists());
        assert_eq!(
            seen.last(),
            Some(&(SpeakerModelState::Verifying, 7 + 100_000))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_corrupt_download_is_rejected_and_removed() {
        let body = vec![1u8; 5000];
        let mut file = file_for(&body);
        file.sha256 = "00";
        let dir = temp_dir("corrupt");
        let (url, _ranges) = serve(body, 1);
        let err = download_file(
            &reqwest::Client::new(),
            &url,
            &file,
            &dir,
            0,
            &mut |_, _| {},
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("failed verification"), "{err}");
        assert!(!dir.join("model.onnx").exists());
        assert!(!dir.join("model.onnx.partial").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn installed_only_when_both_files_are_there() {
        let dir = temp_dir("installed");
        assert!(paths_in(&dir).is_none());
        std::fs::write(dir.join(SEGMENTATION.file_name), b"x").unwrap();
        std::fs::write(dir.join(format!("{}.partial", EMBEDDING.file_name)), b"x").unwrap();
        assert!(paths_in(&dir).is_none());
        std::fs::write(dir.join(EMBEDDING.file_name), b"x").unwrap();
        assert!(paths_in(&dir).is_some());
        delete_files(&dir).unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sha256_matches_a_known_digest() {
        let dir = temp_dir("sha");
        let path = dir.join("abc");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
