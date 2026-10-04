//! Kaldi-compatible log mel filterbank features for the speaker-embedding model.
//!
//! WeSpeaker models are trained on Kaldi `compute-fbank-feats` output, so the
//! frontend has to match it closely: 25 ms Hamming windows every 10 ms with
//! `snip_edges`, DC removal, 0.97 pre-emphasis, a 512-point power spectrum and
//! 80 triangular mel bins between 20 Hz and Nyquist, then the natural log. No
//! dither (we want deterministic embeddings). Input samples are in [-1, 1] and
//! are scaled to the 16-bit range first, as WeSpeaker does. Cepstral mean
//! normalisation (per-utterance mean subtraction) is not part of the ONNX
//! graph, so [`compute_fbank`] applies it when asked.

use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

pub const SAMPLE_RATE: f32 = 16_000.0;
pub const NUM_MEL_BINS: usize = 80;
const FRAME_LENGTH: usize = 400; // 25 ms
const FRAME_SHIFT: usize = 160; // 10 ms
const PADDED_LENGTH: usize = 512;
const PREEMPH: f32 = 0.97;
const LOW_FREQ: f32 = 20.0;
const WAVE_SCALE: f32 = 32_768.0;

fn mel_scale(freq: f32) -> f32 {
    1127.0 * (1.0 + freq / 700.0).ln()
}

/// Precomputed window, mel filters and FFT plan; reuse across calls.
pub struct Fbank {
    window: Vec<f32>,
    /// Per mel bin: first FFT bin index and the non-zero weights from there.
    mel_banks: Vec<(usize, Vec<f32>)>,
    fft: Arc<dyn Fft<f32>>,
}

impl Default for Fbank {
    fn default() -> Self {
        Self::new()
    }
}

impl Fbank {
    pub fn new() -> Self {
        let a = 2.0 * std::f64::consts::PI / (FRAME_LENGTH - 1) as f64;
        let window = (0..FRAME_LENGTH)
            .map(|i| (0.54 - 0.46 * (a * i as f64).cos()) as f32)
            .collect();

        // Kaldi MelBanks with htk_mode=false, high_freq = Nyquist. Only the
        // first PADDED_LENGTH / 2 bins take part (Kaldi drops the Nyquist bin).
        let num_fft_bins = PADDED_LENGTH / 2;
        let fft_bin_width = SAMPLE_RATE / PADDED_LENGTH as f32;
        let mel_low = mel_scale(LOW_FREQ);
        let mel_high = mel_scale(SAMPLE_RATE / 2.0);
        let mel_delta = (mel_high - mel_low) / (NUM_MEL_BINS + 1) as f32;
        let mel_banks = (0..NUM_MEL_BINS)
            .map(|bin| {
                let left = mel_low + bin as f32 * mel_delta;
                let center = mel_low + (bin + 1) as f32 * mel_delta;
                let right = mel_low + (bin + 2) as f32 * mel_delta;
                let mut first = None;
                let mut weights = Vec::new();
                for i in 0..num_fft_bins {
                    let mel = mel_scale(fft_bin_width * i as f32);
                    if mel > left && mel < right {
                        let w = if mel <= center {
                            (mel - left) / (center - left)
                        } else {
                            (right - mel) / (right - center)
                        };
                        first.get_or_insert(i);
                        weights.push(w);
                    }
                }
                (first.unwrap_or(0), weights)
            })
            .collect();

        let fft = FftPlanner::<f32>::new().plan_fft_forward(PADDED_LENGTH);
        Self {
            window,
            mel_banks,
            fft,
        }
    }

    /// Number of frames Kaldi produces for `num_samples` with `snip_edges`.
    pub fn num_frames(num_samples: usize) -> usize {
        if num_samples < FRAME_LENGTH {
            0
        } else {
            1 + (num_samples - FRAME_LENGTH) / FRAME_SHIFT
        }
    }

