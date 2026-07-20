//! `handy` CLI companion. These subcommands run headlessly: they talk to the
//! running app's localhost server (auto-starting the app if needed) and never
//! launch the GUI. Output goes to the parent console (Windows: via CONOUT$ after
//! AttachConsole) and/or a `--out` file. `handy mcp --stdio` is the stdio MCP
//! bridge for Claude Code, proxying newline-delimited JSON-RPC to `/mcp`.

use crate::cli::Commands;
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::process::Command;
use std::time::Duration;

/// Best-effort attach to the parent console so CLI output is visible (the app is
/// built as a Windows GUI subsystem binary with no console of its own).
#[cfg(windows)]
fn attach_console() {
    unsafe extern "system" {
        fn AttachConsole(dw_process_id: u32) -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}
#[cfg(not(windows))]
fn attach_console() {}

/// Print a line to the parent console. On Windows we write to CONOUT$ directly
/// because the GUI subsystem's cached std handles are not wired to the console.
#[cfg(windows)]
fn out_line(s: &str) {
    use std::fs::OpenOptions;
    if let Ok(mut f) = OpenOptions::new().write(true).open("CONOUT$") {
        let _ = writeln!(f, "{}", s);
    } else {
        println!("{}", s);
    }
}
#[cfg(not(windows))]
fn out_line(s: &str) {
    println!("{}", s);
}

/// Read `mcp_server_enabled` from the persisted settings store (same dir as the
/// sidecar). `None` if unknown (old settings / unreadable) — caller should try.
fn mcp_enabled_in_settings() -> Option<bool> {
    // Portable-aware (T-114): in portable mode the settings store lives in
    // `<exe_dir>\data\` (keyed off current_exe, no AppHandle here), exactly
    // like the sidecar this pairs with — otherwise the CLI would read the
    // OS-profile settings while the app uses the portable ones.
    let path = match crate::portable::portable_data_dir() {
        Some(dir) => dir.join("settings_store.json"),
        None => dirs::config_dir()?
            .join("pr.handy")
            .join("settings_store.json"),
    };
    let txt = std::fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&txt).ok()?;
    v.get("settings")?.get("mcp_server_enabled")?.as_bool()
}

fn read_sidecar() -> Option<(u16, String)> {
    let txt = std::fs::read_to_string(crate::mcp::sidecar_path()).ok()?;
    let v: Value = serde_json::from_str(&txt).ok()?;
    let port = v.get("port")?.as_u64()? as u16;
    let token = v.get("token")?.as_str()?.to_string();
    Some((port, token))
}

fn health_ok(port: u16) -> bool {
    ureq::get(&format!("http://127.0.0.1:{}/health", port))
        .timeout(Duration::from_millis(800))
        .call()
        .is_ok()
}

/// Find the running server (port + token), auto-starting the app if needed.
fn ensure_server() -> Result<(u16, String), String> {
    if let Some((port, token)) = read_sidecar() {
        if health_ok(port) {
            return Ok((port, token));
        }
    }
    // Don't auto-start a hidden app that will never host a server (and would be
    // left orphaned) when the server is explicitly disabled in settings.
    if mcp_enabled_in_settings() == Some(false) {
        return Err(
            "Handy's MCP/CLI server is disabled. Enable it in Advanced → MCP & CLI.".to_string(),
        );
    }
    // Auto-start the app (hidden); it hosts the server if MCP is enabled.
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Command::new(exe)
        .arg("--start-hidden")
        .spawn()
        .map_err(|e| format!("failed to start Handy: {}", e))?;
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(500));
        if let Some((port, token)) = read_sidecar() {
            if health_ok(port) {
                return Ok((port, token));
            }
        }
    }
    Err("Handy's MCP/CLI server is not reachable. Enable it in Advanced → MCP & CLI.".to_string())
}

