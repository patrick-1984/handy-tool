use crate::settings::LlmProvider;
use log::{debug, info};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct JsonSchema {
    name: String,
    strict: bool,
    schema: Value,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
    json_schema: JsonSchema,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    think: Option<bool>,
    /// OpenAI-standard top-level reasoning effort ("none", "low", "medium",
    /// "high"). Omitted when unset so providers that reject unknown fields
    /// never see it.
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
    /// OpenRouter-style nested reasoning object — kept distinct from the
    /// OpenAI top-level field so the two dialects are never conflated.
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: Option<String>,
}

/// Build headers for API requests based on provider type
fn build_headers(provider: &LlmProvider, api_key: &str) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();

    // Common headers
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(USER_AGENT, HeaderValue::from_static("Handy/1.0"));
    headers.insert("X-Title", HeaderValue::from_static("Handy"));

    // Provider-specific auth headers
    if !api_key.is_empty() {
        if provider.kind == "anthropic" {
            headers.insert(
                "x-api-key",
                HeaderValue::from_str(api_key)
                    .map_err(|e| format!("Invalid API key header value: {}", e))?,
            );
            headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
        } else {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", api_key))
                    .map_err(|e| format!("Invalid authorization header value: {}", e))?,
            );
        }
    }

    Ok(headers)
}

/// Time allowed to establish a connection to the provider. `pub(crate)` so
/// `token_count.rs`/`model_testing.rs` can reuse the same connect budget for
/// their own clients (T-202) instead of drifting from it independently.
pub(crate) const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Total-request deadline — generous enough for slow local models, but a
/// stalled provider can never hang post-processing forever.
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Upper bound on an LLM response body (success or error) read into memory.
/// A misbehaving/hostile endpoint that keeps streaming bytes past this is
/// rejected instead of being buffered without limit (T-202). `pub(crate)` so
/// `token_count.rs`/`model_testing.rs` share the exact same cap instead of
/// drifting from it independently (T-202 finding 4).
pub(crate) const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024; // 10 MB

/// Create an HTTP client with provider-specific headers
fn create_client(provider: &LlmProvider, api_key: &str) -> Result<reqwest::Client, String> {
    let headers = build_headers(provider, api_key)?;
    reqwest::Client::builder()
        .default_headers(headers)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))
}

/// Phase-labels a failed `.send()`/read so a timeout tells the user WHERE it
/// happened instead of a bare "operation timed out" (T-202). Pure/testable:
/// takes the two flags `reqwest::Error` already exposes (`is_timeout`,
/// `is_connect`) plus the raw message, rather than a live `reqwest::Error` —
/// unit tests don't need a real network round-trip to exercise the mapping.
///
/// `request_timeout` is the caller's own total-request budget (each of
/// `llm_client`/`token_count`/`model_testing` configures its own — 120s for
/// post-processing, 15/30s or 300s for token-count/model-testing depending on
/// local vs cloud) so the reported number always matches the client that was
/// actually used, not a hardcoded value borrowed from another module.
pub(crate) fn request_error_message(
    is_timeout: bool,
    is_connect_phase: bool,
    raw: &str,
    request_timeout: std::time::Duration,
) -> String {
    match (is_timeout, is_connect_phase) {
        (true, true) => format!(
            "Connection timed out after {}s — could not reach the provider",
            CONNECT_TIMEOUT.as_secs()
        ),
        (true, false) => format!(
            "Request timed out after {}s — provider did not finish responding in time",
            request_timeout.as_secs()
        ),
        (false, true) => format!("Connection failed: {}", raw),
        (false, false) => format!("HTTP request failed: {}", raw),
    }
}

/// Describe a `.send()` failure (or, via `read_body_capped`, a mid-body read
/// failure), labeling connect-phase vs total-request-phase timeouts — see
/// `request_error_message`. `pub(crate)` so `token_count.rs`/`model_testing.rs`
/// can label their own `.send()` errors the same way if needed.
pub(crate) fn describe_request_error(
    e: &reqwest::Error,
    request_timeout: std::time::Duration,
) -> String {
    request_error_message(
        e.is_timeout(),
        e.is_connect(),
        &e.to_string(),
        request_timeout,
    )
}