    /// Row-major `[num_frames][NUM_MEL_BINS]` log mel energies.
    /// With `cmn`, the per-bin mean over all frames is subtracted.
    pub fn compute(&self, samples: &[f32], cmn: bool) -> Vec<f32> {
        let num_frames = Self::num_frames(samples.len());
        let mut out = Vec::with_capacity(num_frames * NUM_MEL_BINS);
        let mut frame = [0f32; FRAME_LENGTH];
        let mut buf = vec![Complex32::new(0.0, 0.0); PADDED_LENGTH];
        let mut scratch = vec![Complex32::new(0.0, 0.0); self.fft.get_inplace_scratch_len()];
        let mut power = [0f32; PADDED_LENGTH / 2 + 1];

        for f in 0..num_frames {
            let start = f * FRAME_SHIFT;
            for (dst, src) in frame.iter_mut().zip(&samples[start..start + FRAME_LENGTH]) {
                *dst = src * WAVE_SCALE;
            }

            let mean = frame.iter().sum::<f32>() / FRAME_LENGTH as f32;
            frame.iter_mut().for_each(|s| *s -= mean);

            for i in (1..FRAME_LENGTH).rev() {
                frame[i] -= PREEMPH * frame[i - 1];
            }
            frame[0] -= PREEMPH * frame[0];

            for (i, c) in buf.iter_mut().enumerate() {
                let v = if i < FRAME_LENGTH {
                    frame[i] * self.window[i]
                } else {
                    0.0
                };
                *c = Complex32::new(v, 0.0);
            }
            self.fft.process_with_scratch(&mut buf, &mut scratch);
            for (p, c) in power.iter_mut().zip(&buf) {
                *p = c.norm_sqr();
            }

            for (first, weights) in &self.mel_banks {
                let energy: f32 = weights
                    .iter()
                    .zip(&power[*first..])
                    .map(|(w, p)| w * p)
                    .sum();
                out.push(energy.max(f32::EPSILON).ln());
            }
        }

        if cmn && num_frames > 0 {
            let mut means = [0f64; NUM_MEL_BINS];
            for (i, v) in out.iter().enumerate() {
                means[i % NUM_MEL_BINS] += *v as f64;
            }
            for m in means.iter_mut() {
                *m /= num_frames as f64;
            }
            for (i, v) in out.iter_mut().enumerate() {
                *v -= means[i % NUM_MEL_BINS] as f32;
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic test signal: two tones plus a chirp-ish term and a DC
    /// offset, 0.25 s at 16 kHz. Mirrored exactly by the Python generator
    /// that produced the reference values below.
    fn test_signal() -> Vec<f32> {
        (0..4000)
            .map(|i| {
                let t = i as f64 / 16_000.0;
                let v = 0.3 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()
                    + 0.2 * (2.0 * std::f64::consts::PI * 1234.5 * t).sin()
                    + 0.1 * (2.0 * std::f64::consts::PI * (200.0 + 3000.0 * t) * t).sin()
                    + 0.01;
                v as f32
            })
            .collect()
    }

    // Reference values from kaldi-native-fbank 1.22 (Python): FbankOptions
    // with dither=0, snip_edges=True, window_type="hamming", samp_freq=16000,
    // num_bins=80, everything else default (remove_dc_offset, preemph 0.97,
    // low 20 Hz, high = Nyquist, log power), fed `test_signal() * 32768`.
    // Frames 0, 7 and 22 at bins 0, 5, 17, 40, 63, 79.
    const REF_BINS: [usize; 6] = [0, 5, 17, 40, 63, 79];
    const REF: [(usize, [f32; 6]); 3] = [
        (
            0,
            [13.10405, 15.15568, 13.91601, 14.58684, 13.05701, 12.95074],
        ),
        (
            7,
            [12.62808, 13.03083, 14.34994, 14.29592, 11.95935, 11.71864],
        ),
        (
            22,
            [13.06524, 12.93227, 13.42619, 14.05879, 10.49253, 9.98073],
        ),
    ];

    #[test]
    fn frame_count_matches_kaldi_snip_edges() {
        assert_eq!(Fbank::num_frames(0), 0);
        assert_eq!(Fbank::num_frames(399), 0);
        assert_eq!(Fbank::num_frames(400), 1);
        assert_eq!(Fbank::num_frames(559), 1);
        assert_eq!(Fbank::num_frames(560), 2);
        assert_eq!(Fbank::num_frames(16_000), 98);
    }

    #[test]
    fn matches_kaldi_native_fbank_reference() {
        let feats = Fbank::new().compute(&test_signal(), false);
        assert_eq!(feats.len(), Fbank::num_frames(4000) * NUM_MEL_BINS);
        for (frame, expected) in REF {
            for (b, want) in REF_BINS.iter().zip(expected) {
                let got = feats[frame * NUM_MEL_BINS + b];
                assert!(
                    (got - want).abs() < 2e-3 * want.abs().max(1.0),
                    "frame {frame} bin {b}: got {got}, want {want}"
                );
            }
        }
    }

    #[test]
    fn cmn_zeroes_the_per_bin_mean() {
        let feats = Fbank::new().compute(&test_signal(), true);
        let frames = feats.len() / NUM_MEL_BINS;
        for b in 0..NUM_MEL_BINS {
            let mean: f32 = (0..frames)
                .map(|f| feats[f * NUM_MEL_BINS + b])
                .sum::<f32>()
                / frames as f32;
            assert!(mean.abs() < 1e-3, "bin {b} mean {mean}");
        }
    }
}
