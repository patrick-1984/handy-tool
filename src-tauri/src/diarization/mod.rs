//! Speaker detection for "Make note with speakers": label who is speaking in
//! a History entry's recording before a note is made from it. It runs only
//! on demand from History; recording, live text, VAD and normal
//! transcription never use it.
//!
//! The recording is diarized with pyannote segmentation-3.0 and a WeSpeaker
//! ResNet34 embedding model (the sherpa-onnx pipeline, see [`pipeline`]) and
//! the text is labelled `[Person N]: text`, one paragraph per turn.
//!
//! How the text gets to the speakers depends on what the selected model
//! reports ([`TimingSupport`]):
//!
//! 1. Word or token timestamps (Parakeet, SenseVoice): transcribe once, then
//!    diarize and give every word to the speaker its onset overlaps most
//!    ([`align::assign_words`]).
//! 2. Segment timestamps (Whisper): transcribe once, diarize, give every
//!    segment to its main speaker; segments that clearly mix voices are
//!    transcribed again per speaker, a bounded number of times
//!    ([`align::assign_segments`]).
//!
//!    In both, "once" means one pass in pieces of at most 40 s cut at pauses,
//!    as the Files page does, with the times shifted back onto the
//!    recording's clock: most engines have no long-form mode, and dictation
//!    waits for the engine only one piece at a time.
//! 3. No timestamps (Moonshine, FLM, API and OpenRouter transcription):
//!    diarize first, then transcribe every speaker turn separately (turns
//!    cover the whole recording; turns over 30 s are split at pauses).
//!
//! No text is dropped: text outside the diarized speech goes to the nearest
//! speaker. With one speaker the text has no labels.
//!
//! Errors (ONNX, a panic, a failed engine call) are returned rather than
//! hidden behind a plain transcription: the caller still has the entry's
//! stored text and can offer a normal note instead.

mod align;
mod clustering;
mod fbank;
pub mod models;
mod pipeline;
mod turns;

use crate::audio_toolkit::{apply_custom_words, filter_transcription_output};
use anyhow::{Result, anyhow};
use log::{debug, info};
use ort::execution_providers::CPUExecutionProvider;
use ort::session::{Session, builder::GraphOptimizationLevel};
use ort::value::Tensor;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

pub use align::{TimedText, group_tokens};
pub use pipeline::Segment;

/// Cosine-distance threshold for clustering speaker embeddings. Higher merges
/// more (fewer speakers). Same default as sherpa-onnx; on synthetic 2–4
/// speaker test recordings 0.45–0.6 gave identical results and 0.4 started
/// splitting one voice in two.
pub const CLUSTER_THRESHOLD: f32 = 0.5;

/// Recordings longer than this (samples, 40 s) are transcribed in pieces cut
/// at pauses (`translator::split_speech_segments`, the same cut the Files
/// page uses).
const MAX_SINGLE_PASS_SAMPLES: usize = 40 * pipeline::SAMPLE_RATE;

/// What the selected model can report about timing, which decides how
/// speaker detection gives text to speakers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingSupport {
    /// No model loaded.
    Unknown,
    /// Text only.
    None,
    /// Segment start/end times (Whisper).
    Segments,
    /// Word or token times (Parakeet, SenseVoice).
    Words,
}

/// A transcription with the timing the engine produced. Times are seconds
/// from the start of the audio passed in.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct TimedTranscript {
    /// The engine's text, before the usual clean-up.
    pub text: String,
    /// Word timestamps (token rows grouped into words where needed).
    pub words: Option<Vec<TimedText>>,
    /// Segment timestamps.
    pub segments: Option<Vec<TimedText>>,
    /// For [`post_process`](Self::post_process): the custom words and their
    /// threshold at the time of the call.
    pub custom_words: Vec<String>,
    pub word_correction_threshold: f64,
}

impl TimedTranscript {
    /// The clean-up a normal transcription gets (custom words, filler words),
    /// for a piece of this transcript's text.
    pub fn post_process(&self, text: &str) -> String {
        let corrected =
            apply_custom_words(text, &self.custom_words, self.word_correction_threshold);
        filter_transcription_output(&corrected)
    }

