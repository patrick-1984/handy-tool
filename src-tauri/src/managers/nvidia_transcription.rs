//! NVIDIA hosted Riva Recognize over negotiated HTTP/2 (never HTTP/1 or prior knowledge).
use super::nvidia_proto;
use anyhow::{Context, Result, bail};
use http_body_util::BodyExt;
use std::time::Duration;

pub const DEFAULT_FUNCTION_ID: &str = "b702f636-f60c-4a3d-a6f4-f3568c13bd7d";
const ENDPOINT: &str =
    "https://grpc.nvcf.nvidia.com/nvidia.riva.asr.RivaSpeechRecognition/Recognize";
// Hosted Whisper is a short-form model. Keep unary requests at most 25 s
// (800 kB PCM). Split only after stop, in original order.
const CHUNK_SAMPLES: usize = 25 * 16_000;
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// Obvious regional defaults; existing BCP-47 tags pass through, other codes
/// stay as supplied. A function may support fewer languages than Whisper.
pub fn language_code(language: &str) -> &str {
    match language.trim() {
        "" | "auto" => "multi",
        "en" => "en-US",
        "pl" => "pl-PL",
        "de" => "de-DE",
        "fr" => "fr-FR",
        "es" => "es-ES",
        "it" => "it-IT",
        "pt" => "pt-BR",
        "nl" => "nl-NL",
        "ja" => "ja-JP",
        "ko" => "ko-KR",
        "zh" | "zh-Hans" => "zh-CN",
        "zh-Hant" => "zh-TW",
        "ru" => "ru-RU",
        "uk" => "uk-UA",
        "cs" => "cs-CZ",
        "sk" => "sk-SK",
        "sv" => "sv-SE",
        "da" => "da-DK",
        "fi" => "fi-FI",
        "nb" | "no" => "nb-NO",
        "tr" => "tr-TR",
        "hi" => "hi-IN",
        "el" => "el-GR",
        "ro" => "ro-RO",
        "hu" => "hu-HU",
        "bg" => "bg-BG",
        "hr" => "hr-HR",
        "vi" => "vi-VN",
        "th" => "th-TH",
        "id" => "id-ID",
        "ms" => "ms-MY",
        "ta" => "ta-IN",
        "he" => "he-IL",
        other => other,
    }
}

pub fn validate_config(api_key: &str, function_id: &str) -> Result<()> {
    if api_key.trim().is_empty() {
        bail!(
            "NVIDIA API key is not configured. Set it in Advanced settings > Transcription providers > NVIDIA."
        );
    }
    let id = function_id.trim();
    if id.len() != 36
        || !id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        bail!(
            "NVIDIA Function ID must be a UUID. Set it in Advanced settings > Transcription providers > NVIDIA."
        );
    }
    Ok(())
}

/// Entry point used by dictation and Files. The caller supplies 16 kHz mono f32.
/// Keep the async runtime on this synchronous worker (as with other STT engines).
pub fn transcribe(
    api_key: &str,
    function_id: &str,
    audio: &[f32],
    language: &str,
) -> Result<String> {
    validate_config(api_key, function_id)?;
    if audio.is_empty() {
        return Ok(String::new());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(transcribe_async(
        api_key.trim(),
        function_id.trim(),
        audio,
        language,
    ))
}

async fn transcribe_async(
    api_key: &str,
    function_id: &str,
    audio: &[f32],
    language: &str,
) -> Result<String> {
    let client = crate::net::client_builder()
        .use_native_tls() // native-tls-alpn advertises h2 over TLS, including Windows SChannel
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(120))
        .build()
        .context("Could not create NVIDIA HTTP/2 client")?;
    let count = audio.len().div_ceil(CHUNK_SAMPLES);
    let mut texts = Vec::new();
    for (index, pcm) in pcm_chunks(audio).enumerate() {
        let text = recognize(&client, api_key, function_id, &pcm, language_code(language))
            .await
            .map_err(|error| anyhow::anyhow!("NVIDIA chunk {}/{count}: {error:#}", index + 1))?;
        if !text.is_empty() {
            texts.push(text);
        }
    }
    Ok(texts.join(" "))
}

