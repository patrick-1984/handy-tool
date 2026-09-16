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

/// Bound a mixed sum to the valid PCM range.
///
/// Linear right up to full scale, clamped at it.
///
/// This began as a soft knee at 0.7, and that was WRONG in a way worth recording: a
/// knee below full scale reshapes loud samples unconditionally, so a microphone
/// sample of 0.9 came back as ~0.846 even when the system leg contributed exactly
/// nothing. That breaks the one invariant everyone who enables this feature depends
/// on - that mixing with a silent system leg is identical to microphone-only - and it
/// breaks it precisely on the loudest speech, where a listener would blame the
/// engine. Any curve that compresses below 1.0 has this property, so the knee has to
/// sit AT full scale.
///
/// Distortion is therefore confined to sums that would overflow the format anyway,
/// and `system_audio_gain` exists to keep sums out of that territory.
///
/// Deliberately NOT `(a + b) * 0.5`: halving the sum halves the microphone too, and
/// the whole pipeline - including the VAD threshold - is tuned to today's mic level.
pub fn soft_clip(x: f32) -> f32 {
    x.clamp(-1.0, 1.0)
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
    /// Alignment target in samples at 16 kHz. See [`SlaveRing::with_target`].
    target: usize,
}

impl Default for SlaveRing {
    fn default() -> Self {
        Self::new()
    }
}

impl SlaveRing {
    pub fn new() -> Self {
        Self::with_target(RING_TARGET)
    }

    /// Build a ring holding `target` samples at 16 kHz.
    ///
    /// The target IS the alignment: it is how far the system-audio leg lags the
    /// microphone. 100 ms is a sensible default, but the true offset is hardware -
    /// a USB headset, a Bluetooth link and an HDMI monitor each buffer differently,
    /// and the endpoint's own driver adds more - so it has to be tunable rather than
    /// guessed once and baked in.
    pub fn with_target(target: usize) -> Self {
        Self {
            buf: VecDeque::with_capacity(RING_HIGH_WATER),
            stats: DriftStats::default(),
            saw_signal: false,
            target: target.min(RING_HIGH_WATER / 2),
        }
    }

    /// Current alignment target, in samples at 16 kHz.
    pub fn target(&self) -> usize {
        self.target
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

    /// Take everything still buffered, without drift correction.
    ///
    /// The ring is a ~100 ms DELAY LINE: pulls happen 1:1 with master frames, so when
    /// the master stops there is still far-end audio in here that was captured DURING
    /// the take and has never been mixed. Dropping it truncates the other participant
    /// mid-word at the end of every take. Drift correction is deliberately skipped -
    /// there is no master left to drift against.
    pub fn drain(&mut self) -> Vec<f32> {
        let out: Vec<f32> = self.buf.drain(..).collect();
        out
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

        if len + CORRECTION_BATCH <= self.target {
            let debt = self.target - len;
            let front_quiet = self.front_is_quiet();
            if front_quiet || debt >= HARD_CORRECTION_DEBT {
                for _ in 0..CORRECTION_BATCH {
                    self.buf.push_front(0.0);
                }
                self.stats.inserted += CORRECTION_BATCH;
            }
        } else if len >= self.target + CORRECTION_BATCH {
            let excess = len - self.target;
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

/// Convert a user-facing alignment offset in milliseconds to a ring target.
///
/// Clamped to 0..=2000 ms. Negative values are meaningless here - the system leg
/// cannot be pulled EARLIER than the microphone, because the microphone is the clock
/// and the audio has not been captured yet. A user who needs the opposite correction
/// is describing a microphone that lags, which is not something this ring can fix.
pub fn target_from_delay_ms(delay_ms: i32) -> usize {
    let ms = delay_ms.clamp(0, 2_000) as usize;
    (ms * 16_000) / 1_000
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
    fn drain_returns_the_buffered_tail_without_correcting_drift() {
        // The ring is a delay line: when the master stops, what is left in here is
        // far-end audio captured DURING the take that no master frame will pull.
        let mut ring = SlaveRing::new();
        ring.push(&[0.1, 0.2, 0.3]);
        let tail = ring.drain();
        assert_eq!(tail, vec![0.1, 0.2, 0.3]);
        assert!(ring.is_empty());
        assert_eq!(
            ring.stats().inserted,
            0,
            "a flush must not manufacture samples"
        );
        assert_eq!(ring.stats().dropped, 0);
    }

    #[test]
    fn the_alignment_target_is_configurable_and_bounded() {
        assert_eq!(target_from_delay_ms(100), 1_600); // the default, 100 ms @ 16 kHz
        assert_eq!(target_from_delay_ms(0), 0);
        assert_eq!(target_from_delay_ms(250), 4_000);
        // A negative offset is meaningless: the ring cannot pull audio earlier than
        // the microphone, because the microphone is the clock.
        assert_eq!(target_from_delay_ms(-500), 0);
        // And a typo cannot produce an unbounded ring.
        assert_eq!(target_from_delay_ms(999_999), target_from_delay_ms(2_000));
        assert!(SlaveRing::with_target(usize::MAX).target() <= RING_HIGH_WATER / 2);
    }

    #[test]
    fn a_larger_target_holds_more_audio_before_correcting() {
        // 250 ms of hardware lag: the ring must settle at 4000 samples, not 1600.
        let mut ring = SlaveRing::with_target(target_from_delay_ms(250));
        assert_eq!(ring.target(), 4_000);
        ring.push(&vec![0.0; 4_000 + CORRECTION_BATCH * 2]);
        let mut out = Vec::new();
        ring.pull(10, &mut out);
        assert_eq!(
            ring.stats().dropped,
            CORRECTION_BATCH,
            "should trim toward 4000"
        );

        // The default ring would have trimmed this same occupancy much harder.
        let mut small = SlaveRing::new();
        small.push(&vec![0.0; 4_000 + CORRECTION_BATCH * 2]);
        small.pull(10, &mut out);
        assert!(small.stats().dropped >= CORRECTION_BATCH);
    }

    #[test]
    fn a_silent_slave_is_identical_to_mic_only_across_the_whole_range() {
        // The earlier version of this test only swept +/-0.3, which never reaches
        // soft_clip's 0.7 knee - so it could not have caught a loud sample being
        // reshaped by a mix that adds nothing. Sweep the full range.
        for i in -100..=100 {
            let mic = i as f32 / 100.0;
            let mut buf = vec![mic];
            mix_into(&mut buf, &[0.0], 1.0);
            assert_eq!(buf[0], mic, "silent slave must not alter mic sample {mic}");
        }
    }

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
    fn clipping_is_transparent_across_the_whole_valid_range() {
        // Every representable PCM sample must pass through untouched. A knee below
        // full scale would fail here - and would mean enabling the mix quietly
        // changed loud microphone audio.
        for i in -100..=100 {
            let x = i as f32 / 100.0;
            assert_eq!(soft_clip(x), x, "must be untouched inside full scale: {x}");
        }
        // Only genuine overflow is bounded, and symmetrically.
        assert_eq!(soft_clip(1.4), 1.0);
        assert_eq!(soft_clip(-1.4), -1.0);
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
