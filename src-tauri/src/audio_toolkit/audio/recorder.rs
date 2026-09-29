use std::{
    collections::VecDeque,
    io::Error,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use cpal::{
    Device, Sample, SizedSample,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

use crate::audio_toolkit::audio::mixer::{self, Downmix, SlaveRing, mix_into};
use crate::audio_toolkit::{
    VoiceActivityDetector,
    audio::{
        AudioVisualiser, ClosedChunk, FrameResampler, OpusChunkWriter, StartParams, glue_chunks,
    },
    constants,
    vad::{self, VadFrame},
};

/// Live preview callback: all kept audio of the take so far, and whether this
/// update ends an utterance (speech stopped, or the take was paused).
type SegmentCb = Arc<Mutex<Option<Arc<dyn Fn(Vec<f32>, bool) + Send + Sync + 'static>>>>;

/// What the overlay shows besides the spectrum levels.
#[derive(Clone, Copy, Debug, Default)]
pub struct MicState {
    /// The microphone is live (a cold one is not until it has warmed up).
    pub live: bool,
    /// Voice-like sound too quiet to be kept was heard just now.
    pub too_quiet: bool,
}

/// Spectrum levels for the overlay, and the microphone's state.
type LevelCb = Arc<dyn Fn(Vec<f32>, MicState) + Send + Sync + 'static>;
/// Reports a cold-started microphone's measured warm-up, in ms.
type WarmupCb = Arc<dyn Fn(u32) + Send + Sync + 'static>;

/// A microphone that takes longer than this from open to its first audio was
/// idle ("cold") - the only kind that fades in.
const COLD_START_MIN: Duration = Duration::from_millis(250);
/// 50 ms levels measured after a cold start (3 s) to find its warm-up.
const WARMUP_MEASURE_WINDOWS: usize = 60;
/// Below this a cold microphone is still sending digital silence (~ -100 dBFS):
/// a working one always has some noise. This PC's Realtek input sent 0.5 s of
/// exact zeros after a replug, then faded in.
const SILENT_SAMPLE: f32 = 1e-5;
const SILENT_DB: f32 = -100.0;
/// A cold microphone still silent this long after its first audio is shown as
/// live anyway (a muted one would otherwise say "Starting mic..." forever).
const SILENT_GIVE_UP: Duration = Duration::from_secs(3);

/// "Too quiet": a frame (30 ms) sounds a little like a voice at this speech
/// probability or more - the detector itself keeps frames only above 0.3.
const QUIET_PROB_MIN: f32 = 0.08;
/// ...and stands this far above the room's noise floor.
const QUIET_ABOVE_FLOOR_DB: f32 = 8.0;
/// Frames looked back over (1.5 s), and how many must be such near-speech (0.7 s).
const QUIET_WINDOW_FRAMES: usize = 50;
const QUIET_NEEDED_FRAMES: usize = 23;
/// After a hint, frames before the next one may come (4.5 s).
const QUIET_COOLDOWN_FRAMES: usize = 150;
/// How long the pill shows the hint.
const QUIET_HINT_SHOWN: Duration = Duration::from_secs(2);

/// How much new speech triggers a live preview update between pauses.
const LIVE_PREVIEW_INTERVAL_SECS: f32 = 1.5;

/// A take paused this long releases the microphone device (the system mic
/// indicator goes out); resuming starts it again.
const PAUSE_RELEASES_MIC_AFTER: Duration = Duration::from_secs(10 * 60);
type ClosedChunkCb = Arc<Mutex<Option<Arc<dyn Fn(ClosedChunk) + Send + Sync + 'static>>>>;

/// File-storage chunking: don't split the `.opus` file before this (~10 min);
/// recordings shorter than this stay a single file. Past it, cut at silence.
const FILE_SOFT_SAMPLES: usize = 10 * 60 * 16_000;
/// Force a file cut even mid-speech (~11 min @ 16 kHz).
const FILE_HARD_SAMPLES: usize = 11 * 60 * 16_000;

/// Transcription segmenting (on-the-fly): once this much speech (~20 s) has
/// accumulated, cut a transcription segment at the next VAD silence so it is
/// transcribed in the background while recording continues. This is independent
/// of file chunking — it's what makes a long recording finish almost instantly.
const SEG_SOFT_SAMPLES: usize = 20 * 16_000;
/// Force a transcription segment cut even mid-speech (~45 s) so an unbroken
/// monologue still streams to the engine.
const SEG_HARD_SAMPLES: usize = 45 * 16_000;

/// Minimum interval between mic-level callbacks while NOT recording (~16 Hz).
/// The mic can be always-on, so idle spectrum updates are throttled to avoid
/// flooding the event system; recording keeps full rate so the overlay
/// visualizer stays smooth.
const LEVEL_IDLE_INTERVAL: Duration = Duration::from_millis(60);

enum Cmd {
    /// Begin recording. `Some(params)` streams chunked Opus to disk; `None`
    /// records to memory only (legacy / crash-safety-off path).
    Start(Option<StartParams>),
    Stop(mpsc::Sender<Vec<f32>>),
    /// Stop and discard everything (no reply, no files kept).
    Cancel,
    /// Stop taking in audio but keep the take open; `Resume` continues it.
    Pause,
    Resume,
    /// Cut the take's kept audio back to this many samples (undo last word),
    /// then reply, so the caller knows every later live-preview snapshot is cut.
    Cut(usize, usize, mpsc::Sender<()>),
    /// Reply with a copy of the take's kept audio so far (undo last word).
    Snapshot(mpsc::Sender<Vec<f32>>),
    Shutdown,
}

/// Which side of the audio graph a capture device sits on: an ordinary input
/// (microphone) or an output endpoint captured via loopback (system audio).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointRole {
    Capture,
    RenderLoopback,
}

pub struct AudioRecorder {
    device: Option<Device>,
    cmd_tx: Option<mpsc::Sender<Cmd>>,
    /// Set by the CPAL error callback when the stream faults (device unplugged,
    /// endpoint invalidated, driver reset). The callback must stay trivial, so it
    /// only does a relaxed store - exactly like `first_buffer_seen`. Callers check
    /// `stream_faulted()` before REUSING an already-open recorder; without it a
    /// dead stream is silently kept and every later take returns no audio while
    /// the overlay still says "recording".
    stream_errored: Arc<AtomicBool>,
    /// Set by the worker when it could not negotiate a config, build the stream,
    /// or start it. Distinct from `stream_errored`, which only fires once a stream
    /// EXISTS and then faults: an arm failure means no stream was ever built, so the
    /// error callback can never fire and `stream_errored` stays false forever.
    /// Without this an always-on arm failure leaves is_open=true and every later
    /// take silently records nothing until the app restarts.
    arm_errored: Arc<AtomicBool>,
    /// The system-audio (loopback) leg, when one is open. The ring is shared with the
    /// owner thread's audio callback; the consumer pulls from it per master frame.
    sys_ring: Arc<Mutex<SlaveRing>>,
    sys_errored: Arc<AtomicBool>,
    sys_kill_tx: Option<mpsc::Sender<()>>,
    sys_handle: Option<std::thread::JoinHandle<()>>,
    /// Linear gain applied to the system leg before mixing.
    sys_gain: f32,
    /// Configured system-leg lag in ms; sizes the ring at arm time.
    sys_delay_ms: i32,
    worker_handle: Option<std::thread::JoinHandle<()>>,
    vad: Option<Arc<Mutex<Box<dyn vad::VoiceActivityDetector>>>>,
    level_cb: Option<LevelCb>,
    warmup_cb: Option<WarmupCb>,
    /// How long after its first audio a cold-started microphone counts as live.
    warmup_ms: Arc<AtomicU32>,
    /// Watch for speech too quiet to be kept (the "Too quiet" hint).
    quiet_hint: Arc<AtomicBool>,
    segment_cb: SegmentCb,
    closed_chunk_cb: ClosedChunkCb,
}

impl AudioRecorder {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(AudioRecorder {
            device: None,
            cmd_tx: None,
            stream_errored: Arc::new(AtomicBool::new(false)),
            arm_errored: Arc::new(AtomicBool::new(false)),
            sys_ring: Arc::new(Mutex::new(SlaveRing::new())),
            sys_errored: Arc::new(AtomicBool::new(false)),
            sys_kill_tx: None,
            sys_handle: None,
            sys_gain: 1.0,
            sys_delay_ms: 100,
            worker_handle: None,
            vad: None,
            level_cb: None,
            warmup_cb: None,
            warmup_ms: Arc::new(AtomicU32::new(0)),
            quiet_hint: Arc::new(AtomicBool::new(false)),
            segment_cb: Arc::new(Mutex::new(None)),
            closed_chunk_cb: Arc::new(Mutex::new(None)),
        })
    }

    pub fn with_vad(mut self, vad: Box<dyn VoiceActivityDetector>) -> Self {
        self.vad = Some(Arc::new(Mutex::new(vad)));
        self
    }

    pub fn with_level_callback<F>(mut self, cb: F) -> Self
    where
        F: Fn(Vec<f32>, MicState) + Send + Sync + 'static,
    {
        self.level_cb = Some(Arc::new(cb));
        self
    }

    /// Called once per cold microphone start with its measured warm-up (ms).
    pub fn with_warmup_callback<F>(mut self, cb: F) -> Self
    where
        F: Fn(u32) + Send + Sync + 'static,
    {
        self.warmup_cb = Some(Arc::new(cb));
        self
    }

    /// How long a cold start's first audio must age before the levels report
    /// the microphone as live (0 = at once). Set before `open()`.
    pub fn set_warmup_ms(&self, ms: u32) {
        self.warmup_ms.store(ms, Ordering::Relaxed);
    }

    /// Whether takes watch for speech too quiet to be kept. Read per frame.
    pub fn set_quiet_hint(&self, on: bool) {
        self.quiet_hint.store(on, Ordering::Relaxed);
    }

    /// Set the live preview callback: it fires every ~1.5 s of new speech and at
    /// each speech→silence boundary (or pause), with all kept audio of the take so
    /// far and whether the utterance just ended.
    pub fn set_segment_callback<F>(&self, cb: F)
    where
        F: Fn(Vec<f32>, bool) + Send + Sync + 'static,
    {
        *self.segment_cb.lock().unwrap() = Some(Arc::new(cb));
    }

    /// Clear the segment callback.
    pub fn clear_segment_callback(&self) {
        *self.segment_cb.lock().unwrap() = None;
    }

    /// Set a callback that fires each time a recording chunk is finalized
    /// (including the final chunk on stop). Receives the chunk's index, file
    /// path, and PCM samples (for background transcription).
    pub fn set_closed_chunk_callback<F>(&self, cb: F)
    where
        F: Fn(ClosedChunk) + Send + Sync + 'static,
    {
        *self.closed_chunk_cb.lock().unwrap() = Some(Arc::new(cb));
    }

    /// Clear the closed-chunk callback.
    pub fn clear_closed_chunk_callback(&self) {
        *self.closed_chunk_cb.lock().unwrap() = None;
    }

    /// Open the system-audio (loopback) leg alongside an already-open microphone.
    ///
    /// `cpal::Stream` is `!Send` on 0.16 (it holds a raw HANDLE), so the stream has to
    /// be built, played and dropped on ONE thread. This spawns a thread that does
    /// exactly that and then blocks until the kill channel closes - it never touches
    /// the pipeline itself, it only feeds the ring.
    ///
    /// Downmix is `FrontPair`, not the mean: a 5.1 endpoint carries a stereo meeting
    /// in FL/FR and a 6-channel mean is ~9.5 dB down, enough for the VAD to discard
    /// the far end as noise.
    pub fn open_system_audio(&mut self, device: Device) -> Result<(), Box<dyn std::error::Error>> {
        if self.sys_kill_tx.is_some() {
            return Ok(()); // already open
        }
        // Size the ring to the configured alignment BEFORE the callback can push to
        // it. The target is the delay, so this is the one place it can be applied.
        if let Ok(mut r) = self.sys_ring.lock() {
            *r = SlaveRing::with_target(mixer::target_from_delay_ms(self.sys_delay_ms));
        }
        let ring = Arc::clone(&self.sys_ring);
        let errored = Arc::clone(&self.sys_errored);
        errored.store(false, Ordering::Release);

        let (kill_tx, kill_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<(), String>>(1);

        let handle = std::thread::spawn(move || {
            // Everything fallible reports through `ready_tx` exactly once; a panic
            // here would abort the PROCESS (Cargo.toml sets panic = "abort").
            let built = (|| -> Result<(cpal::Stream, u32, usize), String> {
                let config =
                    AudioRecorder::get_preferred_config_for(&device, EndpointRole::RenderLoopback)
                        .map_err(|e| format!("config: {e}"))?;
                let rate = config.sample_rate().0;
                let channels = config.channels() as usize;
                let stream = AudioRecorder::build_system_stream(
                    &device,
                    &config,
                    Arc::clone(&ring),
                    channels,
                    rate,
                    Arc::clone(&errored),
                )
                .map_err(|e| format!("build: {e}"))?;
                stream.play().map_err(|e| format!("play: {e}"))?;
                Ok((stream, rate, channels))
            })();

            match built {
                Ok((stream, rate, channels)) => {
                    log::info!(
                        "System audio capture open: {} Hz, {} channels",
                        rate,
                        channels
                    );
                    let _ = ready_tx.send(Ok(()));
                    // Hold the stream alive until close(). recv() returns the moment
                    // the sender drops, so teardown is immediate with no polling.
                    let _ = kill_rx.recv();
                    drop(stream);
                }
                Err(e) => {
                    errored.store(true, Ordering::Release);
                    let _ = ready_tx.send(Err(e));
                }
            }
        });

        // Bounded wait: the caller must learn about an arm failure BEFORE the take
        // starts, otherwise the overlay says "recording" over a leg that does not
        // exist. WASAPI Initialize is normally well under 100 ms.
        match ready_rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(())) => {
                self.sys_kill_tx = Some(kill_tx);
                self.sys_handle = Some(handle);
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = handle.join();
                Err(Error::new(
                    std::io::ErrorKind::Other,
                    format!("system audio capture failed to start ({e})"),
                )
                .into())
            }
            Err(_) => {
                // Do NOT join here. The thread is stalled inside a COM call by
                // definition - that is why the timeout fired - so joining it would
                // block the caller unboundedly while it holds the recording-manager
                // locks, which is exactly what the timeout exists to prevent.
                // Dropping the kill sender lets the thread tear its own stream down
                // and exit if the call ever returns; it owns everything it touches.
                drop(kill_tx);
                self.sys_errored.store(true, Ordering::Release);
                Err(Error::new(
                    std::io::ErrorKind::TimedOut,
                    "system audio capture did not start within 3s",
                )
                .into())
            }
        }
    }

    /// Tear down the system-audio leg. Safe to call when it was never opened.
    pub fn close_system_audio(&mut self) {
        drop(self.sys_kill_tx.take());
        if let Some(h) = self.sys_handle.take() {
            let _ = h.join();
        }
        if let Ok(mut r) = self.sys_ring.lock() {
            r.clear();
        }
    }

    /// Set the linear gain applied to the system leg before mixing. Must be called
    /// before `open()`, since the consumer captures it at spawn time.
    pub fn set_system_audio_gain(&mut self, gain: f32) {
        self.sys_gain = gain;
    }

    /// Set how far the system leg lags the microphone, in milliseconds. Must be
    /// called before `open_system_audio()`, which is what sizes the ring.
    pub fn set_system_audio_delay_ms(&mut self, delay_ms: i32) {
        self.sys_delay_ms = delay_ms;
    }

    /// The shared system-audio ring, for the consumer to pull from.
    pub fn system_ring(&self) -> Arc<Mutex<SlaveRing>> {
        Arc::clone(&self.sys_ring)
    }

    /// True when the system-audio leg failed to arm or has faulted.
    pub fn system_audio_faulted(&self) -> bool {
        self.sys_errored.load(Ordering::Acquire)
    }

    /// Open a capture stream. `role` says whether `device` is an ordinary input
    /// (microphone) or an output endpoint to be captured via loopback (system audio);
    /// the two negotiate their config completely differently.
    pub fn open(
        &mut self,
        device: Option<Device>,
        role: EndpointRole,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.worker_handle.is_some() {
            return Ok(()); // already open
        }

        // T-113 (finding 9): timestamp `open()` itself — the caller
        // (`start_microphone_stream`) only times the SYNCHRONOUS portion up
        // to the worker-thread spawn (near-instant: it's just a channel setup
        // + `thread::spawn` dispatch), which made "mic-ready" inaccurate —
        // the worker hadn't done any device negotiation yet. The two new log
        // lines below (config negotiated, stream playing) measure from THIS
        // point, decomposing on-demand mic-open latency into stages that
        // were previously invisible.
        let open_start = Instant::now();

        let (sample_tx, sample_rx) = mpsc::channel::<Vec<f32>>();
        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();

        let host = crate::audio_toolkit::get_cpal_host();
        let device = match device {
            Some(dev) => dev,
            None => host
                .default_input_device()
                .ok_or_else(|| Error::new(std::io::ErrorKind::NotFound, "No input device found"))?,
        };

        let thread_device = device.clone();
        let vad = self.vad.clone();
        // Move the optional level callback into the worker thread
        let level_cb = self.level_cb.clone();
        let warmup_cb = self.warmup_cb.clone();
        let warmup_ms = Arc::clone(&self.warmup_ms);
        let quiet_hint = Arc::clone(&self.quiet_hint);
        let segment_cb = self.segment_cb.clone();
        let closed_chunk_cb = self.closed_chunk_cb.clone();

        // T-113 (finding 9, audio-thread discipline): the CPAL data callback
        // must stay trivial — no logging/formatting/allocation on the audio
        // thread (CLAUDE.md's audio-callback discipline). It only stores a
        // flag + elapsed-nanos-since-`stream_start` into these atomics; the
        // CONSUMER thread (`run_consumer`, below) does the actual one-shot
        // `debug!` log after observing the flag. Previously the callback
        // called `log::debug!` directly on its first invocation, which
        // violated that discipline.
        let first_buffer_seen = Arc::new(AtomicBool::new(false));
        let first_buffer_nanos = Arc::new(AtomicU64::new(0));
        let first_buffer_seen_cb = Arc::clone(&first_buffer_seen);
        let first_buffer_nanos_cb = Arc::clone(&first_buffer_nanos);
        let stream_errored_cb = Arc::clone(&self.stream_errored);
        let arm_errored_worker = Arc::clone(&self.arm_errored);
        // A fresh open starts healthy - clear any fault from a previous stream, and
        // any arm failure from a previous open.
        self.stream_errored.store(false, Ordering::Relaxed);
        self.arm_errored.store(false, Ordering::Relaxed);

        // Only pass the ring when a system leg is actually open: `None` makes
        // apply_system_mix a no-op and leaves the microphone path untouched.
        let sys_ring_for_consumer = if self.sys_kill_tx.is_some() {
            Some(Arc::clone(&self.sys_ring))
        } else {
            None
        };
        let sys_gain = self.sys_gain;

        // The worker reports whether the stream actually started, so a device that
        // refuses to open (Windows microphone privacy switched off answers
        // E_ACCESSDENIED) fails the take instead of "recording" silence.
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

        let worker = std::thread::spawn(move || {
            // Cargo.toml sets panic = "abort", so ANY panic on this worker kills the
            // whole process with no log line. A render endpoint reaches this path on
            // its first call (it advertises no input configs - probe-verified), so a
            // negotiation failure has to be an error the caller can observe.
            let config = match AudioRecorder::get_preferred_config_for(&thread_device, role) {
                Ok(c) => c,
                Err(e) => {
                    log::error!("Capture device config negotiation failed: {e}");
                    arm_errored_worker.store(true, Ordering::Release);
                    let _ = ready_tx.send(Err(e.to_string()));
                    return;
                }
            };
            let config_negotiated_in = open_start.elapsed();
            log::debug!(
                "T-113: recorder worker config negotiated {:?} after open() was called",
                config_negotiated_in
            );

            let sample_rate = config.sample_rate().0;
            let channels = config.channels() as usize;

            log::info!(
                "Using device: {:?}\nSample rate: {}\nChannels: {}\nFormat: {:?}",
                thread_device.name(),
                sample_rate,
                channels,
                config.sample_format()
            );

            // T-113: one-shot timestamp for stream-start→first-CPAL-buffer,
            // decoded by run_consumer from the atomics above. Combined with
            // start_microphone_stream()'s own "Microphone stream initialized
            // in {:?}" (which covers device negotiation up through here),
            // this decomposes the on-demand mic-open latency into "getting
            // the device ready" vs "device ready but CPAL hasn't delivered
            // audio yet".
            let stream_start = Instant::now();
            let stream = match config.sample_format() {
                cpal::SampleFormat::U8 => AudioRecorder::build_stream::<u8>(
                    &thread_device,
                    &config,
                    sample_tx,
                    channels,
                    stream_start,
                    Arc::clone(&first_buffer_seen_cb),
                    Arc::clone(&first_buffer_nanos_cb),
                    Arc::clone(&stream_errored_cb),
                    role,
                ),
                cpal::SampleFormat::I8 => AudioRecorder::build_stream::<i8>(
                    &thread_device,
                    &config,
                    sample_tx,
                    channels,
                    stream_start,
                    Arc::clone(&first_buffer_seen_cb),
                    Arc::clone(&first_buffer_nanos_cb),
                    Arc::clone(&stream_errored_cb),
                    role,
                ),
                cpal::SampleFormat::I16 => AudioRecorder::build_stream::<i16>(
                    &thread_device,
                    &config,
                    sample_tx,
                    channels,
                    stream_start,
                    Arc::clone(&first_buffer_seen_cb),
                    Arc::clone(&first_buffer_nanos_cb),
                    Arc::clone(&stream_errored_cb),
                    role,
                ),
                cpal::SampleFormat::I32 => AudioRecorder::build_stream::<i32>(
                    &thread_device,
                    &config,
                    sample_tx,
                    channels,
                    stream_start,
                    Arc::clone(&first_buffer_seen_cb),
                    Arc::clone(&first_buffer_nanos_cb),
                    Arc::clone(&stream_errored_cb),
                    role,
                ),
                cpal::SampleFormat::F32 => AudioRecorder::build_stream::<f32>(
                    &thread_device,
                    &config,
                    sample_tx,
                    channels,
                    stream_start,
                    Arc::clone(&first_buffer_seen_cb),
                    Arc::clone(&first_buffer_nanos_cb),
                    Arc::clone(&stream_errored_cb),
                    role,
                ),
                other => {
                    // panic = "abort" (Cargo.toml:143) makes any panic here a silent
                    // whole-process death, and a loopback MASTER reaches this path in
                    // system-audio-only mode.
                    log::error!("Unsupported capture sample format: {other:?}");
                    arm_errored_worker.store(true, Ordering::Release);
                    let _ = ready_tx.send(Err(format!("unsupported sample format {other:?}")));
                    return;
                }
            };
            let stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Capture stream construction failed: {e}");
                    arm_errored_worker.store(true, Ordering::Release);
                    let _ = ready_tx.send(Err(e.to_string()));
                    return;
                }
            };

            if let Err(e) = stream.play() {
                log::error!("Capture stream failed to start: {e}");
                arm_errored_worker.store(true, Ordering::Release);
                let _ = ready_tx.send(Err(e.to_string()));
                return;
            }
            let _ = ready_tx.send(Ok(()));
            let stream_playing_in = open_start.elapsed();
            log::debug!(
                "T-113: recorder worker ready (stream playing) {:?} after open() was called",
                stream_playing_in
            );

            // keep the stream alive while we process samples
            run_consumer(
                &stream,
                sample_rate,
                vad,
                sample_rx,
                cmd_rx,
                level_cb,
                warmup_cb,
                warmup_ms,
                quiet_hint,
                segment_cb,
                closed_chunk_cb,
                first_buffer_seen,
                first_buffer_nanos,
                config_negotiated_in,
                stream_playing_in,
                sys_ring_for_consumer,
                sys_gain,
            );
            // stream is dropped here, after run_consumer returns
        });

        // Bounded wait for the stream to start (normally well under 300 ms, even
        // from a cold device). A timeout is NOT a failure: a Bluetooth headset
        // switching profile can take seconds and then records normally.
        match ready_rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                // The worker has already returned; nothing is left running.
                let _ = worker.join();
                return Err(Error::new(stream_error_kind(&e), e).into());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                log::warn!("Capture stream did not report ready within 3s; continuing");
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = worker.join();
                return Err(Error::new(
                    std::io::ErrorKind::Other,
                    "capture worker exited before the stream started",
                )
                .into());
            }
        }

        self.device = Some(device);
        self.cmd_tx = Some(cmd_tx);
        self.worker_handle = Some(worker);

        Ok(())
    }

    /// Begin recording. If `params` is `Some`, the recording is also streamed to
    /// crash-safe Opus chunk files in `params.dir` (see [`OpusChunkWriter`]).
    pub fn start(&self, params: Option<StartParams>) -> Result<(), Box<dyn std::error::Error>> {
        // A recorder with no `cmd_tx` was never opened, or has been closed. Sending
        // nothing and returning Ok() reports a recording that does not exist:
        // `try_start_recording` reads this as success, the overlay says "recording",
        // and the take comes back empty. Fail loudly instead.
        let tx = self
            .cmd_tx
            .as_ref()
            .ok_or_else(|| Error::new(std::io::ErrorKind::NotConnected, "recorder is not open"))?;
        tx.send(Cmd::Start(params))?;
        Ok(())
    }

    pub fn stop(&self) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        // Check BEFORE creating the channel. Previously `resp_tx` was constructed
        // first, so when `cmd_tx` was `None` nothing consumed it, the sender stayed
        // alive in this scope, and `resp_rx.recv()` blocked the calling thread
        // forever - a hard hang, not an empty result.
        let tx = self
            .cmd_tx
            .as_ref()
            .ok_or_else(|| Error::new(std::io::ErrorKind::NotConnected, "recorder is not open"))?;
        let (resp_tx, resp_rx) = mpsc::channel();
        tx.send(Cmd::Stop(resp_tx))?;
        Ok(resp_rx.recv()?) // wait for the samples (and for chunk gluing to finish)
    }

    /// Stop recording and discard all audio + chunk files for this take.
    pub fn cancel(&self) -> Result<(), Box<dyn std::error::Error>> {
        let tx = self
            .cmd_tx
            .as_ref()
            .ok_or_else(|| Error::new(std::io::ErrorKind::NotConnected, "recorder is not open"))?;
        tx.send(Cmd::Cancel)?;
        Ok(())
    }

    fn send(&self, cmd: Cmd) -> Result<(), Box<dyn std::error::Error>> {
        let tx = self
            .cmd_tx
            .as_ref()
            .ok_or_else(|| Error::new(std::io::ErrorKind::NotConnected, "recorder is not open"))?;
        tx.send(cmd)?;
        Ok(())
    }

    /// Stop taking in audio without ending the take.
    pub fn pause(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.send(Cmd::Pause)
    }

    /// Continue a paused take.
    pub fn resume(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.send(Cmd::Resume)
    }

    /// Remove the take's kept samples `from..to`; returns once they are cut.
    pub fn cut(&self, from: usize, to: usize) -> Result<(), Box<dyn std::error::Error>> {
        let (tx, rx) = mpsc::channel();
        self.send(Cmd::Cut(from, to, tx))?;
        rx.recv_timeout(Duration::from_secs(1))?;
        Ok(())
    }

    /// A copy of the take's kept audio so far.
    pub fn snapshot(&self) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        let (tx, rx) = mpsc::channel();
        self.send(Cmd::Snapshot(tx))?;
        Ok(rx.recv_timeout(Duration::from_secs(1))?)
    }

    /// True when the CPAL error callback has reported a fault on the current
    /// stream. Consult this before reusing an open recorder.
    pub fn stream_faulted(&self) -> bool {
        // The SLAVE counts too. Without it, unplugging the playback endpoint in mixed
        // mode leaves a dead loopback leg attached: the microphone stays healthy, the
        // reuse check says "fine", and every later take records only the user's own
        // voice while the UI still reports system audio is being captured.
        self.stream_errored.load(Ordering::Relaxed)
            || self.arm_errored.load(Ordering::Acquire)
            || self.sys_errored.load(Ordering::Acquire)
    }

    pub fn close(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Tear the system-audio leg down first: it is independent of the consumer
        // thread, and leaving it running would hold the endpoint open forever.
        self.close_system_audio();
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(Cmd::Shutdown);
        }
        if let Some(h) = self.worker_handle.take() {
            let _ = h.join();
        }
        self.device = None;
        Ok(())
    }

    fn build_stream<T>(
        device: &cpal::Device,
        config: &cpal::SupportedStreamConfig,
        sample_tx: mpsc::Sender<Vec<f32>>,
        channels: usize,
        stream_start: Instant,
        first_buffer_seen: Arc<AtomicBool>,
        first_buffer_nanos: Arc<AtomicU64>,
        stream_errored: Arc<AtomicBool>,
        role: EndpointRole,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: Sample + SizedSample + Send + 'static,
        f32: cpal::FromSample<T>,
    {
        // A render endpoint opened as the MASTER (system-audio-only mode) must fold
        // the same way the mixed slave does. The plain mean puts a 5.1 endpoint
        // carrying its meeting in FL/FR about 9.5 dB down - quiet enough for the VAD
        // to discard the far end as noise rather than merely sound faint.
        let downmix = match role {
            EndpointRole::RenderLoopback => Downmix::FrontPair,
            EndpointRole::Capture => Downmix::Mean,
        };
        let mut output_buffer = Vec::new();

        let stream_cb = move |data: &[T], _: &cpal::InputCallbackInfo| {
            // T-113 (finding 9): the audio callback stays trivial — two
            // atomic stores, nothing else; no logging, formatting, or any
            // other non-trivial work happens here. The CONSUMER thread
            // (`run_consumer`) emits the actual one-shot `debug!` after
            // observing `first_buffer_seen`.
            //
            // Adversarial-review finding 9: the timestamp is stored BEFORE
            // the flag is published, and the publish/observe pair uses
            // Release/Acquire rather than Relaxed on both sides. With
            // Relaxed on both, there is no happens-before relationship
            // between the two independent atomics, so the consumer thread
            // could observe `first_buffer_seen == true` before
            // `first_buffer_nanos`'s store became visible to it and log a
            // bogus 0ns startup latency. This callback is only ever invoked
            // from CPAL's single dedicated audio thread, so a plain
            // load-check + two stores (no `swap`/CAS) is safe — nothing else
            // ever writes these atomics concurrently with this callback.
            if !first_buffer_seen.load(Ordering::Relaxed) {
                first_buffer_nanos
                    .store(stream_start.elapsed().as_nanos() as u64, Ordering::Relaxed);
                first_buffer_seen.store(true, Ordering::Release);
            }
            output_buffer.clear();

            if channels == 1 {
                // Direct conversion without intermediate Vec
                output_buffer.extend(data.iter().map(|&sample| sample.to_sample::<f32>()));
            } else {
                // Convert to mono directly
                let frame_count = data.len() / channels;
                output_buffer.reserve(frame_count);

                let mut scratch: Vec<f32> = Vec::with_capacity(channels);
                for frame in data.chunks_exact(channels) {
                    scratch.clear();
                    scratch.extend(frame.iter().map(|&sample| sample.to_sample::<f32>()));
                    output_buffer.push(downmix.fold(&scratch, channels));
                }
            }

            if sample_tx.send(output_buffer.clone()).is_err() {
                log::error!("Failed to send samples");
            }
        };

        device.build_input_stream(
            &config.clone().into(),
            stream_cb,
            move |err| {
                // Audio-callback discipline: one relaxed store, nothing else.
                stream_errored.store(true, Ordering::Relaxed);
                log::error!("Stream error: {}", err);
            },
            None,
        )
    }

    /// Which side of the audio graph a capture device sits on.
    ///
    /// A WASAPI *render* endpoint opened for input gives loopback capture (cpal ORs
    /// `AUDCLNT_STREAMFLAGS_LOOPBACK` automatically), but it advertises no INPUT
    /// configs at all - probe-verified on real hardware: `supported_input_configs`
    /// returns 0 ranges and `default_input_config()` errors. Its only buildable
    /// config is the endpoint's own output mix format.
    /// Build the loopback capture stream. Its callback does the minimum the repo's
    /// audio-thread discipline allows: fold to mono, resample, push into the ring, and
    /// one relaxed store on error. No logging on the happy path, no allocation beyond
    /// a reusable scratch buffer.
    fn build_system_stream(
        device: &cpal::Device,
        config: &cpal::SupportedStreamConfig,
        ring: Arc<Mutex<SlaveRing>>,
        channels: usize,
        in_rate: u32,
        errored: Arc<AtomicBool>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError> {
        // The loopback leg arrives at the endpoint's mix rate (48 kHz in practice) and
        // must reach the pipeline's 16 kHz; the ring is defined in 16 kHz samples, so
        // the conversion happens here on the owner thread.
        let mut resampler = FrameResampler::new(
            in_rate as usize,
            constants::WHISPER_SAMPLE_RATE as usize,
            Duration::from_millis(30),
        );
        let mut mono: Vec<f32> = Vec::with_capacity(4096);
        let err_cb = Arc::clone(&errored);

        device.build_input_stream(
            &config.clone().into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                mono.clear();
                let ch = channels.max(1);
                for frame in data.chunks_exact(ch) {
                    mono.push(Downmix::FrontPair.fold(frame, ch));
                }
                resampler.push(&mono, |out| {
                    if let Ok(mut r) = ring.lock() {
                        r.push(out);
                    }
                });
            },
            move |e| {
                err_cb.store(true, Ordering::Relaxed);
                log::error!("System audio stream error: {e}");
            },
            None,
        )
    }

    pub fn get_preferred_config_for(
        device: &cpal::Device,
        role: EndpointRole,
    ) -> Result<cpal::SupportedStreamConfig, Box<dyn std::error::Error>> {
        match role {
            EndpointRole::Capture => Self::get_preferred_config(device),
            EndpointRole::RenderLoopback => {
                // Do NOT run the 16 kHz preference scan here: WASAPI shared mode
                // delivers the endpoint's mix format regardless of what is asked for.
                // FrameResampler already handles 48k->16k for ordinary microphones.
                Ok(device.default_output_config()?)
            }
        }
    }

    fn get_preferred_config(
        device: &cpal::Device,
    ) -> Result<cpal::SupportedStreamConfig, Box<dyn std::error::Error>> {
        let supported_configs = device.supported_input_configs()?;
        let mut best_config: Option<cpal::SupportedStreamConfigRange> = None;

        // Try to find a config that supports 16kHz, prioritizing better formats
        for config_range in supported_configs {
            if config_range.min_sample_rate().0 <= constants::WHISPER_SAMPLE_RATE
                && config_range.max_sample_rate().0 >= constants::WHISPER_SAMPLE_RATE
            {
                match best_config {
                    None => best_config = Some(config_range),
                    Some(ref current) => {
                        // Prioritize F32 > I16 > I32 > others
                        let score = |fmt: cpal::SampleFormat| match fmt {
                            cpal::SampleFormat::F32 => 4,
                            cpal::SampleFormat::I16 => 3,
                            cpal::SampleFormat::I32 => 2,
                            _ => 1,
                        };

                        if score(config_range.sample_format()) > score(current.sample_format()) {
                            best_config = Some(config_range);
                        }
                    }
                }
            }
        }

        if let Some(config) = best_config {
            return Ok(config.with_sample_rate(cpal::SampleRate(constants::WHISPER_SAMPLE_RATE)));
        }

        // If no config supports 16kHz, fall back to default
        Ok(device.default_input_config()?)
    }
}