/// Riva takes raw little-endian PCM, with no WAV header. Bound each upload
/// independently so a long take never exceeds the short-audio unary limit.
fn pcm_chunks(audio: &[f32]) -> impl Iterator<Item = Vec<u8>> + '_ {
    audio.chunks(CHUNK_SAMPLES).map(|chunk| {
        chunk
            .iter()
            .flat_map(|sample| ((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
            .collect()
    })
}

fn transport_error(error: reqwest::Error) -> anyhow::Error {
    if error.is_timeout() {
        anyhow::anyhow!("NVIDIA request timed out; try again or check your connection")
    } else {
        anyhow::anyhow!("NVIDIA connection failed: {error}")
    }
}

async fn recognize(
    client: &reqwest::Client,
    api_key: &str,
    function_id: &str,
    pcm: &[u8],
    language: &str,
) -> Result<String> {
    let mut authorization = reqwest::header::HeaderValue::from_str(&format!("Bearer {api_key}"))
        .map_err(|_| anyhow::anyhow!("NVIDIA API key contains invalid header characters"))?;
    authorization.set_sensitive(true);
    let response = client
        .post(ENDPOINT)
        .header(reqwest::header::AUTHORIZATION, authorization)
        .header("function-id", function_id)
        .header(reqwest::header::CONTENT_TYPE, "application/grpc")
        .header("te", "trailers")
        .body(nvidia_proto::recognize_request(pcm, language))
        .send()
        .await
        .map_err(transport_error)?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        bail!("NVIDIA API key refused (HTTP {status}); check your key and function access");
    }
    if !status.is_success() {
        bail!("NVIDIA request failed (HTTP {status})");
    }
    if response.version() != reqwest::Version::HTTP_2 {
        bail!("NVIDIA requires HTTP/2; TLS did not negotiate h2");
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !content_type.starts_with("application/grpc") {
        bail!("NVIDIA returned an unexpected content type");
    }
    let mut metadata = response.headers().clone();
    // A trailers-only response may already carry the gRPC error in headers.
    if metadata.contains_key("grpc-status") {
        nvidia_proto::grpc_status(
            metadata.get("grpc-status").and_then(|v| v.to_str().ok()),
            metadata.get("grpc-message").and_then(|v| v.to_str().ok()),
        )?;
    }
    // bytes()/bytes_stream() discard trailers. Preserve them through BodyExt.
    let mut body = reqwest::Body::from(response);
    let mut bytes = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(transport_error)?;
        if let Some(data) = frame.data_ref() {
            if bytes.len() + data.len() > MAX_RESPONSE_BYTES {
                bail!("NVIDIA response exceeds 8 MiB");
            }
            bytes.extend_from_slice(data);
        }
        if let Some(trailers) = frame.trailers_ref() {
            metadata.extend(trailers.clone());
        }
    }
    nvidia_proto::grpc_status(
        metadata.get("grpc-status").and_then(|v| v.to_str().ok()),
        metadata.get("grpc-message").and_then(|v| v.to_str().ok()),
    )?;
    nvidia_proto::recognize_response(&bytes).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn language_defaults_and_tags() {
        for (input, expected) in [
            ("auto", "multi"),
            ("en", "en-US"),
            ("pl", "pl-PL"),
            ("fr-CA", "fr-CA"),
            ("zh-Hans", "zh-CN"),
            ("zh-Hant", "zh-TW"),
            ("cy", "cy"),
        ] {
            assert_eq!(language_code(input), expected);
        }
    }
    #[test]
    fn config_requires_key_and_function_uuid() {
        assert!(validate_config("", DEFAULT_FUNCTION_ID).is_err());
        assert!(validate_config("test-key", " ").is_err());
        assert!(validate_config("test-key", "../../wrong").is_err());
        validate_config(" test-key ", DEFAULT_FUNCTION_ID).unwrap();
    }
    #[test]
    fn pcm_uploads_are_bounded_headerless_and_keep_boundary_samples() {
        let mut audio = vec![0f32; CHUNK_SAMPLES * 2 + 1];
        audio[0] = -2.0;
        audio[CHUNK_SAMPLES - 1] = 2.0;
        audio[CHUNK_SAMPLES] = -1.0;
        audio[CHUNK_SAMPLES * 2] = 0.5;
        let uploads: Vec<_> = pcm_chunks(&audio).collect();
        assert_eq!(uploads.len(), 3);
        assert_eq!(uploads[0].len(), 800_000);
        assert_eq!(uploads[1].len(), 800_000);
        assert_eq!(&uploads[0][..2], &[1, 128]);
        assert_eq!(&uploads[0][799_998..], &[255, 127]);
        assert_eq!(&uploads[1][..2], &[1, 128]);
        assert_eq!(uploads[2], [255, 63]);
        assert_eq!(uploads.iter().map(Vec::len).sum::<usize>(), audio.len() * 2);
    }
    /// Explicit network opt-in only; no secrets or audio committed to the repo.
    #[test]
    #[ignore = "uses the owner's NVIDIA key and /tmp/ht-nvidia-sample.wav"]
    fn live_nvidia_sample() {
        let key_path = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap()
            .join(".config/nvidia-api-key");
        let key = std::fs::read_to_string(key_path).expect("NVIDIA key file");
        let mut wav = hound::WavReader::open("/tmp/ht-nvidia-sample.wav").expect("sample WAV");
        let spec = wav.spec();
        assert_eq!(
            (spec.sample_rate, spec.channels, spec.bits_per_sample),
            (16000, 1, 16)
        );
        let audio: Vec<_> = wav
            .samples::<i16>()
            .map(|s| s.unwrap() as f32 / 32768.0)
            .collect();
        let text = transcribe(&key, DEFAULT_FUNCTION_ID, &audio, "auto").unwrap();
        assert!(!text.is_empty());
        println!("NVIDIA live transcript: {text}");
    }
}
