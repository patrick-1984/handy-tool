use rustfft::{Fft, FftPlanner, num_complex::Complex32};
use std::sync::Arc;

const DB_MIN: f32 = -55.0;
const DB_MAX: f32 = -8.0;
const GAIN: f32 = 1.3;
const CURVE_POWER: f32 = 0.7;

pub struct AudioVisualiser {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    bucket_ranges: Vec<(usize, usize)>,
    fft_input: Vec<Complex32>,
    noise_floor: Vec<f32>,
    buffer: Vec<f32>,
    window_size: usize,
    buckets: usize,
    /// Per-band lift in dB for the higher bands (see `with_tilt`).
    tilt_db: Vec<f32>,
    bucket_centers_hz: Vec<f32>,
}

impl AudioVisualiser {
    pub fn new(
        sample_rate: u32,
        window_size: usize,
        buckets: usize,
        freq_min: f32,
        freq_max: f32,
    ) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(window_size);

        // Pre-compute Hann window
        let window: Vec<f32> = (0..window_size)
            .map(|i| {
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / window_size as f32).cos())
            })
            .collect();

        // Pre-compute bucket frequency ranges
        let nyquist = sample_rate as f32 / 2.0;
        let freq_min = freq_min.min(nyquist);
        let freq_max = freq_max.min(nyquist);

        let mut bucket_ranges = Vec::with_capacity(buckets);
        let mut bucket_centers_hz = Vec::with_capacity(buckets);

        for b in 0..buckets {
            // Narrower bands at the low end, where a voice has most detail - but
            // not so narrow that neighbouring bars read the same FFT bin and
            // move as twins (the square law did that to the first four of 23).
            let log_start = (b as f32 / buckets as f32).powf(1.5);
            let log_end = ((b + 1) as f32 / buckets as f32).powf(1.5);

            let start_hz = freq_min + (freq_max - freq_min) * log_start;
            let end_hz = freq_min + (freq_max - freq_min) * log_end;
            bucket_centers_hz.push((start_hz + end_hz) / 2.0);

            let start_bin = ((start_hz * window_size as f32) / sample_rate as f32) as usize;
            let mut end_bin = ((end_hz * window_size as f32) / sample_rate as f32) as usize;

            // Ensure each bucket has at least one bin
            if end_bin <= start_bin {
                end_bin = start_bin + 1;
            }

            // Clamp to valid range
            let start_bin = start_bin.min(window_size / 2);
            let end_bin = end_bin.min(window_size / 2);

            bucket_ranges.push((start_bin, end_bin));
        }

        Self {
            fft,
            window,
            bucket_ranges,
            fft_input: vec![Complex32::new(0.0, 0.0); window_size],
            noise_floor: vec![-40.0; buckets], // Initialize to reasonable noise floor
            buffer: Vec::with_capacity(window_size * 2),
            window_size,
            buckets,
            tilt_db: vec![0.0; buckets],
            bucket_centers_hz,
        }
    }

    /// Lifts the bands above `pivot_hz` by `db_per_octave`: a voice has most of
    /// its energy low, so without it the right-hand bars barely move.
    pub fn with_tilt(mut self, db_per_octave: f32, pivot_hz: f32) -> Self {
        self.tilt_db = self
            .bucket_centers_hz
            .iter()
            .map(|&hz| db_per_octave * (hz.max(pivot_hz) / pivot_hz).log2())
            .collect();
        self
    }

    pub fn feed(&mut self, samples: &[f32]) -> Option<Vec<f32>> {
        // Add new samples to buffer
        self.buffer.extend_from_slice(samples);

        // Only process if we have enough samples
        if self.buffer.len() < self.window_size {
            return None;
        }

        // Take the required window of samples
        let window_samples = &self.buffer[..self.window_size];

        // Remove DC component
        let mean = window_samples.iter().sum::<f32>() / self.window_size as f32;

        // Apply window function and prepare FFT input
        for (i, &sample) in window_samples.iter().enumerate() {
            let windowed_sample = (sample - mean) * self.window[i];
            self.fft_input[i] = Complex32::new(windowed_sample, 0.0);
        }

        // Perform FFT
        self.fft.process(&mut self.fft_input);

        // Compute power spectrum and bucket levels
        let mut buckets = vec![0.0; self.buckets];

        for (bucket_idx, &(start_bin, end_bin)) in self.bucket_ranges.iter().enumerate() {
            if start_bin >= end_bin || end_bin > self.fft_input.len() / 2 {
                continue;
            }

            // Calculate average power in this frequency range
            let mut power_sum = 0.0;
            for bin_idx in start_bin..end_bin {
                let magnitude = self.fft_input[bin_idx].norm();
                power_sum += magnitude * magnitude;
            }

            let avg_power = power_sum / (end_bin - start_bin) as f32;

            // Convert to dB with proper scaling
            let db = if avg_power > 1e-12 {
                20.0 * (avg_power.sqrt() / self.window_size as f32).log10()
                    + self.tilt_db[bucket_idx]
            } else {
                -80.0 // Very low floor for zero power
            };

            // Only update noise floor when signal is quiet (below current floor + 10dB)
            if db < self.noise_floor[bucket_idx] + 10.0 {
                const NOISE_ALPHA: f32 = 0.001; // Very slow adaptation
                self.noise_floor[bucket_idx] =
                    NOISE_ALPHA * db + (1.0 - NOISE_ALPHA) * self.noise_floor[bucket_idx];
            }

            // Map configurable dB range to 0-1 with gain and curve shaping
            let normalized = ((db - DB_MIN) / (DB_MAX - DB_MIN)).clamp(0.0, 1.0);
            buckets[bucket_idx] = (normalized * GAIN).powf(CURVE_POWER).clamp(0.0, 1.0);
        }

        // Apply light smoothing to reduce jitter
        for i in 1..buckets.len() - 1 {
            buckets[i] = buckets[i] * 0.7 + buckets[i - 1] * 0.15 + buckets[i + 1] * 0.15;
        }

        // Clear processed samples from buffer
        self.buffer.clear();

        Some(buckets)
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        // Reset noise floor to initial values
        self.noise_floor.fill(-40.0);
    }
}

#[cfg(test)]
mod tilt_tests {
    use super::AudioVisualiser;

    #[test]
    fn higher_bands_are_lifted_by_octaves_above_the_pivot() {
        let vis = AudioVisualiser::new(48_000, 512, 23, 400.0, 4000.0).with_tilt(8.0, 500.0);
        // Bands at or below 500 Hz are left alone...
        assert_eq!(vis.tilt_db[0], 0.0);
        // ...and the top band (about 3.8 kHz, ~2.9 octaves up) gets about +23 dB.
        let top = vis.tilt_db[22];
        assert!((22.0..25.0).contains(&top), "top band lift {top}");
        // Never less for a higher band.
        assert!(vis.tilt_db.windows(2).all(|w| w[1] >= w[0]));
    }
}
