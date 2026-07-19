use std::{
    io::Error,
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

use cpal::{
    Device, Sample, SizedSample,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

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

pub struct AudioRecorder {
    device: Option<Device>,
    cmd_tx: Option<mpsc::Sender<Cmd>>,
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

    pub fn open(&mut self, device: Option<Device>) -> Result<(), Box<dyn std::error::Error>> {
        if self.worker_handle.is_some() {
            return Ok(()); // already open
        }

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

        let worker = std::thread::spawn(move || {
            let config = AudioRecorder::get_preferred_config(&thread_device)
                .expect("failed to fetch preferred config");

            let sample_rate = config.sample_rate().0;
            let channels = config.channels() as usize;

            log::info!(
                "Using device: {:?}\nSample rate: {}\nChannels: {}\nFormat: {:?}",
                thread_device.name(),
                sample_rate,
                channels,
                config.sample_format()
            );

            let stream = match config.sample_format() {
                cpal::SampleFormat::U8 => {
                    AudioRecorder::build_stream::<u8>(&thread_device, &config, sample_tx, channels)
                        .unwrap()
                }
                cpal::SampleFormat::I8 => {
                    AudioRecorder::build_stream::<i8>(&thread_device, &config, sample_tx, channels)
                        .unwrap()
                }
                cpal::SampleFormat::I16 => {
                    AudioRecorder::build_stream::<i16>(&thread_device, &config, sample_tx, channels)
                        .unwrap()
                }
                cpal::SampleFormat::I32 => {
                    AudioRecorder::build_stream::<i32>(&thread_device, &config, sample_tx, channels)
                        .unwrap()
                }
                cpal::SampleFormat::F32 => {
                    AudioRecorder::build_stream::<f32>(&thread_device, &config, sample_tx, channels)
                        .unwrap()
                }
                _ => panic!("unsupported sample format"),
            };

            stream.play().expect("failed to start stream");

            // keep the stream alive while we process samples
            run_consumer(
                sample_rate,
                vad,
                sample_rx,
                cmd_rx,
                level_cb,
                segment_cb,
                closed_chunk_cb,
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
        if let Some(tx) = &self.cmd_tx {
            tx.send(Cmd::Start(params))?;
        }
        Ok(())
    }

    pub fn stop(&self) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        let (resp_tx, resp_rx) = mpsc::channel();
        if let Some(tx) = &self.cmd_tx {
            tx.send(Cmd::Stop(resp_tx))?;
        }
        Ok(resp_rx.recv()?) // wait for the samples (and for chunk gluing to finish)
    }

    /// Stop recording and discard all audio + chunk files for this take.
    pub fn cancel(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(tx) = &self.cmd_tx {
            tx.send(Cmd::Cancel)?;
        }
        Ok(())
    }

    pub fn close(&mut self) -> Result<(), Box<dyn std::error::Error>> {
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
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: Sample + SizedSample + Send + 'static,
        f32: cpal::FromSample<T>,
    {
        let mut output_buffer = Vec::new();

        let stream_cb = move |data: &[T], _: &cpal::InputCallbackInfo| {
            output_buffer.clear();

            if channels == 1 {
                // Direct conversion without intermediate Vec
                output_buffer.extend(data.iter().map(|&sample| sample.to_sample::<f32>()));
            } else {
                // Convert to mono directly
                let frame_count = data.len() / channels;
                output_buffer.reserve(frame_count);

                for frame in data.chunks_exact(channels) {
                    let mono_sample = frame
                        .iter()
                        .map(|&sample| sample.to_sample::<f32>())
                        .sum::<f32>()
                        / channels as f32;
                    output_buffer.push(mono_sample);
                }
            }

            if sample_tx.send(output_buffer.clone()).is_err() {
                log::error!("Failed to send samples");
            }
        };

        device.build_input_stream(
            &config.clone().into(),
            stream_cb,
            |err| log::error!("Stream error: {}", err),
            None,
        )
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
) {
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

    loop {
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
                process_frame(
                    frame,
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
                            process_frame(
                                frame,
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
                        process_frame(
                            frame,
                            true,
                            &vad,
                            &mut chunk_state,
                            &mut processed_samples,
                            &mut segment_start_idx,
                            &segment_cb,
                            &closed_chunk_cb,
                        )
                    });

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
                    processed_samples.clear();
                    segment_start_idx = 0;
                }
                Cmd::Shutdown => return,
            }
        }
    }
}