    fn has_timing(&self) -> bool {
        self.words.is_some() || self.segments.is_some()
    }
}

/// The transcription side of speaker detection (the transcription manager
/// with the right model and locks, or a fake in tests).
pub trait SpeakerTranscriber {
    fn timing_support(&self) -> TimingSupport;
    /// One engine call with timestamps; raw text.
    fn transcribe_timed(&self, audio: &[f32]) -> Result<TimedTranscript>;
    /// One normal engine call; cleaned-up text.
    fn transcribe(&self, audio: &[f32]) -> Result<String>;
}

/// Intra-op threads per speaker-model session: half the cores, 2 to 4. More
/// threads were slower in measurements (small models, thread overhead), and
/// this leaves cores free for the rest of the system.
fn ort_threads() -> usize {
    std::thread::available_parallelism()
        .map_or(2, |n| n.get() / 2)
        .clamp(2, 4)
}

/// A CPU session for one of the speaker models.
fn create_session(path: &Path, threads: usize) -> Result<Session> {
    Ok(Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_execution_providers([CPUExecutionProvider::default().build()])?
        .with_intra_threads(threads)?
        .with_inter_threads(1)?
        .commit_from_file(path)?)
}

/// ONNX sessions for the two speaker models.
pub struct Diarizer {
    segmentation: Session,
    embedding: Session,
    fbank: fbank::Fbank,
}

impl Diarizer {
    pub fn new(segmentation: &Path, embedding: &Path) -> Result<Self> {
        Self::with_threads(segmentation, embedding, ort_threads())
    }

    fn with_threads(segmentation: &Path, embedding: &Path, threads: usize) -> Result<Self> {
        let started = Instant::now();
        let segmentation = create_session(segmentation, threads)
            .map_err(|e| anyhow!("Failed to load speaker segmentation model: {}", e))?;
        let embedding = create_session(embedding, threads)
            .map_err(|e| anyhow!("Failed to load speaker embedding model: {}", e))?;
        debug!(
            "Speaker models loaded in {:?} ({} threads)",
            started.elapsed(),
            threads
        );
        Ok(Self {
            segmentation,
            embedding,
            fbank: fbank::Fbank::new(),
        })
    }

    /// Diarize 16 kHz mono samples into speaker segments (seconds).
    pub fn diarize(&mut self, samples: &[f32], threshold: f32) -> Result<Vec<Segment>> {
        let params = pipeline::Params::for_samples(samples.len(), threshold);
        self.diarize_with(samples, &params, true)
    }

    /// [`diarize`](Self::diarize) with explicit parameters and feature mean
    /// normalisation selectable (WeSpeaker applies it; sherpa-onnx does not)
    /// — for accuracy checks.
    fn diarize_with(
        &mut self,
        samples: &[f32],
        params: &pipeline::Params,
        cmn: bool,
    ) -> Result<Vec<Segment>> {
        let Self {
            segmentation,
            embedding,
            fbank,
        } = self;
        let seg = Diarizer::segment_with(segmentation);
        let emb = Diarizer::embed_with(embedding, fbank, cmn);
        pipeline::diarize(samples, params, seg, emb)
    }

    fn segment_with(session: &mut Session) -> impl FnMut(&[f32], usize) -> Result<Vec<f32>> + '_ {
        move |windows, count| {
            let input =
                Tensor::from_array(([count, 1, windows.len() / count.max(1)], windows.to_vec()))?;
            let outputs = session.run(ort::inputs!["x" => input])?;
            let (_, scores) = outputs["y"].try_extract_tensor::<f32>()?;
            Ok(scores.to_vec())
        }
    }

    fn embed_with<'a>(
        session: &'a mut Session,
        fbank: &'a fbank::Fbank,
        cmn: bool,
    ) -> impl FnMut(&[f32]) -> Result<Option<Vec<f32>>> + 'a {
        move |audio| {
            let feats = fbank.compute(audio, cmn);
            let frames = feats.len() / fbank::NUM_MEL_BINS;
            if frames == 0 {
                return Ok(None);
            }
            let input = Tensor::from_array(([1usize, frames, fbank::NUM_MEL_BINS], feats))?;
            let outputs = session.run(ort::inputs!["feats" => input])?;
            let (_, embedding) = outputs["embs"].try_extract_tensor::<f32>()?;
            Ok(Some(embedding.to_vec()))
        }
    }
}

