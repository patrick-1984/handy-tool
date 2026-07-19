use anyhow::Result;
use log::{debug, error, info, warn};
use std::io::{BufRead, BufReader, Write};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Windows: run child processes without flashing a console window. Handy is a
/// GUI-subsystem app, so spawning a console program (e.g. `flm`) without this
/// pops up a black console window that flickers and vanishes.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Apply the no-window flag on Windows; no-op elsewhere.
fn no_window(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

const FLM_BASE_URL: &str = "http://127.0.0.1:52625";
/// Generous because `flm serve` auto-downloads a missing model (multi-GB) on
/// first use before the health endpoint comes up. Real failures still exit
/// early: the poll loop returns as soon as the child process dies.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(300);
const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(500);
/// After a failed start, don't retry (and re-block callers) for this long —
/// a broken FLM must not stall every take with a fresh multi-minute wait.
const FAILURE_COOLDOWN: Duration = Duration::from_secs(60);

/// Unix-ms timestamp of the last failed `start_serve` (0 = none).
static LAST_START_FAILURE_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub struct FlmManager {
    child: Option<Child>,
    model_name: String,
}

impl FlmManager {
    /// Detect whether `flm` is available on PATH or in common install locations.
    /// Cached after the first call: this spawns `flm --version`, and it used to
    /// run on every model-list rebuild — on Windows that flashed a console window
    /// periodically. Detection is stable within a session, so we memoize it.
    #[cfg(not(target_os = "macos"))]
    pub fn detect_flm() -> Option<PathBuf> {
        use std::sync::OnceLock;
        static CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();
        CACHE.get_or_init(Self::detect_flm_uncached).clone()
    }

    #[cfg(not(target_os = "macos"))]
    fn detect_flm_uncached() -> Option<PathBuf> {
        info!("Detecting FLM installation...");

        // Check PATH first (no console flash on Windows via no_window()).
        match no_window(Command::new("flm").arg("--version")).output() {
            Ok(output) if output.status.success() => {
                let version = String::from_utf8_lossy(&output.stdout);
                info!("FLM found on PATH (version: {})", version.trim());
                return Some(PathBuf::from("flm"));
            }
            Ok(output) => {
                info!(
                    "FLM found on PATH but --version failed with status: {}",
                    output.status
                );
            }
            Err(e) => {
                info!("FLM not found on PATH: {}", e);
            }
        }

        // Check common Windows install locations
        #[cfg(target_os = "windows")]
        {
            let candidates = [
                dirs::home_dir()
                    .map(|h| h.join("AppData").join("Local").join("flm").join("flm.exe")),
                dirs::home_dir().map(|h| h.join(".flm").join("flm.exe")),
                Some(PathBuf::from(r"C:\Program Files\flm\flm.exe")),
                Some(PathBuf::from(r"C:\Program Files (x86)\flm\flm.exe")),
            ];
            for candidate in candidates.into_iter().flatten() {
                info!("Checking FLM candidate: {}", candidate.display());
                if candidate.exists() {
                    info!("FLM found at: {}", candidate.display());
                    return Some(candidate);
                }
            }
        }

        // Check common Linux install locations
        #[cfg(target_os = "linux")]
        {
            let candidates = [
                dirs::home_dir().map(|h| h.join(".local").join("bin").join("flm")),
                Some(PathBuf::from("/usr/local/bin/flm")),
                Some(PathBuf::from("/usr/bin/flm")),
            ];
            for candidate in candidates.into_iter().flatten() {
                info!("Checking FLM candidate: {}", candidate.display());
                if candidate.exists() {
                    info!("FLM found at: {}", candidate.display());
                    return Some(candidate);
                }
            }
        }

        warn!("FLM not found in any known location");
        None
    }

    #[cfg(target_os = "macos")]
    pub fn detect_flm() -> Option<PathBuf> {
        None // FLM is Windows/Linux only
    }

    /// True when a recent `start_serve` failed and the cooldown hasn't
    /// elapsed — callers should fail fast instead of re-blocking for minutes.
    pub fn recently_failed() -> bool {
        let last = LAST_START_FAILURE_MS.load(std::sync::atomic::Ordering::Relaxed);
        last != 0 && now_ms().saturating_sub(last) < FAILURE_COOLDOWN.as_millis() as u64
    }

    /// Start `flm serve` with the given model name and wait until the health endpoint responds.
    pub fn start_serve(model_name: &str) -> Result<Self> {
        if Self::recently_failed() {
            anyhow::bail!(
                "FLM failed to start less than a minute ago; not retrying yet \
                 (select the model again later or check the FLM installation)"
            );
        }
        let flm_path =
            Self::detect_flm().ok_or_else(|| anyhow::anyhow!("FLM not found on this system"))?;

        info!(
            "Starting FLM serve: {} serve {} (path: {})",
            flm_path.display(),
            model_name,
            flm_path.display()
        );

        // FLM (v0.9.21+) cannot load a Whisper model as the MAIN serve model —
        // `flm serve whisper-v3:turbo` fails with "Unsupported model family or
        // non-llm". Whisper runs as the ASR sidecar: no positional model,
        // `--asr 1`, and the transcription endpoint appears on the same port.
        let mut command = Command::new(&flm_path);
        command
            .args([
                "serve",
                "--port",
                "52625",
                "--host",
                "127.0.0.1",
                "--asr",
                "1",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        no_window(&mut command);
        let mut child = command.spawn().map_err(|e| {
            LAST_START_FAILURE_MS.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
            anyhow::anyhow!("Failed to spawn FLM process: {}", e)
        })?;

        info!("FLM process spawned (pid: {:?})", child.id());

        // Spawn threads to continuously drain stdout/stderr so the FLM process
        // doesn't block on a full pipe buffer, and capture output for diagnostics.
        let stderr_log: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
        if let Some(stdout) = child.stdout.take() {
            std::thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    match line {
                        Ok(l) => info!("FLM stdout: {}", l),
                        Err(_) => break,
                    }
                }
            });
        }
        if let Some(stderr) = child.stderr.take() {
            let log_handle = Arc::clone(&stderr_log);
            std::thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    match line {
                        Ok(l) => {
                            info!("FLM stderr: {}", l);
                            if let Ok(mut buf) = log_handle.lock() {
                                if !buf.is_empty() {
                                    buf.push('\n');
                                }
                                buf.push_str(&l);
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        let mut manager = FlmManager {
            child: Some(child),
            model_name: model_name.to_string(),
        };

        // Poll readiness until the server answers. FLM ≥0.9.45 has no /health
        // route (it 404s forever) — /v1/models is the working liveness probe.
        let start = Instant::now();
        let health_url = format!("{}/v1/models", FLM_BASE_URL);

        loop {
            if start.elapsed() > HEALTH_TIMEOUT {
                let captured = stderr_log.lock().map(|b| b.clone()).unwrap_or_default();
                if !captured.is_empty() {
                    error!("FLM stderr output at timeout:\n{}", captured.trim());
                }
                manager.stop();
                LAST_START_FAILURE_MS.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
                return Err(anyhow::anyhow!(
                    "FLM server did not become ready within {:?}. stderr: {}",
                    HEALTH_TIMEOUT,
                    captured.trim()
                ));
            }

            // Check if the child process has exited unexpectedly
            if let Some(ref mut child) = manager.child {
                if let Ok(Some(status)) = child.try_wait() {
                    let captured = stderr_log.lock().map(|b| b.clone()).unwrap_or_default();
                    if !captured.is_empty() {
                        error!("FLM stderr: {}", captured.trim());
                    }
                    LAST_START_FAILURE_MS.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
                    return Err(anyhow::anyhow!(
                        "FLM process exited prematurely with status: {}. stderr: {}",
                        status,
                        captured.trim()
                    ));
                }
            }

            match ureq::get(&health_url).call() {
                Ok(_) => {
                    info!(
                        "FLM server ready in {:?} for model: {}",
                        start.elapsed(),
                        model_name
                    );
                    return Ok(manager);
                }
                Err(e) => {
                    info!(
                        "FLM health poll failed ({:.1}s elapsed): {}",
                        start.elapsed().as_secs_f32(),
                        e
                    );
                    std::thread::sleep(HEALTH_POLL_INTERVAL);
                }
            }
        }
    }

    /// Send audio samples to FLM for transcription via the OpenAI-compatible endpoint.
    /// When `translate_to_english` is true, uses `/v1/audio/translations` instead.
    pub fn transcribe(
        &self,
        audio_samples: Vec<f32>,
        language: Option<&str>,
        translate_to_english: bool,
    ) -> Result<String> {
        let wav_bytes = samples_to_wav(&audio_samples, 16000)?;
        let endpoint = if translate_to_english {
            "v1/audio/translations"
        } else {
            "v1/audio/transcriptions"
        };
        let url = format!("{}/{}", FLM_BASE_URL, endpoint);

        debug!(
            "Sending {} bytes of WAV to FLM (model: {}, language: {:?})",
            wav_bytes.len(),
            self.model_name,
            language
        );

        let response = ureq::post(&url)
            .set(
                "Content-Type",
                "multipart/form-data; boundary=----FlmBoundary",
            )
            .send_bytes(&build_multipart_body(
                &wav_bytes,
                &self.model_name,
                language,
            ))
            .map_err(|e| anyhow::anyhow!("FLM transcription request failed: {}", e))?;

        let body = response
            .into_string()
            .map_err(|e| anyhow::anyhow!("Failed to read FLM response: {}", e))?;

        // FLM returns HTTP 200 with a literal `null` body when its ASR model
        // is not loaded (e.g. NPU context creation failed at startup —
        // "Failed to create context virtual (0xc01e0009)"). Surfacing that as
        // an empty transcript would silently swallow takes.
        if body.trim().is_empty() || body.trim() == "null" {
            anyhow::bail!(
                "FLM returned no transcription — its ASR model is not loaded \
                 (check the FLM/NPU driver; see 'flm serve --asr 1' output)"
            );
        }

        // Parse OpenAI-compatible response: {"text": "..."}
        let parsed: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| anyhow::anyhow!("Failed to parse FLM response: {}", e))?;

        let text = parsed
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();

        debug!("FLM transcription result: {}", text);
        Ok(text)
    }

    /// Stop the FLM subprocess.
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            debug!("Stopping FLM process for model: {}", self.model_name);
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for FlmManager {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Convert f32 samples (16kHz mono) to WAV bytes.
fn samples_to_wav(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>> {
    let num_samples = samples.len();
    let bytes_per_sample = 2u16; // 16-bit PCM
    let data_size = (num_samples * bytes_per_sample as usize) as u32;
    let file_size = 36 + data_size;

    let mut buf = Vec::with_capacity(file_size as usize + 8);

    // RIFF header
    buf.write_all(b"RIFF")?;
    buf.write_all(&file_size.to_le_bytes())?;
    buf.write_all(b"WAVE")?;

    // fmt chunk
    buf.write_all(b"fmt ")?;
    buf.write_all(&16u32.to_le_bytes())?; // chunk size
    buf.write_all(&1u16.to_le_bytes())?; // PCM format
    buf.write_all(&1u16.to_le_bytes())?; // mono
    buf.write_all(&sample_rate.to_le_bytes())?;
    buf.write_all(&(sample_rate * bytes_per_sample as u32).to_le_bytes())?; // byte rate
    buf.write_all(&bytes_per_sample.to_le_bytes())?; // block align
    buf.write_all(&16u16.to_le_bytes())?; // bits per sample

    // data chunk
    buf.write_all(b"data")?;
    buf.write_all(&data_size.to_le_bytes())?;
    for &sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let int_sample = (clamped * 32767.0) as i16;
        buf.write_all(&int_sample.to_le_bytes())?;
    }

    Ok(buf)
}

/// Build a multipart/form-data body for the OpenAI transcription endpoint.
fn build_multipart_body(wav_bytes: &[u8], model: &str, language: Option<&str>) -> Vec<u8> {
    let boundary = "----FlmBoundary";
    let mut body = Vec::new();

    // file field
    let _ = write!(
        body,
        "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\n",
        boundary
    );
    body.extend_from_slice(wav_bytes);
    let _ = write!(body, "\r\n");

    // model field
    let _ = write!(
        body,
        "--{}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n{}\r\n",
        boundary, model
    );

    // language field — omit when None to let server auto-detect
    if let Some(lang) = language {
        let _ = write!(
            body,
            "--{}\r\nContent-Disposition: form-data; name=\"language\"\r\n\r\n{}\r\n",
            boundary, lang
        );
    }

    // closing boundary
    let _ = write!(body, "--{}--\r\n", boundary);
    body
}