fn http_post(url: &str, token: &str, body: &str) -> Result<String, String> {
    match ureq::post(url)
        .set("Authorization", &format!("Bearer {}", token))
        .set("Content-Type", "application/json")
        .timeout(Duration::from_secs(900))
        .send_string(body)
    {
        Ok(resp) => resp.into_string().map_err(|e| e.to_string()),
        Err(ureq::Error::Status(code, resp)) => {
            let txt = resp.into_string().unwrap_or_default();
            Err(format!("HTTP {}: {}", code, txt))
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Call a CLI tool on the server and return its `result` JSON.
fn call_tool(port: u16, token: &str, tool: &str, args: &Value) -> Result<Value, String> {
    let url = format!("http://127.0.0.1:{}/cli/{}", port, tool);
    let body = http_post(&url, token, &args.to_string())?;
    let v: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    if v.get("ok").and_then(|x| x.as_bool()) == Some(true) {
        Ok(v.get("result").cloned().unwrap_or(Value::Null))
    } else {
        Err(v
            .get("error")
            .and_then(|x| x.as_str())
            .unwrap_or("unknown error")
            .to_string())
    }
}

fn read_text(inline: &Option<String>, file: &Option<String>) -> Result<Option<String>, String> {
    if let Some(t) = inline {
        return Ok(Some(t.clone()));
    }
    if let Some(p) = file {
        return std::fs::read_to_string(p)
            .map(Some)
            .map_err(|e| format!("failed to read {}: {}", p, e));
    }
    Ok(None)
}

fn image_to_data_url(path: &str) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("failed to read {}: {}", path, e))?;
    let mime = match path.rsplit('.').next().map(|s| s.to_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    };
    let b64 = crate::managers::openrouter_transcription::base64_encode(&bytes);
    Ok(format!("data:{};base64,{}", mime, b64))
}

fn csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

/// Run a CLI command. Returns the process exit code.
pub fn run(cmd: Commands) -> i32 {
    // The stdio MCP bridge must use the real (piped) stdin/stdout, not a console.
    if let Commands::Mcp { stdio: true } = &cmd {
        return run_mcp_stdio();
    }
    attach_console();

    match dispatch(cmd) {
        Ok(()) => 0,
        Err(e) => {
            out_line(&format!("error: {}", e));
            1
        }
    }
}

fn dispatch(cmd: Commands) -> Result<(), String> {
    match cmd {
        Commands::InstallCli => {
            let path = crate::install_cli_binary()?;
            out_line(&format!("Installed CLI to {}", path));
            Ok(())
        }
        Commands::Mcp { stdio: false } => {
            Err("use `handy mcp --stdio` (stdio transport) for Claude Code".to_string())
        }
        Commands::Mcp { stdio: true } => unreachable!(),
        other => dispatch_remote(other),
    }
}

/// Commands that need the running server.
fn dispatch_remote(cmd: Commands) -> Result<(), String> {
    let (port, token) = ensure_server()?;
    match cmd {
        Commands::TokenCount {
            text,
            file,
            tokenizer,
        } => {
            let text = read_text(&text, &file)?.ok_or("provide text or --file")?;
            let r = call_tool(
                port,
                &token,
                "token_count",
                &json!({ "text": text, "tokenizer": tokenizer }),
            )?;
            out_line(&r.get("tokens").map(|t| t.to_string()).unwrap_or_default());
            Ok(())
        }
        Commands::Type { text, file } => {
            let text = read_text(&text, &file)?.ok_or("provide text or --file")?;
            call_tool(port, &token, "keyboard_type", &json!({ "text": text }))?;
            out_line("typed");
            Ok(())
        }
        Commands::HistoryList { limit } => {
            let mut args = json!({});
            if let Some(l) = limit {
                args["limit"] = json!(l);
            }
            let r = call_tool(port, &token, "history_list", &args)?;
            out_line(&serde_json::to_string_pretty(&r).unwrap_or_default());
            Ok(())
        }
        Commands::HistoryGet { id } => {
            let r = call_tool(port, &token, "history_get", &json!({ "id": id }))?;
            out_line(&serde_json::to_string_pretty(&r).unwrap_or_default());
            Ok(())
        }
        Commands::ProvidersList => {
            let r = call_tool(port, &token, "list_providers", &json!({}))?;
            out_line(&serde_json::to_string_pretty(&r).unwrap_or_default());
            Ok(())
        }
        Commands::ProvidersModels { id } => {
            let r = call_tool(
                port,
                &token,
                "list_provider_models",
                &json!({ "provider_id": id }),
            )?;
            out_line(&serde_json::to_string_pretty(&r).unwrap_or_default());
            Ok(())
        }
        Commands::ProvidersSet {
            id,
            model,
            api_key,
            name,
            base_url,
            enabled,
            sequential,
            concurrency_group,
            persist_price,
            cost_input,
            cost_output,
        } => {
            let mut args = json!({ "id": id });
            let obj = args.as_object_mut().unwrap();
            if let Some(v) = model {
                obj.insert("model".into(), json!(v));
            }
            if let Some(v) = api_key {
                obj.insert("api_key".into(), json!(v));
            }
            if let Some(v) = name {
                obj.insert("name".into(), json!(v));
            }
            if let Some(v) = base_url {
                obj.insert("base_url".into(), json!(v));
            }
            if let Some(v) = enabled {
                obj.insert("enabled".into(), json!(v));
            }
            if let Some(v) = sequential {
                obj.insert("sequential".into(), json!(v));
            }
            if let Some(v) = concurrency_group {
                obj.insert("concurrency_group".into(), json!(v));
            }
            if let Some(v) = persist_price {
                obj.insert("persist_price".into(), json!(v));
            }
            if let Some(v) = cost_input {
                obj.insert("cost_input_per_million".into(), json!(v));
            }
            if let Some(v) = cost_output {
                obj.insert("cost_output_per_million".into(), json!(v));
            }
            let r = call_tool(port, &token, "set_provider", &args)?;
            out_line(&serde_json::to_string_pretty(&r).unwrap_or_default());
            Ok(())
        }
        Commands::ModelTest {
            run,
            judge,
            prompt,
            prompt_file,
            judge_prompt,
            judge_prompt_file,
            preset,
            model_temp,
            model_thinking,
            judge_temp,
            judge_thinking,
            image,
            out,
            json: as_json,
        } => {
            let mut args = json!({
                "run": csv(&run),
                "model_temperature": model_temp,
                "model_thinking": model_thinking,
                "judge_temperature": judge_temp,
                "judge_thinking": judge_thinking,
            });
            let obj = args.as_object_mut().unwrap();
            if let Some(j) = judge {
                obj.insert("judge".into(), json!(csv(&j)));
            }
            if let Some(p) = read_text(&prompt, &prompt_file)? {
                obj.insert("prompt".into(), json!(p));
            }
            if let Some(p) = read_text(&judge_prompt, &judge_prompt_file)? {
                obj.insert("judge_prompt".into(), json!(p));
            }
            if let Some(p) = preset {
                obj.insert("preset".into(), json!(p));
            }
            if let Some(img) = image {
                obj.insert("image_data_url".into(), json!(image_to_data_url(&img)?));
            }
            let r = call_tool(port, &token, "model_test", &args)?;
            if as_json {
                out_line(&serde_json::to_string_pretty(&r).unwrap_or_default());
            } else {
                let report = r.get("report").and_then(|x| x.as_str()).unwrap_or("");
                if let Some(path) = &out {
                    std::fs::write(path, report)
                        .map_err(|e| format!("failed to write {}: {}", path, e))?;
                    out_line(&format!("Report written to {}", path));
                } else {
                    out_line(report);
                }
            }
            Ok(())
        }
        Commands::InstallCli | Commands::Mcp { .. } => unreachable!(),
    }
}

/// stdio MCP bridge: read newline-delimited JSON-RPC from stdin, POST each to the
/// app's `/mcp`, and write JSON responses to stdout (notifications produce none).
fn run_mcp_stdio() -> i32 {
    let (port, token) = match ensure_server() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", e);
            return 1;
        }
    };
    let url = format!("http://127.0.0.1:{}/mcp", port);
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        match http_post(&url, &token, &line) {
            Ok(resp) => {
                // Notifications return an empty body (HTTP 202); emit nothing.
                if !resp.trim().is_empty() {
                    if writeln!(stdout, "{}", resp).is_err() {
                        break;
                    }
                    let _ = stdout.flush();
                }
            }
            Err(e) => {
                // Echo the original request id; emit nothing for a failed
                // notification (no id), per JSON-RPC.
                if let Some(id) = serde_json::from_str::<Value>(&line)
                    .ok()
                    .and_then(|m| m.get("id").cloned())
                {
                    let err = json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32000, "message": e }
                    });
                    if writeln!(stdout, "{}", err).is_err() {
                        break;
                    }
                    let _ = stdout.flush();
                }
            }
        }
    }
    0
}
