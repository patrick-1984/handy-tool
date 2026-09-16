use std::{
    io::Error,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
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

type SegmentCb = Arc<Mutex<Option<Arc<dyn Fn(Vec<f32>) + Send + Sync + 'static>>>>;
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
    level_cb: Option<Arc<dyn Fn(Vec<f32>) + Send + Sync + 'static>>,
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
        F: Fn(Vec<f32>) + Send + Sync + 'static,
    {
        self.level_cb = Some(Arc::new(cb));
        self
    }

    /// Set a callback that fires when VAD detects a speech→silence boundary.
    /// The callback receives the audio samples for that speech segment.
    pub fn set_segment_callback<F>(&self, cb: F)
    where
        F: Fn(Vec<f32>) + Send + Sync + 'static,
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
                    return;
                }
            };
            let stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Capture stream construction failed: {e}");
                    arm_errored_worker.store(true, Ordering::Release);
                    return;
                }
            };

            if let Err(e) = stream.play() {
                log::error!("Capture stream failed to start: {e}");
                arm_errored_worker.store(true, Ordering::Release);
                return;
            }
            let stream_playing_in = open_start.elapsed();
            log::debug!(
                "T-113: recorder worker ready (stream playing) {:?} after open() was called",
                stream_playing_in
            );

            // keep the stream alive while we process samples
            run_consumer(
                sample_rate,
                vad,
                sample_rx,
                cmd_rx,
                level_cb,
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
        samples_since_last as f32 / 16000.0 >= 3.0
    } else {
        false
    };
    if should_emit {
        if let Some(cb) = segment_cb.lock().unwrap().as_ref() {
            let all_audio = out_buf.clone();
            if !all_audio.is_empty() {
                cb(all_audio);
            }
        }
        *segment_start_idx = out_buf.len();
    }
}

#[allow(clippy::too_many_arguments)]
fn run_consumer(
    in_sample_rate: u32,
    vad: Option<Arc<Mutex<Box<dyn vad::VoiceActivityDetector>>>>,
    sample_rx: mpsc::Receiver<Vec<f32>>,
    cmd_rx: mpsc::Receiver<Cmd>,
    level_cb: Option<Arc<dyn Fn(Vec<f32>) + Send + Sync + 'static>>,
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
    let mut segment_start_idx: usize = 0;
    // When chunked Opus recording is active this is `Some`; otherwise we
    // accumulate full PCM in `processed_samples` (live / crash-safety-off).
    let mut chunk_state: Option<ChunkState> = None;

    // ---------- spectrum visualisation setup ---------------------------- //
    const BUCKETS: usize = 16;
    const WINDOW_SIZE: usize = 512;
    let mut visualizer = AudioVisualiser::new(
        in_sample_rate,
        WINDOW_SIZE,
        BUCKETS,
        400.0,  // vocal_min_hz
        4000.0, // vocal_max_hz
    );
    // Last time the level callback fired while idle (see LEVEL_IDLE_INTERVAL).
    let mut last_level_emit: Option<Instant> = None;
    // T-113 (finding 9): one-shot — only the FIRST loop iteration after the
    // audio callback flags `first_buffer_seen` logs the stream-start→
    // first-buffer latency; every later iteration is steady-state and not
    // interesting for start-latency.
    let mut first_buffer_logged = false;

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
            // ---------- spectrum processing ------------------------------ //
            if let Some(buckets) = visualizer.feed(&raw) {
                if let Some(cb) = &level_cb {
                    // Full rate while recording; throttled while idle (the
                    // always-on mic would otherwise flood the event system).
                    let now = Instant::now();
                    if recording
                        || last_level_emit
                            .map_or(true, |t| now.duration_since(t) >= LEVEL_IDLE_INTERVAL)
                    {
                        last_level_emit = Some(now);
                        cb(buckets);
                    }
                }
            }

            // ---------- pipeline ----------------------------------------- //
            frame_resampler.push(&raw, &mut |frame: &[f32]| {
                mix_buf.clear();
                mix_buf.extend_from_slice(frame);
                apply_system_mix(&mut mix_buf, &sys_ring, sys_gain, &mut mix_scratch);
                process_frame(
                    &mix_buf,
                    recording,
                    &vad,
                    &mut chunk_state,
                    &mut processed_samples,
                    &mut segment_start_idx,
                    &segment_cb,
                    &closed_chunk_cb,
                )
            });
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

                    // Drain any audio chunks captured but not yet consumed.
                    while let Ok(remaining) = sample_rx.try_recv() {
                        frame_resampler.push(&remaining, &mut |frame: &[f32]| {
                            mix_buf.clear();
                            mix_buf.extend_from_slice(frame);
                            apply_system_mix(&mut mix_buf, &sys_ring, sys_gain, &mut mix_scratch);
                            process_frame(
                                &mix_buf,
                                true,
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
                            true,
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
                                true,
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
                Cmd::Shutdown => return,
            }
        }
    }
}
