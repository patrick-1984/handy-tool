//! OpenRouter speech-to-text.
//!
//! OpenRouter does **not** accept OpenAI's `multipart/form-data`
//! `/v1/audio/transcriptions` upload. Instead it takes **JSON with base64
//! audio**, via either:
//!   - the dedicated STT endpoint `POST /api/v1/audio/transcriptions` with
//!     `{ model, input_audio: { data, format }, language? }` → text at root
//!     `.text` (best for Whisper / gpt-4o-transcribe / Chirp models); or
//!   - chat completions `POST /api/v1/chat/completions` with an `input_audio`
//!     content part → text at `choices[0].message.content` (for audio-capable
//!     LLMs like Gemini / gpt-4o-audio).
//!
//! Audio is sent as Ogg/Opus by default (≈10× smaller than WAV — light on the
//! network) reusing Handy's own Opus encoder, with WAV as a fallback. Because
//! OpenRouter times out provider calls at ~60 s, this engine is meant to run
//! per VAD segment (Live mode) or per crash-safe chunk, where each clip stays
//! well under that ceiling.

use anyhow::Result;
use log::{debug, info};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::settings::{OpenRouterTranscriptionRoute, TranscriptionAudioFormat};

// Per-recording OpenRouter cost accumulator. A single recording may transcribe
// several segments/chunks (each its own request), so we sum the real usage.cost
// values across all of them. Reset at recording start, read (and reset) when the
// history row is written. Stored as micro-dollars in an atomic for lock-free
// accumulation from the background transcription threads.
static SESSION_COST_MICROS: AtomicU64 = AtomicU64::new(0);
static SESSION_COST_KNOWN: AtomicBool = AtomicBool::new(false);

/// Reset the per-recording cost accumulator (call at recording start).
pub fn reset_session_cost() {
    SESSION_COST_MICROS.store(0, Ordering::SeqCst);
    SESSION_COST_KNOWN.store(false, Ordering::SeqCst);
}

fn add_session_cost(cost: f64) {
    if cost.is_finite() && cost >= 0.0 {
        SESSION_COST_MICROS.fetch_add((cost * 1_000_000.0).round() as u64, Ordering::SeqCst);
        SESSION_COST_KNOWN.store(true, Ordering::SeqCst);
    }
}

/// Take the accumulated per-recording cost (USD) and reset it. `None` when no
/// request reported a cost (e.g. the endpoint didn't include usage.cost).
pub fn take_session_cost() -> Option<f64> {
    let known = SESSION_COST_KNOWN.swap(false, Ordering::SeqCst);
    let micros = SESSION_COST_MICROS.swap(0, Ordering::SeqCst);
    if known {
        Some(micros as f64 / 1_000_000.0)
    } else {
        None
    }
}

const TRANSCRIBE_PROMPT: &str = "Transcribe this audio verbatim. Return only the \
spoken text — no commentary, labels, timestamps, or surrounding quotation marks.";

/// Chat-route English-translation prompt (T-308). The dedicated STT endpoint has
/// no translation control, so translate-to-English is only offered on the Chat
/// route, via this instruction.
const TRANSLATE_PROMPT: &str = "Translate the speech in this audio into English. \
Return only the English translation — no commentary, labels, timestamps, or \
surrounding quotation marks.";

/// Transcribe 16 kHz mono PCM via OpenRouter. `base_url` should be the
/// OpenRouter API root (e.g. `https://openrouter.ai/api/v1`).
pub fn transcribe(
    base_url: &str,
    api_key: &str,
    model: &str,
    audio: &[f32],
    language: Option<&str>,
    route: OpenRouterTranscriptionRoute,
    audio_format: TranscriptionAudioFormat,
    translate: bool,
) -> Result<String> {
    // The dedicated STT endpoint reliably accepts WAV; opus-in-ogg support varies
    // by model (Whisper rejects it), so force WAV for the STT route. The chat
    // `input_audio` route honors the configured format.
    let audio_format = match route {
        OpenRouterTranscriptionRoute::Stt => TranscriptionAudioFormat::Wav,
        OpenRouterTranscriptionRoute::Chat => audio_format,
    };
    let (bytes, fmt) = match audio_format {
        TranscriptionAudioFormat::Wav => (
            crate::managers::api_transcription::samples_to_wav(audio, 16000)?,
            "wav",
        ),
        TranscriptionAudioFormat::Opus => (
            crate::audio_toolkit::audio::encode_samples_to_ogg_opus(audio)?,
            "ogg",
        ),
    };
    let b64 = base64_encode(&bytes);
    let base = base_url.trim_end_matches('/');

    let (url, body) = match route {
        OpenRouterTranscriptionRoute::Stt => (
            format!("{}/audio/transcriptions", base),
            build_stt_body(model, &b64, fmt, language),
        ),
        OpenRouterTranscriptionRoute::Chat => (
            format!("{}/chat/completions", base),
            build_chat_body(model, &b64, fmt, translate),
        ),
    };

    info!(
        "OpenRouter transcription: POST {} (model: {}, route: {:?}, format: {}, {} audio bytes)",
        url,
        model,
        route,
        fmt,
        bytes.len()
    );

    let value = post_json(&url, api_key, &body)?;
    // Accumulate the real per-request cost when OpenRouter reports it.
    if let Some(cost) = value
        .get("usage")
        .and_then(|u| u.get("cost"))
        .and_then(|c| c.as_f64())
    {
        debug!("OpenRouter transcription cost: ${:.6}", cost);
        add_session_cost(cost);
    }
    let text = match route {
        OpenRouterTranscriptionRoute::Stt => parse_stt_response(&value),
        OpenRouterTranscriptionRoute::Chat => parse_chat_response(&value),
    };
    if text.is_empty() {
        // A 2xx with no text usually means a route/model mismatch (e.g. an STT
        // model on the chat route). Surface the raw response to make it diagnosable.
        let snippet: String = value.to_string().chars().take(400).collect();
        log::warn!(
            "OpenRouter transcription returned empty text (route {:?}); response: {}",
            route,
            snippet
        );
    }
    debug!("OpenRouter transcription result ({} chars)", text.len());
    Ok(text)
}

