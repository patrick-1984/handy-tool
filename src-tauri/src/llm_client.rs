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

/// Send a chat completion request to an OpenAI-compatible API
/// Returns Ok(Some(content)) on success, Ok(None) if response has no content,
/// or Err on actual errors (HTTP, parsing, etc.)
pub async fn send_chat_completion(
    provider: &LlmProvider,
    api_key: String,
    model: &str,
    prompt: String,
    disable_thinking: bool,
    temperature: Option<f32>,
) -> Result<Option<String>, String> {
    send_chat_completion_with_schema(
        provider,
        api_key,
        model,
        prompt,
        None,
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

    // Build messages vector
    let mut messages = Vec::new();

    // Add system prompt if provided
    if let Some(system) = system_prompt {
        // Content preview only at debug — release file logs are INFO and must
        // never contain prompt or transcript text
        info!("Post-process: system prompt ({} chars)", system.len());
        debug!(
            "Post-process: system prompt preview: {}",
            &system[..system.len().min(300)]
        );
        messages.push(ChatMessage {
            role: "system".to_string(),
            content: system,
        });
    } else {
        info!("Post-process: no system message — instructions embedded in user message");
    }

    // Add user message
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: user_content,
    });

    // Build response_format if schema is provided
    let response_format = json_schema.map(|schema| ResponseFormat {
        format_type: "json_schema".to_string(),
        json_schema: JsonSchema {
            name: "transcription_output".to_string(),
            strict: true,
            schema,
        },
    });

    let request_body = ChatCompletionRequest {
        model: model.to_string(),
        messages,
        response_format,
        think: if disable_thinking { Some(false) } else { None },
        temperature,
    };

    // Log full request metadata
    info!(
        "Post-process: request body — model: {}, messages: {}, response_format: {}, think: {:?}",
        request_body.model,
        request_body.messages.len(),
        if request_body.response_format.is_some() {
            "json_schema"
        } else {
            "none"
        },
        request_body.think,
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
