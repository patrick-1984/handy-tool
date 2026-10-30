//! Sortformer's front end: 128 log-mel bands every 10 ms, computed as NeMo's
//! `AudioToMelSpectrogramPreprocessor` does for this model (`normalize: NA`):
//! pre-emphasis 0.97, a centred 512-point FFT of a symmetric 400-sample Hann
//! window, power spectrum, Slaney mel filters (0-8 kHz, area-normalised) and
//! `ln(x + 2^-24)`. No normalisation, no dither. (Parakeet's `nemo128.onnx`
//! normalises per band, so it cannot be reused here.)

use rustfft::{FftPlanner, num_complex::Complex};
use std::sync::Arc;

pub const N_MELS: usize = 128;
pub const HOP: usize = 160;
const N_FFT: usize = 512;
const WIN: usize = 400;
const BINS: usize = N_FFT / 2 + 1;
const SAMPLE_RATE: f32 = 16_000.0;
const PREEMPH: f32 = 0.97;
const LOG_GUARD: f32 = 5.960_464_5e-8; // 2^-24

/// Frames for `samples` samples (centred frames, one every 10 ms).
pub fn frame_count(samples: usize) -> usize {
    samples / HOP + 1
}

// Slaney mel scale (librosa's default, `htk=False`).
fn hz_to_mel(hz: f32) -> f32 {
    let f_sp = 200.0 / 3.0;
    let min_log_hz = 1000.0;
    let min_log_mel = min_log_hz / f_sp;
    let logstep = (6.4f32).ln() / 27.0;
    if hz >= min_log_hz {
        min_log_mel + (hz / min_log_hz).ln() / logstep
    } else {
        hz / f_sp
    }
}

fn mel_to_hz(mel: f32) -> f32 {
    let f_sp = 200.0 / 3.0;
    let min_log_hz = 1000.0;
    let min_log_mel = min_log_hz / f_sp;
    let logstep = (6.4f32).ln() / 27.0;
    if mel >= min_log_mel {
        min_log_hz * (logstep * (mel - min_log_mel)).exp()
    } else {
        mel * f_sp
    }
}

/// `librosa.filters.mel(sr=16000, n_fft=512, n_mels=128, norm="slaney")`,
/// row-major `[N_MELS][BINS]`.
fn filterbank() -> Vec<f32> {
    let fmax = SAMPLE_RATE / 2.0;
    let (mel_lo, mel_hi) = (hz_to_mel(0.0), hz_to_mel(fmax));
    let mel_f: Vec<f32> = (0..N_MELS + 2)
        .map(|i| mel_to_hz(mel_lo + (mel_hi - mel_lo) * i as f32 / (N_MELS + 1) as f32))
        .collect();
    let fft_freqs: Vec<f32> = (0..BINS)
        .map(|k| fmax * k as f32 / (BINS - 1) as f32)
        .collect();
    let mut weights = vec![0.0f32; N_MELS * BINS];
    for i in 0..N_MELS {
        let enorm = 2.0 / (mel_f[i + 2] - mel_f[i]);
        let (d0, d1) = (mel_f[i + 1] - mel_f[i], mel_f[i + 2] - mel_f[i + 1]);
        for (k, &f) in fft_freqs.iter().enumerate() {
            let lower = (f - mel_f[i]) / d0;
            let upper = (mel_f[i + 2] - f) / d1;
            weights[i * BINS + k] = lower.min(upper).max(0.0) * enorm;
        }
    }
    weights
}

/// Computes log-mel frames of one recording, a range at a time (a long take
/// never needs all its frames in memory at once).
pub struct MelFrontend {
    window: Vec<f32>,
    filters: Vec<f32>,
    fft: Arc<dyn rustfft::Fft<f32>>,
}

impl Default for MelFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl MelFrontend {
    pub fn new() -> Self {
        // Symmetric Hann (NeMo: periodic=False), centred in the FFT frame.
        let mut window = vec![0.0f32; N_FFT];
        let offset = (N_FFT - WIN) / 2;
        for n in 0..WIN {
            window[offset + n] =
                0.5 - 0.5 * (2.0 * std::f32::consts::PI * n as f32 / (WIN - 1) as f32).cos();
        }
        Self {
            window,
            filters: filterbank(),
            fft: FftPlanner::new().plan_fft_forward(N_FFT),
        }
    }

