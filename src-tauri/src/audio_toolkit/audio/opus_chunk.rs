//! Crash-resilient incremental Opus/Ogg chunk writer + recovery + gluing.
//!
//! Replaces the WAV-based [`super::incremental_wav`] crash-safety. A recording
//! is streamed to one or more `handy-{ts}-chunk-N.opus` files; while a chunk is
//! being written it is named `*-temp.opus` (the crash marker) and renamed to its
//! final name on a clean close.
//!
//! Why this is crash-safe: Ogg is a page-based container. Each page is written
//! whole and is independently decodable up to the last fully-flushed page. If
//! the process dies mid-recording, the file is valid up to its last complete
//! page; [`repair_truncated_opus`] just drops the torn trailing page (and marks
//! the new last page end-of-stream). No FFmpeg needed.
//!
//! The Opus encoder is the only external dependency (`audiopus` = libopus). The
//! Ogg muxing, repair, and gluing are implemented here with std only so the
//! on-disk bytes are fully under our control and unit-testable.
//!
//! Audio params: 16 kHz, mono, VOIP, ~24 kbps VBR.

use anyhow::{Context, Result};
use audiopus::{Application, Bitrate, Channels, SampleRate, coder::Encoder};
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const IN_RATE_HZ: u32 = 16_000;
/// Native Opus frame at 16 kHz = 20 ms.
const ENC_FRAME_SAMPLES: usize = 320;
/// Typical libopus encoder lookahead (~6.5 ms) expressed in 48 kHz samples.
/// Used as the Ogg/Opus `pre_skip`. A few ms of inaccuracy here is inaudible.
const PRE_SKIP_48K: u16 = 312;
const TARGET_BITRATE: i32 = 24_000;
/// Emit (flush) an Ogg page roughly every ~500 ms of audio so the on-disk file
/// is a valid, recoverable stream at ~0.5 s checkpoints.
const PACKETS_PER_PAGE: u32 = 25;
/// Upper bound for a single encoded Opus packet (20 ms @ 24 kbps is ~60 bytes;
/// 4000 is libopus's recommended max output buffer).
const MAX_PACKET_BYTES: usize = 4000;

/// A transcription segment handed to the background transcription pipeline: its
/// index (for ordered concatenation) and PCM. Segments are cut at silence every
/// ~20-45 s, independently of the ~10-min Opus file chunks.
pub struct ClosedChunk {
    pub index: usize,
    pub pcm: Vec<f32>,
}

/// Parameters passed into the recorder to enable chunked Opus output.
#[derive(Clone, Debug)]
pub struct StartParams {
    pub dir: PathBuf,
    pub ts: u64,
}

// ----------------------------------------------------------------------------
// Ogg framing (std-only)
// ----------------------------------------------------------------------------

