//! Multi-provider model testing.
//!
//! Runs one prompt across several registered [`LlmProvider`]s (and, for the
//! judge panel, an arbiter prompt over the assembled answers), capturing each
//! provider's content, token usage, monetary cost and round-trip time.
//!
//! Concurrency honours each provider's family: providers that share a
//! non-empty `concurrency_group` AND are marked `sequential` run one at a time
//! against each other (a single local loader can't serve two models at once),
//! while different families and non-sequential providers run in parallel. The
//! reported round-trip time is the wall-clock until the last provider finished
//! (not the sum of per-call times).

use log::{info, warn};
use serde::Serialize;
use serde_json::{Value, json};
use specta::Type;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

use crate::settings::{self, LlmProvider};

pub const MODEL_TEST_PROGRESS_EVENT: &str = "model-test-progress";

const CLOUD_TIMEOUT_SECS: u64 = 120;
const LOCAL_TIMEOUT_SECS: u64 = 300;

/// Cancellation handle: bumping the generation makes an in-flight run stop
/// before its next provider and suppresses still-pending progress emissions.
pub struct ModelTestState {
    generation: AtomicU64,
}

impl ModelTestState {
    pub fn new() -> Self {
        Self {
            generation: AtomicU64::new(0),
        }
    }
}

/// One provider's result for a single prompt.
#[derive(Serialize, Clone, Type)]
pub struct ChatOutcome {
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    pub ok: bool,
    pub content: String,
    pub error: Option<String>,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    /// USD cost: the real charge for OpenRouter, otherwise computed from the
    /// provider's per-million rates. `None` when usage is unknown.
    pub cost_usd: Option<f64>,
    /// True when `cost_usd` is the provider-reported real charge (OpenRouter),
    /// false when it is estimated from the configured per-million rates.
    pub cost_is_real: bool,
    pub elapsed_ms: u32,
}

#[derive(Serialize, Clone, Type)]
pub struct ModelTestRun {
    pub outcomes: Vec<ChatOutcome>,
    /// Wall-clock from dispatch until the last provider finished (ms).
    pub round_trip_ms: u32,
}

/// One OpenRouter model's pass-through pricing, normalized to USD per 1M tokens.
#[derive(Serialize, Clone, Type)]
pub struct OpenRouterModelPrice {
    pub id: String,
    pub canonical_slug: String,
    pub name: String,
    pub input_per_million: f64,
    pub output_per_million: f64,
}

fn is_local_url(url: &str) -> bool {
    url.contains("127.0.0.1") || url.contains("localhost") || url.contains("[::1]")
}

/// This provider's total-request deadline (local services get a longer
/// budget than cloud APIs). Exposed separately from `http_client` so callers
/// can reuse the exact same duration when phase-labeling a body-read timeout
/// via `read_body_capped` (T-202 finding 4) instead of guessing.
fn request_timeout_for(provider: &LlmProvider) -> Duration {
    let secs = if is_local_url(&provider.base_url) {
        LOCAL_TIMEOUT_SECS
    } else {
        CLOUD_TIMEOUT_SECS
    };
    Duration::from_secs(secs)
}

/// Every client built here gets BOTH a connect deadline (shared with
/// `llm_client`'s post-processing client, T-202) and this provider's
/// total-request deadline. Each provider gets its own `reqwest::Client`
/// instance built fresh per call, so one hung provider's timeout firing can
/// never block or delay another provider's request — including within a
/// sequential lane, where the timeout is what guarantees the lane eventually
/// moves on to the next provider instead of hanging indefinitely.
fn http_client(provider: &LlmProvider) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(crate::llm_client::CONNECT_TIMEOUT)
        .timeout(request_timeout_for(provider))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))
}

fn trim_base(base_url: &str) -> String {
    base_url.trim_end_matches('/').to_string()
}

