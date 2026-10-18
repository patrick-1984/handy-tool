//! Custom STT server: a speech-to-text server on your network (or this PC)
//! that speaks the OpenAI `audio/transcriptions` API and can also tell the
//! speakers apart (diarization).
//!
//! One request per take (the whole recording, so speaker labels stay
//! consistent across it), sent as Ogg/Opus. Two response shapes are read:
//!
//! - OpenAI style: `{"text": "...", "segments": [{"speaker": "A", "text": ...}]}`
//!   (e.g. a `transcribe-diarize-openai` model) - the plain text comes from
//!   `text`, the speaker text is rebuilt from the segments;
//! - readable style: `{"text": "A: ...\n\nB: ..."}` (e.g. `transcribe-diarize`)
//!   - the speaker text is `text` itself and the plain text is the same text
//!   without the speaker labels.
//!
//! With one voice (or a plain `transcribe` model) there is no speaker text.
//! Your custom words are sent as `prompt` (the OpenAI field for vocabulary);
//! Handy also corrects them afterwards, as for every engine.

use anyhow::Result;
use log::{debug, info};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::io::Write;
use std::sync::Mutex;

/// What a take came back as.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SttServerResult {
    /// Clean text, no speakers.
    pub text: String,
    /// One `A: ...` paragraph per speaker turn; `None` with fewer than two
    /// speakers.
    pub diarized: Option<String>,
}

/// One model the server offers (`GET {base}/models`).
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct SttServerModel {
    pub id: String,
    pub description: String,
}

/// The server's answer to `GET {base}/models`: its models and, when it says,
/// what runs behind them.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct SttServerInfo {
    pub models: Vec<SttServerModel>,
    /// e.g. "asr: nvidia/parakeet-tdt-0.6b-v3 · diarization: nvidia/..."
    pub backend: Option<String>,
}

// The speaker text of a dictation take, keyed by the take (failover's take
// number): set by the take's own transcription (failover::transcribe) and
// taken by that same take when it decides what to paste and saves to
// History. Another take can neither read nor overwrite it. Never written by
// watched folders or file jobs.
static SESSION_DIARIZED: Mutex<Option<(u64, String)>> = Mutex::new(None);

/// Forget any speaker text (call at recording start).
pub fn reset_session_diarized() {
    *SESSION_DIARIZED.lock().unwrap_or_else(|p| p.into_inner()) = None;
}

/// Take `take`'s speaker text (`None` clears what it had).
pub fn set_session_diarized(take: u64, text: Option<String>) {
    *SESSION_DIARIZED.lock().unwrap_or_else(|p| p.into_inner()) = text.map(|t| (take, t));
}

/// Take `take`'s speaker text, once - nothing if the slot holds another
/// take's (which then stays for that take).
pub fn take_session_diarized(take: u64) -> Option<String> {
    let mut slot = SESSION_DIARIZED.lock().unwrap_or_else(|p| p.into_inner());
    if slot.as_ref().is_some_and(|(owner, _)| *owner == take) {
        slot.take().map(|(_, text)| text)
    } else {
        None
    }
}

fn endpoint(base_url: &str, path: &str) -> String {
    format!("{}/{}", base_url.trim().trim_end_matches('/'), path)
}

/// Transcribe a take (16 kHz mono PCM).
pub fn transcribe(
    base_url: &str,
    api_key: &str,
    model: &str,
    audio: &[f32],
    language: Option<&str>,
    prompt: Option<&str>,
    speakers: bool,
) -> Result<SttServerResult> {
    let ogg = crate::audio_toolkit::audio::encode_samples_to_ogg_opus(audio)?;
    let url = endpoint(base_url, "audio/transcriptions");
    let boundary = "----HandySttBoundary";
    let body = multipart_body(&ogg, model, language, prompt, boundary);
    info!(
        "STT server: POST {} (model: {}, language: {}, {} bytes Ogg/Opus, prompt: {} chars)",
        url,
        model,
        language.unwrap_or("auto"),
        ogg.len(),
        prompt.map(str::len).unwrap_or(0)
    );
    let mut req = crate::net::ureq_agent(&url).post(&url).set(
        "Content-Type",
        &format!("multipart/form-data; boundary={boundary}"),
    );
    if !api_key.trim().is_empty() {
        req = req.set("Authorization", &format!("Bearer {}", api_key.trim()));
    }
    // A stalled server must not hold the take (and the engine lock) forever:
    // two minutes plus the take's own length, generous for a slow server.
    let audio_secs = audio.len() as u64 / 16_000;
    let response = req
        .timeout(std::time::Duration::from_secs(120 + audio_secs))
        .send_bytes(&body)
        .map_err(|e| anyhow::anyhow!("{url}: {e}"))?;
    let text = response
        .into_string()
        .map_err(|e| anyhow::anyhow!("Failed to read the STT server's answer: {e}"))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| anyhow::anyhow!("The STT server's answer is not JSON: {e}"))?;
    let result = parse_response(&value, speakers);
    debug!(
        "STT server: {} chars, speaker text: {}",
        result.text.len(),
        result.diarized.is_some()
    );
    Ok(result)
}

