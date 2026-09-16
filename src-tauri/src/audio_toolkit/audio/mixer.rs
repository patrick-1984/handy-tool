//! Mixing a microphone leg with a system-audio (loopback) leg.
//!
//! Everything here is PURE: no cpal, no threads, no clock. That is deliberate - the
//! failure modes of this feature are silent (a drifting mix yields a fluent but wrong
//! transcript; a mis-scaled leg yields text the VAD discards as noise), so the logic
//! that can be wrong has to be testable without audio hardware.
//!
//! Two facts measured on real hardware shape the whole design:
//!
//! 1. **A loopback endpoint delivers NO callbacks while nothing is playing** - not
//!    buffers of zeros, nothing at all. Probe: 0 callbacks across 5 s idle, then 801
//!    the instant audio started. So the mic drives the clock and the loopback leg is
//!    a slave read through a ring; anything that *waits* for both legs deadlocks the
//!    moment the far end goes quiet.
//! 2. The two endpoints run on independent clocks, so the slave ring drifts.

use std::collections::VecDeque;

/// Target occupancy of the slave ring, in samples at 16 kHz (100 ms).
///
/// This is also a deliberate ~100 ms delay on the system-audio leg. That is not a
/// defect: it is the buffer that absorbs jitter between two independent clocks, and
/// at these magnitudes it is far below anything the transcription can perceive.
pub const RING_TARGET: usize = 1_600;

/// Hard ceiling on ring occupancy (2 s at 16 kHz). Reached only when the far end has
/// played continuously for minutes without a quiet moment to splice in.
pub const RING_HIGH_WATER: usize = 32_000;

/// Correction batch, in samples at 16 kHz (10 ms).
///
/// Corrections are batched rather than per-sample. At 50 ppm the drift is ~0.8
/// samples/second, and a single-sample splice at a waveform peak is a broadband
/// click - 0.8 of those per second is audible crackle and a fresh transient into the
/// VAD every ~1.2 s. Batching to 10 ms and gating on quiet makes it roughly one
/// inaudible splice every 200 s instead.
pub const CORRECTION_BATCH: usize = 160;

/// Debt beyond which drift is corrected regardless of level (250 ms at 16 kHz).
pub const HARD_CORRECTION_DEBT: usize = 4_000;

/// Below this RMS the slave is considered quiet enough to splice silence into or drop
/// samples from without it being audible. -50 dBFS.
pub const QUIET_RMS: f32 = 0.003_16;

/// How the multichannel input of one leg is folded to mono.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Downmix {
    /// Plain mean across all channels. What the microphone path has always done;
    /// changing it would silently shift level for every existing stereo-mic user.
    Mean,
    /// Front pair only, falling back to the mean for mono/stereo.
    ///
    /// A 5.1 endpoint carries a stereo meeting in FL/FR and silence elsewhere, so a
    /// 6-channel mean is ~9.5 dB down - enough to push the far end under the VAD
    /// threshold and have it discarded as noise rather than merely sound quiet.
    /// Folding the front pair keeps stereo at unity and makes 5.1 identical to it.
    FrontPair,
}

impl Downmix {
    /// Fold one interleaved frame of `channels` samples to a single mono sample.
    pub fn fold(self, frame: &[f32], channels: usize) -> f32 {
        if channels <= 1 {
            return frame.first().copied().unwrap_or(0.0);
        }
        match self {
            Downmix::Mean => frame.iter().take(channels).sum::<f32>() / channels as f32,
            Downmix::FrontPair => {
                // Channels 0 and 1 are FL/FR in every WAVEFORMATEXTENSIBLE layout
                // Windows produces for a render endpoint.
                (frame[0] + frame[1]) * 0.5
            }
        }
    }
}

/// Memoryless soft clip with a knee at 0.7.
///
/// Two legs each peaking at -12 dBFS sum to -6 dBFS, so plain addition is normally
/// safe - but a join chime at full scale plus a mic is not. A memoryless curve is
/// used rather than a look-ahead limiter because there is no attack/release state to
/// get wrong, and `exp` is only evaluated above the knee, i.e. almost never.
///
/// Deliberately NOT `(a + b) * 0.5`: halving the sum halves the microphone too, and
/// the whole pipeline - including the VAD threshold - is tuned to today's mic level.
pub fn soft_clip(x: f32) -> f32 {
    let a = x.abs();
    if a <= 0.7 {
        x
    } else {
        x.signum() * (0.7 + 0.3 * (1.0 - (-(a - 0.7) / 0.3).exp()))
    }
}