/// Accumulates response-body bytes up to `max_bytes`, independent of the
/// transport so the cap logic itself is unit-testable without a live
/// connection. Rejects the moment the running total would exceed the cap,
/// before the offending chunk is appended.
struct CappedBuf {
    buf: Vec<u8>,
    max_bytes: usize,
}

impl CappedBuf {
    fn new(max_bytes: usize) -> Self {
        Self {
            buf: Vec::new(),
            max_bytes,
        }
    }

    fn push(&mut self, chunk: &[u8]) -> Result<(), String> {
        if self.buf.len() + chunk.len() > self.max_bytes {
            return Err(format!(
                "Response body exceeded {} byte limit",
                self.max_bytes
            ));
        }
        self.buf.extend_from_slice(chunk);
        Ok(())
    }

    fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

/// Read a response body via `.chunk()` (bounded byte accumulator) instead of
/// `.text()`/`.json()`, so an oversized or endlessly-streaming body is caught
/// mid-stream rather than buffered without limit first (T-202). Shared by
/// `llm_client`, `token_count`, and `model_testing` (T-202 finding 4) so every
/// provider response — success or error, post-processing or token-count/
/// model-testing — is read through the same bounded reader; there is no
/// second, unbounded `.json()`/`.text()` path left anywhere in the app's LLM
/// traffic.
///
/// `request_timeout` is the caller's own configured total-request deadline,
/// used ONLY to phase-label a timeout that fires mid-body-read (a stalled
/// read here is a total-request-phase timeout, never connect-phase, since the
/// connection and headers already arrived) — see `describe_request_error`.
pub(crate) async fn read_body_capped(
    mut response: reqwest::Response,
    max_bytes: usize,
    request_timeout: std::time::Duration,
) -> Result<Vec<u8>, String> {
    let mut acc = CappedBuf::new(max_bytes);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| describe_request_error(&e, request_timeout))?
    {
        acc.push(&chunk)?;
    }
    Ok(acc.into_bytes())
}

/// Reasoning-dialect switches for one request, derived per provider kind.
#[derive(Debug, Default)]
struct ReasoningFields {
    think: Option<bool>,
    reasoning_effort: Option<String>,
    reasoning: Option<Value>,
}

/// Map the global "disable thinking" switch onto the reasoning dialect each
/// provider kind actually accepts. Capability-aware on purpose: strict
/// endpoints reject unknown fields, so a provider only ever receives its own
/// dialect and never another's.
///
/// - `openrouter`: nested `reasoning` object (OpenRouter's documented
///   representation, distinct from the OpenAI top-level field)
/// - `openai_compatible`/`openai_local`: the nonstandard `think: false`
///   (Ollama/Qwen dialect, preserved for back-compat) plus the OpenAI-standard
///   top-level `reasoning_effort`
/// - `gemini`: `reasoning_effort` only (Google's OpenAI-compat layer maps it
///   to a thinking budget; `think` is not accepted), model-aware — see the arm
/// - everything else (Anthropic, Apple Intelligence): nothing — those dialects
///   have no OpenAI-style reasoning switch
///
/// For `openai_compatible`/`openai_local`, "low" rather than "none": any model
/// behind those endpoints that supports `reasoning_effort` accepts "low",
/// while several reject "none" outright — the lowest universally supported
/// effort is the safe way to say "don't think" there.
fn reasoning_fields(provider: &LlmProvider, disable_thinking: bool) -> ReasoningFields {
    if !disable_thinking {
        return ReasoningFields::default();
    }
    match provider.kind.as_str() {
        "openrouter" => ReasoningFields {
            reasoning: Some(serde_json::json!({ "enabled": false })),
            ..Default::default()
        },
        "openai_compatible" | "openai_local" => ReasoningFields {
            think: Some(false),
            reasoning_effort: Some("low".to_string()),
            ..Default::default()
        },
        // Gemini's OpenAI-compat layer documents "none" as thinking DISABLED,
        // but only eligible models (2.5 Flash family) accept it — 2.5 Pro and
        // Gemini 3 cannot disable reasoning and REJECT "none", and the 2.0
        // family has no thinking to switch off. Send the strongest value each
        // model actually accepts instead of failing the whole request.
        "gemini" => {
            let model = provider.model.to_ascii_lowercase();
            let effort = if model.contains("2.5") && !model.contains("pro") {
                Some("none")
            } else if model.contains("2.5") || model.contains("gemini-3") {
                Some("low")
            } else {
                None // e.g. gemini-2.0-*: not a thinking model
            };
            ReasoningFields {
                reasoning_effort: effort.map(str::to_string),
                ..Default::default()
            }
        }
        _ => ReasoningFields::default(),
    }
}

