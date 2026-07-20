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

/// The HTTP server answers `/v1/models` within tens of ms, but the ASR
/// sub-model loads a second or two LATER and can fail NPU context creation
/// (e.g. "Failed to create context virtual (0xc01e0009)") — leaving the server
/// "up" while every transcription returns null. After the server is reachable
/// we watch FLM's stdout for that failure for this window, so selecting the FLM
/// model fails loudly with the real reason instead of silently producing empty
/// transcripts on every take. No failure marker within the window => assume the
/// ASR model loaded fine (never false-fails a working setup).
const ASR_CONFIRM_TIMEOUT: Duration = Duration::from_secs(8);
const ASR_CONFIRM_POLL: Duration = Duration::from_millis(250);

/// Substrings FLM prints to stdout when the ASR model fails to initialize.
const ASR_FAIL_MARKERS: [&str; 2] = ["Failed to load ASR model", "Failed to create context"];

/// True if FLM's stdout so far contains a marker indicating the ASR model
/// failed to load (e.g. NPU context creation error 0xc01e0009).
fn asr_output_indicates_failure(stdout: &str) -> bool {
    ASR_FAIL_MARKERS.iter().any(|m| stdout.contains(m))
}

/// Build the user-facing error for an ASR-load failure. Only blames the NPU
/// context specifically when FLM actually reported a context-creation failure;
/// a generic "Failed to load ASR model" gets a generic message so we don't
/// misattribute an unrelated ASR error to the NPU.
fn asr_failure_message(stdout: &str) -> String {
    let detail = stdout
        .lines()
        .rev()
        .find(|l| ASR_FAIL_MARKERS.iter().any(|m| l.contains(m)))
        .unwrap_or("")
        .trim();
    if stdout.contains("Failed to create context") {
        format!(
            "FLM started but its ASR (speech-to-text) model failed to load — the NPU could not \
             create an inference context (e.g. error 0xc01e0009). The NPU allows only one app at \
             a time, so this usually means another process already holds it: close any other FLM \
             instances (FLMTray, a standalone `flm serve`, or Lemonade) and reselect the model. \
             If nothing else is using the NPU, update the NPU driver / FLM runtime, or select a \
             non-FLM model. (FLM: {})",
            detail
        )
    } else {
        format!(
            "FLM started but its ASR (speech-to-text) model failed to load, so transcription is \
             unavailable. Check the FLM runtime/logs, or select a non-FLM model. (FLM: {})",
            detail
        )
    }
}

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
        // stdout is ALSO buffered so the ASR-load confirmation below can detect
        // the NPU context-creation failure that FLM only reports on stdout.
        let stderr_log: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
        let stdout_log: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
        // stdout is buffered ONLY during the startup ASR-load confirmation
        // below; once confirmation ends we flip this off so the buffer stops
        // growing for the (possibly long-lived) rest of the FLM process. The
        // drain thread keeps running (and logging) either way so the pipe never
        // blocks.
        let capture_stdout: Arc<std::sync::atomic::AtomicBool> =
            Arc::new(std::sync::atomic::AtomicBool::new(true));
        if let Some(stdout) = child.stdout.take() {
            let log_handle = Arc::clone(&stdout_log);
            let capture = Arc::clone(&capture_stdout);
            std::thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    match line {
                        Ok(l) => {
                            info!("FLM stdout: {}", l);
                            if capture.load(std::sync::atomic::Ordering::Relaxed) {
                                if let Ok(mut buf) = log_handle.lock() {
                                    if !buf.is_empty() {
                                        buf.push('\n');
                                    }
                                    buf.push_str(&l);
                                }
                            }
                        }
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
                manager.stop();
                LAST_START_FAILURE_MS.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
                // Prefer the specific ASR/NPU diagnosis if the marker reached
                // stdout, even when /v1/models never answered. The stdout reader
                // is async, so drain-retry briefly (same bound as the exit path)
                // before falling back to the generic timeout message.
                let mut out = stdout_log.lock().map(|b| b.clone()).unwrap_or_default();
                for _ in 0..10 {
                    if asr_output_indicates_failure(&out) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                    out = stdout_log.lock().map(|b| b.clone()).unwrap_or_default();
                }
                capture_stdout.store(false, std::sync::atomic::Ordering::Relaxed);
                if asr_output_indicates_failure(&out) {
                    error!("FLM ASR model failed to load:\n{}", out.trim());
                    return Err(anyhow::anyhow!("{}", asr_failure_message(&out)));
                }
                let captured = stderr_log.lock().map(|b| b.clone()).unwrap_or_default();
                if !captured.is_empty() {
                    error!("FLM stderr output at timeout:\n{}", captured.trim());
                }
                return Err(anyhow::anyhow!(
                    "FLM server did not become ready within {:?}. stderr: {}",
                    HEALTH_TIMEOUT,
                    captured.trim()
                ));
            }

            // Check if the child process has exited unexpectedly. The ASR/NPU
            // failure marker can be printed to stdout BEFORE /v1/models ever
            // answers, so prefer the specific ASR diagnosis here too (with the
            // same bounded drain-retry as the confirm loop) rather than always
            // emitting the generic premature-exit message.
            if let Some(ref mut child) = manager.child {
                if let Ok(Some(status)) = child.try_wait() {
                    LAST_START_FAILURE_MS.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
                    let mut out = stdout_log.lock().map(|b| b.clone()).unwrap_or_default();
                    for _ in 0..10 {
                        if asr_output_indicates_failure(&out) {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                        out = stdout_log.lock().map(|b| b.clone()).unwrap_or_default();
                    }
                    if asr_output_indicates_failure(&out) {
                        error!("FLM ASR model failed to load:\n{}", out.trim());
                        manager.stop();
                        capture_stdout.store(false, std::sync::atomic::Ordering::Relaxed);
                        return Err(anyhow::anyhow!("{}", asr_failure_message(&out)));
                    }
                    let captured = stderr_log.lock().map(|b| b.clone()).unwrap_or_default();
                    if !captured.is_empty() {
                        error!("FLM stderr: {}", captured.trim());
                    }
                    capture_stdout.store(false, std::sync::atomic::Ordering::Relaxed);
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
                        "FLM server reachable in {:?}; confirming ASR model loaded...",
                        start.elapsed()
                    );
                    break;
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

        // The server is reachable, but the ASR sub-model loads slightly later
        // and can fail NPU context creation. Watch FLM's stdout for that failure
        // for a bounded window; if it appears, fail the start with the real
        // reason so model SELECTION reports it (instead of every take silently
        // yielding an empty transcript). No marker within the window => the ASR
        // model loaded fine.
        let confirm_start = Instant::now();
        let outcome = loop {
            let out = stdout_log.lock().map(|b| b.clone()).unwrap_or_default();

            // Explicit ASR-load failure marker — the definitive signal. Checked
            // FIRST so we always emit the specific NPU/ASR diagnosis, even if
            // the process then exits (which would otherwise hit the generic
            // exit branch below).
            if asr_output_indicates_failure(&out) {
                error!("FLM ASR model failed to load:\n{}", out.trim());
                manager.stop();
                LAST_START_FAILURE_MS.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
                break Err(anyhow::anyhow!("{}", asr_failure_message(&out)));
            }

            // Child died while loading the ASR model? Prefer the ASR-marker
            // diagnosis if it's present in stdout by now; else report the exit.
            if let Some(ref mut child) = manager.child {
                if let Ok(Some(status)) = child.try_wait() {
                    LAST_START_FAILURE_MS.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
                    // The stdout drain thread is async: the process can exit
                    // just after printing the failure marker but before the
                    // reader appends it. Give it a brief bounded window to
                    // surface so we emit the specific NPU/ASR diagnosis rather
                    // than the generic exit message.
                    let mut out = stdout_log.lock().map(|b| b.clone()).unwrap_or_default();
                    for _ in 0..10 {
                        if asr_output_indicates_failure(&out) {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                        out = stdout_log.lock().map(|b| b.clone()).unwrap_or_default();
                    }
                    if asr_output_indicates_failure(&out) {
                        break Err(anyhow::anyhow!("{}", asr_failure_message(&out)));
                    }
                    let captured = stderr_log.lock().map(|b| b.clone()).unwrap_or_default();
                    break Err(anyhow::anyhow!(
                        "FLM process exited while loading the ASR model (status: {}). stderr: {}",
                        status,
                        captured.trim()
                    ));
                }
            }

            if confirm_start.elapsed() > ASR_CONFIRM_TIMEOUT {
                info!(
                    "FLM ASR model confirmed loaded (no failure within {:?}) for model: {}",
                    ASR_CONFIRM_TIMEOUT, model_name
                );
                info!(
                    "FLM server ready in {:?} for model: {}",
                    start.elapsed(),
                    model_name
                );
                break Ok(manager);
            }

            std::thread::sleep(ASR_CONFIRM_POLL);
        };

        // Stop growing the stdout capture buffer for the rest of the (possibly
        // long-lived) FLM process — it was only needed for the confirmation
        // above. The drain thread keeps running (and logging).
        capture_stdout.store(false, std::sync::atomic::Ordering::Relaxed);
        outcome
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

    /// True if the FLM subprocess is still alive (has not exited). Used to skip
    /// a redundant restart when a re-selection lands on an already-healthy FLM,
    /// while still restarting a dead one.
    pub fn is_running(&mut self) -> bool {
        match self.child {
            Some(ref mut child) => matches!(child.try_wait(), Ok(None)),
            None => false,
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

#[cfg(test)]
mod tests {
    use super::asr_output_indicates_failure;

    #[test]
    fn detects_npu_context_failure_in_flm_stdout() {
        // The exact stdout FLM emits on this class of failure.
        let stdout = "[FLM]  Configuring NPU Power Mode to performance (flm default)\n\
                      [FLM]  Using user-specified port: 52625\n\
                      [FLM]  Loading model: C:\\Users\\x\\.flm\\models\\Whisper-V3-Turbo-NPU2\n\
                      [ERROR]  Failed to load ASR model: Failed to create context virtual \
                      (0xc01e0009): There was an error while creating context ";
        assert!(asr_output_indicates_failure(stdout));
    }

    #[test]
    fn does_not_flag_a_healthy_startup() {
        let stdout = "[FLM]  Configuring NPU Power Mode to performance (flm default)\n\
                      [FLM]  Using user-specified port: 52625\n\
                      [FLM]  Loading model: C:\\Users\\x\\.flm\\models\\Whisper-V3-Turbo-NPU2\n\
                      [FLM]  ASR model ready";
        assert!(!asr_output_indicates_failure(stdout));
    }

    #[test]
    fn flags_the_generic_asr_load_failure_marker_too() {
        assert!(asr_output_indicates_failure(
            "[ERROR]  Failed to load ASR model: some other reason"
        ));
        assert!(!asr_output_indicates_failure(""));
    }

    #[test]
    fn message_blames_the_npu_only_on_a_context_failure() {
        use super::asr_failure_message;
        let npu = asr_failure_message(
            "[ERROR]  Failed to load ASR model: Failed to create context virtual (0xc01e0009)",
        );
        assert!(npu.contains("NPU"), "{npu}");
        assert!(npu.contains("0xc01e0009"), "{npu}");

        // Generic ASR failure (no context marker) must NOT be blamed on the NPU.
        let generic = asr_failure_message("[ERROR]  Failed to load ASR model: some other reason");
        assert!(!generic.contains("NPU"), "{generic}");
        assert!(generic.contains("unavailable"), "{generic}");
    }
}