fn api_error(value: &Value, status: u16) -> String {
    let message = value["error"]["message"]
        .as_str()
        .or_else(|| value["error"].as_str())
        .or_else(|| value["message"].as_str())
        .unwrap_or("unknown error");
    format!("HTTP {}: {}", status, message)
}

/// Raw result from a provider before cost is computed.
struct RawChat {
    content: String,
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
    real_cost: Option<f64>,
}

/// Optional per-request extras: reasoning ("thinking") on/off and an attached
/// image (a `data:<mime>;base64,...` URL) for vision-capable runner models.
#[derive(Clone, Copy)]
struct ChatExtras<'a> {
    thinking: Option<bool>,
    image: Option<&'a str>,
}

/// Split a `data:<mime>;base64,<payload>` URL into (mime, base64 payload).
fn parse_data_url(data_url: &str) -> Option<(&str, &str)> {
    data_url.strip_prefix("data:")?.split_once(";base64,")
}

/// Error returned when an attached image is not a usable base64 data URL, so the
/// run surfaces a clear failure instead of silently dropping the image (which the
/// Anthropic/Gemini request shapes would otherwise do).
const BAD_IMAGE: &str = "Attached image is not a valid base64 data URL";

/// Whether an Anthropic model uses the modern adaptive-thinking API surface
/// (`thinking: {type:"adaptive"}`, no `budget_tokens`, and sampling params such as
/// `temperature` rejected with HTTP 400). The 4.6 generation onward — plus Fable —
/// is adaptive; 4.5 and earlier (e.g. the default `claude-haiku-4-5`, Sonnet/Opus
/// 4.x<6, 3.x) use the legacy `budget_tokens` extended-thinking form which requires
/// `temperature = 1`. Unknown/custom ids default to adaptive (the forward trend).
fn anthropic_is_adaptive(model: &str) -> bool {
    let m = model.to_lowercase();
    if m.contains("fable") {
        return true;
    }
    // The first two integer tokens in the id are major-minor, e.g.
    // claude-opus-4-8 -> (4,8); claude-3-5-sonnet-20241022 -> (3,5).
    let nums: Vec<u32> = m
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<u32>().ok())
        .collect();
    match (nums.first(), nums.get(1)) {
        (Some(&major), Some(&minor)) => major > 4 || (major == 4 && minor >= 6),
        // No parseable version (alias/custom) -> assume a current model.
        _ => true,
    }
}

async fn chat_openai_compatible(
    provider: &LlmProvider,
    system: Option<&str>,
    prompt: &str,
    temperature: f64,
    extras: ChatExtras<'_>,
) -> Result<RawChat, String> {
    let client = http_client(provider)?;
    let url = format!("{}/chat/completions", trim_base(&provider.base_url));

    let mut messages = Vec::new();
    if let Some(sys) = system {
        if !sys.trim().is_empty() {
            messages.push(json!({"role": "system", "content": sys}));
        }
    }
    // With an attached image, the user content uses OpenAI's multimodal array.
    let user_content = match extras.image {
        Some(img) => json!([
            {"type": "text", "text": prompt},
            {"type": "image_url", "image_url": {"url": img}}
        ]),
        None => json!(prompt),
    };
    messages.push(json!({"role": "user", "content": user_content}));

    let mut body = json!({
        "model": provider.model,
        "messages": messages,
        "temperature": temperature,
        "stream": false,
    });
    // OpenRouter returns the real per-request cost when usage accounting is
    // requested explicitly.
    if provider.kind == "openrouter" {
        body["usage"] = json!({ "include": true });
    }
    // Reasoning: OpenRouter uses `reasoning.enabled`; other OpenAI-compatible
    // servers (FLM, LM Studio) use the `think` boolean.
    match extras.thinking {
        Some(on) if provider.kind == "openrouter" => {
            body["reasoning"] = json!({ "enabled": on });
        }
        Some(on) => body["think"] = json!(on),
        None => {}
    }

    let mut request = client.post(&url).header("X-Title", "Handy").json(&body);
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
        request_timeout_for(provider),
    )
    .await?;
    let value: Value =
        serde_json::from_slice(&body_bytes).map_err(|e| format!("Invalid response: {}", e))?;
    if !status.is_success() {
        return Err(api_error(&value, status.as_u16()));
    }
    if let Some(error) = value.get("error") {
        if !error.is_null() {
            return Err(api_error(&value, status.as_u16()));
        }
    }

    let content = value["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let input_tokens = value["usage"]["prompt_tokens"].as_u64().map(|n| n as u32);
    let output_tokens = value["usage"]["completion_tokens"]
        .as_u64()
        .map(|n| n as u32);
    // Only OpenRouter reports a real per-request charge (and only it gets
    // `usage.include`). For other OpenAI-compatible endpoints, fall through to
    // the configured per-million estimate even if they emit a `usage.cost`.
    let real_cost = if provider.kind == "openrouter" {
        value["usage"]["cost"].as_f64()
    } else {
        None
    };

    Ok(RawChat {
        content,
        input_tokens,
        output_tokens,
        real_cost,
    })
}