/// Body for the dedicated `/audio/transcriptions` endpoint.
fn build_stt_body(
    model: &str,
    b64: &str,
    format: &str,
    language: Option<&str>,
) -> serde_json::Value {
    // The dedicated /audio/transcriptions endpoint has no `usage` toggle (it
    // always returns usage.cost) and rejects/ignores chat-only fields, so we send
    // only the documented shape: model + input_audio{data,format} (+ optional
    // language). Omit language when None so OpenRouter auto-detects.
    let mut body = serde_json::json!({
        "model": model,
        "input_audio": { "data": b64, "format": format },
    });
    if let Some(lang) = language {
        body["language"] = serde_json::Value::String(lang.to_string());
    }
    body
}

/// Body for the chat-completions `input_audio` route.
fn build_chat_body(model: &str, b64: &str, format: &str, translate: bool) -> serde_json::Value {
    let prompt = if translate {
        TRANSLATE_PROMPT
    } else {
        TRANSCRIBE_PROMPT
    };
    serde_json::json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": [
                { "type": "text", "text": prompt },
                { "type": "input_audio", "input_audio": { "data": b64, "format": format } }
            ]
        }],
        // Ask OpenRouter to include the real per-request cost in the response.
        "usage": { "include": true },
    })
}

/// Dedicated transcription endpoint returns the text at the root `text` field.
fn parse_stt_response(value: &serde_json::Value) -> String {
    value
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// Chat route returns the text at `choices[0].message.content`.
fn parse_chat_response(value: &serde_json::Value) -> String {
    value["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string()
}

fn post_json(url: &str, api_key: &str, body: &serde_json::Value) -> Result<serde_json::Value> {
    let mut req = ureq::post(url)
        // Bound the request so a stalled/misrouted call can't hang the pipeline
        // (the chunked path would otherwise wait on it up to its 15-min backstop).
        .timeout(std::time::Duration::from_secs(120))
        .set("Content-Type", "application/json")
        .set("X-Title", "Handy")
        .set("HTTP-Referer", "https://handy.computer");
    if !api_key.is_empty() {
        req = req.set("Authorization", &format!("Bearer {}", api_key));
    }

    let payload = serde_json::to_string(body)?;
    let response = match req.send_string(&payload) {
        Ok(r) => r,
        // ureq returns non-2xx as Error::Status; its Display drops the body, so
        // read it to surface OpenRouter's actual reason (bad model, no audio
        // support, out of credits, …) instead of just a status code.
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            anyhow::bail!("{}: HTTP {} — {}", url, code, body.trim());
        }
        Err(e) => anyhow::bail!("{}: {}", url, e),
    };
    let body_str = response
        .into_string()
        .map_err(|e| anyhow::anyhow!("Failed to read OpenRouter response: {}", e))?;
    serde_json::from_str(&body_str)
        .map_err(|e| anyhow::anyhow!("Failed to parse OpenRouter response: {}", e))
}

/// Standard base64 (RFC 4648, padded). Hand-rolled std-only to avoid pulling a
/// new dependency (matches the project's std-only Ogg muxer approach).
pub fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_rfc4648_vectors() {
        // The canonical RFC 4648 test vectors.
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(&[0xff, 0xff, 0xff]), "////");
    }

    #[test]
    fn stt_body_includes_language_and_audio() {
        let body = build_stt_body("openai/whisper-large-v3", "QUJD", "ogg", Some("en"));
        assert_eq!(body["model"], "openai/whisper-large-v3");
        assert_eq!(body["input_audio"]["data"], "QUJD");
        assert_eq!(body["input_audio"]["format"], "ogg");
        assert_eq!(body["language"], "en");
    }

    #[test]
    fn stt_body_omits_language_when_none() {
        let body = build_stt_body("m", "QUJD", "wav", None);
        assert!(body.get("language").is_none());
        assert_eq!(body["input_audio"]["format"], "wav");
    }

    #[test]
    fn chat_body_has_text_then_audio_parts() {
        let body = build_chat_body("google/gemini-2.5-flash", "QUJD", "wav", false);
        assert_eq!(body["model"], "google/gemini-2.5-flash");
        let content = &body["messages"][0]["content"];
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["type"], "input_audio");
        assert_eq!(content[1]["input_audio"]["data"], "QUJD");
        assert_eq!(content[1]["input_audio"]["format"], "wav");
        assert_eq!(body["messages"][0]["role"], "user");
    }

    #[test]
    fn parses_stt_root_text_trimmed() {
        let v = serde_json::json!({ "text": "  hello world  " });
        assert_eq!(parse_stt_response(&v), "hello world");
    }

    #[test]
    fn parses_chat_choice_content_trimmed() {
        let v = serde_json::json!({ "choices": [ { "message": { "content": " hi " } } ] });
        assert_eq!(parse_chat_response(&v), "hi");
    }

    #[test]
    fn parses_missing_fields_as_empty() {
        assert_eq!(parse_stt_response(&serde_json::json!({})), "");
        assert_eq!(parse_chat_response(&serde_json::json!({})), "");
    }
}
