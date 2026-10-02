//! Model-agnostic part of the pyannote diarization pipeline, ported from
//! sherpa-onnx `offline-speaker-diarization-pyannote-impl.h`:
//!
//! 1. slide 10 s windows over the audio (last one zero-padded) and run the
//!    segmentation model on each; decode its powerset output into up to three
//!    active "local" speakers per frame,
//! 2. estimate how many people talk in each global frame (overlap average),
//! 3. embed every (window, local speaker) pair from its non-overlapped frames,
//! 4. cluster the embeddings into global speakers,
//! 5. map the local activity back onto global speakers, keep the top-k per
//!    frame and turn the frame grid into time segments (merging short gaps,
//!    dropping short blips).
//!
//! The two neural networks are passed in as closures so this module is unit
//! testable without model files; see `super::Diarizer` for the ONNX side.

use super::clustering;
use anyhow::{Result, anyhow};
use log::debug;
use std::time::Instant;

pub const SAMPLE_RATE: usize = 16_000;
/// Segmentation window (10 s) in samples.
pub const WINDOW_SIZE: usize = 160_000;
/// Hop between segmentation output frames, in samples.
pub const RECEPTIVE_FIELD_SHIFT: usize = 270;
/// Receptive field of one output frame, in samples.
pub const RECEPTIVE_FIELD_SIZE: usize = 991;
/// Local speakers the segmentation model tracks per window.
pub const NUM_LOCAL_SPEAKERS: usize = 3;
/// Powerset classes: none, s0, s1, s2, s0+s1, s0+s2, s1+s2.
pub const NUM_CLASSES: usize = 7;
const POWERSET: [[u8; NUM_LOCAL_SPEAKERS]; NUM_CLASSES] = [
    [0, 0, 0],
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
    [1, 1, 0],
    [1, 0, 1],
    [0, 1, 1],
];
/// A (window, local speaker) pair needs this many clean frames to be embedded.
const MIN_EMBEDDING_FRAMES: usize = 10;
/// Upper bound on segmentation windows up to about an hour of audio; longer
/// recordings get a larger shift. Embedding cost grows with windows × window
/// length, so this is what keeps an hour-long recording to a few CPU minutes.
/// Past an hour the shift stays at half a window (see [`window_shift_for`]),
/// so cost grows linearly again: about 2.5 CPU minutes per hour of audio
/// (measured), still well under the per-turn transcription that follows.
const MAX_CHUNKS: usize = 720;
/// Speech per embedding is capped at 4 s: the embedding model's cost grows
/// with its input, and a few seconds of one voice identify it as well as ten.
pub const MAX_EMBEDDING_SAMPLES: usize = 4 * SAMPLE_RATE;
/// Windows per segmentation model call (batching is ~2x faster on CPU).
const SEGMENTATION_BATCH: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub start: f32,
    pub end: f32,
    pub speaker: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Params {
    /// Cosine-distance cut for complete-linkage clustering.
    pub threshold: f32,
    /// Segments this short (seconds) are dropped.
    pub min_duration_on: f32,
    /// Same-speaker gaps this short (seconds) are bridged.
    pub min_duration_off: f32,
    /// Hop between segmentation windows, in samples.
    pub window_shift: usize,
    /// At most this much clean speech (samples) goes into one embedding.
    pub max_embedding_samples: usize,
}

impl Params {
    pub fn for_samples(num_samples: usize, threshold: f32) -> Self {
        Self {
            threshold,
            // pyannote/sherpa drop segments of 0.3 s or less. We keep them:
            // `turns::build_turns` folds every sub-second piece into a
            // neighbouring turn, whereas a dropped blip (a short "No.") would
            // fall into untranscribed audio between turns.
            min_duration_on: 0.0,
            min_duration_off: 0.5,
            window_shift: window_shift_for(num_samples),
            max_embedding_samples: MAX_EMBEDDING_SAMPLES,
        }
    }
}

