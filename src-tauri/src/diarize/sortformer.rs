//! NVIDIA Streaming Sortformer 4spk v2.1 (ONNX): who speaks when, as four
//! speaker probabilities every 80 ms. The streaming state (speaker cache and
//! FIFO of pre-encoder embeddings, cache compression, silence profile) is a
//! port of NeMo's `SortformerModules.streaming_update` / `_compress_spkcache`
//! at the schedule Vernacula (MIT, Chris Thompson) validated against NeMo:
//! chunks of 124 frames (9.92 s), FIFO 124, cache 188, update period 124.
//! The state logic is pure (no ONNX) so it is unit-tested on its own.

use super::mel::{MelFrontend, N_MELS, frame_count};
use anyhow::{Result, anyhow};
use ndarray::{Array1, Array3};
use ort::inputs;
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::TensorRef;
use std::path::Path;

pub const SPK: usize = 4;
pub const FRAME_SECS: f32 = 0.08;
const EMB: usize = 512;
const SUBSAMPLING: usize = 8;
const CHUNK_FRAMES: usize = 124;
const CHUNK_MEL: usize = CHUNK_FRAMES * SUBSAMPLING; // 992 mel frames = 9.92 s
const FIFO_LEN: usize = 124;
const CACHE_LEN: usize = 188;
const UPDATE_PERIOD: usize = 124;
const SIL_PER_SPK: usize = 3;
const SIL_THRESHOLD: f32 = 0.2;
const PRED_SCORE_THRESHOLD: f32 = 0.25;
const SCORES_BOOST_LATEST: f32 = 0.05;
const STRONG_BOOST_RATE: f32 = 0.75;
const WEAK_BOOST_RATE: f32 = 1.5;
const MIN_POS_SCORES_RATE: f32 = 0.5;

/// The streaming state between chunks.
#[derive(Default)]
pub struct StreamState {
    cache: Vec<f32>,
    /// Predictions for the cache rows; fresh from each pass until the first
    /// compression, then carried with the rows.
    cache_preds: Vec<f32>,
    compressed: bool,
    fifo: Vec<f32>,
    fifo_preds: Vec<f32>,
    mean_sil: Vec<f32>,
    n_sil: usize,
}

impl StreamState {
    pub fn new() -> Self {
        Self {
            mean_sil: vec![0.0; EMB],
            ..Default::default()
        }
    }

    pub fn cache_rows(&self) -> usize {
        self.cache.len() / EMB
    }

    pub fn fifo_rows(&self) -> usize {
        self.fifo.len() / EMB
    }

    pub fn cache(&self) -> &[f32] {
        &self.cache
    }

    pub fn fifo(&self) -> &[f32] {
        &self.fifo
    }

    /// Takes one model pass: `preds` = all output rows (cache ‖ FIFO ‖ chunk)
    /// x 4, `embs` = this chunk's embeddings x 512, `valid` = its valid frames.
    /// Returns the chunk's predictions (`valid` rows x 4).
    pub fn update(&mut self, preds: &[f32], embs: &[f32], valid: usize) -> Vec<f32> {
        let (cache_t, fifo_t) = (self.cache_rows(), self.fifo_rows());
        let valid = valid.min(embs.len() / EMB);
        let row = |r: usize| pred_row(preds, r);
        let mut chunk_preds = Vec::with_capacity(valid * SPK);
        for r in cache_t + fifo_t..cache_t + fifo_t + valid {
            chunk_preds.extend_from_slice(row(r));
        }
        // Fresh predictions for the frames already in the FIFO, then the chunk.
        let mut fifo_preds = Vec::with_capacity((fifo_t + valid) * SPK);
        for r in cache_t..cache_t + fifo_t {
            fifo_preds.extend_from_slice(row(r));
        }
        fifo_preds.extend_from_slice(&chunk_preds);
        self.fifo_preds = fifo_preds;
        self.fifo.extend_from_slice(&embs[..valid * EMB]);
        if !self.compressed {
            self.cache_preds = (0..cache_t).flat_map(|r| row(r).to_vec()).collect();
        }

        if self.fifo_rows() > FIFO_LEN {
            let pop = UPDATE_PERIOD
                .max((valid + fifo_t).saturating_sub(FIFO_LEN))
                .min(self.fifo_rows());
            let pop_embs: Vec<f32> = self.fifo.drain(..pop * EMB).collect();
            let pop_preds: Vec<f32> = self.fifo_preds.drain(..pop * SPK).collect();
            self.update_silence(&pop_embs, &pop_preds);
            self.cache.extend_from_slice(&pop_embs);
            self.cache_preds.extend_from_slice(&pop_preds);
            if self.cache_rows() > CACHE_LEN {
                let (cache, preds) = compress(&self.cache, &self.cache_preds, &self.mean_sil);
                self.cache = cache;
                self.cache_preds = preds;
                self.compressed = true;
            }
        }
        chunk_preds
    }

