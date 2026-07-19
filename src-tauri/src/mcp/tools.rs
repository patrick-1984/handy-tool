//! Tool handlers shared by the MCP endpoint and the CLI JSON API. Each handler
//! takes the live `AppHandle` and a JSON arguments object and returns a JSON
//! result, reusing the same app logic the GUI uses (settings, model testing,
//! token counting, typing, history). API keys are WRITE-ONLY: never returned.

use once_cell::sync::Lazy;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};

use crate::managers::history::HistoryManager;
use crate::mcp::{pricing, report};
use crate::model_testing::run_model_test;
use crate::settings::{self, AppSettings, LlmProvider, NamedImage, NamedText};
use std::sync::{Arc, Mutex, MutexGuard};

/// Serializes settings read-modify-write across the per-request server threads so
/// concurrent mutating tool calls can't clobber each other (lost update).
static SETTINGS_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn settings_guard() -> MutexGuard<'static, ()> {
    SETTINGS_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Name + one-line description + JSON input schema for every exposed tool.
/// Used to answer MCP `tools/list`.
pub fn tool_specs() -> Vec<Value> {
    vec![
        json!({
            "name": "token_count",
            "description": "Count tokens in text using a tokenizer (cl100k_base, o200k_base, or estimate).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": {"type": "string"},
                    "tokenizer": {"type": "string", "enum": ["cl100k_base", "o200k_base", "estimate"]}
                },
                "required": ["text"]
            }
        }),
        json!({
            "name": "keyboard_type",
            "description": "Type/paste text into the currently focused window via Handy's typing engine.",
            "inputSchema": {
                "type": "object",
                "properties": {"text": {"type": "string"}},
                "required": ["text"]
            }
        }),
        json!({
            "name": "history_list",
            "description": "List recent transcription history entries (id, title, timestamp, snippet).",
            "inputSchema": {
                "type": "object",
                "properties": {"limit": {"type": "integer"}}
            }
        }),
        json!({
            "name": "history_get",
            "description": "Get one full transcription history entry by id, including its audio file path.",
            "inputSchema": {
                "type": "object",
                "properties": {"id": {"type": "integer"}},
                "required": ["id"]
            }
        }),
        json!({
            "name": "list_providers",
            "description": "List registered LLM providers (API keys redacted; has_api_key indicates whether one is set).",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "set_provider",
            "description": "Update a registered provider by id (model, name, base_url, api_key [write-only], enabled, sequential, concurrency_group, persist_price, cost_input_per_million, cost_output_per_million). Changing the model auto-fills cost from OpenRouter unless persist_price is set or a cost is given.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "name": {"type": "string"},
                    "model": {"type": "string"},
                    "base_url": {"type": "string"},
                    "api_key": {"type": "string"},
                    "enabled": {"type": "boolean"},
                    "sequential": {"type": "boolean"},
                    "concurrency_group": {"type": "string"},
                    "persist_price": {"type": "boolean"},
                    "cost_input_per_million": {"type": "number"},
                    "cost_output_per_million": {"type": "number"}
                },
                "required": ["id"]
            }
        }),
        json!({
            "name": "list_provider_models",
            "description": "Query a provider's available models live from its API (refreshes the list).",
            "inputSchema": {
                "type": "object",
                "properties": {"provider_id": {"type": "string"}},
                "required": ["provider_id"]
            }
        }),
        json!({
            "name": "list_library",
            "description": "List saved model prompts, judge prompts, and presets.",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "save_model_prompt",
            "description": "Save a reusable model prompt (optionally with an image data URL).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "text": {"type": "string"},
                    "image_data_url": {"type": "string"},
                    "image_name": {"type": "string"}
                },
                "required": ["name", "text"]
            }
        }),
        json!({
            "name": "save_judge_prompt",
            "description": "Save a reusable judge (arbiter) prompt.",
            "inputSchema": {
                "type": "object",
                "properties": {"name": {"type": "string"}, "text": {"type": "string"}},
                "required": ["name", "text"]
            }
        }),
        json!({
            "name": "save_preset",
            "description": "Save a preset pairing a model prompt and a judge prompt under one name.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "model_prompt": {"type": "string"},
                    "judge_prompt": {"type": "string"}
                },
                "required": ["name", "model_prompt", "judge_prompt"]
            }
        }),
        json!({
            "name": "model_test",
            "description": "Run a prompt across selected providers and (optionally) a judge panel, with separate temperature + thinking for models and judge. Returns a Markdown report; with save_path it also writes the report to that file. Providers are selected by id or name.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "prompt": {"type": "string"},
                    "run": {"type": "array", "items": {"type": "string"}},
                    "judge": {"type": "array", "items": {"type": "string"}},
                    "judge_prompt": {"type": "string"},
                    "preset": {"type": "string"},
                    "model_temperature": {"type": "number"},
                    "model_thinking": {"type": "string", "enum": ["auto", "on", "off"]},
                    "judge_temperature": {"type": "number"},
                    "judge_thinking": {"type": "string", "enum": ["auto", "on", "off"]},
                    "image_data_url": {"type": "string"},
                    "save_path": {"type": "string"}
                },
                "required": ["run"]
            }
        }),
        json!({
            "name": "transcription_status",
            "description": "Current dictation pipeline stage: idle, recording, or processing.",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "jump_slots",
            "description": "List the Jumper's 5 target slots (0 = hot anchor, 1-4 static): app + control class per occupied slot. Windows only — empty elsewhere.",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "jump_slot_clear",
            "description": "Clear one Jumper slot (0 = hot anchor, 1-4 static).",
            "inputSchema": {
                "type": "object",
                "properties": {"slot": {"type": "integer", "minimum": 0, "maximum": 4}},
                "required": ["slot"]
            }
        }),
        json!({
            "name": "translator_status",
            "description": "Status of the Translator folder-watch batch transcription: queue, current file/segment, pause reason, session counters.",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "translator_set_enabled",
            "description": "Enable or disable the Translator folder watching.",
            "inputSchema": {
                "type": "object",
                "properties": {"enabled": {"type": "boolean"}},
                "required": ["enabled"]
            }
        }),
    ]
}

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tauri::async_runtime::block_on(f)
}