/// Tracks a recording: the Opus file chunk(s) written for storage (cut at
/// ~10 min) and the transcription segments cut at silence (~20-45 s) that are
/// handed to the background transcription pipeline. File chunking and
/// transcription segmenting are fully independent — the segments are what get
/// transcribed on the fly; the file chunks are just durable storage.
struct ChunkState {
    dir: PathBuf,
    ts: u64,
    // --- Opus file storage (~10-min chunks) ---
    /// 1-based index of the current (open) file chunk.
    file_index: usize,
    writer: Option<OpusChunkWriter>,
    file_samples: usize,
    closed_paths: Vec<PathBuf>,
    /// False after a write error (disk full) — recording continues without files.
    enabled: bool,
    // --- transcription segments (on-the-fly) ---
    /// 0-based index of the next transcription segment (for ordered join).
    seg_index: usize,
    seg_pcm: Vec<f32>,
    seg_samples: usize,
}

impl ChunkState {
    fn start(dir: PathBuf, ts: u64) -> Self {
        let mut s = Self {
            dir,
            ts,
            file_index: 0,
            writer: None,
            file_samples: 0,
            closed_paths: Vec::new(),
            enabled: true,
            seg_index: 0,
            seg_pcm: Vec::new(),
            seg_samples: 0,
        };
        s.open_next_file();
        s
    }