/// The speaker models failed (loading, ONNX, a panic), as opposed to the
/// transcription; lets callers tell the two apart.
#[derive(Debug)]
pub struct DiarizationFailed(String);

impl std::fmt::Display for DiarizationFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "speaker detection failed: {}", self.0)
    }
}

impl std::error::Error for DiarizationFailed {}

/// Diarization is CPU heavy: never run two at once.
static RUN_LOCK: Mutex<()> = Mutex::new(());

/// Load the speaker models, diarize, drop the models again. Loading takes a
/// fraction of a second, keeping them loaded ~66 MB, so they are not cached.
fn run_diarization(paths: &models::ModelPaths, samples: &[f32]) -> Result<Vec<Segment>> {
    let _running = RUN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let started = Instant::now();
    let mut diarizer = Diarizer::new(&paths.segmentation, &paths.embedding)
        .map_err(|e| anyhow::Error::new(DiarizationFailed(e.to_string())))?;
    let result = catch_unwind(AssertUnwindSafe(|| {
        diarizer.diarize(samples, CLUSTER_THRESHOLD)
    }))
    .unwrap_or_else(|_| Err(anyhow!("speaker diarization panicked")))
    .map_err(|e| anyhow::Error::new(DiarizationFailed(e.to_string())));
    drop(diarizer);
    if let Ok(segments) = &result {
        info!(
            "Diarized {:.1}s of audio in {:.2}s: {} speakers, {} segments",
            samples.len() as f64 / pipeline::SAMPLE_RATE as f64,
            started.elapsed().as_secs_f64(),
            speaker_count(segments),
            segments.len(),
        );
    }
    result
}

fn speaker_count(segments: &[Segment]) -> usize {
    let mut seen: Vec<usize> = segments.iter().map(|s| s.speaker).collect();
    seen.sort_unstable();
    seen.dedup();
    seen.len()
}

/// Transcribe a recording with speaker labels (`[Person N]: …`, one
/// paragraph per turn); with a single speaker the text has no labels.
/// Blocking (ONNX and transcription); call from a worker thread.
pub fn transcribe_with_speakers(
    paths: &models::ModelPaths,
    transcriber: &dyn SpeakerTranscriber,
    samples: &[f32],
) -> Result<String> {
    let started = Instant::now();
    let result = speaker_transcription(transcriber, samples, &mut |audio| {
        run_diarization(paths, audio)
    });
    if result.is_ok() {
        info!(
            "Transcription with speakers finished in {:.2}s",
            started.elapsed().as_secs_f64()
        );
    }
    result
}

fn speaker_transcription(
    t: &dyn SpeakerTranscriber,
    samples: &[f32],
    diarize: &mut dyn FnMut(&[f32]) -> Result<Vec<Segment>>,
) -> Result<String> {
    let support = t.timing_support();
    debug!("Speaker detection: model timing support {:?}", support);
    let timed = match support {
        TimingSupport::Segments | TimingSupport::Words => transcribe_in_pieces(t, samples)?,
        TimingSupport::None | TimingSupport::Unknown => {
            return per_turn_transcription(t, samples, diarize);
        }
    };
    if timed.text.trim().is_empty() {
        return Ok(String::new());
    }
    if !timed.has_timing() {
        info!("The transcription has no timestamps; transcribing each speaker turn instead");
        return per_turn_transcription(t, samples, diarize);
    }

    let segments = diarize(samples)?;
    if segments.is_empty() {
        // No speech found: keep the whole transcript, unlabelled.
        info!("Speaker detection found no speech; keeping the transcript without labels");
        return Ok(timed.post_process(&timed.text));
    }

    let blocks = if let Some(words) = &timed.words {
        align::assign_words(&timed.text, words, &segments)
    } else {
        let pieces = timed.segments.as_deref().unwrap_or_default();
        let mut retranscribe =
            |audio: &[f32]| -> Result<String> { Ok(t.transcribe_timed(audio)?.text) };
        align::assign_segments(&timed.text, pieces, &segments, samples, &mut retranscribe)?
    };
    info!(
        "Speaker detection ({}): {} speakers, {} blocks",
        if timed.words.is_some() {
            "words"
        } else {
            "segments"
        },
        speaker_count(&segments),
        blocks.len()
    );
    Ok(format_blocks(&timed, blocks))
}

