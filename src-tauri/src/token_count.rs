//! Token counting via configured LLM providers.
//!
//! Each provider slot has a `kind` that selects the counting strategy:
//! - `anthropic`: POST {base}/v1/messages/count_tokens (free, dedicated)
//! - `gemini`: POST {base}/v1beta/models/{model}:countTokens (free, dedicated)
//! - `openai_local`: bundled tiktoken (offline; the "model" is the tokenizer)
//! - `openai_compatible`: POST {base}/completions (raw prompt, max_tokens=1),
//!   read usage.prompt_tokens, calibrated against a 1-token probe to remove
//!   the server's fixed template/BOS overhead (works for FLM, LM Studio and
//!   other local OpenAI-compatible servers; free when the server is local)

use log::{info, warn};
use serde::Serialize;
use serde_json::{Value, json};
use specta::Type;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

use crate::settings::{self, LlmProvider};

pub const TOKEN_COUNT_PROGRESS_EVENT: &str = "token-count-progress";

const LOCAL_TIMEOUT_SECS: u64 = 30;
const CLOUD_TIMEOUT_SECS: u64 = 15;

/// Cancellation handle for "count with all" sweeps: bumping the generation
/// makes an in-flight sweep stop before its next provider.
pub struct TokenCountState {
    generation: AtomicU64,
}

impl TokenCountState {
    pub fn new() -> Self {
        Self {
            generation: AtomicU64::new(0),
        }
    }
}

#[derive(Serialize, Clone, Type)]
pub struct ProviderCountResult {
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    pub ok: bool,
    pub tokens: Option<u32>,
    pub error: Option<String>,
    pub elapsed_ms: u32,
}

fn is_local_url(url: &str) -> bool {
    url.contains("127.0.0.1") || url.contains("localhost") || url.contains("[::1]")
}

fn provider_timeout(provider: &LlmProvider) -> Duration {
    if is_local_url(&provider.base_url) {
        Duration::from_secs(LOCAL_TIMEOUT_SECS)
    } else {
        Duration::from_secs(CLOUD_TIMEOUT_SECS)
    }
}

/// Every client built here gets BOTH a connect deadline (shared with
/// `llm_client`'s post-processing client, T-202) and this call's total-request
/// deadline — a hung TLS handshake or a stalled mid-response server must never
/// block a token-count sweep forever, and each provider call gets its own
/// independent timeout so one hung provider can't stall the others sharing a
/// sweep.
fn http_client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(crate::llm_client::CONNECT_TIMEOUT)
        .timeout(timeout)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))
}

fn get_provider(app: &AppHandle, provider_id: &str) -> Result<LlmProvider, String> {
    settings::get_settings(app)
        .llm_providers
        .into_iter()
        .find(|p| p.id == provider_id)
        .ok_or_else(|| format!("Unknown LLM provider: {}", provider_id))
}

fn trim_base(base_url: &str) -> String {
    base_url.trim_end_matches('/').to_string()
}

async fn count_anthropic(provider: &LlmProvider, text: &str) -> Result<u32, String> {
    let timeout = provider_timeout(provider);
    let client = http_client(timeout)?;
    let url = format!("{}/v1/messages/count_tokens", trim_base(&provider.base_url));
    let body = json!({
        "model": provider.model,
        "messages": [{"role": "user", "content": text}],
    });
    let response = client
        .post(&url)
        .header("x-api-key", provider.api_key.trim())
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    let status = response.status();
    // Bounded read (T-202 finding 4): a hostile/misconfigured endpoint can no
    // longer exhaust memory via an unbounded `.json()` on either body.
    let body_bytes = crate::llm_client::read_body_capped(
        response,
        crate::llm_client::MAX_RESPONSE_BYTES,
        timeout,
    )
    .await?;
    let value: Value =
        serde_json::from_slice(&body_bytes).map_err(|e| format!("Invalid response: {}", e))?;
    if !status.is_success() {
        return Err(api_error(&value, status.as_u16()));
    }
    value["input_tokens"]
        .as_u64()
        .map(|n| n as u32)
        .ok_or_else(|| "Response missing input_tokens".to_string())
}