    fn chunk_path(&self, index: usize) -> PathBuf {
        self.dir
            .join(format!("handy-{}-chunk-{}.opus", self.ts, index))
    }

    fn full_path(&self) -> PathBuf {
        self.dir.join(format!("handy-{}.opus", self.ts))
    }

    fn open_next_file(&mut self) {
        self.file_index += 1;
        self.file_samples = 0;
        if !self.enabled {
            self.writer = None;
            return;
        }
        let path = self.chunk_path(self.file_index);
        match OpusChunkWriter::create(&path) {
            Ok(w) => self.writer = Some(w),
            Err(e) => {
                log::warn!("Failed to start recording chunk {:?}: {}", path, e);
                self.writer = None;
                self.enabled = false;
            }
        }
    }

    /// Feed a frame of speech: write it to the current Opus file and accumulate
    /// it into the current transcription segment.
    fn push_speech(&mut self, samples: &[f32]) {
        if let Some(w) = self.writer.as_mut() {
            if w.write_frame(samples).is_err() {
                log::warn!("Opus chunk write failed (disk full?); disabling chunk recording");
                self.writer = None;
                self.enabled = false;
            }
        }
        self.file_samples += samples.len();
        self.seg_pcm.extend_from_slice(samples);
        self.seg_samples += samples.len();
    }