/// Smallest hop between segmentation windows: 2 s (80% overlap). pyannote
/// and sherpa-onnx use 1 s; on the synthetic 2–4 speaker test recordings 2 s
/// was as accurate (lower DER on all three) at about half the cost.
pub const MIN_WINDOW_SHIFT: usize = 2 * SAMPLE_RATE;

/// [`MIN_WINDOW_SHIFT`] until that would need more than [`MAX_CHUNKS`]
/// windows (~24 min); beyond that the hop grows so an hour stays around 720
/// windows (5 s hop). Capped at half a window so every frame is still seen by
/// at least two windows, so past an hour the window count (and cost) grows
/// linearly: ~1440 windows for 2 h.
pub fn window_shift_for(num_samples: usize) -> usize {
    let span = num_samples.saturating_sub(WINDOW_SIZE);
    span.div_ceil(MAX_CHUNKS - 1)
        .clamp(MIN_WINDOW_SHIFT, WINDOW_SIZE / 2)
}

/// Start offsets (samples) of the segmentation windows. The final window may
/// run past the end of the audio and is zero-padded by the caller.
pub fn chunk_starts(num_samples: usize, shift: usize) -> Vec<usize> {
    if num_samples == 0 {
        return Vec::new();
    }
    if num_samples <= WINDOW_SIZE {
        return vec![0];
    }
    let full = (num_samples - WINDOW_SIZE) / shift + 1;
    let remainder = (num_samples - WINDOW_SIZE) % shift;
    let has_last = remainder > 0;
    (0..full + usize::from(has_last))
        .map(|i| i * shift)
        .collect()
}

/// Copy one window out of `samples`, zero-padding past the end.
pub fn chunk_samples(samples: &[f32], start: usize) -> Vec<f32> {
    let mut buf = vec![0f32; WINDOW_SIZE];
    let end = (start + WINDOW_SIZE).min(samples.len());
    if start < end {
        buf[..end - start].copy_from_slice(&samples[start..end]);
    }
    buf
}

/// Arg-max over the powerset classes of each frame → per-frame activity of
/// the three local speakers. `logits` is row-major `[frames][NUM_CLASSES]`.
pub fn powerset_decode(logits: &[f32]) -> Vec<[u8; NUM_LOCAL_SPEAKERS]> {
    (0..logits.len() / NUM_CLASSES)
        .map(|f| {
            let row = &logits[f * NUM_CLASSES..(f + 1) * NUM_CLASSES];
            let mut best = 0;
            for (i, v) in row.iter().enumerate() {
                if *v > row[best] {
                    best = i;
                }
            }
            POWERSET[best]
        })
        .collect()
}

type ChunkLabels = Vec<[u8; NUM_LOCAL_SPEAKERS]>;

fn chunk_start_frame(chunk: usize, shift: usize) -> usize {
    (chunk as f64 * shift as f64 / RECEPTIVE_FIELD_SHIFT as f64 + 0.5) as usize
}

fn num_global_frames(num_chunks: usize, shift: usize) -> usize {
    (WINDOW_SIZE + (num_chunks - 1) * shift) / RECEPTIVE_FIELD_SHIFT + 1
}

/// Estimated number of simultaneous speakers per global frame: the average
/// active-speaker count of all windows covering it, rounded.
fn speakers_per_frame(labels: &[ChunkLabels], shift: usize) -> Vec<usize> {
    let total = num_global_frames(labels.len(), shift);
    let mut count = vec![0f32; total];
    let mut weight = vec![0f32; total];
    for (c, chunk) in labels.iter().enumerate() {
        let start = chunk_start_frame(c, shift);
        for (f, row) in chunk.iter().enumerate() {
            let Some(slot) = count.get_mut(start + f) else {
                break;
            };
            *slot += row.iter().map(|&v| v as f32).sum::<f32>();
            weight[start + f] += 1.0;
        }
    }
    count
        .iter()
        .zip(&weight)
        .map(|(c, w)| (c / (w + 1e-12) + 0.5) as usize)
        .collect()
}