fn thinking_from(s: &str) -> Option<bool> {
    match s {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn redact_provider(p: &LlmProvider) -> Value {
    let mut v = serde_json::to_value(p).unwrap_or_else(|_| json!({}));
    let has_key = !p.api_key.trim().is_empty();
    if let Some(obj) = v.as_object_mut() {
        obj.insert("api_key".to_string(), json!(""));
        obj.insert("has_api_key".to_string(), json!(has_key));
    }
    v
}

fn emit_setting_changed(app: &AppHandle, setting: &str, value: Value) {
    let _ = app.emit(
        "settings-changed",
        json!({ "setting": setting, "value": value }),
    );
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

/// Resolve provider selectors (ids or case-insensitive names) to provider ids.
fn resolve_ids(settings: &AppSettings, sel: Option<&Vec<Value>>) -> Vec<String> {
    let Some(sel) = sel else { return Vec::new() };
    sel.iter()
        .filter_map(|v| v.as_str())
        .filter_map(|s| {
            if settings.llm_providers.iter().any(|p| p.id == s) {
                Some(s.to_string())
            } else {
                settings
                    .llm_providers
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(s))
                    .map(|p| p.id.clone())
            }
        })
        .collect()
}

fn preset_model_text(settings: &AppSettings, preset: &settings::ModelTestPreset) -> String {
    if let Some(id) = &preset.model_prompt_id {
        if let Some(p) = settings
            .model_test_library
            .model_prompts
            .iter()
            .find(|x| &x.id == id)
        {
            return p.text.clone();
        }
    }
    preset.model_prompt.clone()
}

fn preset_judge_text(settings: &AppSettings, preset: &settings::ModelTestPreset) -> String {
    if let Some(id) = &preset.judge_prompt_id {
        if let Some(p) = settings
            .model_test_library
            .judge_prompts
            .iter()
            .find(|x| &x.id == id)
        {
            return p.text.clone();
        }
    }
    preset.judge_prompt.clone()
}

/// Dispatch a tool call. Returns the tool's JSON result or an error string.
pub fn call_tool(app: &AppHandle, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "token_count" => {
            let text = str_arg(args, "text").unwrap_or("").to_string();
            let tokenizer = str_arg(args, "tokenizer")
                .unwrap_or("cl100k_base")
                .to_string();
            let n = crate::commands::count_tokens(text, tokenizer)?;
            Ok(json!({ "tokens": n }))
        }
        "keyboard_type" => {
            let text = str_arg(args, "text").ok_or("text is required")?.to_string();
            // paste_plain: never consumes flow one-shots (submit override,
            // anchored delivery) — MCP text must not be redirected.
            crate::clipboard::paste_plain(text, app.clone())?;
            Ok(json!({ "ok": true }))
        }
        "transcription_status" => {
            let stage = match crate::transcription_coordinator::pipeline_stage() {
                crate::transcription_coordinator::STAGE_RECORDING => "recording",
                crate::transcription_coordinator::STAGE_PROCESSING => "processing",
                _ => "idle",
            };
            Ok(json!({ "stage": stage }))
        }
        "jump_slots" => {
            let slots: Vec<Value> = crate::anchor::get_jump_slots(app.clone())
                .into_iter()
                .enumerate()
                .map(|(i, s)| match s {
                    Some(s) => json!({
                        "slot": i,
                        "occupied": true,
                        "app": s.app,
                        "control_class": s.control_class
                    }),
                    None => json!({ "slot": i, "occupied": false }),
                })
                .collect();
            Ok(json!({ "slots": slots }))
        }
        "jump_slot_clear" => {
            let slot = args
                .get("slot")
                .and_then(|v| v.as_u64())
                .ok_or("slot is required")? as usize;
            if slot >= crate::anchor::SLOT_COUNT {
                return Err(format!("slot must be 0..{}", crate::anchor::SLOT_COUNT - 1));
            }
            crate::anchor::clear(app, slot);
            Ok(json!({ "ok": true }))
        }
        "translator_status" => {
            let translator = app
                .try_state::<Arc<crate::managers::translator::TranslatorManager>>()
                .ok_or("translator manager unavailable")?;
            serde_json::to_value(translator.status()).map_err(|e| e.to_string())
        }
        "translator_set_enabled" => {
            let enabled = args
                .get("enabled")
                .and_then(|v| v.as_bool())
                .ok_or("enabled is required")?;
            let _guard = settings_guard();
            let mut settings = crate::settings::get_settings(app);
            settings.translator_enabled = enabled;
            crate::settings::write_settings(app, settings);
            emit_setting_changed(app, "translator_enabled", json!(enabled));
            Ok(json!({ "ok": true }))
        }
        "history_list" => {
            let manager = app
                .try_state::<Arc<HistoryManager>>()
                .ok_or("history manager unavailable")?;
            let entries = block_on(manager.get_history_entries()).map_err(|e| e.to_string())?;
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
            let items: Vec<Value> = entries
                .iter()
                .take(limit)
                .map(|e| {
                    let snippet: String = e.transcription_text.chars().take(160).collect();
                    json!({
                        "id": e.id,
                        "title": e.title,
                        "timestamp": e.timestamp,
                        "saved": e.saved,
                        "snippet": snippet
                    })
                })
                .collect();
            Ok(json!({ "count": items.len(), "entries": items }))
        }
        "history_get" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_i64())
                .ok_or("id is required")?;
            let manager = app
                .try_state::<Arc<HistoryManager>>()
                .ok_or("history manager unavailable")?;
            let entries = block_on(manager.get_history_entries()).map_err(|e| e.to_string())?;
            let entry = entries
                .into_iter()
                .find(|e| e.id == id)
                .ok_or("history entry not found")?;
            let audio_path = manager.get_audio_file_path(&entry.file_name);
            let mut v = serde_json::to_value(&entry).unwrap_or_else(|_| json!({}));
            if let Some(obj) = v.as_object_mut() {
                obj.insert(
                    "audio_path".to_string(),
                    json!(audio_path.to_string_lossy()),
                );
            }
            Ok(v)
        }
        "list_providers" => {
            let settings = settings::get_settings(app);
            let list: Vec<Value> = settings.llm_providers.iter().map(redact_provider).collect();
            Ok(json!({ "providers": list }))
        }
        "set_provider" => set_provider(app, args),
        "list_provider_models" => {
            let provider_id = str_arg(args, "provider_id")
                .ok_or("provider_id is required")?
                .to_string();
            let models = block_on(crate::token_count::list_provider_models(
                app.clone(),
                provider_id,
            ))?;
            Ok(json!({ "models": models }))
        }
        "list_library" => {
            let settings = settings::get_settings(app);
            Ok(serde_json::to_value(&settings.model_test_library).unwrap_or_else(|_| json!({})))
        }
        "save_model_prompt" => {
            let name = str_arg(args, "name").ok_or("name is required")?.to_string();
            let text = str_arg(args, "text").unwrap_or("").to_string();
            let image = str_arg(args, "image_data_url").map(|d| NamedImage {
                name: str_arg(args, "image_name").unwrap_or("image").to_string(),
                data_url: d.to_string(),
            });
            let _guard = settings_guard();
            let mut s = settings::get_settings(app);
            let id = new_id();
            s.model_test_library.model_prompts.push(NamedText {
                id: id.clone(),
                name,
                text,
                image,
            });
            persist_library(app, s);
            Ok(json!({ "id": id }))
        }
        "save_judge_prompt" => {
            let name = str_arg(args, "name").ok_or("name is required")?.to_string();
            let text = str_arg(args, "text").unwrap_or("").to_string();
            let _guard = settings_guard();
            let mut s = settings::get_settings(app);
            let id = new_id();
            s.model_test_library.judge_prompts.push(NamedText {
                id: id.clone(),
                name,
                text,
                image: None,
            });
            persist_library(app, s);
            Ok(json!({ "id": id }))
        }
        "save_preset" => {
            let name = str_arg(args, "name").ok_or("name is required")?.to_string();
            let model_prompt = str_arg(args, "model_prompt").unwrap_or("").to_string();
            let judge_prompt = str_arg(args, "judge_prompt").unwrap_or("").to_string();
            let _guard = settings_guard();
            let mut s = settings::get_settings(app);
            let mp_id = new_id();
            let jp_id = new_id();
            s.model_test_library.model_prompts.push(NamedText {
                id: mp_id.clone(),
                name: name.clone(),
                text: model_prompt.clone(),
                image: None,
            });
            s.model_test_library.judge_prompts.push(NamedText {
                id: jp_id.clone(),
                name: name.clone(),
                text: judge_prompt.clone(),
                image: None,
            });
            let preset_id = new_id();
            s.model_test_library
                .presets
                .push(settings::ModelTestPreset {
                    id: preset_id.clone(),
                    name,
                    model_prompt_id: Some(mp_id),
                    judge_prompt_id: Some(jp_id),
                    model_prompt,
                    judge_prompt,
                });
            persist_library(app, s);
            Ok(json!({ "id": preset_id }))
        }
        "model_test" => model_test(app, args),
        other => Err(format!("unknown tool: {}", other)),
    }
}