    /// Running mean of the embeddings of frames nobody speaks in.
    fn update_silence(&mut self, embs: &[f32], preds: &[f32]) {
        let rows = embs.len() / EMB;
        let mut sum = vec![0.0f32; EMB];
        let mut count = 0usize;
        for r in 0..rows {
            let total: f32 = preds[r * SPK..(r + 1) * SPK].iter().sum();
            if total < SIL_THRESHOLD {
                count += 1;
                for (s, e) in sum.iter_mut().zip(&embs[r * EMB..(r + 1) * EMB]) {
                    *s += e;
                }
            }
        }
        if count == 0 {
            return;
        }
        let n = self.n_sil + count;
        for (m, s) in self.mean_sil.iter_mut().zip(sum) {
            *m = (*m * self.n_sil as f32 + s) / n as f32;
        }
        self.n_sil = n;
    }
}

/// Row `r` of the model's predictions; zeros past its end (the int8 model
/// does not trim its output, the fp32 one does).
fn pred_row(preds: &[f32], r: usize) -> &[f32] {
    const ZERO: [f32; SPK] = [0.0; SPK];
    if (r + 1) * SPK <= preds.len() {
        &preds[r * SPK..(r + 1) * SPK]
    } else {
        &ZERO
    }
}

/// NeMo's `_compress_spkcache`: keep the 188 most telling cache frames, per
/// speaker, plus 3 silence slots each. Returns (embeddings, predictions).
pub fn compress(emb: &[f32], preds: &[f32], mean_sil: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let n = emb.len() / EMB;
    let per_spk = CACHE_LEN / SPK - SIL_PER_SPK; // 44
    let strong = (per_spk as f32 * STRONG_BOOST_RATE).floor() as usize; // 33
    let weak = (per_spk as f32 * WEAK_BOOST_RATE).floor() as usize; // 66
    let min_pos = (per_spk as f32 * MIN_POS_SCORES_RATE).floor() as usize; // 22
    let ln_half = 0.5f32.ln();

    // Log scores.
    let mut scores = vec![0.0f32; n * SPK];
    for t in 0..n {
        let p = &preds[t * SPK..(t + 1) * SPK];
        let log1_sum: f32 = p
            .iter()
            .map(|&x| (1.0 - x).max(PRED_SCORE_THRESHOLD).ln())
            .sum();
        for s in 0..SPK {
            scores[t * SPK + s] = p[s].max(PRED_SCORE_THRESHOLD).ln()
                - (1.0 - p[s]).max(PRED_SCORE_THRESHOLD).ln()
                + log1_sum
                - ln_half;
        }
    }
    // Only speech frames; with enough positive ones, drop the non-positive.
    for (i, sc) in scores.iter_mut().enumerate() {
        if preds[i] <= 0.5 {
            *sc = f32::NEG_INFINITY;
        }
    }
    for s in 0..SPK {
        let positives = (0..n).filter(|&t| scores[t * SPK + s] > 0.0).count();
        if positives >= min_pos {
            for t in 0..n {
                let i = t * SPK + s;
                if preds[i] > 0.5 && scores[i] <= 0.0 {
                    scores[i] = f32::NEG_INFINITY;
                }
            }
        }
    }
    // The newest frames get a small boost.
    for t in CACHE_LEN..n {
        for s in 0..SPK {
            scores[t * SPK + s] += SCORES_BOOST_LATEST;
        }
    }
    boost_topk(&mut scores, n, strong, -2.0 * ln_half);
    boost_topk(&mut scores, n, weak, -ln_half);

    // Speaker-major candidates, 3 silence slots (+inf) at the end of each.
    let ext = n + SIL_PER_SPK;
    let score_at = |s: usize, t: usize| -> f32 {
        if t >= n {
            f32::INFINITY
        } else {
            scores[t * SPK + s]
        }
    };
    let mut candidates: Vec<(f32, usize, usize)> = (0..SPK)
        .flat_map(|s| (0..ext).map(move |t| (s, t)))
        .map(|(s, t)| (score_at(s, t), t, s))
        .collect();
    // Highest first; ties by frame, then speaker (deterministic).
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    candidates.truncate(CACHE_LEN);
    // Back to speaker-major order; unusable picks (-inf) last.
    let mut picks: Vec<Option<(usize, usize)>> = candidates
        .iter()
        .map(|&(score, t, s)| (score != f32::NEG_INFINITY).then_some((s, t)))
        .collect();
    picks.sort_by_key(|p| p.map_or(usize::MAX, |(s, t)| s * ext + t));

    let mut out_emb = Vec::with_capacity(CACHE_LEN * EMB);
    let mut out_preds = Vec::with_capacity(CACHE_LEN * SPK);
    for pick in picks {
        match pick {
            Some((_, t)) if t < n => {
                out_emb.extend_from_slice(&emb[t * EMB..(t + 1) * EMB]);
                out_preds.extend_from_slice(&preds[t * SPK..(t + 1) * SPK]);
            }
            // A silence slot or nothing usable: the silence embedding.
            _ => {
                out_emb.extend_from_slice(mean_sil);
                out_preds.extend_from_slice(&[0.0; SPK]);
            }
        }
    }
    (out_emb, out_preds)
}