/// For every (chunk, local speaker) with enough single-speaker frames, the
/// sample ranges where only that speaker is active.
#[allow(clippy::type_complexity)]
fn embedding_regions(
    labels: &[ChunkLabels],
    shift: usize,
) -> Vec<((usize, usize), Vec<(usize, usize)>)> {
    let mut out = Vec::new();
    for (c, chunk) in labels.iter().enumerate() {
        let num_frames = chunk.len();
        let offset = c * shift;
        let to_sample =
            |f: usize| (f as f32 / num_frames as f32 * WINDOW_SIZE as f32) as usize + offset;
        for s in 0..NUM_LOCAL_SPEAKERS {
            // Overlapped frames would mix voices into the embedding.
            let active: Vec<bool> = chunk
                .iter()
                .map(|row| row[s] == 1 && row.iter().map(|&v| v as usize).sum::<usize>() < 2)
                .collect();
            if active.iter().filter(|&&a| a).count() < MIN_EMBEDDING_FRAMES {
                continue;
            }
            let mut ranges = Vec::new();
            let mut run_start = None;
            for (f, &a) in active.iter().enumerate() {
                match (a, run_start) {
                    (true, None) => run_start = Some(f),
                    (false, Some(st)) => {
                        ranges.push((to_sample(st), to_sample(f)));
                        run_start = None;
                    }
                    _ => {}
                }
            }
            if let Some(st) = run_start {
                ranges.push((to_sample(st), to_sample(num_frames - 1)));
            }
            out.push(((c, s), ranges));
        }
    }
    out
}

/// The audio of `ranges` back to back, at most `cap` samples of it: the
/// longest clean stretches are kept (whole where possible), in time order.
fn embedding_audio(samples: &[f32], ranges: &[(usize, usize)], cap: usize) -> Vec<f32> {
    let n = samples.len();
    let mut ranges: Vec<(usize, usize)> = ranges
        .iter()
        .map(|&(s, e)| (s.min(n), e.min(n)))
        .filter(|(s, e)| s < e)
        .collect();
    let total: usize = ranges.iter().map(|(s, e)| e - s).sum();
    if total > cap {
        ranges.sort_by(|a, b| (b.1 - b.0).cmp(&(a.1 - a.0)).then(a.0.cmp(&b.0)));
        let mut left = cap;
        let mut kept = Vec::new();
        for (s, e) in ranges {
            if left == 0 {
                break;
            }
            let take = (e - s).min(left);
            // Middle of a stretch: its edges are the least reliable frames.
            let from = s + (e - s - take) / 2;
            kept.push((from, from + take));
            left -= take;
        }
        kept.sort_unstable();
        ranges = kept;
    }
    let mut audio = Vec::with_capacity(total.min(cap));
    for (s, e) in ranges {
        audio.extend_from_slice(&samples[s..e]);
    }
    audio
}