/// Assemble the request body. Trusted instructions travel as the system
/// message; `user_content` is the untrusted (delimited) data. Factored out of
/// the send path so serialization is unit-testable without a network.
fn build_request_body(
    provider: &LlmProvider,
    model: &str,
    user_content: String,
    system_prompt: Option<String>,
    json_schema: Option<Value>,
    disable_thinking: bool,
    temperature: Option<f32>,
) -> ChatCompletionRequest {
    let mut messages = Vec::new();

    if let Some(system) = system_prompt {
        messages.push(ChatMessage {
            role: "system".to_string(),
            content: system,
        });
    }

    messages.push(ChatMessage {
        role: "user".to_string(),
        content: user_content,
    });

    let response_format = json_schema.map(|schema| ResponseFormat {
        format_type: "json_schema".to_string(),
        json_schema: JsonSchema {
            name: "transcription_output".to_string(),
            strict: true,
            schema,
        },
    });

    let ReasoningFields {
        think,
        reasoning_effort,
        reasoning,
    } = reasoning_fields(provider, disable_thinking);

    ChatCompletionRequest {
        model: model.to_string(),
        messages,
        response_format,
        think,
        reasoning_effort,
        reasoning,
        temperature,
    }
}

/// Send a chat completion request to an OpenAI-compatible API (no structured
/// output). Trusted instructions travel as the system message; `user_content`
/// carries the untrusted, delimited data.
/// Returns Ok(Some(content)) on success, Ok(None) if response has no content,
/// or Err on actual errors (HTTP, parsing, etc.)
pub async fn send_chat_completion(
    provider: &LlmProvider,
    api_key: String,
    model: &str,
    user_content: String,
    system_prompt: Option<String>,
    disable_thinking: bool,
    temperature: Option<f32>,
) -> Result<Option<String>, String> {
    send_chat_completion_with_schema(
        provider,
        api_key,
        model,
        user_content,
        system_prompt,
        None,
        disable_thinking,
        temperature,
    )
    .await
}

