use super::{VadFrame, VoiceActivityDetector};
use anyhow::Result;
use std::collections::VecDeque;

pub struct SmoothedVad {
    inner_vad: Box<dyn VoiceActivityDetector>,
    prefill_frames: usize,
    hangover_frames: usize,
    onset_frames: usize,

    frame_buffer: VecDeque<Vec<f32>>,
    hangover_counter: usize,
    onset_counter: usize,
    in_speech: bool,
    /// Set to true on the frame where speech ends (in_speech transitions true→false).
    just_ended: bool,

    temp_out: Vec<f32>,
}

impl SmoothedVad {
    pub fn new(
        inner_vad: Box<dyn VoiceActivityDetector>,
        prefill_frames: usize,
        hangover_frames: usize,
        onset_frames: usize,
    ) -> Self {
        Self {
            inner_vad,
            prefill_frames,
            hangover_frames,
            onset_frames,
            frame_buffer: VecDeque::new(),
            hangover_counter: 0,
            onset_counter: 0,
            in_speech: false,
            just_ended: false,
            temp_out: Vec::new(),
        }
    }
}

impl VoiceActivityDetector for SmoothedVad {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>> {
        // Reset the edge-detection flag at the start of every frame
        self.just_ended = false;

        // 1. Buffer every incoming frame for possible pre-roll
        self.frame_buffer.push_back(frame.to_vec());
        while self.frame_buffer.len() > self.prefill_frames + 1 {
            self.frame_buffer.pop_front();
        }

        // 2. Delegate to the wrapped boolean VAD
        let is_voice = self.inner_vad.is_voice(frame)?;

        match (self.in_speech, is_voice) {
            // Potential start of speech - need to accumulate onset frames
            (false, true) => {
                self.onset_counter += 1;
                if self.onset_counter >= self.onset_frames {
                    // We have enough consecutive voice frames to trigger speech
                    self.in_speech = true;
                    self.hangover_counter = self.hangover_frames;
                    self.onset_counter = 0; // Reset for next time

                    // Collect prefill + current frame
                    self.temp_out.clear();
                    for buf in &self.frame_buffer {
                        self.temp_out.extend(buf);
                    }
                    Ok(VadFrame::Speech(&self.temp_out))
                } else {
                    // Not enough frames yet, still silence
                    Ok(VadFrame::Noise)
                }
            }

            // Ongoing Speech
            (true, true) => {
                self.hangover_counter = self.hangover_frames;
                Ok(VadFrame::Speech(frame))
            }

            // End of Speech or interruption during onset phase
            (true, false) => {
                if self.hangover_counter > 0 {
                    self.hangover_counter -= 1;
                    Ok(VadFrame::Speech(frame))
                } else {
                    self.in_speech = false;
                    self.just_ended = true;
                    Ok(VadFrame::Noise)
                }
            }

            // Silence or broken onset sequence
            (false, false) => {
                self.onset_counter = 0; // Reset onset counter on silence
                Ok(VadFrame::Noise)
            }
        }
    }

    fn reset(&mut self) {
        self.frame_buffer.clear();
        self.hangover_counter = 0;
        self.onset_counter = 0;
        self.in_speech = false;
        self.just_ended = false;
        self.temp_out.clear();
    }

    fn speech_ended(&self) -> bool {
        self.just_ended
    }

    /// Release voiced frames held back by an unconfirmed onset. If recording
    /// stops while `onset_counter > 0`, the inner VAD said those last frames
    /// were voice but the smoother hadn't released them yet — without this they
    /// would be dropped (a clipped final word).
    fn flush(&mut self) -> Option<Vec<f32>> {
        if self.onset_counter == 0 {
            return None;
        }
        let pending: Vec<f32> = self
            .frame_buffer
            .iter()
            .rev()
            .take(self.onset_counter)
            .rev()
            .flat_map(|f| f.iter().copied())
            .collect();
        self.onset_counter = 0;
        if pending.is_empty() {
            None
        } else {
            Some(pending)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Inner VAD that classifies frames from a fixed script.
    struct ScriptedVad {
        script: Vec<bool>,
        i: usize,
    }

    impl VoiceActivityDetector for ScriptedVad {
        fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>> {
            let voiced = self.script.get(self.i).copied().unwrap_or(false);
            self.i += 1;
            Ok(if voiced {
                VadFrame::Speech(frame)
            } else {
                VadFrame::Noise
            })
        }
    }

    fn smoothed(script: Vec<bool>) -> SmoothedVad {
        // prefill 2, hangover 2, onset 2 — onset needs two consecutive voiced frames.
        SmoothedVad::new(Box::new(ScriptedVad { script, i: 0 }), 2, 2, 2)
    }

    #[test]
    fn flush_releases_unconfirmed_onset_frames() {
        let mut vad = smoothed(vec![true]);
        // One voiced frame: onset not yet confirmed → classified Noise (held back).
        let frame = vec![0.5f32; 4];
        assert!(!vad.push_frame(&frame).unwrap().is_speech());
        // Stop: flush releases exactly that held-back voiced frame.
        let flushed = vad.flush().expect("pending onset audio");
        assert_eq!(flushed, frame);
        // Idempotent: nothing left afterwards.
        assert!(vad.flush().is_none());
    }

    #[test]
    fn flush_is_none_without_pending_onset() {
        let mut vad = smoothed(vec![false, true, true, true]);
        let frame = vec![0.1f32; 4];
        assert!(!vad.push_frame(&frame).unwrap().is_speech()); // silence
        assert!(vad.flush().is_none());
        // Confirmed speech (two voiced frames) → nothing held back either.
        assert!(!vad.push_frame(&frame).unwrap().is_speech()); // onset 1/2
        assert!(vad.push_frame(&frame).unwrap().is_speech()); // onset confirmed
        assert!(vad.push_frame(&frame).unwrap().is_speech()); // ongoing speech
        assert!(vad.flush().is_none());
    }
}