    /// Finalize the current Opus file chunk (rename temp→final) and open the
    /// next one. Storage only — does not touch transcription segments.
    fn close_file_chunk(&mut self) {
        if let Some(w) = self.writer.take() {
            match w.finalize() {
                Ok(p) => self.closed_paths.push(p),
                Err(e) => log::warn!("Failed to finalize file chunk {}: {}", self.file_index, e),
            }
        }
        self.open_next_file();
    }

    /// Take the current transcription segment's PCM for background transcription.
    fn take_segment(&mut self) -> ClosedChunk {
        let index = self.seg_index;
        let pcm = std::mem::take(&mut self.seg_pcm);
        self.seg_index += 1;
        self.seg_samples = 0;
        ClosedChunk { index, pcm }
    }

    /// Discard the in-progress file and delete all finalized chunk files for
    /// this take (used on cancel).
    fn discard_all(&mut self) {
        if let Some(w) = self.writer.take() {
            w.discard();
        }
        for p in self.closed_paths.drain(..) {
            let _ = std::fs::remove_file(&p);
        }
    }
}

/// Process one resampled 16 kHz frame: run VAD, then feed every active sink —
/// the full-recording PCM accumulator, the current Opus chunk (if chunked), the
/// live-segment callback (if set), and chunk cuts at silence (if chunked). These
/// are independent: e.g. Live mode with crash-safe recording both emits live
/// segments AND writes Opus chunks.
///
/// NOTE: `out_buf` accumulates the whole recording. A future optimization can
/// skip this in pure-chunked mode (transcription there is per-chunk), bounding
/// memory for very long recordings.
/// Mix the system-audio leg into a master frame, in place.
///
/// Every frame goes through here, including the ones drained at `Cmd::Stop`: keeping
/// it in ONE place is what stops the stop-path quietly delivering unmixed audio.
/// With no system leg this is a no-op and the master frame is untouched - which is
/// what makes "mixed mode with nothing playing is bit-identical to mic-only" true.
fn apply_system_mix(
    frame: &mut Vec<f32>,
    sys_ring: &Option<Arc<Mutex<SlaveRing>>>,
    sys_gain: f32,
    scratch: &mut Vec<f32>,
) {
    let Some(ring) = sys_ring else { return };
    let Ok(mut r) = ring.lock() else { return };
    r.pull(frame.len(), scratch);
    mix_into(frame, scratch, sys_gain);
}

