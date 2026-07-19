use anyhow::Result;
use log::{debug, info};
use std::io::Write;

/// Transcribe audio via an OpenAI-compatible `audio/transcriptions` endpoint.
/// When `translate_to_english` is true, tries `audio/translations` first and
/// falls back to `audio/transcriptions` if the server returns 404.
/// The base_url should already include the version prefix (e.g. `http://host/v1`).
pub fn transcribe(
    base_url: &str,
    api_key: &str,
    model: &str,
    audio: Vec<f32>,
    language: Option<&str>,
    translate_to_english: bool,
) -> Result<String> {
    let wav_bytes = samples_to_wav(&audio, 16000)?;
    let base = base_url.trim_end_matches('/');
    let boundary = "----HandyBoundary";
    let body = build_multipart_body(&wav_bytes, model, boundary, language);

    // Try translations endpoint first when requested
    if translate_to_english {
        let url = format!("{}/audio/translations", base);
        info!(
            "API transcription: POST {} (model: {}, language: {}, {} bytes WAV)",
            url,
            model,
            language.unwrap_or("auto"),
            wav_bytes.len()
        );

        match send_multipart_request(&url, api_key, boundary, &body) {
            Ok(text) => return Ok(text),
            Err(e) => {
                let msg = format!("{}", e);
                if msg.contains("404") {
                    info!(
                        "API transcription: translations endpoint returned 404, falling back to transcriptions"
                    );
                } else {
                    return Err(e);
                }
            }
        }
    }

    // Default: transcriptions endpoint
    let url = format!("{}/audio/transcriptions", base);
    info!(
        "API transcription: POST {} (model: {}, language: {}, {} bytes WAV)",
        url,
        model,
        language.unwrap_or("auto"),
        wav_bytes.len()
    );

    send_multipart_request(&url, api_key, boundary, &body)
}

/// Send a multipart request and parse the OpenAI-compatible response.
fn send_multipart_request(url: &str, api_key: &str, boundary: &str, body: &[u8]) -> Result<String> {
    let mut req = ureq::post(url).set(
        "Content-Type",
        &format!("multipart/form-data; boundary={}", boundary),
    );

    if !api_key.is_empty() {
        req = req.set("Authorization", &format!("Bearer {}", api_key));
    }

    let response = req
        .send_bytes(body)
        .map_err(|e| anyhow::anyhow!("{}: {}", url, e))?;

    let body_str = response
        .into_string()
        .map_err(|e| anyhow::anyhow!("Failed to read API response: {}", e))?;

    // Parse OpenAI-compatible response: {"text": "..."}
    let parsed: serde_json::Value = serde_json::from_str(&body_str)
        .map_err(|e| anyhow::anyhow!("Failed to parse API response: {}", e))?;

    let text = parsed
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    debug!("API transcription result: {}", text);
    Ok(text)
}

/// Convert f32 samples (16kHz mono) to WAV bytes.
pub(crate) fn samples_to_wav(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>> {
    let num_samples = samples.len();
    let bytes_per_sample = 2u16; // 16-bit PCM
    let data_size = (num_samples * bytes_per_sample as usize) as u32;
    let file_size = 36 + data_size;

    let mut buf = Vec::with_capacity(file_size as usize + 8);

    // RIFF header
    buf.write_all(b"RIFF")?;
    buf.write_all(&file_size.to_le_bytes())?;
    buf.write_all(b"WAVE")?;

    // fmt chunk
    buf.write_all(b"fmt ")?;
    buf.write_all(&16u32.to_le_bytes())?;
    buf.write_all(&1u16.to_le_bytes())?; // PCM
    buf.write_all(&1u16.to_le_bytes())?; // mono
    buf.write_all(&sample_rate.to_le_bytes())?;
    buf.write_all(&(sample_rate * bytes_per_sample as u32).to_le_bytes())?;
    buf.write_all(&bytes_per_sample.to_le_bytes())?;
    buf.write_all(&16u16.to_le_bytes())?; // bits per sample

    // data chunk
    buf.write_all(b"data")?;
    buf.write_all(&data_size.to_le_bytes())?;
    for &sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let int_sample = (clamped * 32767.0) as i16;
        buf.write_all(&int_sample.to_le_bytes())?;
    }

    Ok(buf)
}

/// Build a multipart/form-data body for the OpenAI transcription endpoint.
fn build_multipart_body(
    wav_bytes: &[u8],
    model: &str,
    boundary: &str,
    language: Option<&str>,
) -> Vec<u8> {
    let mut body = Vec::new();

    // file field
    let _ = write!(
        body,
        "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\n",
        boundary
    );
    body.extend_from_slice(wav_bytes);
    let _ = write!(body, "\r\n");

    // model field
    if !model.is_empty() {
        let _ = write!(
            body,
            "--{}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n{}\r\n",
            boundary, model
        );
    }

    // language field — omit when None to let server auto-detect
    if let Some(lang) = language {
        let _ = write!(
            body,
            "--{}\r\nContent-Disposition: form-data; name=\"language\"\r\n\r\n{}\r\n",
            boundary, lang
        );
    }

    // closing boundary
    let _ = write!(body, "--{}--\r\n", boundary);
    body
}