async fn chat_anthropic(
    provider: &LlmProvider,
    system: Option<&str>,
    prompt: &str,
    temperature: f64,
    extras: ChatExtras<'_>,
) -> Result<RawChat, String> {
    let client = http_client(provider)?;
    let url = format!("{}/v1/messages", trim_base(&provider.base_url));

    let user_content = match extras.image {
        Some(img) => {
            let (mime, b64) = parse_data_url(img).ok_or_else(|| BAD_IMAGE.to_string())?;
            json!([
                {"type": "text", "text": prompt},
                {"type": "image", "source": {"type": "base64", "media_type": mime, "data": b64}}
            ])
        }
        None => json!(prompt),
    };
    let mut body = json!({
        "model": provider.model,
        "max_tokens": 4096,
        "messages": [{"role": "user", "content": user_content}],
    });
    if let Some(sys) = system {
        if !sys.trim().is_empty() {
            body["system"] = json!(sys);
        }
    }
    // Thinking + sampling differ by model generation (see anthropic_is_adaptive):
    //   - Adaptive (4.6+/Fable): `thinking: {type:"adaptive"}`; `temperature` is
    //     rejected with 400 on 4.7+/Fable, so it is omitted entirely.
    //   - Legacy (<=4.5, 3.x): `budget_tokens` extended thinking (requires
    //     temperature = 1); otherwise the user's temperature is sent as-is.
    if anthropic_is_adaptive(&provider.model) {
        if extras.thinking == Some(true) {
            body["thinking"] = json!({ "type": "adaptive" });
        }
    } else if extras.thinking == Some(true) {
        body["thinking"] = json!({ "type": "enabled", "budget_tokens": 1024 });
        body["temperature"] = json!(1.0);
    } else {
        body["temperature"] = json!(temperature);
    }

    let response = client
        .post(&url)
        .header("x-api-key", provider.api_key.trim())
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    let status = response.status();
    let body_bytes = crate::llm_client::read_body_capped(
        response,
        crate::llm_client::MAX_RESPONSE_BYTES,
        request_timeout_for(provider),
    )
    .await?;
    let value: Value =
        serde_json::from_slice(&body_bytes).map_err(|e| format!("Invalid response: {}", e))?;
    if !status.is_success() {
        return Err(api_error(&value, status.as_u16()));
    }

    // `content` is an array of blocks; concatenate the text blocks.
    let content = value["content"]
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();
    let input_tokens = value["usage"]["input_tokens"].as_u64().map(|n| n as u32);
    let output_tokens = value["usage"]["output_tokens"].as_u64().map(|n| n as u32);

    Ok(RawChat {
        content,
        input_tokens,
        output_tokens,
        real_cost: None,
    })
}