/// Turn per-frame global speaker activity into time segments.
fn frames_to_segments(activity: &[Vec<u8>], num_speakers: usize, params: &Params) -> Vec<Segment> {
    let scale = RECEPTIVE_FIELD_SHIFT as f32 / SAMPLE_RATE as f32;
    let offset = 0.5 * RECEPTIVE_FIELD_SIZE as f32 / SAMPLE_RATE as f32;
    let time = |frame: usize| frame as f32 * scale + offset;
    let mut result = Vec::new();
    for spk in 0..num_speakers {
        let mut segments: Vec<Segment> = Vec::new();
        let mut start = None;
        for (f, row) in activity.iter().enumerate() {
            match (row[spk] > 0, start) {
                (true, None) => start = Some(f),
                (false, Some(st)) => {
                    segments.push(Segment {
                        start: time(st),
                        end: time(f),
                        speaker: spk,
                    });
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(st) = start {
            segments.push(Segment {
                start: time(st),
                end: time(activity.len().saturating_sub(1)),
                speaker: spk,
            });
        }

        // Bridge short pauses of the same speaker.
        let mut merged: Vec<Segment> = Vec::new();
        for seg in segments {
            match merged.last_mut() {
                Some(last) if seg.start - last.end <= params.min_duration_off => {
                    last.end = last.end.max(seg.end);
                }
                _ => merged.push(seg),
            }
        }
        result.extend(
            merged
                .into_iter()
                .filter(|s| s.end - s.start > params.min_duration_on),
        );
    }
    result.sort_by(|a, b| a.start.total_cmp(&b.start));
    result
}

/// Run the full pipeline.
///
/// - `segment(windows, count)` gets `count` zero-padded [`WINDOW_SIZE`]
///   windows back to back and returns the model's `[count][frames][NUM_CLASSES]`
///   scores, row-major.
/// - `embed(samples)` returns a speaker embedding for concatenated audio, or
///   `None` when the audio is too short / the model produced NaNs.
///
/// Speaker ids in the result are cluster indices (not yet renumbered).
pub fn diarize<S, E>(
    samples: &[f32],
    params: &Params,
    mut segment: S,
    mut embed: E,
) -> Result<Vec<Segment>>
where
    S: FnMut(&[f32], usize) -> Result<Vec<f32>>,
    E: FnMut(&[f32]) -> Result<Option<Vec<f32>>>,
{
    let n = samples.len();
    let shift = params.window_shift.max(1);
    let starts = chunk_starts(n, shift);
    if starts.is_empty() {
        return Ok(Vec::new());
    }

    let started = Instant::now();
    let mut labels: Vec<ChunkLabels> = Vec::with_capacity(starts.len());
    for batch in starts.chunks(SEGMENTATION_BATCH) {
        let mut input = Vec::with_capacity(batch.len() * WINDOW_SIZE);
        for &start in batch {
            input.extend(chunk_samples(samples, start));
        }
        let logits = segment(&input, batch.len())?;
        let per_window = logits.len() / batch.len();
        if per_window == 0 || logits.len() % batch.len() != 0 || per_window % NUM_CLASSES != 0 {
            return Err(anyhow!(
                "segmentation model returned {} values for {} windows",
                logits.len(),
                batch.len()
            ));
        }
        labels.extend(logits.chunks_exact(per_window).map(powerset_decode));
    }
    debug!(
        "Diarization: segmented {} windows (shift {} samples) in {:?}",
        starts.len(),
        shift,
        started.elapsed()
    );
    // Frames of the window that actually contain audio (the rest is padding).
    let audio_frames = n / RECEPTIVE_FIELD_SHIFT + 1;

    if labels.len() == 1 {
        // One window: its local speakers are already consistent, so no
        // embeddings or clustering are needed (same as sherpa-onnx).
        let activity: Vec<Vec<u8>> = labels[0]
            .iter()
            .take(audio_frames)
            .map(|row| row.to_vec())
            .collect();
        return Ok(frames_to_segments(&activity, NUM_LOCAL_SPEAKERS, params));
    }

    let per_frame = speakers_per_frame(&labels, shift);
    if per_frame.iter().all(|&k| k == 0) {
        return Ok(Vec::new());
    }

    let started = Instant::now();
    let mut keys = Vec::new();
    let mut embeddings = Vec::new();
    for (key, ranges) in embedding_regions(&labels, shift) {
        let audio = embedding_audio(samples, &ranges, params.max_embedding_samples);
        if let Some(embedding) = embed(&audio)?
            && embedding.iter().all(|v| v.is_finite())
        {
            keys.push(key);
            embeddings.push(embedding);
        }
    }
    debug!(
        "Diarization: {} speaker embeddings in {:?}",
        embeddings.len(),
        started.elapsed()
    );
    if embeddings.is_empty() {
        return Ok(Vec::new());
    }

    let cluster_of = clustering::cluster(&embeddings, params.threshold);
    let num_clusters = cluster_of.iter().max().map_or(0, |m| m + 1);

    // Global per-frame activity count for each cluster.
    let total = num_global_frames(labels.len(), shift);
    let mut count = vec![vec![0u32; num_clusters]; total];
    let mut map = vec![[None::<usize>; NUM_LOCAL_SPEAKERS]; labels.len()];
    for ((c, s), cl) in keys.iter().zip(&cluster_of) {
        map[*c][*s] = Some(*cl);
    }
    for (c, chunk) in labels.iter().enumerate() {
        let start = chunk_start_frame(c, shift);
        for (f, row) in chunk.iter().enumerate() {
            let Some(slot) = count.get_mut(start + f) else {
                break;
            };
            // Two local speakers mapped to the same cluster still count once.
            let mut seen = vec![false; num_clusters];
            for s in 0..NUM_LOCAL_SPEAKERS {
                if row[s] == 1
                    && let Some(cl) = map[c][s]
                {
                    seen[cl] = true;
                }
            }
            for (cl, hit) in seen.iter().enumerate() {
                if *hit {
                    slot[cl] += 1;
                }
            }
        }
    }
    count.truncate(audio_frames.min(total));

    // Keep the k most active clusters per frame, k = estimated speaker count.
    let activity: Vec<Vec<u8>> = count
        .iter()
        .enumerate()
        .map(|(f, row)| {
            let k = per_frame.get(f).copied().unwrap_or(0);
            let mut order: Vec<usize> = (0..num_clusters).collect();
            order.sort_by(|&a, &b| row[b].cmp(&row[a]).then(a.cmp(&b)));
            let mut out = vec![0u8; num_clusters];
            for &cl in order.iter().take(k) {
                if row[cl] > 0 {
                    out[cl] = 1;
                }
            }
            out
        })
        .collect();

    Ok(frames_to_segments(&activity, num_clusters, params))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logits_for(classes: &[usize]) -> Vec<f32> {
        classes
            .iter()
            .flat_map(|&c| {
                let mut row = vec![-5.0f32; NUM_CLASSES];
                row[c] = -0.01;
                row
            })
            .collect()
    }

    #[test]
    fn powerset_decode_maps_classes_to_speakers() {
        let decoded = powerset_decode(&logits_for(&[0, 1, 2, 3, 4, 5, 6]));
        assert_eq!(
            decoded,
            vec![
                [0, 0, 0],
                [1, 0, 0],
                [0, 1, 0],
                [0, 0, 1],
                [1, 1, 0],
                [1, 0, 1],
                [0, 1, 1]
            ]
        );
    }

    #[test]
    fn powerset_decode_takes_argmax_of_log_probs() {
        let row = [-3.0, -0.2, -2.0, -4.0, -1.9, -5.0, -6.0];
        assert_eq!(powerset_decode(&row), vec![[1, 0, 0]]);
        let row = [-3.0, -2.2, -2.0, -4.0, -0.1, -5.0, -6.0];
        assert_eq!(powerset_decode(&row), vec![[1, 1, 0]]);
    }

    #[test]
    fn chunks_cover_the_audio() {
        assert!(chunk_starts(0, 16_000).is_empty());
        assert_eq!(chunk_starts(1000, 16_000), vec![0]);
        assert_eq!(chunk_starts(WINDOW_SIZE, 16_000), vec![0]);
        // 12 s: windows at 0, 1, 2 s exactly reach the end; no padded tail.
        assert_eq!(chunk_starts(192_000, 16_000), vec![0, 16_000, 32_000]);
        // 12.5 s: one extra zero-padded window.
        assert_eq!(
            chunk_starts(200_000, 16_000),
            vec![0, 16_000, 32_000, 48_000]
        );
    }

    #[test]
    fn window_shift_scales_for_long_audio() {
        let min = 60 * SAMPLE_RATE;
        assert_eq!(window_shift_for(5 * min), MIN_WINDOW_SHIFT);
        assert_eq!(window_shift_for(20 * min), MIN_WINDOW_SHIFT);
        let hour = 60 * min;
        let shift = window_shift_for(hour);
        assert!(shift > 4 * SAMPLE_RATE && shift <= WINDOW_SIZE / 2);
        assert!(chunk_starts(hour, shift).len() <= MAX_CHUNKS);
        // Never less than 50% overlap, even for very long recordings.
        assert_eq!(window_shift_for(5 * hour), WINDOW_SIZE / 2);
    }

    #[test]
    fn short_gaps_merge_and_blips_drop() {
        let params = Params {
            min_duration_on: 0.3,
            ..Params::for_samples(0, 0.5)
        };
        // Frame time step is 270/16000 = 16.875 ms.
        let mut activity = vec![vec![0u8; 1]; 400];
        (10..100).for_each(|f| activity[f][0] = 1); // ~1.5 s
        (120..200).for_each(|f| activity[f][0] = 1); // gap ~0.34 s → merged
        (300..310).for_each(|f| activity[f][0] = 1); // ~0.17 s blip → dropped
        let segs = frames_to_segments(&activity, 1, &params);
        assert_eq!(segs.len(), 1);
        let offset = 0.5 * RECEPTIVE_FIELD_SIZE as f32 / SAMPLE_RATE as f32;
        assert!((segs[0].start - (10.0 * 0.016875 + offset)).abs() < 1e-4);
        assert!((segs[0].end - (200.0 * 0.016875 + offset)).abs() < 1e-4);
    }

    #[test]
    fn embedding_audio_is_capped_to_the_longest_stretches() {
        let samples: Vec<f32> = (0..100).map(|i| i as f32).collect();
        // Under the cap: everything, in order.
        assert_eq!(
            embedding_audio(&samples, &[(0, 3), (10, 12)], 10),
            vec![0.0, 1.0, 2.0, 10.0, 11.0]
        );
        // Over the cap: the 6-sample stretch whole, then the middle of the
        // 5-sample one, back in time order; the short one is dropped.
        assert_eq!(
            embedding_audio(&samples, &[(0, 2), (20, 25), (50, 56)], 8),
            vec![21.0, 22.0, 50.0, 51.0, 52.0, 53.0, 54.0, 55.0]
        );
        // Ranges past the end are clipped.
        assert_eq!(
            embedding_audio(&samples, &[(98, 120)], 10),
            vec![98.0, 99.0]
        );
    }

    /// Fake models: the "segmentation model" sees local speaker 0 wherever
    /// the signal is positive and local speaker 1 wherever it is negative; the
    /// "embedding model" maps the sign of the audio to one of two voices.
    #[test]
    fn end_to_end_with_fake_models() {
        // 30 s: A for 0–12 s, B for 12–22 s, A again for 22–30 s.
        let n = 30 * SAMPLE_RATE;
        let samples: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                if (12.0..22.0).contains(&t) { -0.5 } else { 0.5 }
            })
            .collect();
        let frames = 589;
        let segment = |windows: &[f32], count: usize| {
            let classes: Vec<usize> = (0..count * frames)
                .map(|i| {
                    let (w, f) = (i / frames, i % frames);
                    let center = (f * WINDOW_SIZE / frames).min(WINDOW_SIZE - 1);
                    let v = windows[w * WINDOW_SIZE + center];
                    if v > 0.0 {
                        1
                    } else if v < 0.0 {
                        2
                    } else {
                        0
                    }
                })
                .collect();
            Ok(logits_for(&classes))
        };
        let embed = |audio: &[f32]| {
            let mean = audio.iter().sum::<f32>() / audio.len().max(1) as f32;
            Ok(Some(if mean > 0.0 {
                vec![1.0, 0.1, 0.0]
            } else {
                vec![0.0, 0.1, 1.0]
            }))
        };
        let params = Params::for_samples(n, 0.5);
        let segs = diarize(&samples, &params, segment, embed).unwrap();
        let speakers: Vec<usize> = segs.iter().map(|s| s.speaker).collect();
        assert_eq!(speakers, vec![0, 1, 0], "{segs:?}");
        assert!((segs[0].start - 0.0).abs() < 0.1);
        assert!((segs[0].end - 12.0).abs() < 0.1, "{segs:?}");
        assert!((segs[1].start - 12.0).abs() < 0.1, "{segs:?}");
        assert!((segs[1].end - 22.0).abs() < 0.1, "{segs:?}");
        assert!((segs[2].end - 30.0).abs() < 0.1, "{segs:?}");
    }
}