/// Root-mean-square of a block, used only to decide whether the slave is quiet
/// enough to splice.
pub fn rms(block: &[f32]) -> f32 {
    if block.is_empty() {
        return 0.0;
    }
    (block.iter().map(|s| s * s).sum::<f32>() / block.len() as f32).sqrt()
}

/// What a drift correction did on one pull, for logging and for tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DriftStats {
    /// Samples of silence spliced in because the slave was running behind.
    pub inserted: usize,
    /// Samples dropped because the slave was running ahead.
    pub dropped: usize,
    /// Pulls that underran the ring entirely and were zero-filled.
    pub underruns: usize,
    /// Samples discarded at the high-water mark.
    pub overflowed: usize,
}

/// The system-audio leg, buffered against the microphone's clock.
///
/// The mic is the master: it drives the pipeline exactly as it does today, and each of
/// its frames pulls the same number of samples from here. An empty ring zero-fills,
/// which is both correct (nothing is playing) and the reason this cannot stall.
#[derive(Debug)]
pub struct SlaveRing {
    buf: VecDeque<f32>,
    stats: DriftStats,
    /// True once any non-zero sample has been written. A system-audio take that never
    /// sees one is almost certainly pointed at the wrong endpoint - a perfectly
    /// successful recording of silence, which is the single most likely way this
    /// feature fails without anyone noticing.
    saw_signal: bool,
}

impl Default for SlaveRing {
    fn default() -> Self {
        Self::new()
    }
}

impl SlaveRing {
    pub fn new() -> Self {
        Self {
            buf: VecDeque::with_capacity(RING_HIGH_WATER),
            stats: DriftStats::default(),
            saw_signal: false,
        }
    }