/// The server's models (`GET {base}/models`).
pub fn list_models(base_url: &str, api_key: &str) -> Result<SttServerInfo> {
    let url = endpoint(base_url, "models");
    let mut req = crate::net::ureq_agent(&url)
        .get(&url)
        .timeout(std::time::Duration::from_secs(15));
    if !api_key.trim().is_empty() {
        req = req.set("Authorization", &format!("Bearer {}", api_key.trim()));
    }
    let text = req
        .call()
        .map_err(|e| anyhow::anyhow!("{url}: {e}"))?
        .into_string()
        .map_err(|e| anyhow::anyhow!("Failed to read the STT server's model list: {e}"))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| anyhow::anyhow!("The STT server's model list is not JSON: {e}"))?;
    Ok(parse_models(&value))
}

fn multipart_body(
    ogg: &[u8],
    model: &str,
    language: Option<&str>,
    prompt: Option<&str>,
    boundary: &str,
) -> Vec<u8> {
    let mut body = Vec::new();
    let _ = write!(
        body,
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.ogg\"\r\nContent-Type: audio/ogg\r\n\r\n"
    );
    body.extend_from_slice(ogg);
    let _ = write!(body, "\r\n");
    let mut field = |name: &str, value: &str| {
        let _ = write!(
            body,
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
        );
    };
    if !model.trim().is_empty() {
        field("model", model.trim());
    }
    if let Some(language) = language {
        field("language", language);
    }
    if let Some(prompt) = prompt.map(str::trim).filter(|p| !p.is_empty()) {
        field("prompt", prompt);
    }
    let _ = write!(body, "--{boundary}--\r\n");
    body
}

/// Looks like a diarization speaker label: `A`, `B2`, `Speaker 1`,
/// `SPEAKER_00` - not an ordinary word such as `Note`.
fn speaker_like(label: &str) -> bool {
    let mut chars = label.chars();
    let letter_then_digits = matches!(chars.next(), Some(c) if c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_digit())
        && label.len() <= 3;
    let lower = label.to_ascii_lowercase();
    let numbered = lower
        .strip_prefix("speaker")
        .map(|rest| rest.trim_start_matches([' ', '_', '-']))
        .is_some_and(|n| !n.is_empty() && n.len() <= 3 && n.chars().all(|c| c.is_ascii_digit()));
    letter_then_digits || numbered
}

/// A speaker label at the start of a readable-style paragraph: `A: text`.
fn split_label(paragraph: &str) -> Option<(&str, &str)> {
    let (label, rest) = paragraph.split_once(':')?;
    let label = label.trim();
    speaker_like(label).then_some((label, rest.trim()))
}