/// Send a chat completion request with structured output support
/// When json_schema is provided, uses structured outputs mode
/// system_prompt is used as the system message when provided
pub async fn send_chat_completion_with_schema(
    provider: &LlmProvider,
    api_key: String,
    model: &str,
    user_content: String,
    system_prompt: Option<String>,
    json_schema: Option<Value>,
    disable_thinking: bool,
    temperature: Option<f32>,
) -> Result<Option<String>, String> {
    let base_url = provider.base_url.trim_end_matches('/');
    let url = format!("{}/chat/completions", base_url);

    let has_system = system_prompt.is_some();
    let has_schema = json_schema.is_some();
    info!(
        "Post-process: POST {} (model: {}, provider: {}, system: {}, structured: {}, think: {})",
        url,
        model,
        provider.id,
        has_system,
        has_schema,
        if disable_thinking {
            "disabled"
        } else {
            "default"
        }
    );

    let client = create_client(provider, &api_key)?;

    if let Some(system) = system_prompt.as_deref() {
        // Content preview only at debug — release file logs are INFO and must
        // never contain prompt or transcript text
        info!("Post-process: system prompt ({} chars)", system.len());
        debug!(
            "Post-process: system prompt preview: {}",
            &system[..system.len().min(300)]
        );
    } else {
        info!("Post-process: no system message — instructions embedded in user message");
    }

    let request_body = build_request_body(
        provider,
        model,
        user_content,
        system_prompt,
        json_schema,
        disable_thinking,
        temperature,
    );

    // Log full request metadata
    info!(
        "Post-process: request body — model: {}, messages: {}, response_format: {}, think: {:?}, reasoning_effort: {:?}, reasoning: {}",
        request_body.model,
        request_body.messages.len(),
        if request_body.response_format.is_some() {
            "json_schema"
        } else {
            "none"
        },
        request_body.think,
        request_body.reasoning_effort,
        if request_body.reasoning.is_some() {
            "nested"
        } else {
            "none"
        },
    );
    for (i, msg) in request_body.messages.iter().enumerate() {
        info!(
            "Post-process:   message[{}] role={}, {} chars",
            i,
            msg.role,
            msg.content.len(),
        );
        // Content previews only at debug (privacy: transcript text must not
        // reach INFO release file logs); guard avoids the allocation entirely
        // when debug logging is off
        if log::log_enabled!(log::Level::Debug) {
            let preview = if msg.content.len() <= 600 {
                msg.content.clone()
            } else {
                format!(
                    "{}...[truncated]...{}",
                    &msg.content[..300],
                    &msg.content[msg.content.len() - 200..]
                )
            };
            debug!("Post-process:   message[{}] content:\n{}", i, preview);
        }
    }

    let request_start = std::time::Instant::now();
    info!("Post-process: sending request...");

    let response = client
        .post(&url)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| describe_request_error(&e, REQUEST_TIMEOUT))?;

    let status = response.status();
    let elapsed_ms = request_start.elapsed().as_millis();
    info!(
        "Post-process: response status {} in {}ms",
        status, elapsed_ms
    );

    if !status.is_success() {
        let error_bytes = read_body_capped(response, MAX_RESPONSE_BYTES, REQUEST_TIMEOUT)
            .await
            .unwrap_or_else(|e| e.into_bytes());
        let error_text = String::from_utf8_lossy(&error_bytes);
        debug!(
            "Post-process: error response body ({} chars): {}",
            error_text.chars().count(),
            error_text.chars().take(500).collect::<String>()
        );
        return Err(format!("API request failed with status {}", status));
    }

    let body_bytes = read_body_capped(response, MAX_RESPONSE_BYTES, REQUEST_TIMEOUT).await?;
    let completion: ChatCompletionResponse = serde_json::from_slice(&body_bytes)
        .map_err(|e| format!("Failed to parse API response: {}", e))?;

    let result = completion
        .choices
        .first()
        .and_then(|choice| choice.message.content.clone());

    info!(
        "Post-process: response content {} chars",
        result.as_ref().map(|s| s.len()).unwrap_or(0)
    );

    Ok(result)
}