/// One timed transcription of the whole recording, made of pieces of at most
/// about [`MAX_SINGLE_PASS_SAMPLES`] cut at pauses, with every piece's times
/// shifted to the recording's clock. The timing counts only if every piece
/// that produced text also produced timing.
fn transcribe_in_pieces(t: &dyn SpeakerTranscriber, samples: &[f32]) -> Result<TimedTranscript> {
    if samples.len() <= MAX_SINGLE_PASS_SAMPLES {
        return t.transcribe_timed(samples);
    }
    let ranges = crate::managers::translator::split_speech_segments(samples);
    debug!("Speaker detection: transcribing in {} pieces", ranges.len());
    let mut whole = TimedTranscript::default();
    let mut texts: Vec<String> = Vec::new();
    let (mut words, mut segments) = (Some(Vec::new()), Some(Vec::new()));
    for range in ranges {
        let piece = t.transcribe_timed(&samples[range.clone()])?;
        whole.custom_words = piece.custom_words.clone();
        whole.word_correction_threshold = piece.word_correction_threshold;
        let text = piece.text.trim();
        if text.is_empty() {
            continue;
        }
        texts.push(text.to_string());
        let offset = range.start as f32 / pipeline::SAMPLE_RATE as f32;
        let shift = |rows: Vec<TimedText>| -> Vec<TimedText> {
            rows.into_iter()
                .map(|r| TimedText::new(r.start + offset, r.end + offset, r.text))
                .collect()
        };
        match (piece.words, words.as_mut()) {
            (Some(rows), Some(all)) => all.extend(shift(rows)),
            _ => words = None,
        }
        match (piece.segments, segments.as_mut()) {
            (Some(rows), Some(all)) => all.extend(shift(rows)),
            _ => segments = None,
        }
    }
    whole.text = texts.join(" ");
    whole.words = words.filter(|w| !w.is_empty());
    whole.segments = segments.filter(|s| !s.is_empty());
    Ok(whole)
}

/// Clean up every block like a normal transcription and label it. With a
/// single speaker the text is returned unlabelled.
fn format_blocks(timed: &TimedTranscript, blocks: Vec<(usize, String)>) -> String {
    if align::block_speakers(&blocks) < 2 {
        let text: Vec<&str> = blocks.iter().map(|(_, t)| t.as_str()).collect();
        return timed.post_process(&text.join(" "));
    }
    let processed: Vec<(usize, String)> = blocks
        .into_iter()
        .map(|(speaker, text)| (speaker, timed.post_process(&text)))
        .collect();
    turns::format_transcript(&processed)
}