    /// Discard everything. Called at take start and cancel.
    ///
    /// The legs arm in parallel, so the ring fills while the take is still being set
    /// up. Without this clear, every mixed take would open with up to two seconds of
    /// audio captured BEFORE the user pressed the key - a transcript bug and a privacy
    /// regression at once.
    pub fn clear(&mut self) {
        self.buf.clear();
        self.stats = DriftStats::default();
        self.saw_signal = false;
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn stats(&self) -> DriftStats {
        self.stats
    }

    pub fn saw_signal(&self) -> bool {
        self.saw_signal
    }

    /// Write resampled 16 kHz mono from the loopback leg.
    ///
    /// Overflow drops the OLDEST samples: the newest audio is the audio that still
    /// matters, and a blocking send from an audio thread is forbidden outright.
    pub fn push(&mut self, samples: &[f32]) {
        if !self.saw_signal && samples.iter().any(|s| *s != 0.0) {
            self.saw_signal = true;
        }
        self.buf.extend(samples.iter().copied());
        while self.buf.len() > RING_HIGH_WATER {
            self.buf.pop_front();
            self.stats.overflowed += 1;
        }
    }

    /// Pull exactly `n` samples to mix against `n` master samples.
    ///
    /// Applies drift correction first, then fills. An underrun zero-fills rather than
    /// waiting - "nothing is playing" is the truthful answer and waiting would hang
    /// the pipeline every time the far end paused.
    pub fn pull(&mut self, n: usize, out: &mut Vec<f32>) {
        self.correct_drift();
        out.clear();
        out.reserve(n);
        for _ in 0..n {
            match self.buf.pop_front() {
                Some(s) => out.push(s),
                None => {
                    out.push(0.0);
                    self.stats.underruns += 1;
                }
            }
        }
    }

    /// Nudge occupancy back toward `RING_TARGET`.
    ///
    /// Both directions are handled: too few samples means the slave clock is slow and
    /// silence is spliced in; too many means it is fast and samples are dropped. Both
    /// are gated on the affected region being quiet, so the edit is inaudible - except
    /// past `HARD_CORRECTION_DEBT`, where an unbounded delay is the worse outcome.
    fn correct_drift(&mut self) {
        let len = self.buf.len();

        // An EMPTY ring is not drift - it is "nothing is playing", which for a
        // loopback endpoint is the normal resting state (it delivers no callbacks at
        // all while idle). Splicing silence in here would inflate the ring with
        // manufactured samples that `pull` then counts as data, so the underrun
        // counter would under-report precisely when the endpoint is dead or wrong -
        // the one situation it exists to expose.
        if self.buf.is_empty() {
            return;
        }

        if len + CORRECTION_BATCH <= RING_TARGET {
            let debt = RING_TARGET - len;
            let front_quiet = self.front_is_quiet();
            if front_quiet || debt >= HARD_CORRECTION_DEBT {
                for _ in 0..CORRECTION_BATCH {
                    self.buf.push_front(0.0);
                }
                self.stats.inserted += CORRECTION_BATCH;
            }
        } else if len >= RING_TARGET + CORRECTION_BATCH {
            let excess = len - RING_TARGET;
            let front_quiet = self.front_is_quiet();
            if front_quiet || excess >= HARD_CORRECTION_DEBT {
                for _ in 0..CORRECTION_BATCH {
                    self.buf.pop_front();
                }
                self.stats.dropped += CORRECTION_BATCH;
            }
        }
    }

    fn front_is_quiet(&self) -> bool {
        let take = CORRECTION_BATCH.min(self.buf.len());
        if take == 0 {
            return true;
        }
        let sum: f32 = self.buf.iter().take(take).map(|s| s * s).sum();
        (sum / take as f32).sqrt() < QUIET_RMS
    }
}

/// Mix one master sample with one slave sample.
#[inline]
pub fn mix_sample(mic: f32, sys: f32, sys_gain: f32) -> f32 {
    soft_clip(mic + sys * sys_gain)
}

/// Mix a whole block in place over the master buffer.
///
/// `slave` is expected to be the same length as `master`; a short slave is treated as
/// silence, which is what an underrun means.
pub fn mix_into(master: &mut [f32], slave: &[f32], sys_gain: f32) {
    for (i, m) in master.iter_mut().enumerate() {
        let s = slave.get(i).copied().unwrap_or(0.0);
        *m = mix_sample(*m, s, sys_gain);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_silent_system_leg_leaves_the_microphone_bit_identical() {
        // The single most important invariant: enabling the mix must not change
        // ordinary dictation when nothing is playing. If this fails, every user who
        // turns the feature on gets subtly different transcription quality.
        let mic: Vec<f32> = (0..512).map(|i| (i as f32 / 512.0) * 0.6 - 0.3).collect();
        let mut mixed = mic.clone();
        mix_into(&mut mixed, &vec![0.0; 512], 1.0);
        assert_eq!(mixed, mic);
    }

    #[test]
    fn soft_clip_is_transparent_below_the_knee_and_bounded_above_it() {
        for x in [-0.7, -0.5, 0.0, 0.25, 0.7] {
            assert_eq!(soft_clip(x), x, "must be untouched below the knee: {x}");
        }
        // Above the knee it compresses but never exceeds 1.0, and stays monotonic.
        let mut last = soft_clip(0.7);
        for i in 71..=200 {
            let v = soft_clip(i as f32 / 100.0);
            assert!(v > last, "must stay monotonic at {i}");
            assert!(v < 1.0, "must stay below full scale at {i}: {v}");
            last = v;
        }
        assert_eq!(soft_clip(-2.0), -soft_clip(2.0), "must be symmetric");
    }

    #[test]
    fn front_pair_downmix_keeps_stereo_at_unity_and_rescues_5_1() {
        // Stereo: identical to the mean, so nothing shifts for existing behaviour.
        let stereo = [0.5, 0.5];
        assert_eq!(Downmix::FrontPair.fold(&stereo, 2), 0.5);
        assert_eq!(Downmix::Mean.fold(&stereo, 2), 0.5);

        // 5.1 carrying the meeting in FL/FR only: the mean loses ~9.5 dB, which is
        // enough for the VAD to discard the far end as noise.
        let surround = [0.5, 0.5, 0.0, 0.0, 0.0, 0.0];
        assert_eq!(Downmix::FrontPair.fold(&surround, 6), 0.5);
        let mean = Downmix::Mean.fold(&surround, 6);
        assert!(mean < 0.17, "the mean really is that much quieter: {mean}");

        // Mono passes through on both.
        assert_eq!(Downmix::FrontPair.fold(&[0.42], 1), 0.42);
    }

    #[test]
    fn an_empty_ring_zero_fills_instead_of_stalling() {
        // Measured: a loopback endpoint delivers NO callbacks while idle. If a pull
        // waited for data, every pause in the call would hang the pipeline.
        let mut ring = SlaveRing::new();
        let mut out = Vec::new();
        ring.pull(480, &mut out);
        assert_eq!(out.len(), 480);
        assert!(out.iter().all(|s| *s == 0.0));
        // Every sample must be counted as an UNDERRUN, not silently manufactured by
        // drift correction: this counter is how a dead or wrong endpoint is detected.
        assert_eq!(ring.stats().underruns, 480);
        assert_eq!(ring.stats().inserted, 0, "an idle endpoint is not drift");
    }

    #[test]
    fn clear_discards_pre_press_audio() {
        // The legs arm in parallel, so the ring fills during setup. Without the clear
        // at take start, every mixed take would open with audio captured BEFORE the
        // key was pressed.
        let mut ring = SlaveRing::new();
        ring.push(&vec![0.5; 5_000]);
        assert!(!ring.is_empty());
        ring.clear();
        assert!(ring.is_empty());
        assert!(!ring.saw_signal());
        assert_eq!(ring.stats(), DriftStats::default());
    }

    #[test]
    fn overflow_drops_the_oldest_and_counts_it() {
        let mut ring = SlaveRing::new();
        ring.push(&vec![0.25; RING_HIGH_WATER + 500]);
        assert_eq!(ring.len(), RING_HIGH_WATER);
        assert_eq!(ring.stats().overflowed, 500);
    }

    #[test]
    fn a_slow_slave_gets_silence_spliced_in_while_quiet() {
        let mut ring = SlaveRing::new();
        // Well below target, and quiet, so a correction is allowed.
        ring.push(&vec![0.0; 100]);
        let before = ring.len();
        let mut out = Vec::new();
        ring.pull(10, &mut out);
        assert!(
            ring.stats().inserted >= CORRECTION_BATCH,
            "expected a splice"
        );
        assert!(ring.len() + 10 > before, "occupancy should have risen");
    }

    #[test]
    fn a_fast_slave_gets_samples_dropped_while_quiet() {
        let mut ring = SlaveRing::new();
        ring.push(&vec![0.0; RING_TARGET + CORRECTION_BATCH * 3]);
        let mut out = Vec::new();
        ring.pull(10, &mut out);
        assert_eq!(ring.stats().dropped, CORRECTION_BATCH);
    }

    #[test]
    fn loud_audio_is_not_spliced_until_the_debt_is_severe() {
        // A correction during speech is audible and lands in the transcript, so a
        // small debt must wait for a quiet moment rather than cutting mid-word.
        let mut ring = SlaveRing::new();
        ring.push(&vec![0.5; RING_TARGET + CORRECTION_BATCH * 2]);
        let mut out = Vec::new();
        ring.pull(10, &mut out);
        assert_eq!(ring.stats().dropped, 0, "must not cut into loud audio");

        // But an unbounded delay is worse than one audible edit.
        let mut ring2 = SlaveRing::new();
        ring2.push(&vec![0.5; RING_TARGET + HARD_CORRECTION_DEBT + 10]);
        ring2.pull(10, &mut out);
        assert_eq!(ring2.stats().dropped, CORRECTION_BATCH);
    }

    #[test]
    fn saw_signal_distinguishes_silence_from_a_wrong_endpoint() {
        let mut ring = SlaveRing::new();
        ring.push(&vec![0.0; 4_000]);
        assert!(!ring.saw_signal(), "digital silence is not signal");
        ring.push(&[0.0, 0.0, 0.01]);
        assert!(ring.saw_signal());
    }

    #[test]
    fn gain_scales_only_the_system_leg() {
        assert_eq!(mix_sample(0.2, 0.1, 2.0), soft_clip(0.4));
        assert_eq!(mix_sample(0.2, 0.0, 2.0), 0.2);
        // Mic gain is always 1.0: the pipeline and the VAD threshold are tuned to it.
        assert_eq!(mix_sample(0.3, 0.0, 0.5), 0.3);
    }

    #[test]
    fn rms_is_zero_for_silence_and_matches_a_known_tone() {
        assert_eq!(rms(&[]), 0.0);
        assert_eq!(rms(&[0.0; 100]), 0.0);
        // A square wave at +/-0.5 has RMS 0.5.
        let sq: Vec<f32> = (0..100)
            .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
            .collect();
        assert!((rms(&sq) - 0.5).abs() < 1e-6);
    }
}
