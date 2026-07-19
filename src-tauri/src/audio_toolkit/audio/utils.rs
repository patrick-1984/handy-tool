use anyhow::Result;
use hound::{WavSpec, WavWriter};
use log::debug;
use std::path::Path;

/// Append `pad_samples` of digital silence (zeros) to `samples`.
///
/// Transducer/CTC-style engines (Parakeet, Moonshine, SenseVoice) need
/// trailing acoustic context to emit their final tokens — audio that ends the
/// instant speech stops loses the last word(s). Mid-recording transcription
/// segments end at a VAD-confirmed silence (the SmoothedVad hangover frames),
/// so they carry that context naturally; the tail segment cut at stop and the
/// ~45 s hard-cut segments do not, so they must be padded before decoding.
pub fn pad_trailing_silence(mut samples: Vec<f32>, pad_samples: usize) -> Vec<f32> {
    let new_len = samples.len() + pad_samples;
    samples.resize(new_len, 0.0);
    samples
}

/// Save audio samples as a WAV file
pub async fn save_wav_file<P: AsRef<Path>>(file_path: P, samples: &[f32]) -> Result<()> {
    let spec = WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = WavWriter::create(file_path.as_ref(), spec)?;

    // Convert f32 samples to i16 for WAV
    for sample in samples {
        let sample_i16 = (sample * i16::MAX as f32) as i16;
        writer.write_sample(sample_i16)?;
    }

    writer.finalize()?;
    debug!("Saved WAV file: {:?}", file_path.as_ref());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_appends_zeros_and_preserves_prefix() {
        let samples = vec![0.5f32, -0.25, 0.125];
        let padded = pad_trailing_silence(samples, 4);
        assert_eq!(padded.len(), 7);
        assert_eq!(&padded[..3], &[0.5, -0.25, 0.125]);
        assert!(padded[3..].iter().all(|&s| s == 0.0));
    }

    #[test]
    fn pad_zero_is_identity() {
        let samples = vec![0.1f32, 0.2];
        let padded = pad_trailing_silence(samples.clone(), 0);
        assert_eq!(padded, samples);
    }

    #[test]
    fn pad_empty_input_yields_pure_silence() {
        let padded = pad_trailing_silence(Vec::new(), 8);
        assert_eq!(padded.len(), 8);
        assert!(padded.iter().all(|&s| s == 0.0));
    }
}
