//! Batch audio-file decoding for the Translator: WAV (via hound) and Ogg/Opus
//! (std-only page walk + audiopus) into 16 kHz mono f32 — the engine's input
//! format. Only the formats Handy itself produces are supported; anything else
//! is a clean error the caller surfaces as a skipped file.
//!
//! The Ogg reader mirrors the writer in [`super::opus_chunk`]: it understands
//! chained streams (glued chunks carry one logical stream per serial) and
//! decodes them in order of first appearance. Torn/truncated tails are dropped
//! (same tolerance as `repair_truncated_opus`), and the writer's ≤20 ms
//! zero-pad tail is left in place — trailing silence is irrelevant for STT.

use anyhow::{Context, Result, bail};
use audiopus::coder::Decoder as OpusDecoder;
use audiopus::{Channels, SampleRate};
use std::collections::HashMap;
use std::path::Path;

pub const TARGET_HZ: u32 = 16_000;

/// Maximum decoded samples of a single Opus packet: 120 ms at 48 kHz. The
/// decoder outputs at 16 kHz here, but sizing for 48 kHz costs nothing.
const MAX_FRAME_SAMPLES: usize = 5760;

/// Decode a supported audio file to 16 kHz mono f32 samples.
pub fn decode_audio_file(path: &Path) -> Result<Vec<f32>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "wav" => decode_wav(path),
        "ogg" | "opus" => decode_ogg_opus(path),
        other => bail!("unsupported audio format '.{other}' (supported: .wav, .ogg, .opus)"),
    }
}

/// File extensions [`decode_audio_file`] accepts (lowercase, no dot).
pub const SUPPORTED_EXTENSIONS: [&str; 3] = ["wav", "ogg", "opus"];

// ---------------------------------------------------------------------------
// WAV
// ---------------------------------------------------------------------------

fn decode_wav(path: &Path) -> Result<Vec<f32>> {
    let mut reader =
        hound::WavReader::open(path).with_context(|| format!("open WAV {}", path.display()))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;

    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<std::result::Result<_, _>>()
            .with_context(|| format!("read float samples from {}", path.display()))?,
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample.clamp(1, 32);
            let scale = (1i64 << (bits - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / scale))
                .collect::<std::result::Result<_, _>>()
                .with_context(|| format!("read int samples from {}", path.display()))?
        }
    };

    let mono: Vec<f32> = if channels == 1 {
        interleaved
    } else {
        interleaved
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    };

    if mono.is_empty() {
        bail!("WAV contains no audio: {}", path.display());
    }
    Ok(resample_to_16k(mono, spec.sample_rate))
}

/// Resample to 16 kHz using the same rubato backend as the live pipeline.
/// Input is zero-padded to the resampler's chunk size; the extra tail is
/// trailing silence, which the transcription engines ignore.
pub fn resample_to_16k(samples: Vec<f32>, in_hz: u32) -> Vec<f32> {
    if in_hz == TARGET_HZ || samples.is_empty() {
        return samples;
    }
    use rubato::{FftFixedIn, Resampler};
    const CHUNK: usize = 1024;
    let mut resampler =
        match FftFixedIn::<f32>::new(in_hz as usize, TARGET_HZ as usize, CHUNK, 1, 1) {
            Ok(r) => r,
            Err(e) => {
                log::warn!("resampler init failed ({e}); falling back to linear interpolation");
                return linear_resample(&samples, in_hz);
            }
        };
    let mut padded = samples;
    let pad = (CHUNK - (padded.len() % CHUNK)) % CHUNK;
    padded.extend(std::iter::repeat(0.0).take(pad));

    let mut out = Vec::with_capacity(padded.len() * TARGET_HZ as usize / in_hz as usize + CHUNK);
    for chunk in padded.chunks(CHUNK) {
        match resampler.process(&[chunk], None) {
            Ok(o) => out.extend_from_slice(&o[0]),
            Err(e) => {
                log::warn!("resample chunk failed ({e}); falling back to linear interpolation");
                return linear_resample(&padded, in_hz);
            }
        }
    }
    out
}