/// Adds `boost` to each speaker's `k` best frames (-inf stays -inf).
fn boost_topk(scores: &mut [f32], n: usize, k: usize, boost: f32) {
    for s in 0..SPK {
        let mut frames: Vec<usize> = (0..n).collect();
        frames.sort_by(|&a, &b| {
            scores[b * SPK + s]
                .total_cmp(&scores[a * SPK + s])
                .then(a.cmp(&b))
        });
        for &t in frames.iter().take(k) {
            scores[t * SPK + s] += boost;
        }
    }
}

/// The ONNX model, loaded for one job.
pub struct Sortformer {
    session: Session,
    mel: MelFrontend,
}

impl Sortformer {
    pub fn load(path: &Path, threads: usize) -> Result<Self> {
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(threads.max(1))?
            .commit_from_file(path)?;
        Ok(Self {
            session,
            mel: MelFrontend::new(),
        })
    }

    /// Speaker probabilities of `samples` (16 kHz mono), `[frames][4]` at
    /// 80 ms. `cancelled` is checked between chunks.
    pub fn probabilities(
        &mut self,
        samples: &[f32],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<f32>> {
        let total_mel = frame_count(samples.len());
        let chunks = total_mel.div_ceil(CHUNK_MEL);
        let mut state = StreamState::new();
        let mut out = Vec::with_capacity(chunks * CHUNK_FRAMES * SPK);
        for idx in 0..chunks {
            if cancelled() {
                return Err(anyhow!("cancelled"));
            }
            let mut frames = self.mel.frames(samples, idx * CHUNK_MEL, CHUNK_MEL);
            let current = frames.len() / N_MELS;
            frames.resize(CHUNK_MEL * N_MELS, 0.0);
            let chunk = Array3::from_shape_vec((1, CHUNK_MEL, N_MELS), frames)?;
            let chunk_len = Array1::from_vec(vec![current as i64]);
            let (cache_t, fifo_t) = (state.cache_rows(), state.fifo_rows());
            let cache = Array3::from_shape_vec((1, cache_t, EMB), state.cache().to_vec())?;
            let cache_len = Array1::from_vec(vec![cache_t as i64]);
            let fifo = Array3::from_shape_vec((1, fifo_t, EMB), state.fifo().to_vec())?;
            let fifo_len = Array1::from_vec(vec![fifo_t as i64]);
            let outputs = self.session.run(inputs![
                "chunk" => TensorRef::from_array_view(chunk.view())?,
                "chunk_lengths" => TensorRef::from_array_view(chunk_len.view())?,
                "spkcache" => TensorRef::from_array_view(cache.view())?,
                "spkcache_lengths" => TensorRef::from_array_view(cache_len.view())?,
                "fifo" => TensorRef::from_array_view(fifo.view())?,
                "fifo_lengths" => TensorRef::from_array_view(fifo_len.view())?,
            ])?;
            let preds: Vec<f32> = outputs
                .get("spkcache_fifo_chunk_preds")
                .ok_or_else(|| anyhow!("the model gave no speaker predictions"))?
                .try_extract_array::<f32>()?
                .iter()
                .copied()
                .collect();
            let embs: Vec<f32> = outputs
                .get("chunk_pre_encode_embs")
                .ok_or_else(|| anyhow!("the model gave no embeddings"))?
                .try_extract_array::<f32>()?
                .iter()
                .copied()
                .collect();
            let valid = current.div_ceil(SUBSAMPLING);
            out.extend(state.update(&preds, &embs, valid));
        }
        Ok(out)
    }
}

/// One speaker's stretch, in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub speaker: usize,
    pub start: f32,
    pub end: f32,
}