/// The level of `samples` in dBFS (-120 for silence).
fn level_dbfs(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return -120.0;
    }
    let mean_sq = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    10.0 * mean_sq.max(1e-12).log10()
}

/// Notices someone speaking too quietly to be heard: frames that sound a
/// little like a voice (speech probability from QUIET_PROB_MIN, below the
/// detector's own threshold) and stand out from the room's noise floor, but are
/// not kept - for about 0.7 s within 1.5 s. Silence and steady noise sit at the
/// floor; music and typing score far lower as speech.
#[derive(Default)]
struct QuietDetector {
    /// Tracks the quiet frames: follows the level down at once, rises slowly.
    floor_db: f32,
    started: bool,
    recent: VecDeque<bool>,
    near: usize,
    cooldown: usize,
}

impl QuietDetector {
    /// Takes one frame; true when the hint should show now.
    fn observe(&mut self, level_db: f32, kept: bool, prob: f32) -> bool {
        if !self.started || level_db < self.floor_db {
            self.floor_db = level_db;
            self.started = true;
        } else {
            self.floor_db += 0.02; // ~0.7 dB per second
        }
        let near =
            !kept && prob >= QUIET_PROB_MIN && level_db >= self.floor_db + QUIET_ABOVE_FLOOR_DB;
        self.recent.push_back(near);
        self.near += near as usize;
        if self.recent.len() > QUIET_WINDOW_FRAMES {
            self.near -= self.recent.pop_front().unwrap_or(false) as usize;
        }
        if self.cooldown > 0 {
            self.cooldown -= 1;
            return false;
        }
        if self.near >= QUIET_NEEDED_FRAMES {
            self.cooldown = QUIET_COOLDOWN_FRAMES;
            self.recent.clear();
            self.near = 0;
            return true;
        }
        false
    }
}

/// The level (dBFS) of each 50 ms of a cold start's first 3 s.
struct WarmupMeter {
    window: usize,
    sum_sq: f64,
    count: usize,
    levels: Vec<f32>,
}

impl WarmupMeter {
    fn new(sample_rate: u32) -> Self {
        Self {
            window: (sample_rate as usize / 20).max(1),
            sum_sq: 0.0,
            count: 0,
            levels: Vec::with_capacity(WARMUP_MEASURE_WINDOWS),
        }
    }

    /// Takes in audio; returns the levels once, when the 3 s are complete.
    fn push(&mut self, samples: &[f32]) -> Option<Vec<f32>> {
        if self.levels.len() >= WARMUP_MEASURE_WINDOWS {
            return None;
        }
        for &s in samples {
            self.sum_sq += s as f64 * s as f64;
            self.count += 1;
            if self.count == self.window {
                let rms = (self.sum_sq / self.count as f64).sqrt();
                self.levels.push((20.0 * rms.max(1e-6).log10()) as f32);
                self.sum_sq = 0.0;
                self.count = 0;
                if self.levels.len() == WARMUP_MEASURE_WINDOWS {
                    return Some(self.levels.clone());
                }
            }
        }
        None
    }
}

/// How long a cold-started microphone took to reach its normal level, in ms,
/// from the level of each 50 ms since its first audio: from its first sound
/// (digital silence before it is not counted - the pill waits for sound anyway)
/// to the start of the first half-second that stays within 6 dB of the floor it
/// settles at (the quietest fifth of the later half - speech only adds level, so
/// it cannot hide a fade-in). A Realtek input was seen starting 15-20 dB quiet
/// and fading in; the first words spoken into that were dropped as noise.
fn warmup_from_levels(levels: &[f32]) -> u32 {
    let Some(sound) = levels.iter().position(|&l| l > SILENT_DB) else {
        return 0;
    };
    let levels = &levels[sound..];
    let mut later = levels[levels.len() / 2..].to_vec();
    later.sort_by(f32::total_cmp);
    let floor = later[later.len() / 5];
    let settled = (0..levels.len())
        .find(|&i| {
            levels[i..(i + 10).min(levels.len())]
                .iter()
                .all(|&l| l >= floor - 6.0)
        })
        .unwrap_or(levels.len());
    settled as u32 * 50
}