fn linear_resample(samples: &[f32], in_hz: u32) -> Vec<f32> {
    let ratio = in_hz as f64 / TARGET_HZ as f64;
    let out_len = (samples.len() as f64 / ratio) as usize;
    (0..out_len)
        .map(|i| {
            let src = i as f64 * ratio;
            let i0 = src as usize;
            let i1 = (i0 + 1).min(samples.len().saturating_sub(1));
            let frac = (src - i0 as f64) as f32;
            samples[i0] * (1.0 - frac) + samples[i1] * frac
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Ogg/Opus
// ---------------------------------------------------------------------------

struct OpusStream {
    decoder: OpusDecoder,
    /// Packet bytes continued from a previous page (lacing 255 run).
    partial: Vec<u8>,
    /// OpusHead + OpusTags consumed?
    headers_seen: u8,
    /// Encoder lookahead to drop from the stream start, in 16 kHz samples.
    pre_skip_16k: usize,
    /// True when the stream begins mid-packet (torn continuation) — skip
    /// segments until the phantom packet ends.
    skipping_continued: bool,
    samples: Vec<f32>,
}

impl OpusStream {
    fn new() -> Result<Self> {
        Ok(Self {
            decoder: OpusDecoder::new(SampleRate::Hz16000, Channels::Mono)
                .map_err(|e| anyhow::anyhow!("opus decoder init: {e:?}"))?,
            partial: Vec::new(),
            headers_seen: 0,
            pre_skip_16k: 0,
            skipping_continued: false,
            samples: Vec::new(),
        })
    }

    fn handle_packet(&mut self, packet: &[u8]) {
        if packet.is_empty() {
            return;
        }
        if self.headers_seen == 0 {
            // OpusHead: magic(8) ver(1) ch(1) pre_skip(u16 LE at 10..12) …
            if packet.starts_with(b"OpusHead") && packet.len() >= 12 {
                let pre_skip_48k = u16::from_le_bytes([packet[10], packet[11]]) as usize;
                self.pre_skip_16k = pre_skip_48k / 3;
            }
            self.headers_seen = 1;
            return;
        }
        if self.headers_seen == 1 {
            // OpusTags (or, on a malformed stream, the first audio packet —
            // treat it as consumed either way; one lost frame is inaudible).
            self.headers_seen = 2;
            return;
        }
        let mut out = vec![0f32; MAX_FRAME_SAMPLES];
        match self.decoder.decode_float(Some(packet), &mut out[..], false) {
            Ok(n) => {
                let mut produced = &out[..n];
                if self.pre_skip_16k > 0 {
                    let drop = self.pre_skip_16k.min(produced.len());
                    produced = &produced[drop..];
                    self.pre_skip_16k -= drop;
                }
                self.samples.extend_from_slice(produced);
            }
            Err(e) => log::debug!("opus packet decode failed (skipping packet): {e:?}"),
        }
    }
}

fn decode_ogg_opus(path: &Path) -> Result<Vec<f32>> {
    let data = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;

    let mut order: Vec<u32> = Vec::new();
    let mut streams: HashMap<u32, OpusStream> = HashMap::new();

    let mut pos = 0usize;
    while pos + 27 <= data.len() {
        if &data[pos..pos + 4] != b"OggS" {
            break; // torn data — keep what we decoded so far
        }
        let header_type = data[pos + 5];
        let serial = u32::from_le_bytes(data[pos + 14..pos + 18].try_into().unwrap());
        let nsegs = data[pos + 26] as usize;
        let seg_table_end = pos + 27 + nsegs;
        if seg_table_end > data.len() {
            break; // truncated within the segment table
        }
        let body_len: usize = data[pos + 27..seg_table_end]
            .iter()
            .map(|&b| b as usize)
            .sum();
        if seg_table_end + body_len > data.len() {
            break; // truncated within the page body
        }

        if !streams.contains_key(&serial) {
            streams.insert(serial, OpusStream::new()?);
            order.push(serial);
        }
        let stream = streams.get_mut(&serial).expect("just inserted");

        // A page whose first segment continues a packet we never saw the start
        // of (bit 0x01 set but nothing buffered) yields a phantom packet —
        // consume and discard it instead of decoding garbage.
        if header_type & 0x01 != 0 && stream.partial.is_empty() {
            stream.skipping_continued = true;
        }
        if header_type & 0x01 == 0 && !stream.partial.is_empty() {
            // The previous page promised a continuation that never came.
            stream.partial.clear();
        }

        let mut off = pos + 27 + nsegs;
        for &lace in &data[pos + 27..seg_table_end] {
            let end = off + lace as usize;
            if stream.skipping_continued {
                if lace < 255 {
                    stream.skipping_continued = false;
                }
            } else {
                stream.partial.extend_from_slice(&data[off..end]);
                if lace < 255 {
                    let packet = std::mem::take(&mut stream.partial);
                    stream.handle_packet(&packet);
                }
            }
            off = end;
        }
        pos = off;
    }

    let mut all = Vec::new();
    for serial in order {
        if let Some(s) = streams.remove(&serial) {
            all.extend(s.samples);
        }
    }
    if all.is_empty() {
        bail!("no decodable Opus audio in {}", path.display());
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_toolkit::audio::encode_samples_to_ogg_opus;

    /// 3 s of a 440 Hz tone at 16 kHz — long enough to span many pages.
    fn tone(len_samples: usize) -> Vec<f32> {
        (0..len_samples)
            .map(|i| (i as f32 * 440.0 * 2.0 * std::f32::consts::PI / 16_000.0).sin() * 0.5)
            .collect()
    }

    #[test]
    fn ogg_opus_round_trip_preserves_length_and_energy() {
        let dir = std::env::temp_dir().join("handy-decode-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roundtrip.opus");

        let original = tone(3 * 16_000);
        let bytes = encode_samples_to_ogg_opus(&original).unwrap();
        std::fs::write(&path, &bytes).unwrap();

        let decoded = decode_audio_file(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        // Opus is lossy and frame-padded: length within one frame + pad, and
        // the signal must carry real energy (not silence / garbage).
        let diff = (decoded.len() as i64 - original.len() as i64).abs();
        assert!(
            diff < 1600,
            "decoded length {} too far from original {}",
            decoded.len(),
            original.len()
        );
        let rms = (decoded.iter().map(|s| s * s).sum::<f32>() / decoded.len().max(1) as f32).sqrt();
        assert!(rms > 0.1, "decoded audio has no energy (rms {rms})");
    }

    #[test]
    fn chained_streams_decode_in_order() {
        // Two independently encoded chunks byte-concatenated = chained Ogg,
        // exactly what glue_chunks() produces.
        let dir = std::env::temp_dir().join("handy-decode-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("chained.opus");

        let a = tone(16_000);
        let b = vec![0.0f32; 16_000]; // silence chunk
        let mut bytes = encode_samples_to_ogg_opus(&a).unwrap();
        bytes.extend(encode_samples_to_ogg_opus(&b).unwrap());
        std::fs::write(&path, &bytes).unwrap();

        let decoded = decode_audio_file(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert!(
            decoded.len() > 28_000,
            "expected ~2 s, got {}",
            decoded.len()
        );
        // First half loud, second half quiet.
        let half = decoded.len() / 2;
        let rms = |s: &[f32]| (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt();
        assert!(rms(&decoded[..half]) > 0.1);
        assert!(rms(&decoded[half..]) < 0.05);
    }

    #[test]
    fn wav_round_trip_resamples_to_16k() {
        let dir = std::env::temp_dir().join("handy-decode-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roundtrip48k.wav");

        // 1 s of tone at 48 kHz stereo, 16-bit.
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for i in 0..48_000 {
            let v = ((i as f32 * 440.0 * 2.0 * std::f32::consts::PI / 48_000.0).sin()
                * 0.5
                * i16::MAX as f32) as i16;
            w.write_sample(v).unwrap();
            w.write_sample(v).unwrap();
        }
        w.finalize().unwrap();

        let decoded = decode_audio_file(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        // ~1 s at 16 kHz (resampler pads to its chunk size — allow slack).
        assert!(
            (15_000..20_000).contains(&decoded.len()),
            "expected ~16000 samples, got {}",
            decoded.len()
        );
        let rms = (decoded.iter().map(|s| s * s).sum::<f32>() / decoded.len().max(1) as f32).sqrt();
        assert!(rms > 0.1, "decoded audio has no energy (rms {rms})");
    }

    #[test]
    fn unsupported_extension_is_rejected() {
        let err = decode_audio_file(Path::new("x.mp3")).unwrap_err();
        assert!(err.to_string().contains("unsupported audio format"));
    }
}