async fn chat_gemini(
    provider: &LlmProvider,
    system: Option<&str>,
    prompt: &str,
    temperature: f64,
    extras: ChatExtras<'_>,
) -> Result<RawChat, String> {
    let client = http_client(provider)?;
    let url = format!(
        "{}/v1beta/models/{}:generateContent?key={}",
        trim_base(&provider.base_url),
        provider.model,
        provider.api_key.trim()
    );

    let mut parts = vec![json!({ "text": prompt })];
    if let Some(img) = extras.image {
        let (mime, b64) = parse_data_url(img).ok_or_else(|| BAD_IMAGE.to_string())?;
        parts.push(json!({ "inline_data": { "mime_type": mime, "data": b64 } }));
    }
    let mut body = json!({
        "contents": [{"role": "user", "parts": parts}],
        "generationConfig": { "temperature": temperature },
    });
    if let Some(sys) = system {
        if !sys.trim().is_empty() {
            body["systemInstruction"] = json!({ "parts": [{"text": sys}] });
        }
    }
    // thinkingBudget 0 disables reasoning; -1 lets the model decide.
    match extras.thinking {
        Some(false) => body["generationConfig"]["thinkingConfig"] = json!({ "thinkingBudget": 0 }),
        Some(true) => body["generationConfig"]["thinkingConfig"] = json!({ "thinkingBudget": -1 }),
        None => {}
    }

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
        request_timeout_for(provider),
    )
    .await?;
    let value: Value =
        serde_json::from_slice(&body_bytes).map_err(|e| format!("Invalid response: {}", e))?;
    if !status.is_success() {
        return Err(api_error(&value, status.as_u16()));
    }

    let content = value["candidates"][0]["content"]["parts"]
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|p| p["text"].as_str())
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();
    let input_tokens = value["usageMetadata"]["promptTokenCount"]
        .as_u64()
        .map(|n| n as u32);
    let output_tokens = value["usageMetadata"]["candidatesTokenCount"]
        .as_u64()
        .map(|n| n as u32);

    Ok(RawChat {
        content,
        input_tokens,
        output_tokens,
        real_cost: None,
    })
}

async fn chat_with_provider(
    provider: &LlmProvider,
    system: Option<&str>,
    prompt: &str,
    temperature: f64,
    extras: ChatExtras<'_>,
) -> ChatOutcome {
    let started = Instant::now();
    let raw = match provider.kind.as_str() {
        "openai_compatible" | "openrouter" | "custom" => {
            chat_openai_compatible(provider, system, prompt, temperature, extras).await
        }
        "anthropic" => chat_anthropic(provider, system, prompt, temperature, extras).await,
        "gemini" => chat_gemini(provider, system, prompt, temperature, extras).await,
        other => Err(format!(
            "Model testing is not supported for kind '{}'",
            other
        )),
    };
    let elapsed_ms = started.elapsed().as_millis() as u32;

    match raw {
        Ok(raw) => {
            let (cost_usd, cost_is_real) = compute_cost(provider, &raw);
            ChatOutcome {
                provider_id: provider.id.clone(),
                provider_name: provider.name.clone(),
                model: provider.model.clone(),
                ok: true,
                content: raw.content,
                error: None,
                input_tokens: raw.input_tokens,
                output_tokens: raw.output_tokens,
                cost_usd,
                cost_is_real,
                elapsed_ms,
            }
        }
        Err(error) => {
            warn!("Model test via '{}' failed: {}", provider.id, error);
            ChatOutcome {
                provider_id: provider.id.clone(),
                provider_name: provider.name.clone(),
                model: provider.model.clone(),
                ok: false,
                content: String::new(),
                error: Some(error),
                input_tokens: None,
                output_tokens: None,
                cost_usd: None,
                cost_is_real: false,
                elapsed_ms,
            }
        }
    }
}