    /// Frames `start..start + count` of `samples`, time-major `[count][N_MELS]`
    /// (frames past the end are not produced: `count` is clipped).
    pub fn frames(&self, samples: &[f32], start: usize, count: usize) -> Vec<f32> {
        let total = frame_count(samples.len());
        let end = start.saturating_add(count).min(total);
        let half = (N_FFT / 2) as isize;
        // Pre-emphasised sample i (zero outside the recording).
        let pre = |i: isize| -> f32 {
            if i < 0 || i as usize >= samples.len() {
                return 0.0;
            }
            let i = i as usize;
            if i == 0 {
                samples[0]
            } else {
                samples[i] - PREEMPH * samples[i - 1]
            }
        };
        let mut out = Vec::with_capacity(end.saturating_sub(start) * N_MELS);
        let mut buf = vec![Complex::new(0.0f32, 0.0); N_FFT];
        let mut power = [0.0f32; BINS];
        for f in start..end {
            let first = (f * HOP) as isize - half;
            for (n, slot) in buf.iter_mut().enumerate() {
                *slot = Complex::new(pre(first + n as isize) * self.window[n], 0.0);
            }
            self.fft.process(&mut buf);
            for (k, p) in power.iter_mut().enumerate() {
                *p = buf[k].norm_sqr();
            }
            for m in 0..N_MELS {
                let row = &self.filters[m * BINS..(m + 1) * BINS];
                let energy: f32 = row.iter().zip(power.iter()).map(|(w, p)| w * p).sum();
                out.push((energy + LOG_GUARD).ln());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_centred_every_ten_ms() {
        assert_eq!(frame_count(0), 1);
        assert_eq!(frame_count(16_000), 101);
        let mel = MelFrontend::new();
        let samples = vec![0.0f32; 16_000];
        assert_eq!(mel.frames(&samples, 0, 1000).len(), 101 * N_MELS);
        assert_eq!(mel.frames(&samples, 100, 5).len(), N_MELS);
    }

    #[test]
    fn silence_is_the_log_guard() {
        let mel = MelFrontend::new();
        let out = mel.frames(&[0.0; 3200], 0, 3);
        assert!(out.iter().all(|v| (v - LOG_GUARD.ln()).abs() < 1e-3));
    }

    #[test]
    fn a_tone_lands_in_its_mel_band() {
        // 1 kHz sine: the loudest band is the one whose centre is nearest 1 kHz.
        let samples: Vec<f32> = (0..16_000)
            .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 16_000.0).sin())
            .collect();
        let mel = MelFrontend::new();
        let frame = mel.frames(&samples, 50, 1);
        let loudest = (0..N_MELS)
            .max_by(|&a, &b| frame[a].total_cmp(&frame[b]))
            .unwrap();
        let (lo, hi) = (hz_to_mel(0.0), hz_to_mel(8000.0));
        let centre = |i: usize| mel_to_hz(lo + (hi - lo) * (i + 1) as f32 / (N_MELS + 1) as f32);
        assert!(
            (centre(loudest) - 1000.0).abs() < 60.0,
            "{}",
            centre(loudest)
        );
    }

    #[test]
    fn slaney_scale_round_trips() {
        for hz in [0.0, 300.0, 999.0, 1000.0, 4000.0, 8000.0] {
            assert!((mel_to_hz(hz_to_mel(hz)) - hz).abs() < 0.05, "{hz}");
        }
    }

    #[test]
    fn a_range_matches_the_same_frames_of_the_whole() {
        let samples: Vec<f32> = (0..8000)
            .map(|i| ((i * 7919) % 101) as f32 / 101.0 - 0.5)
            .collect();
        let mel = MelFrontend::new();
        let whole = mel.frames(&samples, 0, usize::MAX);
        let part = mel.frames(&samples, 10, 7);
        assert_eq!(&whole[10 * N_MELS..17 * N_MELS], &part[..]);
    }
}