/// `speakers`: a speakers model was asked for - only then is a readable
/// `A: ...` answer split into speaker text and plain text.
pub(crate) fn parse_response(value: &serde_json::Value, speakers: bool) -> SttServerResult {
    let text = value
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    // OpenAI style: segments with speakers.
    if let Some(segments) = value.get("segments").and_then(|s| s.as_array()) {
        let mut turns: Vec<(String, String)> = Vec::new();
        for segment in segments {
            let words = segment
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .trim();
            if words.is_empty() {
                continue;
            }
            let speaker = segment
                .get("speaker")
                .and_then(|s| {
                    s.as_str()
                        .map(str::to_string)
                        .or_else(|| s.as_i64().map(|n| n.to_string()))
                })
                .unwrap_or_default();
            match turns.last_mut() {
                Some((last, said)) if *last == speaker => {
                    said.push(' ');
                    said.push_str(words);
                }
                _ => turns.push((speaker, words.to_string())),
            }
        }
        let speakers: std::collections::BTreeSet<&str> = turns
            .iter()
            .map(|(s, _)| s.as_str())
            .filter(|s| !s.is_empty())
            .collect();
        if speakers.len() >= 2 {
            let diarized = turns
                .iter()
                .map(|(speaker, said)| {
                    if speaker.is_empty() {
                        said.clone()
                    } else {
                        format!("{speaker}: {said}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            let text = if text.is_empty() {
                turns
                    .iter()
                    .map(|(_, s)| s.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                text
            };
            return SttServerResult {
                text,
                diarized: Some(diarized),
            };
        }
        return SttServerResult {
            text,
            diarized: None,
        };
    }

    if !speakers {
        return SttServerResult {
            text,
            diarized: None,
        };
    }
    // Readable style: "A: ...\n\nB: ..." paragraphs.
    let paragraphs: Vec<&str> = text
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let labelled: Vec<(&str, &str)> = paragraphs.iter().filter_map(|p| split_label(p)).collect();
    let speakers: std::collections::BTreeSet<&str> = labelled.iter().map(|(l, _)| *l).collect();
    // Every paragraph labelled: the plain text loses the labels; a separate
    // speaker text only with two or more voices.
    if !paragraphs.is_empty() && labelled.len() == paragraphs.len() && speakers.len() == 1 {
        let plain = labelled
            .iter()
            .map(|(_, said)| *said)
            .collect::<Vec<_>>()
            .join(" ");
        return SttServerResult {
            text: plain,
            diarized: None,
        };
    }
    if paragraphs.len() >= 2 && labelled.len() == paragraphs.len() && speakers.len() >= 2 {
        let plain = labelled
            .iter()
            .map(|(_, said)| *said)
            .collect::<Vec<_>>()
            .join(" ");
        return SttServerResult {
            text: plain,
            diarized: Some(paragraphs.join("\n\n")),
        };
    }
    SttServerResult {
        text,
        diarized: None,
    }
}

pub(crate) fn parse_models(value: &serde_json::Value) -> SttServerInfo {
    let models = value
        .get("data")
        .and_then(|d| d.as_array())
        .map(|data| {
            data.iter()
                .filter_map(|m| {
                    let id = m.get("id")?.as_str()?.to_string();
                    let description = m
                        .get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(SttServerModel { id, description })
                })
                .collect()
        })
        .unwrap_or_default();
    let backend = value.get("backend").and_then(|b| b.as_object()).map(|b| {
        b.iter()
            .filter_map(|(k, v)| v.as_str().map(|v| format!("{k}: {v}")))
            .collect::<Vec<_>>()
            .join(" · ")
    });
    SttServerInfo {
        models,
        backend: backend.filter(|b| !b.is_empty()),
    }
}

/// Custom-word correction for the speaker text: each paragraph's words are
/// corrected, its speaker label is kept as it is.
pub fn correct_diarized(diarized: &str, correct: impl Fn(&str) -> String) -> String {
    diarized
        .split("\n\n")
        .map(|paragraph| match split_label(paragraph) {
            Some((label, said)) => format!("{label}: {}", correct(said)),
            None => correct(paragraph),
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse_response_t(value: &serde_json::Value) -> SttServerResult {
        parse_response(value, true)
    }

    #[test]
    fn one_labelled_voice_is_plain_text_without_its_label() {
        let r = parse_response(&json!({"text": "A: hello\n\nA: again"}), true);
        assert_eq!(
            r,
            SttServerResult {
                text: "hello again".into(),
                diarized: None
            }
        );
        let r = parse_response(&json!({"text": "A: hello"}), true);
        assert_eq!(
            r,
            SttServerResult {
                text: "hello".into(),
                diarized: None
            }
        );
    }

    #[test]
    fn speaker_text_belongs_to_its_take() {
        reset_session_diarized();
        set_session_diarized(7, Some("A: x\n\nB: y".into()));
        assert_eq!(take_session_diarized(8), None);
        assert_eq!(take_session_diarized(7).as_deref(), Some("A: x\n\nB: y"));
        assert_eq!(take_session_diarized(7), None);
    }

    #[test]
    fn ordinary_colon_paragraphs_are_not_speakers() {
        let v = json!({"text": "Note: keep the original\n\nAction: send the invoice"});
        let r = parse_response(&v, true);
        assert_eq!(r.diarized, None);
        assert_eq!(
            r.text,
            "Note: keep the original\n\nAction: send the invoice"
        );
        // Plain-text mode never strips labels, even real-looking ones.
        let r = parse_response(&json!({"text": "A: one\n\nB: two"}), false);
        assert_eq!(
            r,
            SttServerResult {
                text: "A: one\n\nB: two".into(),
                diarized: None
            }
        );
        // Numbered labels count.
        let r = parse_response(&json!({"text": "Speaker 1: hi\n\nSPEAKER_02: yo"}), true);
        assert_eq!(r.text, "hi yo");
        assert!(speaker_like("B2") && !speaker_like("Note") && !speaker_like("ABCD"));
    }

    #[test]
    fn openai_segments_become_speaker_paragraphs() {
        let r = parse_response_t(&json!({
            "text": "zobaczyć to wprost Tak jest, dokładnie. dalej",
            "segments": [
                {"speaker": "A", "text": "zobaczyć to wprost"},
                {"speaker": "B", "text": "Tak jest,"},
                {"speaker": "B", "text": "dokładnie."},
                {"speaker": "A", "text": " dalej "}
            ]
        }));
        assert_eq!(r.text, "zobaczyć to wprost Tak jest, dokładnie. dalej");
        assert_eq!(
            r.diarized.as_deref(),
            Some("A: zobaczyć to wprost\n\nB: Tak jest, dokładnie.\n\nA: dalej")
        );
    }

    #[test]
    fn one_voice_has_no_speaker_text() {
        let r = parse_response_t(&json!({
            "text": "hello there",
            "segments": [{"speaker": "A", "text": "hello"}, {"speaker": "A", "text": "there"}]
        }));
        assert_eq!(
            r,
            SttServerResult {
                text: "hello there".into(),
                diarized: None
            }
        );
        let r = parse_response_t(&json!({"text": "just text"}));
        assert_eq!(r.diarized, None);
        assert_eq!(r.text, "just text");
    }

    #[test]
    fn readable_style_keeps_paragraphs_and_strips_labels_for_text() {
        let r = parse_response_t(
            &json!({"text": "A: first part\n\nB: second: with colon\n\nA: third"}),
        );
        assert_eq!(r.text, "first part second: with colon third");
        assert_eq!(
            r.diarized.as_deref(),
            Some("A: first part\n\nB: second: with colon\n\nA: third")
        );
        // A colon in ordinary prose is not a speaker turn.
        let r = parse_response_t(&json!({"text": "Note: one paragraph only"}));
        assert_eq!(r.diarized, None);
    }

    #[test]
    fn models_and_backend_are_read() {
        let info = parse_models(&json!({
            "object": "list",
            "backend": {"asr": "nvidia/parakeet", "diarization": "nvidia/sortformer"},
            "data": [
                {"id": "transcribe", "description": "clean text"},
                {"id": "transcribe-diarize-openai"}
            ]
        }));
        assert_eq!(info.models.len(), 2);
        assert_eq!(info.models[1].description, "");
        assert_eq!(
            info.backend.as_deref(),
            Some("asr: nvidia/parakeet · diarization: nvidia/sortformer")
        );
    }

    #[test]
    fn diarized_correction_keeps_labels() {
        let out = correct_diarized("A: handy tool\n\nB: ok", |s| s.replace("handy", "Handy"));
        assert_eq!(out, "A: Handy tool\n\nB: ok");
    }

    #[test]
    fn multipart_carries_model_language_and_prompt() {
        let body = multipart_body(
            b"OGG",
            "transcribe",
            Some("pl"),
            Some(" Handy, Parakeet "),
            "B",
        );
        let s = String::from_utf8_lossy(&body);
        assert!(s.contains("filename=\"audio.ogg\""));
        assert!(s.contains("name=\"model\"\r\n\r\ntranscribe\r\n"));
        assert!(s.contains("name=\"language\"\r\n\r\npl\r\n"));
        assert!(s.contains("name=\"prompt\"\r\n\r\nHandy, Parakeet\r\n"));
        let s =
            String::from_utf8_lossy(&multipart_body(b"OGG", "", None, Some("  "), "B")).to_string();
        assert!(
            !s.contains("name=\"model\"")
                && !s.contains("name=\"prompt\"")
                && !s.contains("name=\"language\"")
        );
    }
}