async fn count_gemini(provider: &LlmProvider, text: &str) -> Result<u32, String> {
    let timeout = provider_timeout(provider);
    let client = http_client(timeout)?;
    let url = format!(
        "{}/v1beta/models/{}:countTokens?key={}",
        trim_base(&provider.base_url),
        provider.model,
        provider.api_key.trim()
    );
    let body = json!({
        "contents": [{"parts": [{"text": text}]}],
    });
    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    let status = response.status();
    let body_bytes = crate::llm_client::read_body_capped(
        response,
        crate::llm_client::MAX_RESPONSE_BYTES,
        timeout,
    )
    .await?;
    let value: Value =
        serde_json::from_slice(&body_bytes).map_err(|e| format!("Invalid response: {}", e))?;
    if !status.is_success() {
        return Err(api_error(&value, status.as_u16()));
    }
    value["totalTokens"]
        .as_u64()
        .map(|n| n as u32)
        .ok_or_else(|| "Response missing totalTokens".to_string())
}

/// A text that is exactly one token in every common tokenizer, used to
/// measure the server's fixed prompt overhead (BOS, template wrap).
const CALIBRATION_TEXT: &str = "a";

/// OpenAI-compatible servers (FLM, LM Studio, ...) have no counting endpoint.
/// `usage.prompt_tokens` from a 1-token completion includes the server's
/// fixed wrapping (BOS, chat/system template — measured: +17 on LM Studio
/// chat, +13 on FLM even for raw completions). To return the token count of
/// the text itself, probe twice — the text and a known 1-token calibration
/// string — and subtract: tokens = probe(text) - probe("a") + 1.
///
/// The raw `/completions` endpoint is preferred (no chat template; FLM's
/// `/chat/completions` is also outright broken — HTTP 500); servers without
/// it fall back to `/chat/completions` with the same calibration.
async fn count_openai_compatible(provider: &LlmProvider, text: &str) -> Result<u32, String> {
    let client = http_client(provider_timeout(provider))?;

    match probe_usage(&client, provider, text, false).await {
        Ok(text_tokens) => {
            let baseline = probe_usage(&client, provider, CALIBRATION_TEXT, false).await?;
            Ok(text_tokens.saturating_sub(baseline) + 1)
        }
        Err(completions_err) => {
            // Fall back to the chat endpoint with the same calibration
            let text_tokens =
                probe_usage(&client, provider, text, true)
                    .await
                    .map_err(|chat_err| {
                        format!("completions: {}; chat: {}", completions_err, chat_err)
                    })?;
            let baseline = probe_usage(&client, provider, CALIBRATION_TEXT, true).await?;
            Ok(text_tokens.saturating_sub(baseline) + 1)
        }
    }
}