#[allow(clippy::too_many_arguments)]
fn process_frame(
    samples: &[f32],
    recording: bool,
    vad: &Option<Arc<Mutex<Box<dyn vad::VoiceActivityDetector>>>>,
    chunk: &mut Option<ChunkState>,
    out_buf: &mut Vec<f32>,
    segment_start_idx: &mut usize,
    segment_cb: &SegmentCb,
    closed_chunk_cb: &ClosedChunkCb,
) {
    if !recording {
        return;
    }

    // Run VAD and append the speech samples while the lock is held (the speech
    // slice borrows the VAD guard). Defer cut/segment side effects until after
    // the lock is released.
    let speech_ended;
    {
        if let Some(vad_arc) = vad {
            let mut det = vad_arc.lock().unwrap();
            match det.push_frame(samples).unwrap_or(VadFrame::Speech(samples)) {
                VadFrame::Speech(buf) => {
                    out_buf.extend_from_slice(buf);
                    if let Some(chunk) = chunk.as_mut() {
                        chunk.push_speech(buf);
                    }
                }
                VadFrame::Noise => {}
            }
            speech_ended = det.speech_ended();
        } else {
            out_buf.extend_from_slice(samples);
            if let Some(chunk) = chunk.as_mut() {
                chunk.push_speech(samples);
            }
            speech_ended = false;
        }
    }

    if let Some(chunk) = chunk.as_mut() {
        // File storage: cut the .opus file at the first silence after ~10 min
        // (hard cut at 11 min). Storage only — no transcription tied to this.
        let file_cut = (chunk.file_samples >= FILE_SOFT_SAMPLES && speech_ended)
            || chunk.file_samples >= FILE_HARD_SAMPLES;
        if file_cut && chunk.file_samples > 0 {
            chunk.close_file_chunk();
        }

        // Transcription: cut a segment at the first silence after ~20 s of speech
        // (hard cut at ~45 s) and hand it to the background transcription
        // pipeline, so it is transcribed WHILE recording continues. This is what
        // keeps the GPU busy during recording and makes stop near-instant.
        let seg_cut = (chunk.seg_samples >= SEG_SOFT_SAMPLES && speech_ended)
            || chunk.seg_samples >= SEG_HARD_SAMPLES;
        if seg_cut && chunk.seg_samples > 0 {
            let seg = chunk.take_segment();
            if let Some(cb) = closed_chunk_cb.lock().unwrap().as_ref() {
                cb(seg);
            }
        }
    }

    // Live transcription: emit accumulated audio as a segment (no-op if no
    // segment callback is set, i.e. outside Live mode).
    let should_emit = if speech_ended && *segment_start_idx < out_buf.len() {
        true
    } else if *segment_start_idx < out_buf.len() {
        let samples_since_last = out_buf.len() - *segment_start_idx;
        samples_since_last as f32 / 16000.0 >= LIVE_PREVIEW_INTERVAL_SECS
    } else {
        false
    };
    if should_emit {
        if let Some(cb) = segment_cb.lock().unwrap().as_ref() {
            let all_audio = out_buf.clone();
            if !all_audio.is_empty() {
                cb(all_audio, speech_ended);
            }
        }
        *segment_start_idx = out_buf.len();
    }
}

/// Classify a capture-stream start failure. Windows answers E_ACCESSDENIED
/// (0x80070005) when microphone access is switched off in its privacy settings;
/// the message text is localized, the HRESULT is not.
fn stream_error_kind(message: &str) -> std::io::ErrorKind {
    let m = message.to_ascii_lowercase();
    if m.contains("0x80070005") || m.contains("e_accessdenied") {
        std::io::ErrorKind::PermissionDenied
    } else {
        std::io::ErrorKind::Other
    }
}

