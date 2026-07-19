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

/// Time allowed to establish a connection to the provider.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Total-request deadline — generous enough for slow local models, but a
/// stalled provider can never hang post-processing forever.
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

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
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let status = response.status();
    let elapsed_ms = request_start.elapsed().as_millis();
    info!(
        "Post-process: response status {} in {}ms",
        status, elapsed_ms
    );

    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Failed to read error response".to_string());
        return Err(format!(
            "API request failed with status {}: {}",
            status, error_text
        ));
    }

    let completion: ChatCompletionResponse = response
        .json()
        .await
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