fn persist_library(app: &AppHandle, s: AppSettings) {
    let lib = serde_json::to_value(&s.model_test_library).unwrap_or_else(|_| json!({}));
    settings::write_settings(app, s);
    emit_setting_changed(app, "model_test_library", lib);
}

fn set_provider(app: &AppHandle, args: &Value) -> Result<Value, String> {
    let id = str_arg(args, "id").ok_or("id is required")?;
    let _guard = settings_guard();
    let mut settings = settings::get_settings(app);
    let idx = settings
        .llm_providers
        .iter()
        .position(|p| p.id == id)
        .ok_or("provider not found")?;
    let mut p = settings.llm_providers[idx].clone();

    let mut model_changed = false;
    let mut cost_explicit = false;
    if let Some(v) = str_arg(args, "name") {
        p.name = v.to_string();
    }
    if let Some(v) = str_arg(args, "base_url") {
        p.base_url = v.to_string();
    }
    if let Some(v) = str_arg(args, "api_key") {
        // Write-only: accepted here, never returned.
        p.api_key = v.to_string();
    }
    if let Some(v) = str_arg(args, "model") {
        if v != p.model {
            model_changed = true;
        }
        p.model = v.to_string();
    }
    if let Some(v) = str_arg(args, "concurrency_group") {
        p.concurrency_group = v.to_string();
    }
    if let Some(v) = args.get("enabled").and_then(|x| x.as_bool()) {
        p.enabled = v;
    }
    if let Some(v) = args.get("sequential").and_then(|x| x.as_bool()) {
        p.sequential = v;
    }
    if let Some(v) = args.get("persist_price").and_then(|x| x.as_bool()) {
        p.persist_price = v;
    }
    if let Some(v) = args.get("cost_input_per_million").and_then(|x| x.as_f64()) {
        p.cost_input_per_million = v;
        cost_explicit = true;
    }
    if let Some(v) = args.get("cost_output_per_million").and_then(|x| x.as_f64()) {
        p.cost_output_per_million = v;
        cost_explicit = true;
    }

    // Auto-fill cost on model change (mirrors the UI), unless frozen or explicit.
    let auto_kind = matches!(p.kind.as_str(), "gemini" | "anthropic" | "openrouter");
    if model_changed && auto_kind && !p.persist_price && !cost_explicit {
        if let Ok(prices) = block_on(crate::model_testing::fetch_openrouter_model_prices()) {
            if let Some((ci, co)) = pricing::resolve_price(&prices, &p.kind, &p.model) {
                p.cost_input_per_million = ci;
                p.cost_output_per_million = co;
            }
        }
    }

    settings.llm_providers[idx] = p.clone();
    let providers = settings.llm_providers.clone();
    settings::write_settings(app, settings);
    emit_setting_changed(
        app,
        "llm_providers",
        serde_json::to_value(&providers).unwrap_or(Value::Null),
    );
    Ok(redact_provider(&p))
}