#[allow(clippy::too_many_arguments)]
fn run_consumer(
    // Only for a long pause: stopped to release the device, started on resume.
    stream: &cpal::Stream,
    in_sample_rate: u32,
    vad: Option<Arc<Mutex<Box<dyn vad::VoiceActivityDetector>>>>,
    sample_rx: mpsc::Receiver<Vec<f32>>,
    cmd_rx: mpsc::Receiver<Cmd>,
    level_cb: Option<LevelCb>,
    warmup_cb: Option<WarmupCb>,
    warmup_ms: Arc<AtomicU32>,
    quiet_hint: Arc<AtomicBool>,
    segment_cb: SegmentCb,
    closed_chunk_cb: ClosedChunkCb,
    // T-113 (finding 9): the audio callback (`build_stream`'s `stream_cb`)
    // only stores into these atomics — trivial, lock-free, no logging. This
    // consumer thread does the actual one-shot `debug!` once it observes the
    // flag, keeping the log call entirely off the audio thread.
    first_buffer_seen: Arc<AtomicBool>,
    first_buffer_nanos: Arc<AtomicU64>,
    // Precomputed on the worker thread, NOT re-measured here: reading
    // `open_start.elapsed()` from this loop would fold in a `process_frame` pass
    // and a loop turn, contaminating the very number this exists to report.
    config_negotiated_in: Duration,
    stream_playing_in: Duration,
    // The system-audio leg, when mixing. `None` for a plain microphone take, which is
    // what keeps the default path byte-identical.
    sys_ring: Option<Arc<Mutex<SlaveRing>>>,
    sys_gain: f32,
) {
    // Reused across every frame so the hot path allocates nothing.
    let mut mix_buf: Vec<f32> = Vec::with_capacity(1024);
    let mut mix_scratch: Vec<f32> = Vec::with_capacity(1024);
    let mut frame_resampler = FrameResampler::new(
        in_sample_rate as usize,
        constants::WHISPER_SAMPLE_RATE as usize,
        Duration::from_millis(30),
    );

    let mut processed_samples = Vec::<f32>::new();
    let mut recording = false;
    // A paused take stays open (`recording` stays true) but takes in no audio.
    let mut paused = false;
    let mut paused_since: Option<Instant> = None;
    // True once a long pause has stopped the device (PAUSE_RELEASES_MIC_AFTER).
    let mut mic_released = false;
    let mut segment_start_idx: usize = 0;
    // When chunked Opus recording is active this is `Some`; otherwise we
    // accumulate full PCM in `processed_samples` (live / crash-safety-off).
    let mut chunk_state: Option<ChunkState> = None;

    // ---------- spectrum visualisation setup ---------------------------- //
    // The pill's sound bars, low to high pitch (16 in the middle of the pill).
    const BUCKETS: usize = 16;
    const WINDOW_SIZE: usize = 512;
    let mut visualizer = AudioVisualiser::new(
        in_sample_rate,
        WINDOW_SIZE,
        BUCKETS,
        400.0,  // vocal_min_hz
        4000.0, // vocal_max_hz
    )
    // A voice has most of its energy low: measured over 12 of this PC's takes,
    // the right half of the bars averaged 0.01-0.03 against 0.23-0.28 on the left.
    // +8 dB per octave above 500 Hz evens that out (0.12-0.29 everywhere), and
    // the quiet moments between words still show nothing.
    .with_tilt(8.0, 500.0);
    // Last time the level callback fired while idle (see LEVEL_IDLE_INTERVAL).
    let mut last_level_emit: Option<Instant> = None;
    // T-113 (finding 9): one-shot — only the FIRST loop iteration after the
    // audio callback flags `first_buffer_seen` logs the stream-start→
    // first-buffer latency; every later iteration is steady-state and not
    // interesting for start-latency.
    let mut first_buffer_logged = false;
    // Known at the first audio: whether this was a cold start (COLD_START_MIN).
    // Only a cold start is measured for its warm-up, and only its levels say
    // "not live yet": while it still sends digital silence, and then until its
    // expected fade-in has passed since its first sound.
    let mut cold_start: Option<bool> = None;
    let mut first_audio_at: Option<Instant> = None;
    let mut first_sound_at: Option<Instant> = None;
    let mut warmup_meter = WarmupMeter::new(in_sample_rate);
    // The "Too quiet" hint: near-speech the detector drops, and until when the
    // pill says so.
    let mut quiet = QuietDetector::default();
    let mut quiet_until: Option<Instant> = None;

    loop {
        // Acquire pairs with the callback's Release publish (finding 9): this
        // guarantees the `first_buffer_nanos` store above is visible here
        // once the flag reads true, never a stale/default 0ns.
        if !first_buffer_logged && first_buffer_seen.load(Ordering::Acquire) {
            first_buffer_logged = true;
            let to_first_buffer = Duration::from_nanos(first_buffer_nanos.load(Ordering::Relaxed));
            // INFO, not debug: release builds log at INFO, so a debug! line here can
            // never reach the users whose capture latency we are trying to measure.
            // One line, once per open, with every phase broken out.
            log::info!(
                "capture-latency: open->config {:?} | config->playing {:?} | playing->first-buffer {:?} | TOTAL open->audio {:?}",
                config_negotiated_in,
                stream_playing_in.saturating_sub(config_negotiated_in),
                to_first_buffer,
                stream_playing_in + to_first_buffer
            );
        }

        // Bounded wait: commands (Stop/Cancel/Shutdown) must be processed even
        // when NO audio is flowing (e.g. the input stream died mid-recording) —
        // a blocking recv here would make stop_recording() hang forever.
        let raw = match sample_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(s) => Some(s),
            Err(mpsc::RecvTimeoutError::Timeout) => None, // fall through to commands
            Err(mpsc::RecvTimeoutError::Disconnected) => break, // stream closed
        };

        if let Some(raw) = raw {
            // ---------- cold-start warm-up ------------------------------- //
            let first_at = *first_audio_at.get_or_insert_with(Instant::now);
            let cold = *cold_start.get_or_insert_with(|| {
                let to_first = Duration::from_nanos(first_buffer_nanos.load(Ordering::Acquire));
                stream_playing_in + to_first >= COLD_START_MIN
            });
            if cold {
                if let Some(levels) = warmup_meter.push(&raw) {
                    let warmup = warmup_from_levels(&levels);
                    let shown: Vec<String> = levels.iter().map(|l| format!("{l:.0}")).collect();
                    log::info!(
                        "Microphone warm-up after a cold start: {warmup} ms (dBFS per 50 ms: {})",
                        shown.join(" ")
                    );
                    if let Some(cb) = &warmup_cb {
                        cb(warmup);
                    }
                }
            }
            if first_sound_at.is_none() && raw.iter().any(|s| s.abs() > SILENT_SAMPLE) {
                first_sound_at = Some(Instant::now());
            }
            let live = !cold
                || first_sound_at.is_some_and(|at| {
                    at.elapsed() >= Duration::from_millis(warmup_ms.load(Ordering::Relaxed) as u64)
                })
                || first_at.elapsed() >= SILENT_GIVE_UP;

            // ---------- spectrum processing ------------------------------ //
            if let Some(buckets) = visualizer.feed(&raw) {
                if let Some(cb) = &level_cb {
                    // Full rate while recording; throttled while idle (the
                    // always-on mic would otherwise flood the event system).
                    let now = Instant::now();
                    if (recording && !paused)
                        || last_level_emit
                            .map_or(true, |t| now.duration_since(t) >= LEVEL_IDLE_INTERVAL)
                    {
                        last_level_emit = Some(now);
                        let too_quiet = quiet_until.is_some_and(|until| now < until);
                        cb(buckets, MicState { live, too_quiet });
                    }
                }
            }

            // ---------- pipeline ----------------------------------------- //
            let watch_quiet = recording && !paused && quiet_hint.load(Ordering::Relaxed);
            frame_resampler.push(&raw, &mut |frame: &[f32]| {
                mix_buf.clear();
                mix_buf.extend_from_slice(frame);
                apply_system_mix(&mut mix_buf, &sys_ring, sys_gain, &mut mix_scratch);
                let kept_before = processed_samples.len();
                process_frame(
                    &mix_buf,
                    recording && !paused,
                    &vad,
                    &mut chunk_state,
                    &mut processed_samples,
                    &mut segment_start_idx,
                    &segment_cb,
                    &closed_chunk_cb,
                );
                if watch_quiet {
                    let kept = processed_samples.len() > kept_before;
                    let prob = vad
                        .as_ref()
                        .and_then(|v| v.lock().ok().and_then(|d| d.last_probability()));
                    if let Some(prob) = prob {
                        if quiet.observe(level_dbfs(&mix_buf), kept, prob) {
                            quiet_until = Some(Instant::now() + QUIET_HINT_SHOWN);
                            log::info!(
                                "Too quiet: voice-like sound the speech detector is not keeping (floor {:.0} dBFS)",
                                quiet.floor_db
                            );
                        }
                    }
                }
            });
        }

        // A long pause releases the device, so the system mic indicator goes out.
        if let Some(since) = paused_since {
            if !mic_released && since.elapsed() >= PAUSE_RELEASES_MIC_AFTER {
                match stream.pause() {
                    Ok(()) => {
                        mic_released = true;
                        log::info!("Take paused for 10 min: microphone released until resumed");
                    }
                    Err(e) => log::warn!("Could not release the microphone during a pause: {e}"),
                }
            }
        }

        // non-blocking check for a command
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                Cmd::Start(params) => {
                    // The slave ring is a ~100ms delay line that the loopback leg has
                    // been filling since it armed - i.e. since BEFORE the key was
                    // pressed. Without this clear every mixed take opens with up to two
                    // seconds of audio captured before the press: a transcript bug and a
                    // privacy regression at once. The master's own resampler is reset
                    // just below for exactly the same reason.
                    if let Some(ring) = sys_ring.as_ref() {
                        if let Ok(mut r) = ring.lock() {
                            r.clear();
                        }
                    }
                    processed_samples.clear();
                    segment_start_idx = 0;
                    recording = true;
                    paused = false;
                    paused_since = None;
                    quiet = QuietDetector::default();
                    quiet_until = None;
                    if mic_released {
                        let _ = stream.play();
                        mic_released = false;
                    }
                    visualizer.reset(); // Reset visualization buffer
                    // Drop pre-press audio buffered in the resampler (always-on
                    // mic feeds it continuously) — the take starts at the press.
                    frame_resampler.reset();
                    if let Some(v) = &vad {
                        v.lock().unwrap().reset();
                    }
                    chunk_state = params.map(|p| ChunkState::start(p.dir, p.ts));
                }
                Cmd::Stop(reply_tx) => {
                    recording = false;
                    // Audio queued while the take was paused is not part of it.
                    let drain = !paused;
                    paused = false;
                    paused_since = None;
                    if mic_released {
                        let _ = stream.play();
                        mic_released = false;
                    }

                    // Drain any audio chunks captured but not yet consumed.
                    while let Ok(remaining) = sample_rx.try_recv() {
                        frame_resampler.push(&remaining, &mut |frame: &[f32]| {
                            mix_buf.clear();
                            mix_buf.extend_from_slice(frame);
                            apply_system_mix(&mut mix_buf, &sys_ring, sys_gain, &mut mix_scratch);
                            process_frame(
                                &mix_buf,
                                drain,
                                &vad,
                                &mut chunk_state,
                                &mut processed_samples,
                                &mut segment_start_idx,
                                &segment_cb,
                                &closed_chunk_cb,
                            )
                        });
                    }
                    frame_resampler.finish(&mut |frame: &[f32]| {
                        mix_buf.clear();
                        mix_buf.extend_from_slice(frame);
                        apply_system_mix(&mut mix_buf, &sys_ring, sys_gain, &mut mix_scratch);
                        process_frame(
                            &mix_buf,
                            drain,
                            &vad,
                            &mut chunk_state,
                            &mut processed_samples,
                            &mut segment_start_idx,
                            &segment_cb,
                            &closed_chunk_cb,
                        )
                    });

                    // Flush the slave leg's own tail. The ring is a delay line, so
                    // when the master stops it still holds far-end audio captured
                    // during the take that no master frame will ever pull. It is mixed
                    // against silence (the mic really has stopped) and goes through the
                    // VAD like any other frame, so a sentence the other participant was
                    // still finishing is not truncated.
                    if let Some(ring) = sys_ring.as_ref() {
                        let tail = ring.lock().ok().map(|mut r| r.drain()).unwrap_or_default();
                        if !tail.is_empty() {
                            mix_buf.clear();
                            mix_buf.resize(tail.len(), 0.0);
                            mix_into(&mut mix_buf, &tail, sys_gain);
                            process_frame(
                                &mix_buf,
                                drain,
                                &vad,
                                &mut chunk_state,
                                &mut processed_samples,
                                &mut segment_start_idx,
                                &segment_cb,
                                &closed_chunk_cb,
                            );
                        }
                    }

                    // Flush audio the VAD is still holding back (voiced frames in
                    // an unconfirmed onset) so a trailing word isn't dropped.
                    if let Some(vad_arc) = &vad {
                        if let Some(tail) = vad_arc.lock().unwrap().flush() {
                            processed_samples.extend_from_slice(&tail);
                            if let Some(chunk) = chunk_state.as_mut() {
                                chunk.push_speech(&tail);
                            }
                        }
                    }

                    // Hand off the final transcription segment (the tail speech),
                    // so the last bit transcribes too.
                    if let Some(chunk) = chunk_state.as_mut() {
                        if chunk.seg_samples > 0 {
                            let seg = chunk.take_segment();
                            if let Some(cb) = closed_chunk_cb.lock().unwrap().as_ref() {
                                cb(seg);
                            }
                        }
                    }

                    // Finalize the Opus file storage + produce the full file.
                    if let Some(mut chunk) = chunk_state.take() {
                        if chunk.file_samples > 0 {
                            // Finalize the current file (this also opens a fresh,
                            // empty writer, discarded just below).
                            chunk.close_file_chunk();
                        }
                        if let Some(w) = chunk.writer.take() {
                            // Drop the empty trailing writer's temp file.
                            w.discard();
                        }
                        let full = chunk.full_path();
                        if chunk.closed_paths.len() == 1 {
                            // Recording under ~10 min never hit a file cut: the
                            // single chunk IS the recording — rename it to the full
                            // name so there's no redundant `-chunk-1` file.
                            if let Err(e) = std::fs::rename(&chunk.closed_paths[0], &full) {
                                log::warn!(
                                    "Failed to finalize single-chunk recording {:?}: {}",
                                    full,
                                    e
                                );
                            }
                        } else if !chunk.closed_paths.is_empty() {
                            // Multi-chunk (>10 min): keep the chunk files and glue
                            // a full copy alongside them.
                            if let Err(e) = glue_chunks(&chunk.closed_paths, &full) {
                                log::warn!("Failed to glue chunks into {:?}: {}", full, e);
                            }
                        }
                    }

                    let _ = reply_tx.send(std::mem::take(&mut processed_samples));
                }
                Cmd::Cancel => {
                    recording = false;
                    paused = false;
                    paused_since = None;
                    if mic_released {
                        let _ = stream.play();
                        mic_released = false;
                    }
                    if let Some(mut chunk) = chunk_state.take() {
                        chunk.discard_all();
                    }
                    // Same reason as Cmd::Start: otherwise a cancelled take's far-end
                    // audio leaks into whatever take comes next.
                    if let Some(ring) = sys_ring.as_ref() {
                        if let Ok(mut r) = ring.lock() {
                            r.clear();
                        }
                    }
                    processed_samples.clear();
                    segment_start_idx = 0;
                }
                Cmd::Pause => {
                    if recording && !paused {
                        paused = true;
                        paused_since = Some(Instant::now());
                        // Keep what the VAD was still holding back, so the last word
                        // before the pause is not lost...
                        if let Some(vad_arc) = &vad {
                            if let Some(tail) = vad_arc.lock().unwrap().flush() {
                                processed_samples.extend_from_slice(&tail);
                                if let Some(chunk) = chunk_state.as_mut() {
                                    chunk.push_speech(&tail);
                                }
                            }
                        }
                        // ...and let a live preview settle on it: the utterance ended.
                        if segment_start_idx < processed_samples.len() {
                            if let Some(cb) = segment_cb.lock().unwrap().as_ref() {
                                cb(processed_samples.clone(), true);
                            }
                            segment_start_idx = processed_samples.len();
                        }
                    }
                }
                Cmd::Resume => {
                    if paused {
                        paused = false;
                        paused_since = None;
                        if mic_released {
                            if let Err(e) = stream.play() {
                                log::warn!("Could not restart the microphone after a pause: {e}");
                            }
                            mic_released = false;
                        }
                        // Continue as a fresh start of speech, like Cmd::Start: nothing
                        // buffered during the pause may leak into the take.
                        if let Some(ring) = sys_ring.as_ref() {
                            if let Ok(mut r) = ring.lock() {
                                r.clear();
                            }
                        }
                        frame_resampler.reset();
                        visualizer.reset();
                        if let Some(v) = &vad {
                            v.lock().unwrap().reset();
                        }
                    }
                }
                Cmd::Cut(from, to, done) => {
                    cut_kept(&mut processed_samples, &mut segment_start_idx, from, to);
                    let _ = done.send(());
                }
                Cmd::Snapshot(reply) => {
                    let _ = reply.send(processed_samples.clone());
                }
                Cmd::Shutdown => return,
            }
        }
    }
}