/// Ogg page CRC: CRC-32 with polynomial 0x04C11DB7, no input/output reflection,
/// initial value 0 (per the Ogg spec — note this differs from zlib CRC-32).
fn ogg_crc(data: &[u8]) -> u32 {
    let mut crc: u32 = 0;
    for &b in data {
        crc ^= (b as u32) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04c1_1db7
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Lacing segments for a packet of `len` bytes (255-byte segments, terminating
/// segment < 255 — appending a 0 when `len` is an exact multiple of 255).
fn lacing_for(len: usize) -> Vec<u8> {
    let mut v = Vec::new();
    let mut remaining = len;
    loop {
        if remaining >= 255 {
            v.push(255);
            remaining -= 255;
        } else {
            v.push(remaining as u8);
            break;
        }
    }
    v
}

/// Build one complete Ogg page (header + segment table + body) with a correct CRC.
fn build_page(
    header_type: u8,
    granule: u64,
    serial: u32,
    page_seq: u32,
    lacing: &[u8],
    body: &[u8],
) -> Vec<u8> {
    let mut p = Vec::with_capacity(27 + lacing.len() + body.len());
    p.extend_from_slice(b"OggS");
    p.push(0); // stream structure version
    p.push(header_type);
    p.extend_from_slice(&granule.to_le_bytes());
    p.extend_from_slice(&serial.to_le_bytes());
    p.extend_from_slice(&page_seq.to_le_bytes());
    p.extend_from_slice(&[0u8; 4]); // CRC placeholder (offset 22)
    p.push(lacing.len() as u8);
    p.extend_from_slice(lacing);
    p.extend_from_slice(body);
    let crc = ogg_crc(&p);
    p[22..26].copy_from_slice(&crc.to_le_bytes());
    p
}

/// Accumulates packets into pages and writes whole pages to the sink.
struct PageMuxer {
    serial: u32,
    page_seq: u32,
    bos_written: bool,
    lacing: Vec<u8>,
    body: Vec<u8>,
    /// Granule of the last packet completed in the current page.
    granule: u64,
    packets_in_page: u32,
}

impl PageMuxer {
    fn new(serial: u32) -> Self {
        Self {
            serial,
            page_seq: 0,
            bos_written: false,
            lacing: Vec::new(),
            body: Vec::new(),
            granule: 0,
            packets_in_page: 0,
        }
    }

    /// Add one packet to the current page, flushing first if it wouldn't fit in
    /// the 255-segment table. (Our Opus packets are 1 segment, so the pre-flush
    /// path is effectively unreachable, but it keeps the muxer correct.)
    fn add_packet<W: Write>(&mut self, data: &[u8], granule: u64, out: &mut W) -> Result<()> {
        let seg = lacing_for(data.len());
        if !self.lacing.is_empty() && self.lacing.len() + seg.len() > 255 {
            self.flush(out, false)?;
        }
        self.lacing.extend_from_slice(&seg);
        self.body.extend_from_slice(data);
        self.granule = granule;
        self.packets_in_page += 1;
        Ok(())
    }

    /// Emit the current page. With `eos`, an end-of-stream page is emitted even
    /// when empty (a valid zero-length terminating page).
    fn flush<W: Write>(&mut self, out: &mut W, eos: bool) -> Result<()> {
        if self.lacing.is_empty() && !eos {
            return Ok(());
        }
        let header_type = if !self.bos_written {
            0x02 // beginning of stream
        } else if eos {
            0x04 // end of stream
        } else {
            0x00
        };
        let page = build_page(
            header_type,
            self.granule,
            self.serial,
            self.page_seq,
            &self.lacing,
            &self.body,
        );
        out.write_all(&page)?;
        self.bos_written = true;
        self.page_seq += 1;
        self.lacing.clear();
        self.body.clear();
        self.packets_in_page = 0;
        Ok(())
    }
}

/// Derive a stream serial from the file name so distinct chunks get distinct
/// serials (required so a byte-concatenated full file is a valid chained stream).
fn serial_for(path: &Path) -> u32 {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("handy-chunk");
    // FNV-1a 32-bit.
    let mut h: u32 = 0x811c_9dc5;
    for &b in name.as_bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h | 1 // avoid 0
}

// ----------------------------------------------------------------------------
// OpusChunkWriter
// ----------------------------------------------------------------------------

/// Streams 16 kHz mono PCM into a crash-safe Ogg/Opus file, flushing whole pages
/// as it goes. Mirrors the old `IncrementalWavWriter` lifecycle.
pub struct OpusChunkWriter {
    /// Final path (the `-temp` suffix is stripped on `finalize`).
    final_path: PathBuf,
    temp_path: PathBuf,
    writer: BufWriter<File>,
    encoder: Encoder,
    muxer: PageMuxer,
    /// PCM samples not yet forming a full 320-sample Opus frame.
    rebuf: VecDeque<f32>,
    /// Total samples fed into the encoder (multiples of 320).
    encoded_samples_16k: u64,
    /// Total real samples written via `write_frame` (drives the final granule so
    /// the decoder trims the zero-padded tail).
    true_samples_16k: u64,
    enc_out: Vec<u8>,
}

impl OpusChunkWriter {
    /// Create `<path>` (a `*-temp.opus` path). Writes the OpusHead + OpusTags
    /// pages immediately so the file is a valid (empty) Opus stream from t=0.
    pub fn create(final_path: &Path) -> Result<Self> {
        if let Some(parent) = final_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let temp_path = temp_path_for(final_path);

        let mut encoder = Encoder::new(SampleRate::Hz16000, Channels::Mono, Application::Voip)
            .map_err(|e| anyhow::anyhow!("opus encoder init: {:?}", e))?;
        // VBR is libopus's default once a bitrate is set; we just set the rate.
        let _ = encoder.set_bitrate(Bitrate::BitsPerSecond(TARGET_BITRATE));

        let file = File::create(&temp_path)
            .with_context(|| format!("creating recording chunk {:?}", temp_path))?;
        let mut writer = BufWriter::new(file);
        let mut muxer = PageMuxer::new(serial_for(final_path));

        // OpusHead (alone on the BOS page) then OpusTags.
        muxer.add_packet(&opus_head(PRE_SKIP_48K), 0, &mut writer)?;
        muxer.flush(&mut writer, false)?;
        muxer.add_packet(&opus_tags(), 0, &mut writer)?;
        muxer.flush(&mut writer, false)?;
        writer.flush()?;

        Ok(Self {
            final_path: final_path.to_path_buf(),
            temp_path,
            writer,
            encoder,
            muxer,
            rebuf: VecDeque::new(),
            encoded_samples_16k: 0,
            true_samples_16k: 0,
            enc_out: vec![0u8; MAX_PACKET_BYTES],
        })
    }

    pub fn path(&self) -> &Path {
        &self.temp_path
    }

    /// Append a frame of f32 samples (range roughly [-1.0, 1.0]).
    pub fn write_frame(&mut self, samples: &[f32]) -> Result<()> {
        self.true_samples_16k += samples.len() as u64;
        self.rebuf.extend(samples.iter().copied());

        while self.rebuf.len() >= ENC_FRAME_SAMPLES {
            let frame: Vec<f32> = self.rebuf.drain(..ENC_FRAME_SAMPLES).collect();
            self.encode_and_write(&frame, false)?;
            if self.muxer.packets_in_page >= PACKETS_PER_PAGE {
                self.muxer.flush(&mut self.writer, false)?;
                self.writer.flush()?; // push page bytes to the OS (crash checkpoint)
            }
        }
        Ok(())
    }

    fn encode_and_write(&mut self, frame: &[f32], _eos: bool) -> Result<()> {
        let n = self
            .encoder
            .encode_float(frame, &mut self.enc_out)
            .map_err(|e| anyhow::anyhow!("opus encode: {:?}", e))?;
        self.encoded_samples_16k += ENC_FRAME_SAMPLES as u64;
        let granule = PRE_SKIP_48K as u64 + self.encoded_samples_16k * 3;
        let packet = self.enc_out[..n].to_vec();
        self.muxer.add_packet(&packet, granule, &mut self.writer)
    }

    /// Finalize: encode any tail samples (zero-padded), write an EOS page whose
    /// granule reflects the TRUE sample count (so the decoder trims the pad),
    /// fsync, and rename `*-temp.opus` → the final name.
    pub fn finalize(mut self) -> Result<PathBuf> {
        if !self.rebuf.is_empty() {
            let mut frame: Vec<f32> = self.rebuf.drain(..).collect();
            frame.resize(ENC_FRAME_SAMPLES, 0.0);
            self.encode_and_write(&frame, true)?;
        }
        // Stamp the terminating page granule with the true (un-padded) length.
        let final_granule = PRE_SKIP_48K as u64 + self.true_samples_16k * 3;
        self.muxer.granule = final_granule;
        self.muxer.flush(&mut self.writer, true)?;
        self.writer.flush()?;
        if let Ok(f) = self.writer.get_ref().try_clone() {
            let _ = f.sync_all();
        }
        // Move the BufWriter's file out and drop it before renaming.
        drop(self.writer);
        std::fs::rename(&self.temp_path, &self.final_path).with_context(|| {
            format!(
                "finalizing chunk {:?} -> {:?}",
                self.temp_path, self.final_path
            )
        })?;
        Ok(self.final_path)
    }

    /// Close and delete the in-progress file (clean cancel / discard).
    pub fn discard(self) {
        let temp = self.temp_path.clone();
        drop(self.writer);
        if let Err(e) = std::fs::remove_file(&temp) {
            log::debug!("Failed to remove temp chunk {:?}: {}", temp, e);
        }
    }
}

fn temp_path_for(final_path: &Path) -> PathBuf {
    // handy-{ts}-chunk-N.opus -> handy-{ts}-chunk-N-temp.opus
    let stem = final_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("handy-chunk");
    let ext = final_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("opus");
    let temp_name = format!("{}-temp.{}", stem, ext);
    match final_path.parent() {
        Some(p) => p.join(temp_name),
        None => PathBuf::from(temp_name),
    }
}

/// Encode 16 kHz mono PCM into a complete in-memory Ogg/Opus stream (OpusHead +
/// OpusTags + audio pages + EOS page). Reuses the same Opus encoder and Ogg
/// muxing as [`OpusChunkWriter`] but writes to a `Vec<u8>` instead of a file —
/// used to send compact audio to remote transcription APIs. Does not touch the
/// crash-safe on-disk writer.
pub fn encode_samples_to_ogg_opus(samples: &[f32]) -> Result<Vec<u8>> {
    let mut encoder = Encoder::new(SampleRate::Hz16000, Channels::Mono, Application::Voip)
        .map_err(|e| anyhow::anyhow!("opus encoder init: {:?}", e))?;
    let _ = encoder.set_bitrate(Bitrate::BitsPerSecond(TARGET_BITRATE));

    let mut out: Vec<u8> = Vec::new();
    // Single, non-chained stream — the serial is arbitrary.
    let mut muxer = PageMuxer::new(1);

    // Header pages: OpusHead alone on the BOS page, then OpusTags.
    muxer.add_packet(&opus_head(PRE_SKIP_48K), 0, &mut out)?;
    muxer.flush(&mut out, false)?;
    muxer.add_packet(&opus_tags(), 0, &mut out)?;
    muxer.flush(&mut out, false)?;

    let mut enc_out = vec![0u8; MAX_PACKET_BYTES];
    let mut encoded_samples_16k: u64 = 0;
    let true_samples_16k = samples.len() as u64;

    let mut i = 0;
    while i < samples.len() {
        let end = (i + ENC_FRAME_SAMPLES).min(samples.len());
        let mut frame: Vec<f32> = samples[i..end].to_vec();
        if frame.len() < ENC_FRAME_SAMPLES {
            frame.resize(ENC_FRAME_SAMPLES, 0.0); // zero-pad the final frame
        }
        let n = encoder
            .encode_float(&frame, &mut enc_out)
            .map_err(|e| anyhow::anyhow!("opus encode: {:?}", e))?;
        encoded_samples_16k += ENC_FRAME_SAMPLES as u64;
        let granule = PRE_SKIP_48K as u64 + encoded_samples_16k * 3;
        muxer.add_packet(&enc_out[..n], granule, &mut out)?;
        if muxer.packets_in_page >= PACKETS_PER_PAGE {
            muxer.flush(&mut out, false)?;
        }
        i = end;
    }

    // Terminating page: granule reflects the TRUE (un-padded) sample count.
    muxer.granule = PRE_SKIP_48K as u64 + true_samples_16k * 3;
    muxer.flush(&mut out, true)?;
    Ok(out)
}

fn opus_head(pre_skip: u16) -> Vec<u8> {
    let mut v = Vec::with_capacity(19);
    v.extend_from_slice(b"OpusHead");
    v.push(1); // version
    v.push(1); // channel count (mono)
    v.extend_from_slice(&pre_skip.to_le_bytes());
    v.extend_from_slice(&IN_RATE_HZ.to_le_bytes()); // input sample rate (informational)
    v.extend_from_slice(&0i16.to_le_bytes()); // output gain
    v.push(0); // channel mapping family
    v
}

fn opus_tags() -> Vec<u8> {
    let vendor = b"handy";
    let mut v = Vec::new();
    v.extend_from_slice(b"OpusTags");
    v.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    v.extend_from_slice(vendor);
    v.extend_from_slice(&0u32.to_le_bytes()); // user comment count
    v
}

// ----------------------------------------------------------------------------
// Recovery + gluing (std-only)
// ----------------------------------------------------------------------------

/// Recover a chunk left behind by a crash: walk Ogg pages, drop the torn
/// trailing page, mark the new last page end-of-stream, and truncate the file.
/// Returns the recovered sample count at 16 kHz. Errors if the file has no
/// recoverable audio (fewer than the two setup pages).
pub fn repair_truncated_opus(path: &Path) -> Result<u64> {
    let mut buf = Vec::new();
    File::open(path)
        .with_context(|| format!("opening {:?} for repair", path))?
        .read_to_end(&mut buf)?;

    let mut offset = 0usize;
    let mut last_good_end = 0usize;
    let mut last_page_start = 0usize;
    let mut last_granule = 0u64;
    let mut page_count = 0u32;

    while offset + 27 <= buf.len() {
        if &buf[offset..offset + 4] != b"OggS" {
            break; // not at a page boundary -> stop (torn data)
        }
        let nsegs = buf[offset + 26] as usize;
        let header_end = offset + 27 + nsegs;
        if header_end > buf.len() {
            break; // truncated within the segment table
        }
        let body_len: usize = buf[offset + 27..header_end]
            .iter()
            .map(|&b| b as usize)
            .sum();
        let page_end = header_end + body_len;
        if page_end > buf.len() {
            break; // truncated within the page body
        }
        let granule = u64::from_le_bytes(buf[offset + 6..offset + 14].try_into().unwrap());
        if granule != u64::MAX {
            last_granule = granule;
        }
        last_page_start = offset;
        last_good_end = page_end;
        page_count += 1;
        offset = page_end;
    }

    if page_count < 2 {
        anyhow::bail!("no recoverable audio (only {} complete pages)", page_count);
    }

    // Mark the surviving last page as end-of-stream (set header_type bit 0x04)
    // and fix its CRC, so the repaired file is cleanly terminated.
    buf[last_page_start + 5] |= 0x04;
    {
        let nsegs = buf[last_page_start + 26] as usize;
        let body_len: usize = buf[last_page_start + 27..last_page_start + 27 + nsegs]
            .iter()
            .map(|&b| b as usize)
            .sum();
        let page_len = 27 + nsegs + body_len;
        let page = &mut buf[last_page_start..last_page_start + page_len];
        page[22..26].copy_from_slice(&[0u8; 4]);
        let crc = ogg_crc(page);
        page[22..26].copy_from_slice(&crc.to_le_bytes());
    }

    let mut f = OpenOptions::new().write(true).open(path)?;
    f.seek(SeekFrom::Start(0))?;
    f.write_all(&buf[..last_good_end])?;
    f.set_len(last_good_end as u64)?;

    let samples = last_granule.saturating_sub(PRE_SKIP_48K as u64) / 3;
    Ok(samples)
}

/// Concatenate finalized chunk files into a single full Opus file. Each chunk is
/// a complete Ogg stream with a distinct serial, so byte concatenation yields a
/// valid chained Ogg/Opus file (played as one by VLC/FFmpeg/Chromium-WebView2).
/// A single chunk is copied directly.
///
/// NOTE: if a target player ever truncates chained-Ogg playback to the first
/// link, switch this to a single-logical-stream re-mux (read packets, rewrite
/// granules under one serial). The public signature is unaffected.
pub fn glue_chunks(chunk_paths: &[PathBuf], out: &Path) -> Result<()> {
    if chunk_paths.is_empty() {
        anyhow::bail!("no chunks to glue");
    }
    if chunk_paths.len() == 1 {
        std::fs::copy(&chunk_paths[0], out)
            .with_context(|| format!("copying single chunk to {:?}", out))?;
        return Ok(());
    }
    let mut w = BufWriter::new(File::create(out).with_context(|| format!("creating {:?}", out))?);
    for p in chunk_paths {
        let mut f = File::open(p).with_context(|| format!("opening chunk {:?}", p))?;
        std::io::copy(&mut f, &mut w)?;
    }
    w.flush()?;
    Ok(())
}

/// Best-effort total duration (seconds) of an Ogg/Opus file, read-only. Sums the
/// final granule of each logical stream (Handy glues one stream per chunk, each
/// with a distinct serial), converting the 48 kHz granule to real 16 kHz samples
/// the same way the writer stamps it: `(granule - PRE_SKIP_48K) / 3`.
pub fn opus_duration_seconds(path: &Path) -> Result<f64> {
    let data = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let mut per_serial: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    let mut pos = 0usize;
    while pos + 27 <= data.len() {
        if &data[pos..pos + 4] != b"OggS" {
            break;
        }
        let granule = u64::from_le_bytes(data[pos + 6..pos + 14].try_into().unwrap());
        let serial = u32::from_le_bytes(data[pos + 14..pos + 18].try_into().unwrap());
        let page_segments = data[pos + 26] as usize;
        let seg_table_end = pos + 27 + page_segments;
        if seg_table_end > data.len() {
            break;
        }
        let payload_len: usize = data[pos + 27..seg_table_end]
            .iter()
            .map(|&b| b as usize)
            .sum();
        let next = seg_table_end + payload_len;
        if next > data.len() {
            break;
        }
        // 0xFFFF…FFFF granule means no packet completed on this page — ignore it.
        if granule != u64::MAX {
            let e = per_serial.entry(serial).or_insert(0);
            *e = (*e).max(granule);
        }
        pos = next;
    }
    if per_serial.is_empty() {
        anyhow::bail!("no Ogg pages in {}", path.display());
    }
    let samples_16k: u64 = per_serial
        .values()
        .map(|g| g.saturating_sub(PRE_SKIP_48K as u64) / 3)
        .sum();
    Ok(samples_16k as f64 / 16_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn page_count(bytes: &[u8]) -> u32 {
        let mut offset = 0usize;
        let mut n = 0u32;
        while offset + 27 <= bytes.len() && &bytes[offset..offset + 4] == b"OggS" {
            let nsegs = bytes[offset + 26] as usize;
            let header_end = offset + 27 + nsegs;
            if header_end > bytes.len() {
                break;
            }
            let body_len: usize = bytes[offset + 27..header_end]
                .iter()
                .map(|&b| b as usize)
                .sum();
            offset = header_end + body_len;
            n += 1;
        }
        n
    }

    fn crc_ok(bytes: &[u8]) -> bool {
        let mut offset = 0usize;
        while offset + 27 <= bytes.len() && &bytes[offset..offset + 4] == b"OggS" {
            let nsegs = bytes[offset + 26] as usize;
            let header_end = offset + 27 + nsegs;
            if header_end > bytes.len() {
                return false;
            }
            let body_len: usize = bytes[offset + 27..header_end]
                .iter()
                .map(|&b| b as usize)
                .sum();
            let page_end = header_end + body_len;
            if page_end > bytes.len() {
                return false;
            }
            let mut page = bytes[offset..page_end].to_vec();
            let stored = u32::from_le_bytes(page[22..26].try_into().unwrap());
            page[22..26].copy_from_slice(&[0u8; 4]);
            if ogg_crc(&page) != stored {
                return false;
            }
            offset = page_end;
        }
        true
    }

    /// A parsed Ogg page header (minus lacing/CRC, which the tests don't need
    /// once framing is validated by [`crc_ok`]/[`page_count`]). Used to assert
    /// on BOS/EOS flags, granule positions, serials, and packet contents.
    struct PageInfo {
        header_type: u8,
        granule: u64,
        serial: u32,
        page_seq: u32,
        body: Vec<u8>,
    }

    fn parse_pages(bytes: &[u8]) -> Vec<PageInfo> {
        let mut offset = 0usize;
        let mut pages = Vec::new();
        while offset + 27 <= bytes.len() && &bytes[offset..offset + 4] == b"OggS" {
            let header_type = bytes[offset + 5];
            let granule = u64::from_le_bytes(bytes[offset + 6..offset + 14].try_into().unwrap());
            let serial = u32::from_le_bytes(bytes[offset + 14..offset + 18].try_into().unwrap());
            let page_seq = u32::from_le_bytes(bytes[offset + 18..offset + 22].try_into().unwrap());
            let nsegs = bytes[offset + 26] as usize;
            let header_end = offset + 27 + nsegs;
            if header_end > bytes.len() {
                break;
            }
            let body_len: usize = bytes[offset + 27..header_end]
                .iter()
                .map(|&b| b as usize)
                .sum();
            let page_end = header_end + body_len;
            if page_end > bytes.len() {
                break;
            }
            pages.push(PageInfo {
                header_type,
                granule,
                serial,
                page_seq,
                body: bytes[header_end..page_end].to_vec(),
            });
            offset = page_end;
        }
        pages
    }

    /// A 440 Hz tone at 16 kHz mono, matching `decode.rs`'s test helper: opus's
    /// VOIP high-pass filter can flatten a DC/constant signal, so tests that
    /// need "real audible energy" use a tone, not `vec![x; n]`.
    fn tone(len_samples: usize) -> Vec<f32> {
        (0..len_samples)
            .map(|i| (i as f32 * 440.0 * 2.0 * std::f32::consts::PI / 16_000.0).sin() * 0.5)
            .collect()
    }

    #[test]
    fn encodes_valid_ogg_opus() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("handy-1-chunk-1.opus");
        let mut w = OpusChunkWriter::create(&path).unwrap();
        let frame = vec![0.1f32; 480]; // 30ms input frames, as the pipeline emits
        for _ in 0..60 {
            w.write_frame(&frame).unwrap();
        }
        let out = w.finalize().unwrap();
        assert_eq!(out, path);
        assert!(path.exists());
        assert!(!temp_path_for(&path).exists()); // temp renamed away

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"OggS");
        assert!(page_count(&bytes) >= 3); // Head + Tags + >=1 audio page
        assert!(crc_ok(&bytes), "all page CRCs valid");
    }

    #[test]
    fn in_memory_ogg_opus_is_valid() {
        // 1 second of 16 kHz mono PCM.
        let samples = vec![0.1f32; 16_000];
        let bytes = encode_samples_to_ogg_opus(&samples).unwrap();
        assert_eq!(&bytes[0..4], b"OggS");
        assert!(bytes.windows(8).any(|w| w == b"OpusHead"));
        assert!(page_count(&bytes) >= 3); // Head + Tags + >=1 audio page
        assert!(crc_ok(&bytes), "all page CRCs valid");
    }

    #[test]
    fn in_memory_ogg_opus_handles_empty() {
        // No samples -> still a valid (header pages + EOS) stream.
        let bytes = encode_samples_to_ogg_opus(&[]).unwrap();
        assert_eq!(&bytes[0..4], b"OggS");
        assert!(crc_ok(&bytes), "all page CRCs valid");
    }

    #[test]
    fn repairs_truncated_chunk() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("handy-2-chunk-1.opus");
        {
            let mut w = OpusChunkWriter::create(&path).unwrap();
            for _ in 0..120 {
                w.write_frame(&vec![0.2f32; 480]).unwrap();
            }
            let _ = w.finalize().unwrap();
        }
        // Simulate a crash: chop off the trailing bytes mid-page.
        let mut bytes = std::fs::read(&path).unwrap();
        bytes.truncate(bytes.len() - 37);
        std::fs::write(&path, &bytes).unwrap();

        let samples = repair_truncated_opus(&path).unwrap();
        assert!(samples > 0);
        let repaired = std::fs::read(&path).unwrap();
        assert!(crc_ok(&repaired), "repaired file has valid page CRCs");
        assert!(page_count(&repaired) >= 2);
    }

    #[test]
    fn glue_single_chunk_is_copy() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("handy-3-chunk-1.opus");
        let mut w = OpusChunkWriter::create(&path).unwrap();
        w.write_frame(&vec![0.0f32; 4800]).unwrap();
        let chunk = w.finalize().unwrap();
        let out = dir.path().join("handy-3.opus");
        glue_chunks(&[chunk.clone()], &out).unwrap();
        assert_eq!(std::fs::read(&chunk).unwrap(), std::fs::read(&out).unwrap());
    }

    #[test]
    fn glue_two_chunks_concatenates() {
        let dir = TempDir::new().unwrap();
        let mut paths = Vec::new();
        for i in 1..=2 {
            let p = dir.path().join(format!("handy-4-chunk-{}.opus", i));
            let mut w = OpusChunkWriter::create(&p).unwrap();
            w.write_frame(&vec![0.05f32; 4800]).unwrap();
            paths.push(w.finalize().unwrap());
        }
        let out = dir.path().join("handy-4.opus");
        glue_chunks(&paths, &out).unwrap();
        let glued = std::fs::read(&out).unwrap();
        let expected: usize = paths
            .iter()
            .map(|p| std::fs::metadata(p).unwrap().len() as usize)
            .sum();
        assert_eq!(glued.len(), expected);
        assert!(crc_ok(&glued), "chained file pages all CRC-valid");
        // Distinct serials per chunk (required for a valid chain).
        assert_ne!(serial_for(&paths[0]), serial_for(&paths[1]));
    }

    // ------------------------------------------------------------------
    // Single-chunk structure: headers, CRCs, granule positions, BOS/EOS.
    // ------------------------------------------------------------------

    #[test]
    fn header_pages_have_correct_structure() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("handy-10-chunk-1.opus");
        let mut w = OpusChunkWriter::create(&path).unwrap();
        w.write_frame(&vec![0.1f32; 480]).unwrap();
        w.finalize().unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert!(crc_ok(&bytes), "all page CRCs valid");
        let pages = parse_pages(&bytes);
        assert_eq!(pages.len(), 3, "head + tags + one audio/EOS page");

        // Page 0: BOS, lone OpusHead packet with the fields we actually stamp.
        assert_eq!(pages[0].header_type & 0x02, 0x02, "first page must be BOS");
        assert_eq!(pages[0].page_seq, 0);
        assert_eq!(pages[0].serial, serial_for(&path));
        assert_eq!(&pages[0].body[0..8], b"OpusHead");
        assert_eq!(pages[0].body[8], 1, "OpusHead version must be 1");
        assert_eq!(pages[0].body[9], 1, "mono channel count");
        let pre_skip = u16::from_le_bytes([pages[0].body[10], pages[0].body[11]]);
        assert_eq!(pre_skip, PRE_SKIP_48K);
        let rate = u32::from_le_bytes(pages[0].body[12..16].try_into().unwrap());
        assert_eq!(rate, IN_RATE_HZ);
        assert_eq!(pages[0].granule, 0, "header page carries no audio granule");

        // Page 1: OpusTags, not BOS/EOS.
        assert_eq!(pages[1].header_type, 0x00);
        assert_eq!(&pages[1].body[0..8], b"OpusTags");
        let vendor_len = u32::from_le_bytes(pages[1].body[8..12].try_into().unwrap()) as usize;
        assert_eq!(&pages[1].body[12..12 + vendor_len], b"handy");

        // Last page: EOS, granule reflects the TRUE (un-padded) sample count,
        // even though write_frame's 480 samples get zero-padded to 640 for
        // encoding (480 -> one 320-frame now, one zero-padded 320-frame at
        // finalize).
        let last = pages.last().unwrap();
        assert_eq!(last.header_type & 0x04, 0x04, "last page must be EOS");
        let expected_granule = PRE_SKIP_48K as u64 + 480 * 3;
        assert_eq!(last.granule, expected_granule);
    }

    #[test]
    fn single_chunk_granules_and_flags_are_correct_across_pages() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("handy-11-chunk-1.opus");
        let mut w = OpusChunkWriter::create(&path).unwrap();
        // 120 native 320-sample frames = 120 packets, forcing multiple
        // ~500 ms pages (PACKETS_PER_PAGE = 25) beyond the two header pages.
        let frame = vec![0.15f32; ENC_FRAME_SAMPLES];
        for _ in 0..120 {
            w.write_frame(&frame).unwrap();
        }
        w.finalize().unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert!(crc_ok(&bytes), "all page CRCs valid");
        let pages = parse_pages(&bytes);
        assert!(
            pages.len() >= 5,
            "expected head+tags+multiple audio pages, got {}",
            pages.len()
        );

        // Only the first page is BOS, only the last page is EOS; every page
        // seq is sequential starting at 0.
        for (i, p) in pages.iter().enumerate() {
            assert_eq!(p.page_seq, i as u32, "page_seq must be sequential");
            let is_first = i == 0;
            let is_last = i == pages.len() - 1;
            assert_eq!(
                p.header_type & 0x02 != 0,
                is_first,
                "BOS flag mismatch at page {i}"
            );
            assert_eq!(
                p.header_type & 0x04 != 0,
                is_last,
                "EOS flag mismatch at page {i}"
            );
        }

        // Audio-page granules (skip the two all-zero header pages) never regress.
        let audio_granules: Vec<u64> = pages[2..].iter().map(|p| p.granule).collect();
        for pair in audio_granules.windows(2) {
            assert!(
                pair[1] >= pair[0],
                "granule regressed: {} -> {}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn zero_frame_chunk_produces_valid_minimal_stream() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("handy-12-chunk-1.opus");
        let w = OpusChunkWriter::create(&path).unwrap();
        // No write_frame calls at all: a chunk opened and immediately
        // finalized (e.g. a recording stopped the instant it started).
        let out = w.finalize().unwrap();
        assert_eq!(out, path);

        let bytes = std::fs::read(&path).unwrap();
        assert!(crc_ok(&bytes), "all page CRCs valid");
        let pages = parse_pages(&bytes);
        assert_eq!(pages.len(), 3, "head + tags + an empty EOS page");
        let last = pages.last().unwrap();
        assert_eq!(last.header_type & 0x04, 0x04, "final page must carry EOS");
        assert!(last.body.is_empty(), "EOS page has no audio segments");
        assert_eq!(
            last.granule, PRE_SKIP_48K as u64,
            "granule reflects zero true samples"
        );
    }

    #[test]
    fn glue_chunks_rejects_empty_list() {
        let dir = TempDir::new().unwrap();
        let out = dir.path().join("handy-19.opus");
        let err = glue_chunks(&[], &out).unwrap_err();
        assert!(
            err.to_string().contains("no chunks"),
            "unexpected error: {err}"
        );
    }

    // ------------------------------------------------------------------
    // Multi-chunk glue: distinct serials, each chain's own BOS/EOS (T-110).
    // ------------------------------------------------------------------

    #[test]
    fn glue_preserves_bos_eos_per_chain() {
        let dir = TempDir::new().unwrap();
        let mut paths = Vec::new();
        for i in 1..=2 {
            let p = dir.path().join(format!("handy-13-chunk-{}.opus", i));
            let mut w = OpusChunkWriter::create(&p).unwrap();
            w.write_frame(&vec![0.05f32; 4800]).unwrap();
            paths.push(w.finalize().unwrap());
        }
        let out = dir.path().join("handy-13.opus");
        glue_chunks(&paths, &out).unwrap();
        let glued = std::fs::read(&out).unwrap();
        assert!(crc_ok(&glued), "chained file pages all CRC-valid");
        let pages = parse_pages(&glued);

        let serial_a = serial_for(&paths[0]);
        let serial_b = serial_for(&paths[1]);
        assert_ne!(
            serial_a, serial_b,
            "distinct chunks must carry distinct serials"
        );

        let chain_a: Vec<&PageInfo> = pages.iter().filter(|p| p.serial == serial_a).collect();
        let chain_b: Vec<&PageInfo> = pages.iter().filter(|p| p.serial == serial_b).collect();
        assert!(!chain_a.is_empty() && !chain_b.is_empty());

        // Each chain has its own BOS at its first page and its own EOS at its
        // last page -- a chained-Ogg file is really two independent logical
        // streams, not one continuing stream.
        assert_eq!(chain_a.first().unwrap().header_type & 0x02, 0x02);
        assert_eq!(chain_a.last().unwrap().header_type & 0x04, 0x04);
        assert_eq!(chain_b.first().unwrap().header_type & 0x02, 0x02);
        assert_eq!(chain_b.last().unwrap().header_type & 0x04, 0x04);
        // Each chain's own page_seq restarts at 0 (independent per-serial
        // numbering, as the Ogg spec requires).
        assert_eq!(chain_a.first().unwrap().page_seq, 0);
        assert_eq!(chain_b.first().unwrap().page_seq, 0);

        // Glue is a straight byte concat: all of A's pages precede all of B's.
        let a_end = pages.iter().rposition(|p| p.serial == serial_a).unwrap();
        let b_start = pages.iter().position(|p| p.serial == serial_b).unwrap();
        assert!(
            a_end < b_start,
            "chunk A's pages must all come before chunk B's"
        );
    }

    #[test]
    fn glued_chunks_with_distinct_serials_decode_in_order() {
        // T-110: encode_samples_to_ogg_opus always uses serial 1, so gluing two
        // of its outputs (as the old test did) exercises ONE decoder stream
        // treating the second header as a stray packet, not the real per-serial
        // path that production `glue_chunks` (distinct serial per chunk) takes.
        // This test goes through OpusChunkWriter + distinct filenames, exactly
        // like the recorder does, so the decoder's per-serial state machine in
        // `decode.rs` is genuinely exercised end to end.
        use crate::audio_toolkit::audio::decode_audio_file;

        let dir = TempDir::new().unwrap();
        let loud = tone(16_000); // ~1s, real energy (not DC -- opus's VOIP
        // high-pass filter would flatten a constant signal)
        let quiet = vec![0.0f32; 16_000]; // ~1s silence

        let p1 = dir.path().join("handy-14-chunk-1.opus");
        let mut w1 = OpusChunkWriter::create(&p1).unwrap();
        w1.write_frame(&loud).unwrap();
        let c1 = w1.finalize().unwrap();

        let p2 = dir.path().join("handy-14-chunk-2.opus");
        let mut w2 = OpusChunkWriter::create(&p2).unwrap();
        w2.write_frame(&quiet).unwrap();
        let c2 = w2.finalize().unwrap();

        // The whole point of this test vs. the encode_samples_to_ogg_opus-based
        // one: these two chunks really do carry distinct serials.
        assert_ne!(serial_for(&c1), serial_for(&c2));

        let glued = dir.path().join("handy-14.opus");
        glue_chunks(&[c1, c2], &glued).unwrap();

        let decoded = decode_audio_file(&glued).unwrap();
        assert!(
            decoded.len() > 28_000,
            "expected ~2s decoded, got {}",
            decoded.len()
        );

        let half = decoded.len() / 2;
        let rms = |s: &[f32]| (s.iter().map(|x| x * x).sum::<f32>() / s.len().max(1) as f32).sqrt();
        assert!(
            rms(&decoded[..half]) > 0.1,
            "first half should carry the loud chunk's signal"
        );
        assert!(
            rms(&decoded[half..]) < 0.05,
            "second half should carry the quiet chunk's signal"
        );
    }

    // ------------------------------------------------------------------
    // Crash recovery: truncated final page.
    // ------------------------------------------------------------------

    #[test]
    fn repair_recovers_a_chunk_with_zero_audio_frames() {
        // Simulate a crash immediately after the chunk was opened -- before
        // any write_frame call and before finalize() ever ran.
        let dir = TempDir::new().unwrap();
        let final_path = dir.path().join("handy-15-chunk-1.opus");
        let w = OpusChunkWriter::create(&final_path).unwrap();
        let temp_path = w.path().to_path_buf();
        drop(w);

        let samples = repair_truncated_opus(&temp_path).unwrap();
        assert_eq!(samples, 0, "no audio was ever written");

        let repaired = std::fs::read(&temp_path).unwrap();
        assert!(crc_ok(&repaired), "repaired file has valid page CRCs");
        let pages = parse_pages(&repaired);
        assert_eq!(pages.len(), 2, "only head + tags pages existed to recover");
        assert_eq!(
            pages.last().unwrap().header_type & 0x04,
            0x04,
            "last surviving page is marked EOS"
        );
    }

    #[test]
    fn repair_rejects_a_file_with_no_complete_pages() {
        let dir = TempDir::new().unwrap();
        let final_path = dir.path().join("handy-16-chunk-1.opus");
        let w = OpusChunkWriter::create(&final_path).unwrap();
        let temp_path = w.path().to_path_buf();
        drop(w);

        let mut bytes = std::fs::read(&temp_path).unwrap();
        bytes.truncate(10); // well within page 0's 27-byte header
        std::fs::write(&temp_path, &bytes).unwrap();

        let err = repair_truncated_opus(&temp_path).unwrap_err();
        assert!(
            err.to_string().contains("no recoverable audio"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn repair_handles_truncation_at_many_offsets() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("handy-17-chunk-1.opus");
        {
            let mut w = OpusChunkWriter::create(&path).unwrap();
            for _ in 0..200 {
                w.write_frame(&vec![0.2f32; 480]).unwrap();
            }
            w.finalize().unwrap();
        }
        let full = std::fs::read(&path).unwrap();
        let full_len = full.len();

        // Chop off a handful of different tail lengths, all landing mid-page
        // (never on a page boundary), and confirm each repaired result is a
        // structurally valid, CRC-clean Ogg stream with an EOS-marked final
        // page and earlier pages preserved intact -- never a torn/corrupt file.
        for chop in [1usize, 5, 20, 37, 60, 90] {
            assert!(
                chop < full_len,
                "chop {chop} exceeds file length {full_len}"
            );
            let mut truncated = full.clone();
            truncated.truncate(full_len - chop);
            std::fs::write(&path, &truncated).unwrap();

            let samples = repair_truncated_opus(&path).unwrap();
            assert!(samples > 0, "chop {chop}: expected some recovered audio");

            let repaired = std::fs::read(&path).unwrap();
            assert!(
                crc_ok(&repaired),
                "chop {chop}: repaired file has valid CRCs"
            );
            let pages = parse_pages(&repaired);
            assert!(
                pages.len() >= 2,
                "chop {chop}: expected head+tags to survive"
            );
            assert_eq!(
                pages.last().unwrap().header_type & 0x04,
                0x04,
                "chop {chop}: last surviving page must be marked EOS"
            );
            assert!(
                repaired.len() <= truncated.len(),
                "chop {chop}: repair must not grow the file"
            );

            // Earlier pages are preserved byte-for-byte -- repair only ever
            // rewrites the last surviving page's header_type bit and CRC, then
            // truncates. Recompute that last page's start offset (by walking
            // all pages up to it) and diff the untouched prefix against the
            // pristine original file.
            let mut last_page_start = 0usize;
            for _ in 0..pages.len() - 1 {
                let nsegs = repaired[last_page_start + 26] as usize;
                let header_end = last_page_start + 27 + nsegs;
                let body_len: usize = repaired[last_page_start + 27..header_end]
                    .iter()
                    .map(|&b| b as usize)
                    .sum();
                last_page_start = header_end + body_len;
            }
            assert_eq!(
                &repaired[..last_page_start],
                &full[..last_page_start],
                "chop {chop}: earlier pages preserved byte-for-byte"
            );
        }
    }

    // ------------------------------------------------------------------
    // CRC validation catches corruption.
    // ------------------------------------------------------------------

    #[test]
    fn crc_check_detects_body_corruption() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("handy-18-chunk-1.opus");
        let mut w = OpusChunkWriter::create(&path).unwrap();
        w.write_frame(&vec![0.25f32; 4800]).unwrap();
        w.finalize().unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert!(crc_ok(&bytes), "sanity: original file is CRC-clean");

        // Flip one bit deep in the last page's body. The page stays
        // structurally valid (same lengths/offsets), so only the CRC can
        // catch the tampering.
        let mut corrupted = bytes.clone();
        let tail = corrupted.len() - 1;
        corrupted[tail] ^= 0x01;
        assert!(
            !crc_ok(&corrupted),
            "flipping a body byte must invalidate its page CRC"
        );

        // And corruption in the segment table / header area is caught too.
        let mut corrupted_header = bytes.clone();
        corrupted_header[7] ^= 0xFF; // inside the granule field of page 0
        assert!(
            !crc_ok(&corrupted_header),
            "flipping a header byte must invalidate its page CRC"
        );
    }
}