/// For models without timestamps: diarize first, then transcribe every
/// speaker turn separately. The turns cover the whole recording.
fn per_turn_transcription(
    t: &dyn SpeakerTranscriber,
    samples: &[f32],
    diarize: &mut dyn FnMut(&[f32]) -> Result<Vec<Segment>>,
) -> Result<String> {
    let segments = diarize(samples)?;
    let mut turns = turns::cover_audio(turns::build_turns(&segments, samples.len()), samples.len());
    if turns.is_empty() && !samples.is_empty() {
        // No speech found: let the engine judge the whole recording.
        turns.push(turns::Turn {
            start: 0,
            end: samples.len(),
            speaker: 0,
        });
    }
    let turns = turns::split_long_turns(turns, samples);
    info!(
        "Per-turn transcription with speakers: {} speakers, {} turns",
        turns::speaker_count(&turns),
        turns.len()
    );
    let mut texts = Vec::with_capacity(turns.len());
    for turn in &turns {
        let text = t.transcribe(&samples[turn.start..turn.end])?;
        texts.push((turn.speaker, text));
    }
    Ok(turns::format_transcript(&texts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const SR: usize = pipeline::SAMPLE_RATE;

    /// A fake engine: every call is logged as (start, len) in samples of the
    /// recording (found by the first sample's value, which is its index).
    struct Fake {
        support: TimingSupport,
        calls: RefCell<Vec<(usize, usize)>>,
    }

    impl Fake {
        fn new(support: TimingSupport) -> Self {
            Self {
                support,
                calls: RefCell::new(Vec::new()),
            }
        }

        fn log(&self, audio: &[f32]) -> (usize, usize) {
            let call = (audio.first().map_or(0, |v| *v as usize), audio.len());
            self.calls.borrow_mut().push(call);
            call
        }
    }

    impl SpeakerTranscriber for Fake {
        fn timing_support(&self) -> TimingSupport {
            self.support
        }

        fn transcribe_timed(&self, audio: &[f32]) -> Result<TimedTranscript> {
            let (start, len) = self.log(audio);
            // One word per second of audio, named after its absolute second.
            let words: Vec<TimedText> = (0..len / SR)
                .map(|i| TimedText::new(i as f32, i as f32 + 0.5, format!("w{}", start / SR + i)))
                .collect();
            let text = words
                .iter()
                .map(|w| w.text.clone())
                .collect::<Vec<_>>()
                .join(" ");
            let timing = Some(words);
            Ok(match self.support {
                TimingSupport::Words => TimedTranscript {
                    text,
                    words: timing,
                    ..Default::default()
                },
                _ => TimedTranscript {
                    text,
                    segments: timing,
                    ..Default::default()
                },
            })
        }

        fn transcribe(&self, audio: &[f32]) -> Result<String> {
            let (start, len) = self.log(audio);
            Ok(format!("turn{}-{}", start / SR, (start + len) / SR))
        }
    }

    /// Audio whose every sample is its own index (so the fake can tell where
    /// a piece starts) — loud enough to count as speech.
    fn ramp(secs: usize) -> Vec<f32> {
        (0..secs * SR).map(|i| i as f32).collect()
    }

    fn seg(start: f32, end: f32, speaker: usize) -> Segment {
        Segment {
            start,
            end,
            speaker,
        }
    }

    #[test]
    fn single_speaker_blocks_are_joined_without_labels() {
        let timed = TimedTranscript {
            text: "Hello there. Thank you.".to_string(),
            ..Default::default()
        };
        assert_eq!(
            format_blocks(&timed, vec![(3, "Hello there.".to_string())]),
            "Hello there."
        );
        assert_eq!(format_blocks(&timed, Vec::new()), "");
        assert_eq!(
            format_blocks(
                &timed,
                vec![(0, "Hello there.".to_string()), (1, "Hi.".to_string())]
            ),
            "[Person 1]: Hello there.\n\n[Person 2]: Hi."
        );
    }

    #[test]
    fn post_processing_applies_custom_words() {
        let timed = TimedTranscript {
            custom_words: vec!["Kubernetes".to_string()],
            word_correction_threshold: 0.18,
            ..Default::default()
        };
        assert_eq!(timed.post_process("I use kubernetis"), "I use Kubernetes");
    }

    #[test]
    fn word_timed_engines_transcribe_once_and_label_words() {
        let fake = Fake::new(TimingSupport::Words);
        let samples = ramp(4);
        let text = speaker_transcription(&fake, &samples, &mut |_| {
            Ok(vec![seg(0.0, 2.0, 7), seg(2.0, 4.0, 3)])
        })
        .unwrap();
        assert_eq!(text, "[Person 1]: w0 w1\n\n[Person 2]: w2 w3");
        assert_eq!(fake.calls.borrow().len(), 1);
    }

    #[test]
    fn long_recordings_are_transcribed_in_pieces_on_the_recording_clock() {
        let fake = Fake::new(TimingSupport::Words);
        let samples = ramp(100);
        let timed = transcribe_in_pieces(&fake, &samples).unwrap();
        let calls = fake.calls.borrow();
        assert!(calls.len() > 1, "{calls:?}");
        assert!(calls.iter().all(|(_, len)| *len <= 45 * SR));
        // Every word sits at its own absolute second.
        let words = timed.words.unwrap();
        for w in &words {
            assert_eq!(w.text, format!("w{}", w.start as usize), "{w:?}");
        }
        assert_eq!(timed.text.split(' ').count(), words.len());
    }

    #[test]
    fn whisper_segments_are_assigned_to_speakers() {
        let fake = Fake::new(TimingSupport::Segments);
        let samples = ramp(30);
        let text = speaker_transcription(&fake, &samples, &mut |_| {
            Ok(vec![seg(0.0, 15.0, 0), seg(15.0, 30.0, 1)])
        })
        .unwrap();
        assert_eq!(fake.calls.borrow().as_slice(), &[(0, 30 * SR)]);
        assert!(text.starts_with("[Person 1]: w0 w1"), "{text}");
        assert!(text.contains("\n\n[Person 2]: w15 w16"), "{text}");
        // A long recording: in pieces, segment times on the recording's clock.
        let fake = Fake::new(TimingSupport::Segments);
        let timed = transcribe_in_pieces(&fake, &ramp(100)).unwrap();
        assert!(fake.calls.borrow().len() > 1);
        for s in timed.segments.unwrap() {
            assert_eq!(s.text, format!("w{}", s.start as usize), "{s:?}");
        }
    }

    #[test]
    fn engines_without_timestamps_transcribe_turns_covering_everything() {
        let fake = Fake::new(TimingSupport::None);
        let samples = ramp(20);
        // Speech only at 2–6 s and 12–16 s: the turns still cover 0–20 s.
        let text = speaker_transcription(&fake, &samples, &mut |_| {
            Ok(vec![seg(2.0, 6.0, 1), seg(12.0, 16.0, 0)])
        })
        .unwrap();
        assert_eq!(text, "[Person 1]: turn0-9\n\n[Person 2]: turn9-20");
        // No speech found at all: the whole recording, unlabelled.
        let fake = Fake::new(TimingSupport::None);
        let text = speaker_transcription(&fake, &samples, &mut |_| Ok(Vec::new())).unwrap();
        assert_eq!(text, "turn0-20");
    }

    #[test]
    fn no_diarized_speech_keeps_the_transcript() {
        let fake = Fake::new(TimingSupport::Words);
        let text = speaker_transcription(&fake, &ramp(3), &mut |_| Ok(Vec::new())).unwrap();
        assert_eq!(text, "w0 w1 w2");
    }

    #[test]
    fn diarization_errors_are_returned() {
        let fake = Fake::new(TimingSupport::Words);
        let err = speaker_transcription(&fake, &ramp(3), &mut |_| {
            Err(anyhow::Error::new(DiarizationFailed("boom".into())))
        })
        .unwrap_err();
        assert!(err.is::<DiarizationFailed>());
    }

    fn var(k: &str) -> String {
        std::env::var(k).unwrap_or_else(|_| panic!("{k} not set"))
    }

    /// End-to-end accuracy check against the real speaker models; prints one
    /// `RESULT` JSON line with the diarized segments. Not run by default:
    ///
    /// ```text
    /// HANDY_DIAR_WAV=meeting.wav HANDY_DIAR_SEG=seg.onnx HANDY_DIAR_EMB=emb.onnx \
    ///   cargo test --lib diarization::tests::diarize_wav -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "needs speaker models and a test recording (HANDY_DIAR_* env vars)"]
    fn diarize_wav() {
        let samples =
            crate::audio_toolkit::audio::decode_audio_file(Path::new(&var("HANDY_DIAR_WAV")))
                .unwrap();
        let started = Instant::now();
        let mut diarizer = Diarizer::new(
            Path::new(&var("HANDY_DIAR_SEG")),
            Path::new(&var("HANDY_DIAR_EMB")),
        )
        .unwrap();
        let segments = diarizer.diarize(&samples, CLUSTER_THRESHOLD).unwrap();
        let json: Vec<_> = segments
            .iter()
            .map(|s| serde_json::json!({"start": s.start, "end": s.end, "speaker": s.speaker}))
            .collect();
        println!(
            "RESULT {}",
            serde_json::json!({
                "audio_seconds": samples.len() as f64 / SR as f64,
                "elapsed": started.elapsed().as_secs_f64(),
                "segments": json,
            })
        );
    }
}