fn compute_cost(provider: &LlmProvider, raw: &RawChat) -> (Option<f64>, bool) {
    if let Some(real) = raw.real_cost {
        return (Some(real), true);
    }
    match (raw.input_tokens, raw.output_tokens) {
        (Some(it), Some(ot)) => {
            let cost = it as f64 / 1_000_000.0 * provider.cost_input_per_million
                + ot as f64 / 1_000_000.0 * provider.cost_output_per_million;
            (Some(cost), false)
        }
        _ => (None, false),
    }
}

/// The scheduling lane a provider belongs to. Sequential providers in a named
/// family share a lane (run one-by-one); everything else gets its own lane and
/// runs in parallel.
fn lane_key(provider: &LlmProvider) -> String {
    if provider.sequential && !provider.concurrency_group.trim().is_empty() {
        format!("family:{}", provider.concurrency_group.trim())
    } else {
        format!("solo:{}", provider.id)
    }
}

/// Resolve the requested provider ids to providers, preserving the caller's
/// order, then group them into ordered concurrency lanes.
fn build_lanes(app: &AppHandle, provider_ids: &[String]) -> Vec<Vec<LlmProvider>> {
    let registry = settings::get_settings(app).llm_providers;
    let mut lanes: Vec<(String, Vec<LlmProvider>)> = Vec::new();

    for id in provider_ids {
        // Skip unknown and disabled providers (disabled = unconfigured slot).
        let provider = match registry.iter().find(|p| &p.id == id) {
            Some(p) if p.enabled => p.clone(),
            _ => continue,
        };
        let key = lane_key(&provider);
        match lanes.iter_mut().find(|(k, _)| k == &key) {
            Some((_, list)) => list.push(provider),
            None => lanes.push((key, vec![provider])),
        }
    }

    lanes.into_iter().map(|(_, list)| list).collect()
}

#[tauri::command]
#[specta::specta]
pub async fn run_model_test(
    app: AppHandle,
    system: Option<String>,
    prompt: String,
    provider_ids: Vec<String>,
    temperature: f64,
    thinking: Option<bool>,
    image_data_url: Option<String>,
) -> Result<ModelTestRun, String> {
    let state = app.state::<ModelTestState>();
    let generation = state.generation.fetch_add(1, Ordering::SeqCst) + 1;

    let lanes = build_lanes(&app, &provider_ids);
    info!(
        "Model test: {} provider(s) across {} lane(s), temp {}, thinking {:?}, image {}",
        provider_ids.len(),
        lanes.len(),
        temperature,
        thinking,
        image_data_url.is_some()
    );

    let started = Instant::now();

    // Each lane runs its providers sequentially; lanes run concurrently.
    let lane_futures = lanes.into_iter().map(|providers| {
        let app = app.clone();
        let system = system.clone();
        let prompt = prompt.clone();
        let image_data_url = image_data_url.clone();
        async move {
            let extras = ChatExtras {
                thinking,
                image: image_data_url.as_deref(),
            };
            let mut outs = Vec::with_capacity(providers.len());
            for provider in providers {
                // Abort the lane if a newer run superseded this one.
                let current = app.state::<ModelTestState>();
                if current.generation.load(Ordering::SeqCst) != generation {
                    break;
                }
                let outcome =
                    chat_with_provider(&provider, system.as_deref(), &prompt, temperature, extras)
                        .await;
                let still_current = app
                    .state::<ModelTestState>()
                    .generation
                    .load(Ordering::SeqCst)
                    == generation;
                if still_current {
                    let _ = app.emit(MODEL_TEST_PROGRESS_EVENT, outcome.clone());
                    outs.push(outcome);
                }
            }
            outs
        }
    });

    let lane_results = futures_util::future::join_all(lane_futures).await;
    let round_trip_ms = started.elapsed().as_millis() as u32;

    let outcomes: Vec<ChatOutcome> = lane_results.into_iter().flatten().collect();

    Ok(ModelTestRun {
        outcomes,
        round_trip_ms,
    })
}