// NeMo's CALLHOME-tuned post-processing (as Vernacula uses it).
const MEDIAN_WINDOW: usize = 11;
const ONSET: f32 = 0.641;
const OFFSET: f32 = 0.561;
const PAD_ONSET: f32 = 0.229;
const PAD_OFFSET: f32 = 0.079;
const MIN_ON: f32 = 0.511;
const MIN_OFF: f32 = 0.296;

/// Median filter (window 11, truncated at the edges) per speaker.
pub fn smooth(probs: &[f32]) -> Vec<f32> {
    let frames = probs.len() / SPK;
    let half = MEDIAN_WINDOW / 2;
    let mut out = vec![0.0f32; probs.len()];
    let mut window = Vec::with_capacity(MEDIAN_WINDOW);
    for s in 0..SPK {
        for t in 0..frames {
            window.clear();
            let (lo, hi) = (t.saturating_sub(half), (t + half + 1).min(frames));
            window.extend((lo..hi).map(|u| probs[u * SPK + s]));
            window.sort_by(f32::total_cmp);
            let m = window.len();
            out[t * SPK + s] = if m % 2 == 1 {
                window[m / 2]
            } else {
                (window[m / 2 - 1] + window[m / 2]) / 2.0
            };
        }
    }
    out
}

/// Smoothed probabilities -> each speaker's segments (hysteresis, padding,
/// short ones dropped, short gaps closed). Sorted by start.
pub fn segments(smoothed: &[f32]) -> Vec<Segment> {
    let frames = smoothed.len() / SPK;
    let mut all = Vec::new();
    for s in 0..SPK {
        let mut raw: Vec<(f32, f32)> = Vec::new();
        let mut open: Option<usize> = None;
        for t in 0..frames {
            let p = smoothed[t * SPK + s];
            match open {
                None if p >= ONSET => open = Some(t),
                Some(start) if p < OFFSET => {
                    raw.push((start as f32 * FRAME_SECS, t as f32 * FRAME_SECS));
                    open = None;
                }
                _ => {}
            }
        }
        if let Some(start) = open {
            raw.push((start as f32 * FRAME_SECS, frames as f32 * FRAME_SECS));
        }
        let mut kept: Vec<(f32, f32)> = Vec::new();
        for (a, b) in raw {
            let (a, b) = ((a - PAD_ONSET).max(0.0), b + PAD_OFFSET);
            if b - a < MIN_ON {
                continue;
            }
            match kept.last_mut() {
                Some(last) if a - last.1 < MIN_OFF => last.1 = last.1.max(b),
                _ => kept.push((a, b)),
            }
        }
        all.extend(kept.into_iter().map(|(start, end)| Segment {
            speaker: s,
            start,
            end,
        }));
    }
    all.sort_by(|a, b| a.start.total_cmp(&b.start));
    all
}

/// One speaker turn of a recording, in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Turn {
    pub speaker: usize,
    pub start: f32,
    pub end: f32,
}

/// Turns shorter than this join a neighbour (too short to transcribe well).
const MIN_TURN_SECS: f32 = 1.0;