/// Remove `from..to` from the kept audio (undo last word): what was recorded
/// after `to` moves up to `from`, and the next live snapshot reads from there.
fn cut_kept(samples: &mut Vec<f32>, segment_start: &mut usize, from: usize, to: usize) {
    let to = to.min(samples.len());
    if from < to {
        samples.drain(from..to);
        *segment_start = (*segment_start).min(from);
    }
}

#[cfg(test)]
mod cut_tests {
    use super::cut_kept;

    #[test]
    fn a_cut_keeps_what_was_recorded_after_it() {
        let mut samples: Vec<f32> = (0..10).map(|i| i as f32).collect();
        let mut segment_start = 8;
        cut_kept(&mut samples, &mut segment_start, 3, 6);
        assert_eq!(samples, vec![0.0, 1.0, 2.0, 6.0, 7.0, 8.0, 9.0]);
        assert_eq!(segment_start, 3);
    }

    #[test]
    fn a_cut_past_the_end_or_empty_is_clamped() {
        let mut samples: Vec<f32> = (0..5).map(|i| i as f32).collect();
        let mut segment_start = 5;
        cut_kept(&mut samples, &mut segment_start, 3, 99);
        assert_eq!(samples, vec![0.0, 1.0, 2.0]);
        assert_eq!(segment_start, 3);
        cut_kept(&mut samples, &mut segment_start, 2, 2);
        cut_kept(&mut samples, &mut segment_start, 7, 9);
        assert_eq!(samples, vec![0.0, 1.0, 2.0]);
    }
}

#[cfg(test)]
mod quiet_tests {
    use super::{QUIET_COOLDOWN_FRAMES, QuietDetector};

    /// Feeds `n` frames; returns on which frame (1-based) the hint fired.
    fn feed(d: &mut QuietDetector, n: usize, level: f32, kept: bool, prob: f32) -> Option<usize> {
        (1..=n).find(|_| d.observe(level, kept, prob))
    }

    #[test]
    fn quiet_speech_above_the_room_noise_is_noticed() {
        let mut d = QuietDetector::default();
        assert_eq!(feed(&mut d, 30, -62.0, false, 0.02), None); // room noise
        assert_eq!(feed(&mut d, 40, -45.0, false, 0.15), Some(23)); // ~0.7 s of near-speech
    }

    #[test]
    fn noise_speech_that_is_kept_and_unvoiced_sound_are_not() {
        let mut d = QuietDetector::default();
        assert_eq!(feed(&mut d, 30, -62.0, false, 0.02), None);
        assert_eq!(feed(&mut d, 100, -25.0, true, 0.9), None); // normal speech, kept
        assert_eq!(feed(&mut d, 100, -40.0, false, 0.01), None); // typing / music
        assert_eq!(feed(&mut d, 100, -61.0, false, 0.2), None); // at the noise floor
    }

    #[test]
    fn it_waits_before_saying_it_again() {
        let mut d = QuietDetector::default();
        feed(&mut d, 30, -62.0, false, 0.02);
        assert!(feed(&mut d, 40, -45.0, false, 0.15).is_some());
        assert_eq!(
            feed(&mut d, QUIET_COOLDOWN_FRAMES, -45.0, false, 0.15),
            None
        );
        assert!(feed(&mut d, 40, -45.0, false, 0.15).is_some());
    }
}

#[cfg(test)]
mod warmup_tests {
    use super::{WARMUP_MEASURE_WINDOWS, WarmupMeter, warmup_from_levels};

    #[test]
    fn a_steady_microphone_needs_no_warm_up() {
        assert_eq!(warmup_from_levels(&[-60.0; 60]), 0);
    }

    #[test]
    fn a_fade_in_is_measured_until_the_level_settles() {
        // 0.5 s near-silent, then rising, settled from 0.7 s (like a cold Realtek start).
        let mut levels = vec![-77.0; 10];
        levels.extend([-70.0, -68.0, -63.0, -61.0]);
        levels.extend(vec![-60.0; 46]);
        assert_eq!(warmup_from_levels(&levels), 600);
    }

    #[test]
    fn speech_after_the_fade_in_does_not_hide_it() {
        let mut levels = vec![-78.0; 16];
        for i in 0..44 {
            levels.push(if i % 4 < 2 { -20.0 } else { -60.0 });
        }
        assert_eq!(warmup_from_levels(&levels), 800);
    }

    #[test]
    fn digital_silence_before_the_first_sound_is_not_counted() {
        // This PC's Realtek input after a replug (log, 2026-09-27): 0.5 s of exact
        // zeros, 0.5 s at -74 dBFS, then the room at about -62, then speech.
        let mut levels = vec![-120.0; 10];
        levels.extend(vec![-74.0; 10]);
        levels.extend(vec![-62.0; 30]);
        levels.extend([
            -13.0, -17.0, -38.0, -21.0, -14.0, -16.0, -21.0, -26.0, -41.0, -30.0,
        ]);
        assert_eq!(warmup_from_levels(&levels), 500);
    }

    #[test]
    fn a_microphone_that_stays_silent_is_not_measured() {
        assert_eq!(warmup_from_levels(&[-120.0; 60]), 0);
    }

    #[test]
    fn the_meter_reports_once_after_three_seconds() {
        let mut meter = WarmupMeter::new(16_000);
        let second = vec![0.001f32; 16_000];
        assert!(meter.push(&second).is_none());
        assert!(meter.push(&second).is_none());
        let levels = meter.push(&second).expect("3 s measured");
        assert_eq!(levels.len(), WARMUP_MEASURE_WINDOWS);
        assert!((levels[0] + 60.0).abs() < 0.1);
        assert!(meter.push(&second).is_none());
    }
}

#[cfg(test)]
mod stream_error_tests {
    use super::stream_error_kind;
    use std::io::ErrorKind;

    #[test]
    fn access_denied_hresult_means_permission_denied_in_any_language() {
        assert_eq!(
            stream_error_kind("Access is denied. (0x80070005)"),
            ErrorKind::PermissionDenied
        );
        assert_eq!(
            stream_error_kind("Odmowa dostępu. (0x80070005)"),
            ErrorKind::PermissionDenied
        );
        assert_eq!(
            stream_error_kind("E_ACCESSDENIED"),
            ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn other_stream_errors_are_not_permission_denied() {
        assert_eq!(
            stream_error_kind("The device is in use. (0x8889000A)"),
            ErrorKind::Other
        );
    }
}