#[tauri::command]
#[specta::specta]
pub fn cancel_model_test(app: AppHandle) -> Result<(), String> {
    let state = app.state::<ModelTestState>();
    state.generation.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

/// Write a UTF-8 text file (used to export the model-testing Markdown report).
/// The frontend picks the path via the save dialog; writing on the backend
/// avoids widening the frontend's `$APPDATA`-scoped filesystem permissions.
#[tauri::command]
#[specta::specta]
pub fn write_text_file(path: String, contents: String) -> Result<(), String> {
    std::fs::write(&path, contents).map_err(|e| format!("Failed to write file: {}", e))
}

/// Fetch OpenRouter's public model catalogue with pass-through pricing,
/// normalized to USD per 1M tokens. Public endpoint — no API key needed. The UI
/// uses this to auto-fill cost fields, including for Gemini/Anthropic models via
/// their OpenRouter slugs (those providers don't expose pricing themselves).
#[tauri::command]
#[specta::specta]
pub async fn fetch_openrouter_model_prices() -> Result<Vec<OpenRouterModelPrice>, String> {
    let timeout = Duration::from_secs(CLOUD_TIMEOUT_SECS);
    let client = reqwest::Client::builder()
        .connect_timeout(crate::llm_client::CONNECT_TIMEOUT)
        .timeout(timeout)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;
    let response = client
        .get("https://openrouter.ai/api/v1/models")
        .header("X-Title", "Handy")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    let body_bytes = crate::llm_client::read_body_capped(
        response,
        crate::llm_client::MAX_RESPONSE_BYTES,
        timeout,
    )
    .await?;
    let value: Value =
        serde_json::from_slice(&body_bytes).map_err(|e| format!("Invalid response: {}", e))?;

    // OpenRouter pricing is USD per single token (string) — ×1e6 for per-1M.
    let per_million = |v: &Value| -> f64 {
        v.as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .map(|per_token| per_token * 1_000_000.0)
            .unwrap_or(0.0)
    };

    let models = value["data"]
        .as_array()
        .ok_or_else(|| "Response missing model list".to_string())?
        .iter()
        .filter_map(|m| {
            let id = m["id"].as_str()?.to_string();
            Some(OpenRouterModelPrice {
                canonical_slug: m["canonical_slug"].as_str().unwrap_or(&id).to_string(),
                name: m["name"].as_str().unwrap_or(&id).to_string(),
                input_per_million: per_million(&m["pricing"]["prompt"]),
                output_per_million: per_million(&m["pricing"]["completion"]),
                id,
            })
        })
        .collect();
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptive_models_are_46_plus_and_fable() {
        for m in [
            "claude-opus-4-8",
            "claude-opus-4-7",
            "claude-sonnet-4-6",
            "claude-opus-4-6-20260101",
            "claude-fable-5",
            "anthropic/claude-opus-4.8",
            "some-future-5-0-model",
        ] {
            assert!(anthropic_is_adaptive(m), "{m} should be adaptive");
        }
    }

    #[test]
    fn legacy_models_use_budget_tokens() {
        for m in [
            "claude-haiku-4-5",
            "claude-sonnet-4-5",
            "claude-opus-4-1-20250805",
            "claude-3-5-sonnet-20241022",
            "claude-3-7-sonnet",
            "claude-opus-4-0",
        ] {
            assert!(!anthropic_is_adaptive(m), "{m} should be legacy");
        }
    }

    #[test]
    fn unknown_ids_default_to_adaptive() {
        assert!(anthropic_is_adaptive("my-custom-alias"));
        assert!(anthropic_is_adaptive(""));
    }

    #[test]
    fn parse_data_url_splits_mime_and_payload() {
        assert_eq!(
            parse_data_url("data:image/png;base64,AAAA"),
            Some(("image/png", "AAAA"))
        );
        assert_eq!(parse_data_url("not-a-data-url"), None);
        assert_eq!(parse_data_url("data:image/png,AAAA"), None);
    }
}
