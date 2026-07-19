//! Crash-resilient incremental WAV writer.
//!
//! Unlike [`hound::WavWriter`], which only writes correct RIFF/`data` size
//! fields when `finalize()` is called, this writer appends PCM samples to a
//! file as they arrive and periodically rewrites the size header. The file on
//! disk is therefore a valid, playable WAV at roughly one-second checkpoints.
//!
//! If the process dies mid-recording, the audio is not lost: the PCM payload
//! after the 44-byte header is intact, and [`repair_wav_header`] recomputes the
//! size fields from the actual file length to turn the leftover file into a
//! valid WAV.
//!
//! Format matches `save_wav_file`: 16 kHz, mono, 16-bit signed PCM.

use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const SAMPLE_RATE: u32 = 16_000;
const CHANNELS: u16 = 1;
const BITS_PER_SAMPLE: u16 = 16;
const HEADER_LEN: u64 = 44;
/// Rewrite the size header roughly once per second of audio.
const FLUSH_INTERVAL_SAMPLES: u32 = SAMPLE_RATE;

/// Build a 44-byte canonical PCM WAV header for `data_bytes` of audio payload.
fn wav_header(data_bytes: u32) -> [u8; 44] {
    let byte_rate = SAMPLE_RATE * CHANNELS as u32 * (BITS_PER_SAMPLE as u32 / 8);
    let block_align = CHANNELS * (BITS_PER_SAMPLE / 8);
    let riff_size = 36u32.saturating_add(data_bytes);

    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&riff_size.to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    h[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    h[22..24].copy_from_slice(&CHANNELS.to_le_bytes());
    h[24..28].copy_from_slice(&SAMPLE_RATE.to_le_bytes());
    h[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    h[32..34].copy_from_slice(&block_align.to_le_bytes());
    h[34..36].copy_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_bytes.to_le_bytes());
    h
}

/// Appends 16 kHz / mono / 16-bit PCM samples to a file as they arrive and
/// periodically rewrites the size header so the file is a valid WAV at ~1s
/// checkpoints. Surviving a crash is handled by [`repair_wav_header`].
pub struct IncrementalWavWriter {
    path: PathBuf,
    writer: BufWriter<File>,
    /// Total i16 samples written so far.
    samples_written: u32,
    /// Samples written since the last header rewrite.
    samples_since_flush: u32,
}

impl IncrementalWavWriter {
    /// Create a new file and write the initial (zero-length) header.
    pub fn create(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let file =
            File::create(path).with_context(|| format!("creating recording file {:?}", path))?;
        let mut writer = BufWriter::new(file);
        writer.write_all(&wav_header(0))?;
        writer.flush()?;
        Ok(Self {
            path: path.to_path_buf(),
            writer,
            samples_written: 0,
            samples_since_flush: 0,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append a frame of f32 samples (range roughly [-1.0, 1.0]).
    pub fn write_frame(&mut self, samples: &[f32]) -> Result<()> {
        for &s in samples {
            // Match save_wav_file's conversion (saturating f32 -> i16 cast).
            let v = (s * i16::MAX as f32) as i16;
            self.writer.write_all(&v.to_le_bytes())?;
        }
        self.samples_written = self.samples_written.saturating_add(samples.len() as u32);
        self.samples_since_flush = self
            .samples_since_flush
            .saturating_add(samples.len() as u32);

        if self.samples_since_flush >= FLUSH_INTERVAL_SAMPLES {
            self.checkpoint()?;
            self.samples_since_flush = 0;
        }
        Ok(())
    }

    /// Flush buffered bytes and rewrite the size header so the file is a valid
    /// WAV up to this point. Leaves the cursor at the end for further appends.
    fn checkpoint(&mut self) -> Result<()> {
        self.writer.flush()?;
        let data_bytes = self.samples_written.saturating_mul(2);
        let f = self.writer.get_mut();
        f.seek(SeekFrom::Start(4))?;
        f.write_all(&36u32.saturating_add(data_bytes).to_le_bytes())?; // RIFF size
        f.seek(SeekFrom::Start(40))?;
        f.write_all(&data_bytes.to_le_bytes())?; // data chunk size
        f.seek(SeekFrom::End(0))?;
        f.flush()?;
        Ok(())
    }

    /// Finalize as a valid WAV and keep the file on disk.
    pub fn finalize(mut self) -> Result<PathBuf> {
        self.checkpoint()?;
        Ok(self.path)
    }

    /// Close and delete the file. Used on a clean stop, where the canonical
    /// history WAV is written separately and this safety copy is no longer
    /// needed.
    pub fn discard(self) {
        let IncrementalWavWriter { path, writer, .. } = self;
        drop(writer);
        if let Err(e) = std::fs::remove_file(&path) {
            log::debug!("Failed to remove recording safety file {:?}: {}", path, e);
        }
    }
}

/// Recover a recording file left behind by a crash: recompute the RIFF/`data`
/// size fields from the actual file length so the file becomes a valid WAV.
/// Returns the number of i16 samples recovered.
pub fn repair_wav_header(path: &Path) -> Result<u32> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .with_context(|| format!("opening {:?} for repair", path))?;

    let len = file.metadata()?.len();
    if len < HEADER_LEN {
        anyhow::bail!("file too small to contain audio ({} bytes)", len);
    }

    let data_bytes = (len - HEADER_LEN) as u32;
    file.seek(SeekFrom::Start(4))?;
    file.write_all(&36u32.saturating_add(data_bytes).to_le_bytes())?;
    file.seek(SeekFrom::Start(40))?;
    file.write_all(&data_bytes.to_le_bytes())?;
    file.flush()?;

    Ok(data_bytes / 2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn writes_and_recovers_after_simulated_crash() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("handy-123.recording.wav");

        // Write ~1.5s of audio in 30ms frames, then drop WITHOUT finalizing to
        // simulate a crash (BufWriter flushes its buffer on drop).
        {
            let mut w = IncrementalWavWriter::create(&path).unwrap();
            let frame = vec![0.25f32; 480]; // 30ms @ 16kHz
            for _ in 0..50 {
                w.write_frame(&frame).unwrap();
            }
        }

        // Recovery recomputes the size fields from the file length.
        let recovered = repair_wav_header(&path).unwrap();
        assert_eq!(recovered, 50 * 480);

        // The repaired file must be a valid WAV with the expected spec.
        let reader = hound::WavReader::open(&path).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, 16_000);
        assert_eq!(spec.bits_per_sample, 16);
        assert_eq!(reader.len(), 50 * 480);
    }

    #[test]
    fn finalize_produces_valid_wav() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("handy-456.recording.wav");

        let mut w = IncrementalWavWriter::create(&path).unwrap();
        w.write_frame(&vec![0.1f32; 100]).unwrap();
        let out = w.finalize().unwrap();

        let reader = hound::WavReader::open(&out).unwrap();
        assert_eq!(reader.len(), 100);
    }

    #[test]
    fn repair_rejects_header_only_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("handy-789.recording.wav");
        // Create with header but no samples, then truncate below 44 bytes.
        let _ = IncrementalWavWriter::create(&path).unwrap();
        std::fs::write(&path, b"RIFF").unwrap();
        assert!(repair_wav_header(&path).is_err());
    }
}