// Model listing lives in `token_count::list_provider_models`, which is
// kind-aware (Anthropic /v1/models, Gemini /v1beta/models, OpenAI /models).

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(kind: &str) -> LlmProvider {
        LlmProvider {
            id: format!("test-{}", kind),
            kind: kind.to_string(),
            enabled: true,
            name: "Test".to_string(),
            base_url: "http://localhost:1234/v1".to_string(),
            allow_base_url_edit: true,
            api_key: String::new(),
            model: "test-model".to_string(),
            supports_structured_output: false,
            cost_input_per_million: 0.0,
            cost_output_per_million: 0.0,
            concurrency_group: String::new(),
            sequential: false,
            persist_price: false,
        }
    }

    /// Serialize a request body exactly as the send path would.
    fn request_json(kind: &str, json_schema: Option<Value>, disable_thinking: bool) -> Value {
        let request = build_request_body(
            &provider(kind),
            "test-model",
            "<transcript>\nhello\n</transcript>".to_string(),
            Some("system instructions".to_string()),
            json_schema,
            disable_thinking,
            None,
        );
        serde_json::to_value(&request).expect("request body must serialize")
    }

    #[test]
    fn reasoning_fields_omitted_when_thinking_not_disabled() {
        for kind in [
            "openai_compatible",
            "openai_local",
            "openrouter",
            "gemini",
            "anthropic",
        ] {
            let body = request_json(kind, None, false);
            let obj = body.as_object().unwrap();
            assert!(!obj.contains_key("think"), "kind {}", kind);
            assert!(!obj.contains_key("reasoning_effort"), "kind {}", kind);
            assert!(!obj.contains_key("reasoning"), "kind {}", kind);
        }
    }

    #[test]
    fn openai_compatible_disable_thinking_sends_think_and_low_effort() {
        // Legacy call shape: no response_format
        let body = request_json("openai_compatible", None, true);
        assert_eq!(body["think"], serde_json::json!(false));
        assert_eq!(body["reasoning_effort"], serde_json::json!("low"));
        let obj = body.as_object().unwrap();
        assert!(!obj.contains_key("reasoning"));
        assert!(!obj.contains_key("response_format"));
    }

    #[test]
    fn structured_call_carries_reasoning_fields_and_schema() {
        let schema = serde_json::json!({ "type": "object" });
        let body = request_json("openai_compatible", Some(schema), true);
        assert_eq!(body["reasoning_effort"], serde_json::json!("low"));
        assert_eq!(
            body["response_format"]["type"],
            serde_json::json!("json_schema")
        );
    }

    #[test]
    fn openrouter_uses_nested_reasoning_only() {
        let body = request_json("openrouter", None, true);
        assert_eq!(body["reasoning"], serde_json::json!({ "enabled": false }));
        let obj = body.as_object().unwrap();
        assert!(!obj.contains_key("reasoning_effort"));
        assert!(!obj.contains_key("think"));
    }

    #[test]
    fn gemini_gets_effort_without_think() {
        // "none" (thinking OFF) only where Google accepts it: the 2.5 Flash
        // family. 2.5 Pro / Gemini 3 can't disable reasoning (best-effort
        // "low"); the 2.0 family has no thinking switch at all.
        let mk = |model: &str| {
            let mut p = provider("gemini");
            p.model = model.to_string();
            reasoning_fields(&p, true)
        };
        assert_eq!(
            mk("gemini-2.5-flash").reasoning_effort.as_deref(),
            Some("none")
        );
        assert_eq!(
            mk("gemini-2.5-flash-lite").reasoning_effort.as_deref(),
            Some("none")
        );
        assert_eq!(
            mk("gemini-2.5-pro").reasoning_effort.as_deref(),
            Some("low")
        );
        assert_eq!(
            mk("gemini-3-flash").reasoning_effort.as_deref(),
            Some("low")
        );
        assert_eq!(mk("gemini-2.0-flash").reasoning_effort, None);
        assert_eq!(mk("gemini-2.5-flash").think, None);
        assert!(mk("gemini-2.5-flash").reasoning.is_none());
    }

    #[test]
    fn dialects_without_reasoning_switch_receive_none() {
        for kind in ["anthropic", "apple_intelligence"] {
            let body = request_json(kind, None, true);
            let obj = body.as_object().unwrap();
            assert!(!obj.contains_key("think"), "kind {}", kind);
            assert!(!obj.contains_key("reasoning_effort"), "kind {}", kind);
            assert!(!obj.contains_key("reasoning"), "kind {}", kind);
        }
    }

    #[test]
    fn request_error_message_labels_connect_phase_timeout() {
        let msg = request_error_message(true, true, "operation timed out", REQUEST_TIMEOUT);
        assert!(msg.contains("Connection timed out"), "{msg}");
        assert!(
            msg.contains(&CONNECT_TIMEOUT.as_secs().to_string()),
            "{msg}"
        );
    }

    #[test]
    fn request_error_message_labels_total_request_timeout() {
        let msg = request_error_message(true, false, "operation timed out", REQUEST_TIMEOUT);
        assert!(msg.contains("Request timed out"), "{msg}");
        assert!(
            msg.contains(&REQUEST_TIMEOUT.as_secs().to_string()),
            "{msg}"
        );
        // Must NOT be mislabeled as a connect-phase failure.
        assert!(!msg.contains("Connection timed out"), "{msg}");
    }

    #[test]
    fn request_error_message_total_request_timeout_uses_the_callers_own_budget() {
        // token_count/model_testing configure their own total-request timeout
        // per call (15/30/300s), distinct from llm_client's 120s — the
        // reported number must reflect whichever budget actually applied,
        // never a value borrowed from another module.
        let msg = request_error_message(
            true,
            false,
            "operation timed out",
            std::time::Duration::from_secs(15),
        );
        assert!(msg.contains("Request timed out after 15s"), "{msg}");
    }

    #[test]
    fn request_error_message_non_timeout_connect_failure_passes_through_raw() {
        let msg = request_error_message(false, true, "connection refused", REQUEST_TIMEOUT);
        assert!(msg.contains("Connection failed"), "{msg}");
        assert!(msg.contains("connection refused"), "{msg}");
    }

    #[test]
    fn request_error_message_generic_failure_passes_through_raw() {
        let msg = request_error_message(false, false, "some other reqwest error", REQUEST_TIMEOUT);
        assert!(msg.contains("HTTP request failed"), "{msg}");
        assert!(msg.contains("some other reqwest error"), "{msg}");
    }

    #[test]
    fn capped_buf_accepts_chunks_up_to_the_limit() {
        let mut acc = CappedBuf::new(4);
        acc.push(b"ab").unwrap();
        acc.push(b"cd").unwrap();
        assert_eq!(acc.into_bytes(), b"abcd".to_vec());
    }

    #[test]
    fn capped_buf_accepts_exactly_at_the_boundary() {
        // Total size exactly equal to max_bytes must NOT be rejected.
        let mut acc = CappedBuf::new(3);
        acc.push(b"abc").unwrap();
        assert_eq!(acc.into_bytes(), b"abc".to_vec());
    }

    #[test]
    fn capped_buf_rejects_the_chunk_that_crosses_the_limit() {
        let mut acc = CappedBuf::new(4);
        acc.push(b"ab").unwrap();
        let err = acc.push(b"cde").unwrap_err();
        assert!(err.contains("exceeded 4 byte limit"), "{err}");
    }

    #[test]
    fn capped_buf_rejects_a_single_oversized_chunk() {
        let mut acc = CappedBuf::new(2);
        let err = acc.push(b"abc").unwrap_err();
        assert!(err.contains("exceeded 2 byte limit"), "{err}");
    }

    /// A tiny local TCP listener stands in for "a fast hostile/misconfigured
    /// endpoint" — a real HTTP response, not a mocked `reqwest::Response`, so
    /// this exercises `read_body_capped`'s actual `.chunk()` loop rather than
    /// just `CappedBuf` in isolation (T-202 finding 4). The server writes the
    /// body across two separate writes with a short sleep between them, so
    /// the running total crosses the cap on the SECOND `.chunk()` call, not
    /// the first — proving the multi-chunk accumulation path, not just a
    /// single-oversized-chunk rejection (already covered above).
    #[test]
    fn read_body_capped_rejects_an_over_cap_multi_chunk_stream() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local test listener");
        let addr = listener.local_addr().unwrap();

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            // Drain the client's request line/headers BEFORE responding. If the
            // server writes the response while the client is still sending its
            // request, hyper sees inbound data early and fails the send with
            // UnexpectedMessage — the source of this test's flakiness.
            let mut req = [0u8; 1024];
            let _ = stream.read(&mut req);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 16\r\n\r\n")
                .ok();
            // First chunk: 8 bytes, comfortably under the 10-byte cap alone.
            stream.write_all(&[b'a'; 8]).ok();
            stream.flush().ok();
            std::thread::sleep(std::time::Duration::from_millis(50));
            // Second chunk: another 8 bytes — 16 total pushes it over the cap.
            stream.write_all(&[b'b'; 8]).ok();
            stream.flush().ok();
            // Keep the connection open long enough for the client to read the
            // second chunk (and hit the cap) BEFORE we drop the socket. Dropping
            // immediately can close the connection abruptly (RST) and surface as
            // "error decoding response body" instead of the cap rejection,
            // making this test flaky under load.
            std::thread::sleep(std::time::Duration::from_millis(500));
        });

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build a local tokio runtime for the test");

        let result: Result<Vec<u8>, String> = rt.block_on(async {
            let client = reqwest::Client::new();
            let response = client
                .get(format!("http://{}/", addr))
                .send()
                .await
                .expect("request should reach the local test server");
            read_body_capped(response, 10, REQUEST_TIMEOUT).await
        });

        server.join().expect("test server thread must not panic");

        let err = result.expect_err("a body over the cap must be rejected, not buffered fully");
        assert!(err.contains("exceeded 10 byte limit"), "{err}");
    }

    /// A body read that stalls past the client's total-request timeout must
    /// be classified as a total-request-phase timeout, never connect-phase —
    /// the connection and headers already arrived, only the body stalled.
    /// Uses a short client timeout (not the app's real 120s) so the test
    /// doesn't have to wait a real total-request deadline out to observe the
    /// classification (T-202 finding 4).
    #[test]
    fn body_read_timeout_is_labeled_as_total_request_phase() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local test listener");
        let addr = listener.local_addr().unwrap();

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            // Drain the client's request BEFORE responding, so hyper doesn't see
            // early inbound data and fail the send with UnexpectedMessage.
            let mut req = [0u8; 1024];
            let _ = stream.read(&mut req);
            // Send clean, Content-Length-framed headers promising 100 body
            // bytes, then send only 1 and go silent — so the client's BODY read
            // genuinely stalls waiting for the promised remainder. Using a
            // proper Content-Length (rather than close-delimited framing) keeps
            // hyper from rejecting the response under load with UnexpectedMessage.
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n1")
                .ok();
            stream.flush().ok();
            // Keep the connection OPEN and silent well past the client timeout
            // below, so the timeout — not a connection close (which would be a
            // decode error) — is what ends the stalled body read.
            std::thread::sleep(std::time::Duration::from_secs(5));
        });

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build a local tokio runtime for the test");

        // reqwest's client `.timeout()` is the mechanism that fires here
        // (read_body_capped uses its timeout arg only to CLASSIFY the error).
        // A moderate 2s total-request timeout: generous enough that header
        // arrival never races it (no flaky send under load), yet it fires
        // during the server's 5s body stall so the classified error is a
        // total-request timeout. Same value passed to read_body_capped for the
        // message text.
        let timeout = std::time::Duration::from_secs(2);
        let outcome: Result<Vec<u8>, String> = rt.block_on(async {
            let client = reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .expect("build client");
            let response = client
                .get(format!("http://{}/", addr))
                .send()
                .await
                .expect("headers should arrive well before the stall");
            read_body_capped(response, MAX_RESPONSE_BYTES, timeout).await
        });

        server.join().expect("test server thread must not panic");

        let err = outcome.expect_err("a stalled body read must time out, not hang forever");
        assert!(err.contains("Request timed out after"), "{err}");
        assert!(!err.contains("Connection timed out"), "{err}");
    }

    #[test]
    fn system_and_user_roles_stay_separate() {
        let body = request_json("openai_compatible", None, false);
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], serde_json::json!("system"));
        assert_eq!(messages[1]["role"], serde_json::json!("user"));
        assert!(
            messages[1]["content"]
                .as_str()
                .unwrap()
                .starts_with("<transcript>")
        );
    }
}