/// POST a 1-token generation and return `usage.prompt_tokens`.
async fn probe_usage(
    client: &reqwest::Client,
    provider: &LlmProvider,
    text: &str,
    use_chat: bool,
) -> Result<u32, String> {
    let base = trim_base(&provider.base_url);
    let (url, body) = if use_chat {
        (
            format!("{}/chat/completions", base),
            json!({
                "model": provider.model,
                "messages": [{"role": "user", "content": text}],
                "max_tokens": 1,
                "stream": false,
            }),
        )
    } else {
        (
            format!("{}/completions", base),
            json!({
                "model": provider.model,
                "prompt": text,
                "max_tokens": 1,
                "stream": false,
            }),
        )
    };

    let mut request = client.post(&url).json(&body);
    if !provider.api_key.trim().is_empty() {
        request = request.bearer_auth(provider.api_key.trim());
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    let status = response.status();
    let body_bytes = crate::llm_client::read_body_capped(
        response,
        crate::llm_client::MAX_RESPONSE_BYTES,
        provider_timeout(provider),
    )
    .await?;
    let value: Value =
        serde_json::from_slice(&body_bytes).map_err(|e| format!("Invalid response: {}", e))?;
    if !status.is_success() {
        return Err(api_error(&value, status.as_u16()));
    }
    if let Some(error) = value.get("error") {
        // Some servers (FLM) return 200 with an error object in the body
        if !error.is_null() {
            return Err(api_error(&value, status.as_u16()));
        }
    }
    value["usage"]["prompt_tokens"]
        .as_u64()
        .map(|n| n as u32)
        .ok_or_else(|| "Response missing usage.prompt_tokens".to_string())
}

fn count_openai_local(provider: &LlmProvider, text: &str) -> Result<u32, String> {
    let bpe = match provider.model.as_str() {
        "cl100k_base" => tiktoken_rs::cl100k_base(),
        // o200k_base is the default and covers GPT-4o/o-series models
        _ => tiktoken_rs::o200k_base(),
    }
    .map_err(|e| format!("Failed to load tokenizer: {}", e))?;
    Ok(bpe.encode_with_special_tokens(text).len() as u32)
}

fn api_error(value: &Value, status: u16) -> String {
    let message = value["error"]["message"]
        .as_str()
        .or_else(|| value["error"].as_str())
        .unwrap_or("unknown error");
    format!("HTTP {}: {}", status, message)
}

async fn count_with_provider(provider: &LlmProvider, text: &str) -> ProviderCountResult {
    let started = Instant::now();
    let counted = match provider.kind.as_str() {
        "anthropic" => count_anthropic(provider, text).await,
        "gemini" => count_gemini(provider, text).await,
        "openai_local" => count_openai_local(provider, text),
        // OpenRouter is OpenAI-compatible; the 1-token probe is effectively free.
        "openai_compatible" | "openrouter" => count_openai_compatible(provider, text).await,
        other => Err(format!("Token counting not supported for kind: {}", other)),
    };
    let elapsed_ms = started.elapsed().as_millis() as u32;
    match counted {
        Ok(tokens) => ProviderCountResult {
            provider_id: provider.id.clone(),
            provider_name: provider.name.clone(),
            model: provider.model.clone(),
            ok: true,
            tokens: Some(tokens),
            error: None,
            elapsed_ms,
        },
        Err(error) => {
            warn!(
                "Token count via provider '{}' failed: {}",
                provider.id, error
            );
            ProviderCountResult {
                provider_id: provider.id.clone(),
                provider_name: provider.name.clone(),
                model: provider.model.clone(),
                ok: false,
                tokens: None,
                error: Some(error),
                elapsed_ms,
            }
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn count_tokens_via_provider(
    app: AppHandle,
    provider_id: String,
    text: String,
) -> Result<ProviderCountResult, String> {
    let provider = get_provider(&app, &provider_id)?;
    Ok(count_with_provider(&provider, &text).await)
}

/// The built-in tokenizers, always included in a "count with all" sweep
/// alongside the configured providers: (tokenizer id, display label).
const BUILTIN_TOKENIZERS: [(&str, &str); 3] = [
    ("cl100k_base", "cl100k (GPT-4 / GPT-3.5)"),
    ("o200k_base", "o200k (GPT-4o / o1)"),
    ("estimate", "Estimate (~1.3x words)"),
];

/// Counts with the built-in tokenizers (always first, serialized — they are
/// instant) and then every enabled provider. Failures are silent: each
/// provider's error is recorded in its result and the sweep continues.
/// Results are also emitted one-by-one as `token-count-progress` events.
///
/// `parallel` selects the provider strategy:
/// - `false` (serialized): one provider at a time. Right when several slots
///   share one local service that must load/unload each model — parallel
///   requests would just thrash the loader.
/// - `true`: all providers queried at once; total time = slowest provider.
///   Right for independent services and cloud APIs.
#[tauri::command]
#[specta::specta]
pub async fn count_tokens_all_providers(
    app: AppHandle,
    text: String,
    parallel: bool,
) -> Result<Vec<ProviderCountResult>, String> {
    let state = app.state::<TokenCountState>();
    let generation = state.generation.fetch_add(1, Ordering::SeqCst) + 1;

    let providers: Vec<LlmProvider> = settings::get_settings(&app)
        .llm_providers
        .into_iter()
        .filter(|p| p.enabled)
        .collect();

    let mut results = Vec::with_capacity(BUILTIN_TOKENIZERS.len() + providers.len());

    for (tokenizer, label) in BUILTIN_TOKENIZERS {
        let started = Instant::now();
        let counted = crate::commands::count_tokens(text.clone(), tokenizer.to_string());
        let elapsed_ms = started.elapsed().as_millis() as u32;
        let result = match counted {
            Ok(tokens) => ProviderCountResult {
                provider_id: format!("builtin:{}", tokenizer),
                provider_name: label.to_string(),
                model: tokenizer.to_string(),
                ok: true,
                tokens: Some(tokens as u32),
                error: None,
                elapsed_ms,
            },
            Err(error) => ProviderCountResult {
                provider_id: format!("builtin:{}", tokenizer),
                provider_name: label.to_string(),
                model: tokenizer.to_string(),
                ok: false,
                tokens: None,
                error: Some(error),
                elapsed_ms,
            },
        };
        let _ = app.emit(TOKEN_COUNT_PROGRESS_EVENT, result.clone());
        results.push(result);
    }

    if parallel {
        // All providers at once; each result is emitted the moment its
        // provider finishes, so rows appear in completion order.
        // Cancellation suppresses emission of still-in-flight results.
        let provider_futures = providers.into_iter().map(|provider| {
            let app = app.clone();
            let text = text.clone();
            async move {
                let result = count_with_provider(&provider, &text).await;
                let state = app.state::<TokenCountState>();
                if state.generation.load(Ordering::SeqCst) == generation {
                    let _ = app.emit(TOKEN_COUNT_PROGRESS_EVENT, result.clone());
                    Some(result)
                } else {
                    info!("Token count sweep cancelled; dropping result");
                    None
                }
            }
        });
        let provider_results = futures_util::future::join_all(provider_futures).await;
        results.extend(provider_results.into_iter().flatten());
    } else {
        // One provider at a time, in slot order.
        for provider in &providers {
            if state.generation.load(Ordering::SeqCst) != generation {
                info!("Token count sweep cancelled");
                break;
            }
            let result = count_with_provider(provider, &text).await;
            let _ = app.emit(TOKEN_COUNT_PROGRESS_EVENT, result.clone());
            results.push(result);
        }
    }

    Ok(results)
}

#[tauri::command]
#[specta::specta]
pub fn cancel_token_count_sweep(app: AppHandle) -> Result<(), String> {
    let state = app.state::<TokenCountState>();
    state.generation.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

/// Lists the models a provider offers, queried live from its API.
#[tauri::command]
#[specta::specta]
pub async fn list_provider_models(
    app: AppHandle,
    provider_id: String,
) -> Result<Vec<String>, String> {
    let provider = get_provider(&app, &provider_id)?;
    let base = trim_base(&provider.base_url);

    match provider.kind.as_str() {
        "openai_local" => Ok(vec!["o200k_base".to_string(), "cl100k_base".to_string()]),
        "anthropic" => {
            let timeout = provider_timeout(&provider);
            let client = http_client(timeout)?;
            let response = client
                .get(format!("{}/v1/models", base))
                .header("x-api-key", provider.api_key.trim())
                .header("anthropic-version", "2023-06-01")
                .send()
                .await
                .map_err(|e| format!("Request failed: {}", e))?;
            let body_bytes = crate::llm_client::read_body_capped(
                response,
                crate::llm_client::MAX_RESPONSE_BYTES,
                timeout,
            )
            .await?;
            let value: Value = serde_json::from_slice(&body_bytes)
                .map_err(|e| format!("Invalid response: {}", e))?;
            extract_ids(&value["data"], "id")
        }
        "gemini" => {
            let timeout = provider_timeout(&provider);
            let client = http_client(timeout)?;
            let response = client
                .get(format!(
                    "{}/v1beta/models?key={}",
                    base,
                    provider.api_key.trim()
                ))
                .send()
                .await
                .map_err(|e| format!("Request failed: {}", e))?;
            let body_bytes = crate::llm_client::read_body_capped(
                response,
                crate::llm_client::MAX_RESPONSE_BYTES,
                timeout,
            )
            .await?;
            let value: Value = serde_json::from_slice(&body_bytes)
                .map_err(|e| format!("Invalid response: {}", e))?;
            let models = extract_ids(&value["models"], "name")?;
            Ok(models
                .into_iter()
                .map(|name| name.trim_start_matches("models/").to_string())
                .collect())
        }
        _ => {
            let timeout = provider_timeout(&provider);
            let client = http_client(timeout)?;
            let mut request = client.get(format!("{}/models", base));
            if !provider.api_key.trim().is_empty() {
                request = request.bearer_auth(provider.api_key.trim());
            }
            let response = request
                .send()
                .await
                .map_err(|e| format!("Request failed: {}", e))?;
            let body_bytes = crate::llm_client::read_body_capped(
                response,
                crate::llm_client::MAX_RESPONSE_BYTES,
                timeout,
            )
            .await?;
            let value: Value = serde_json::from_slice(&body_bytes)
                .map_err(|e| format!("Invalid response: {}", e))?;
            extract_ids(&value["data"], "id")
        }
    }
}

/// Known OpenRouter speech-to-text model ids, used as an offline fallback when
/// the live models query is unavailable (verified July 2026).
const OPENROUTER_STT_FALLBACK: &[&str] = &[
    "openai/whisper-large-v3",
    "openai/whisper-large-v3-turbo",
    "openai/whisper-1",
    "openai/gpt-4o-transcribe",
    "openai/gpt-4o-mini-transcribe",
    "mistralai/voxtral-mini-transcribe",
    "microsoft/mai-transcribe-1.5",
    "nvidia/parakeet-tdt-0.6b-v3",
    "qwen/qwen3-asr-flash-2026-02-10",
    "google/chirp-3",
];

/// Lists OpenRouter speech-to-text models. The default `/models` list EXCLUDES
/// STT models, so this queries `?output_modalities=transcription` (public — no
/// key required). Falls back to a curated list on any error so the picker is
/// never empty. Used specifically by the OpenRouter-transcription model picker.
#[tauri::command]
#[specta::specta]
pub async fn list_openrouter_transcription_models(
    app: AppHandle,
    provider_id: String,
) -> Result<Vec<String>, String> {
    let fallback = || {
        OPENROUTER_STT_FALLBACK
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    };
    let (base, key) = match get_provider(&app, &provider_id) {
        Ok(p) => (trim_base(&p.base_url), p.api_key.trim().to_string()),
        Err(_) => ("https://openrouter.ai/api/v1".to_string(), String::new()),
    };
    let timeout = std::time::Duration::from_secs(15);
    let client = match http_client(timeout) {
        Ok(c) => c,
        Err(_) => return Ok(fallback()),
    };
    let mut request = client.get(format!("{}/models?output_modalities=transcription", base));
    if !key.is_empty() {
        request = request.bearer_auth(&key);
    }
    let value: Value = match request.send().await {
        Ok(r) => match crate::llm_client::read_body_capped(
            r,
            crate::llm_client::MAX_RESPONSE_BYTES,
            timeout,
        )
        .await
        {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(v) => v,
                Err(_) => return Ok(fallback()),
            },
            Err(_) => return Ok(fallback()),
        },
        Err(_) => return Ok(fallback()),
    };
    match extract_ids(&value["data"], "id") {
        Ok(ids) if !ids.is_empty() => Ok(ids),
        _ => Ok(fallback()),
    }
}

/// Reads a text file for the Token Count page. Done backend-side so the
/// frontend fs scope (limited to $APPDATA) doesn't need widening.
#[tauri::command]
#[specta::specta]
pub fn read_text_file_for_count(path: String) -> Result<String, String> {
    const MAX_BYTES: u64 = 10 * 1024 * 1024;
    let metadata = std::fs::metadata(&path).map_err(|e| format!("Failed to read file: {}", e))?;
    if metadata.len() > MAX_BYTES {
        return Err("File is too large (max 10 MB)".to_string());
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("Failed to read file: {}", e))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn extract_ids(list: &Value, key: &str) -> Result<Vec<String>, String> {
    list.as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item[key].as_str().map(String::from))
                .collect()
        })
        .ok_or_else(|| "Response missing model list".to_string())
}
