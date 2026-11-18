use anyhow::Result;
use log::{debug, info};
use std::io::Write;

/// Transcribe audio via an OpenAI-compatible `audio/transcriptions` endpoint.
/// When `translate_to_english` is true, tries `audio/translations` first and
/// falls back to `audio/transcriptions` if the server returns 404 (the server
/// is then remembered and reported once, see take_translation_unsupported_notice).
/// `language` None = the server detects it (never send "en" for Auto).
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

    // Try translations endpoint first when requested - unless this server
    // already said it has none (404) in this session.
    if translate_to_english && !translations_missing(base) {
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
                if is_not_found(&e) {
                    info!(
                        "API transcription: translations endpoint returned 404, falling back to transcriptions"
                    );
                    mark_translations_missing(base);
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

/// Servers that answered 404 on `audio/translations` this session (they can
/// only transcribe), and whether the user was told about each one yet.
static NO_TRANSLATIONS: std::sync::Mutex<Vec<(String, bool)>> = std::sync::Mutex::new(Vec::new());

fn translations_missing(base: &str) -> bool {
    NO_TRANSLATIONS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .any(|(b, _)| b == base)
}

/// Servers kept at most (the oldest is forgotten first).
const NO_TRANSLATIONS_MAX: usize = 16;

fn mark_translations_missing(base: &str) {
    let mut list = NO_TRANSLATIONS.lock().unwrap_or_else(|p| p.into_inner());
    if !list.iter().any(|(b, _)| b == base) {
        if list.len() >= NO_TRANSLATIONS_MAX {
            list.remove(0);
        }
        list.push((base.to_string(), false));
    }
}

/// The HTTP status a server answered with, kept on the error so a caller can
/// tell a real 404 from any error whose text happens to contain "404" (a URL
/// with port 4040, say).
#[derive(Debug)]
struct HttpStatus(u16);

impl std::fmt::Display for HttpStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HTTP {}", self.0)
    }
}

impl std::error::Error for HttpStatus {}

/// The server answered 404 Not Found (it has no such endpoint).
fn is_not_found(e: &anyhow::Error) -> bool {
    e.chain()
        .any(|cause| matches!(cause.downcast_ref::<HttpStatus>(), Some(HttpStatus(404))))
}

fn request_error(url: &str, e: ureq::Error) -> anyhow::Error {
    match e {
        ureq::Error::Status(code, _) => {
            anyhow::Error::new(HttpStatus(code)).context(format!("{url}: status code {code}"))
        }
        other => anyhow::anyhow!("{}: {}", url, other),
    }
}

/// A server found unable to translate that the user has not been told about
/// yet (each is reported once a session); the caller shows the notice.
pub fn take_translation_unsupported_notice() -> Option<String> {
    let mut list = NO_TRANSLATIONS.lock().unwrap_or_else(|p| p.into_inner());
    list.iter_mut()
        .find(|(_, told)| !*told)
        .map(|(base, told)| {
            *told = true;
            base.clone()
        })
}

/// Send a multipart request and parse the OpenAI-compatible response.
fn send_multipart_request(url: &str, api_key: &str, boundary: &str, body: &[u8]) -> Result<String> {
    let mut req = crate::net::ureq_agent(url).post(url).set(
        "Content-Type",
        &format!("multipart/form-data; boundary={}", boundary),
    );

    if !api_key.is_empty() {
        req = req.set("Authorization", &format!("Bearer {}", api_key));
    }

    let response = req.send_bytes(body).map_err(|e| request_error(url, e))?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::normalize_language_for_engine;

    /// The server list is shared: tests that use it run one at a time.
    static LIST_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn body_for(language: Option<&str>) -> String {
        String::from_utf8_lossy(&build_multipart_body(
            b"RIFF",
            "whisper-large-v3",
            "B",
            language,
        ))
        .to_string()
    }

    #[test]
    fn auto_sends_no_language_so_the_server_detects_it() {
        // Auto must not become "en": Whisper then translates other languages.
        let language = normalize_language_for_engine("auto");
        assert_eq!(language, None);
        let body = body_for(language.as_deref());
        assert!(!body.contains("name=\"language\""), "{body}");
        assert!(body.contains("name=\"model\""));
    }

    #[test]
    fn a_chosen_language_is_sent_as_is() {
        let language = normalize_language_for_engine("pl");
        let body = body_for(language.as_deref());
        assert!(body.contains("name=\"language\"\r\n\r\npl\r\n"), "{body}");
    }

    #[test]
    fn only_a_real_404_counts_as_no_translations_endpoint() {
        let not_found = anyhow::Error::new(HttpStatus(404)).context("x: status code 404");
        assert!(is_not_found(&not_found));
        let server_error = anyhow::Error::new(HttpStatus(500)).context("x");
        assert!(!is_not_found(&server_error));
        // "404" in the URL (port 4040) of a connection error is not a 404.
        let refused = anyhow::anyhow!("http://host:4040/v1/audio/translations: connection refused");
        assert!(!is_not_found(&refused));
    }

    #[test]
    fn the_server_list_is_capped() {
        let _serial = LIST_TESTS.lock().unwrap_or_else(|p| p.into_inner());
        for i in 0..(NO_TRANSLATIONS_MAX + 5) {
            mark_translations_missing(&format!("https://cap-{i}.invalid/v1"));
        }
        let len = NO_TRANSLATIONS.lock().unwrap().len();
        assert!(len <= NO_TRANSLATIONS_MAX, "{len}");
        assert!(translations_missing(&format!(
            "https://cap-{}.invalid/v1",
            NO_TRANSLATIONS_MAX + 4
        )));
    }

    #[test]
    fn a_server_without_translations_is_reported_once() {
        let _serial = LIST_TESTS.lock().unwrap_or_else(|p| p.into_inner());
        let base = "https://test-no-translations.invalid/v1";
        assert!(!translations_missing(base));
        mark_translations_missing(base);
        mark_translations_missing(base);
        assert!(translations_missing(base));
        let mut told = Vec::new();
        while let Some(server) = take_translation_unsupported_notice() {
            told.push(server);
        }
        assert_eq!(told.iter().filter(|s| *s == base).count(), 1);
        assert!(take_translation_unsupported_notice().is_none());
        // Still remembered: no further translations request to this server.
        assert!(translations_missing(base));
    }
}