fn model_test(app: &AppHandle, args: &Value) -> Result<Value, String> {
    let settings = settings::get_settings(app);

    let mut main_prompt = str_arg(args, "prompt").unwrap_or("").to_string();
    let mut judge_prompt = str_arg(args, "judge_prompt").unwrap_or("").to_string();
    if let Some(pname) = str_arg(args, "preset") {
        if let Some(preset) = settings
            .model_test_library
            .presets
            .iter()
            .find(|p| p.id == pname || p.name.eq_ignore_ascii_case(pname))
        {
            if main_prompt.trim().is_empty() {
                main_prompt = preset_model_text(&settings, preset);
            }
            if judge_prompt.trim().is_empty() {
                judge_prompt = preset_judge_text(&settings, preset);
            }
        } else {
            return Err(format!("preset not found: {}", pname));
        }
    }
    if main_prompt.trim().is_empty() {
        return Err("prompt (or a preset providing one) is required".to_string());
    }

    let run_ids = resolve_ids(&settings, args.get("run").and_then(|v| v.as_array()));
    if run_ids.is_empty() {
        return Err("no matching run providers (select by id or name)".to_string());
    }
    let judge_ids = resolve_ids(&settings, args.get("judge").and_then(|v| v.as_array()));

    let model_temp = args
        .get("model_temperature")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.3);
    let model_thinking_s = str_arg(args, "model_thinking")
        .unwrap_or("auto")
        .to_string();
    let judge_temp = args
        .get("judge_temperature")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.3);
    let judge_thinking_s = str_arg(args, "judge_thinking")
        .unwrap_or("auto")
        .to_string();
    let image = str_arg(args, "image_data_url").map(|s| s.to_string());

    let main_run = block_on(run_model_test(
        app.clone(),
        None,
        main_prompt.clone(),
        run_ids,
        model_temp,
        thinking_from(&model_thinking_s),
        image,
    ))?;

    let ok_count = main_run.outcomes.iter().filter(|o| o.ok).count();
    let mut judge_run_opt = None;
    if !judge_ids.is_empty() && !judge_prompt.trim().is_empty() && ok_count > 0 {
        let (sys, user) =
            report::build_judge_prompt(&judge_prompt, &main_prompt, &main_run.outcomes);
        let jr = block_on(run_model_test(
            app.clone(),
            Some(sys),
            user,
            judge_ids,
            judge_temp,
            thinking_from(&judge_thinking_s),
            None,
        ))?;
        judge_run_opt = Some(jr);
    }

    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let report_md = report::build_report(&report::ReportInput {
        timestamp: &ts,
        main_prompt: &main_prompt,
        model_temperature: model_temp,
        model_thinking: &model_thinking_s,
        main_run: &main_run,
        judge_prompt: if judge_run_opt.is_some() {
            Some(judge_prompt.as_str())
        } else {
            None
        },
        judge_temperature: judge_temp,
        judge_thinking: &judge_thinking_s,
        judge_run: judge_run_opt.as_ref(),
    });

    let mut result = json!({
        "report": report_md,
        "model_run": serde_json::to_value(&main_run).unwrap_or(Value::Null),
        "judge_run": judge_run_opt
            .as_ref()
            .map(|r| serde_json::to_value(r).unwrap_or(Value::Null)),
    });
    if let Some(path) = str_arg(args, "save_path") {
        std::fs::write(path, &report_md).map_err(|e| format!("failed to write {}: {}", path, e))?;
        result["saved_path"] = json!(path);
    }
    Ok(result)
}