/// Splits a recording of `total_secs` into speaker turns that cover it all:
/// each 80 ms frame goes to the most likely speaker among those active there;
/// a pause is shared between the turns around it; turns under a second join
/// the one before them. No speech found: one turn, speaker 0.
pub fn turns(smoothed: &[f32], segs: &[Segment], total_secs: f32) -> Vec<Turn> {
    let frames = smoothed.len() / SPK;
    let label: Vec<Option<usize>> = (0..frames)
        .map(|t| {
            let mid = (t as f32 + 0.5) * FRAME_SECS;
            (0..SPK)
                .filter(|&s| {
                    segs.iter()
                        .any(|g| g.speaker == s && g.start <= mid && mid < g.end)
                })
                .max_by(|&a, &b| smoothed[t * SPK + a].total_cmp(&smoothed[t * SPK + b]))
        })
        .collect();
    // Runs of labelled frames (speaker, first frame, end frame), same speaker
    // across a pause kept as one run.
    let mut runs: Vec<(usize, usize, usize)> = Vec::new();
    for (t, l) in label.iter().enumerate() {
        let Some(s) = *l else { continue };
        match runs.last_mut() {
            Some(last) if last.0 == s => last.2 = t + 1,
            _ => runs.push((s, t, t + 1)),
        }
    }
    if runs.is_empty() {
        return vec![Turn {
            speaker: 0,
            start: 0.0,
            end: total_secs,
        }];
    }
    // Cover the whole recording: cut each pause in its middle.
    let mut bounds: Vec<(usize, f32, f32)> = Vec::with_capacity(runs.len());
    for (i, &(s, first, end)) in runs.iter().enumerate() {
        let start = if i == 0 {
            0.0
        } else {
            let prev_end = runs[i - 1].2;
            (prev_end + first) as f32 / 2.0 * FRAME_SECS
        };
        let stop = if i + 1 == runs.len() {
            total_secs.max(end as f32 * FRAME_SECS)
        } else {
            (end + runs[i + 1].1) as f32 / 2.0 * FRAME_SECS
        };
        bounds.push((s, start, stop));
    }
    // Short turns join their neighbour (the one before, or after for the first).
    loop {
        let Some(i) = (0..bounds.len())
            .filter(|&i| bounds.len() > 1 && bounds[i].2 - bounds[i].1 < MIN_TURN_SECS)
            .min_by(|&a, &b| (bounds[a].2 - bounds[a].1).total_cmp(&(bounds[b].2 - bounds[b].1)))
        else {
            break;
        };
        if i > 0 {
            bounds[i - 1].2 = bounds[i].2;
        } else {
            bounds[1].1 = bounds[0].1;
        }
        bounds.remove(i);
        // Neighbours of the same speaker are one turn.
        let mut merged: Vec<(usize, f32, f32)> = Vec::with_capacity(bounds.len());
        for b in bounds.drain(..) {
            match merged.last_mut() {
                Some(last) if last.0 == b.0 => last.2 = b.2,
                _ => merged.push(b),
            }
        }
        bounds = merged;
    }
    bounds
        .into_iter()
        .map(|(speaker, start, end)| Turn {
            speaker,
            start,
            end,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probs(rows: &[[f32; SPK]]) -> Vec<f32> {
        rows.iter().flatten().copied().collect()
    }

    #[test]
    fn first_chunk_fills_the_fifo_and_returns_its_frames() {
        let mut st = StreamState::new();
        let preds = vec![0.9f32; CHUNK_FRAMES * SPK];
        let embs = vec![1.0f32; CHUNK_FRAMES * EMB];
        let out = st.update(&preds, &embs, CHUNK_FRAMES);
        assert_eq!(out.len(), CHUNK_FRAMES * SPK);
        assert_eq!((st.cache_rows(), st.fifo_rows()), (0, CHUNK_FRAMES));
    }

    #[test]
    fn steady_state_is_cache_188_fifo_124() {
        let mut st = StreamState::new();
        for _ in 0..5 {
            let rows = st.cache_rows() + st.fifo_rows() + CHUNK_FRAMES;
            // Speaker 0 then 1 alternate so compression has speech to keep.
            let preds: Vec<f32> = (0..rows)
                .flat_map(|r| {
                    if r % 2 == 0 {
                        [0.95, 0.02, 0.0, 0.0]
                    } else {
                        [0.03, 0.9, 0.0, 0.0]
                    }
                })
                .collect();
            let embs: Vec<f32> = (0..CHUNK_FRAMES * EMB).map(|i| (i % 7) as f32).collect();
            let out = st.update(&preds, &embs, CHUNK_FRAMES);
            assert_eq!(out.len(), CHUNK_FRAMES * SPK);
        }
        assert_eq!((st.cache_rows(), st.fifo_rows()), (CACHE_LEN, FIFO_LEN));
    }

    #[test]
    fn a_short_last_chunk_returns_only_its_valid_frames() {
        let mut st = StreamState::new();
        let preds = vec![0.5f32; CHUNK_FRAMES * SPK];
        let embs = vec![0.0f32; CHUNK_FRAMES * EMB];
        assert_eq!(st.update(&preds, &embs, 63).len(), 63 * SPK);
        assert_eq!(st.fifo_rows(), 63);
    }

    #[test]
    fn compression_keeps_188_rows_with_silence_slots() {
        let n = 248;
        let preds: Vec<f32> = (0..n)
            .flat_map(|t| {
                if t < 124 {
                    [0.9, 0.0, 0.0, 0.0]
                } else {
                    [0.0, 0.8, 0.0, 0.0]
                }
            })
            .collect();
        let emb: Vec<f32> = (0..n).flat_map(|t| vec![t as f32; EMB]).collect();
        let sil = vec![-1.0f32; EMB];
        let (e, p) = compress(&emb, &preds, &sil);
        assert_eq!(e.len(), CACHE_LEN * EMB);
        assert_eq!(p.len(), CACHE_LEN * SPK);
        // Every speaker block ends with its 3 silence slots (or unused rows),
        // so at least 12 rows carry the silence embedding.
        let silent = (0..CACHE_LEN).filter(|&r| e[r * EMB] == -1.0).count();
        assert!(silent >= SPK * SIL_PER_SPK, "{silent}");
        // Kept frames of speaker 0 come before those of speaker 1.
        let first_spk1 = (0..CACHE_LEN).find(|&r| p[r * SPK + 1] > 0.5).unwrap();
        let last_spk0 = (0..CACHE_LEN).rev().find(|&r| p[r * SPK] > 0.5).unwrap();
        assert!(last_spk0 < first_spk1);
    }

    #[test]
    fn two_speakers_become_two_turns() {
        // 4 s of speaker 0, 0.4 s pause, 4 s of speaker 1 (80 ms frames).
        let mut rows = Vec::new();
        rows.extend(std::iter::repeat([0.95, 0.0, 0.0, 0.0]).take(50));
        rows.extend(std::iter::repeat([0.0, 0.0, 0.0, 0.0]).take(5));
        rows.extend(std::iter::repeat([0.0, 0.95, 0.0, 0.0]).take(50));
        let p = smooth(&probs(&rows));
        let segs = segments(&p);
        assert_eq!(segs.iter().filter(|s| s.speaker == 0).count(), 1);
        assert_eq!(segs.iter().filter(|s| s.speaker == 1).count(), 1);
        let t = turns(&p, &segs, 105.0 * FRAME_SECS);
        assert_eq!(t.len(), 2);
        assert_eq!((t[0].speaker, t[1].speaker), (0, 1));
        assert_eq!(t[0].start, 0.0);
        assert_eq!(t[0].end, t[1].start);
        assert!((t[1].end - 105.0 * FRAME_SECS).abs() < 1e-4);
        assert!((t[0].end - 4.2).abs() < 0.1, "{}", t[0].end);
    }

    #[test]
    fn a_blip_joins_the_turn_around_it() {
        let mut rows = Vec::new();
        rows.extend(std::iter::repeat([0.95, 0.0, 0.0, 0.0]).take(40));
        rows.extend(std::iter::repeat([0.0, 0.95, 0.0, 0.0]).take(9)); // 0.72 s
        rows.extend(std::iter::repeat([0.95, 0.0, 0.0, 0.0]).take(40));
        let p = probs(&rows);
        let segs = segments(&smooth(&p));
        let t = turns(&smooth(&p), &segs, 89.0 * FRAME_SECS);
        assert_eq!(t.len(), 1, "{t:?}");
        assert_eq!(t[0].speaker, 0);
    }

    #[test]
    fn no_speech_is_one_turn() {
        let p = vec![0.0f32; 40 * SPK];
        let t = turns(&p, &segments(&p), 3.2);
        assert_eq!(
            t,
            vec![Turn {
                speaker: 0,
                start: 0.0,
                end: 3.2
            }]
        );
    }

    #[test]
    fn median_filter_removes_a_single_spike() {
        let mut rows = vec![[0.0f32; SPK]; 21];
        rows[10] = [1.0, 0.0, 0.0, 0.0];
        let s = smooth(&probs(&rows));
        assert_eq!(s[10 * SPK], 0.0);
    }
}
