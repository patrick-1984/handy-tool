use anyhow::Result;

pub enum VadFrame<'a> {
    /// Speech – may aggregate several frames (prefill + current + hangover)
    Speech(&'a [f32]),
    /// Non-speech (silence, noise). Down-stream code can ignore it.
    Noise,
}

impl<'a> VadFrame<'a> {
    #[inline]
    pub fn is_speech(&self) -> bool {
        matches!(self, VadFrame::Speech(_))
    }
}

pub trait VoiceActivityDetector: Send + Sync {
    /// Primary streaming API: feed one 30-ms frame, get keep/drop decision.
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>>;

    fn is_voice(&mut self, frame: &[f32]) -> Result<bool> {
        Ok(self.push_frame(frame)?.is_speech())
    }

    fn reset(&mut self) {}

    /// Returns true on the frame where speech transitions to silence.
    /// Used by the recorder to detect segment boundaries for progressive transcription.
    fn speech_ended(&self) -> bool {
        false
    }

    /// Flush any audio the detector is still holding back (e.g. voiced frames
    /// buffered during an unconfirmed onset). Called once when recording stops
    /// so trailing speech isn't silently dropped. Returns `None` when nothing
    /// is pending.
    fn flush(&mut self) -> Option<Vec<f32>> {
        None
    }
}

mod silero;
mod smoothed;

pub use silero::SileroVad;
pub use smoothed::SmoothedVad;
