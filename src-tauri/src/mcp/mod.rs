//! Localhost MCP + CLI server hosted by the running Handy app.
//!
//! One `tiny_http` server on 127.0.0.1 exposes a shared set of tool handlers
//! (see [`tools`]) behind three routes:
//! - `POST /mcp`        — MCP JSON-RPC (Streamable HTTP) for the Claude app, and
//!                        the target of the `handy mcp --stdio` bridge.
//! - `POST /cli/<tool>` — plain JSON API used by the `handy` CLI.
//! - `GET  /health`     — unauthenticated liveness used for discovery.
//!
//! All authed routes require `Authorization: Bearer <token>`. On start the
//! server writes a `handy-mcp.json` sidecar (port + token) so the CLI/bridge can
//! discover it without knowing the settings-store format.

pub mod pricing;
pub mod report;
pub mod tools;

use once_cell::sync::Lazy;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::AppHandle;
use tiny_http::{Header, Method, Response, Server};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_PROTOCOL: &str = "2025-06-18";

// ---------------------------------------------------------------------------
// Tauri commands (MCP/CLI tab in Advanced settings)
// ---------------------------------------------------------------------------

/// Status of the localhost MCP/CLI server, for the settings UI.
#[derive(serde::Serialize, specta::Type)]
pub struct McpStatus {
    pub enabled: bool,
    pub running: bool,
    pub port: u16,
    /// Bearer token for configuring an MCP client (localhost only).
    pub token: String,
    pub cli_installed: bool,
    pub cli_path: String,
}

fn build_status(app: &AppHandle) -> McpStatus {
    let settings = crate::settings::get_settings(app);
    let (running, _) = status();
    let cli_path = crate::cli_install_path().unwrap_or_default();
    McpStatus {
        enabled: settings.mcp_server_enabled,
        running,
        port: settings.mcp_server_port,
        token: settings.mcp_server_token,
        cli_installed: cli_path.exists(),
        cli_path: cli_path.to_string_lossy().to_string(),
    }
}

/// Apply the current settings: (re)start the server if enabled, else stop it.
fn apply(app: &AppHandle) -> Result<McpStatus, String> {
    let mut settings = crate::settings::get_settings(app);
    if settings.mcp_server_enabled {
        if settings.mcp_server_token.is_empty() {
            settings.mcp_server_token = uuid::Uuid::new_v4().to_string();
            crate::settings::write_settings(app, settings.clone());
        }
        start(
            app.clone(),
            settings.mcp_server_port,
            settings.mcp_server_token.clone(),
        )?;
    } else {
        stop();
    }
    Ok(build_status(app))
}

#[tauri::command]
#[specta::specta]
pub fn get_mcp_status(app: AppHandle) -> McpStatus {
    build_status(&app)
}

#[tauri::command]
#[specta::specta]
pub fn set_mcp_enabled(app: AppHandle, enabled: bool) -> Result<McpStatus, String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.mcp_server_enabled = enabled;
    if enabled && settings.mcp_server_token.is_empty() {
        settings.mcp_server_token = uuid::Uuid::new_v4().to_string();
    }
    crate::settings::write_settings(&app, settings);
    apply(&app)
}

#[tauri::command]
#[specta::specta]
pub fn change_mcp_port(app: AppHandle, port: u16) -> Result<McpStatus, String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.mcp_server_port = port;
    crate::settings::write_settings(&app, settings);
    apply(&app)
}

#[tauri::command]
#[specta::specta]
pub fn regenerate_mcp_token(app: AppHandle) -> Result<McpStatus, String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.mcp_server_token = uuid::Uuid::new_v4().to_string();
    crate::settings::write_settings(&app, settings);
    apply(&app)
}

#[tauri::command]
#[specta::specta]
pub fn install_cli(_app: AppHandle) -> Result<String, String> {
    // `install_cli_binary()` itself refuses in portable mode (T-114 gap #3)
    // — it would write outside the portable folder, to a machine/user PATH
    // location (%LOCALAPPDATA%\Microsoft\WindowsApps on Windows) — and
    // returns an informative error the Settings UI surfaces.
    crate::install_cli_binary()
}

struct ServerState {
    running: Arc<AtomicBool>,
    port: u16,
    server: Arc<Server>,
    handle: Option<std::thread::JoinHandle<()>>,
}

static SERVER: Lazy<Mutex<Option<ServerState>>> = Lazy::new(|| Mutex::new(None));

/// Path of the discovery sidecar, computed identically by the app and the
/// CLI. `handy.exe` is a single binary serving both, so
/// `crate::portable::portable_data_dir()` (keyed off `current_exe()`, no
/// `AppHandle` needed/available here) resolves the same way for either
/// caller: portable mode puts the sidecar in `<exe_dir>\data\` alongside
/// settings/history/models instead of the OS profile dir (T-114).
pub fn sidecar_path() -> PathBuf {
    if let Some(dir) = crate::portable::portable_data_dir() {
        return dir.join("handy-mcp.json");
    }
    let base = dirs::config_dir().unwrap_or_else(std::env::temp_dir);
    base.join("pr.handy").join("handy-mcp.json")
}

/// Whether the server is currently running, and on which port.
pub fn status() -> (bool, u16) {
    let guard = SERVER.lock().ok();
    match guard.as_ref().and_then(|g| g.as_ref()) {
        Some(s) => (s.running.load(Ordering::SeqCst), s.port),
        None => (false, 0),
    }
}

/// Start (or restart) the server on `port` with bearer `token`. Idempotent: an
/// existing server is stopped first.
pub fn start(app: AppHandle, port: u16, token: String) -> Result<(), String> {
    stop();
    if port < 1024 {
        return Err(format!(
            "Port {} is not allowed; choose a port in 1024–65535",
            port
        ));
    }
    let server = Arc::new(
        Server::http(("127.0.0.1", port))
            .map_err(|e| format!("Failed to bind 127.0.0.1:{}: {}", port, e))?,
    );
    let running = Arc::new(AtomicBool::new(true));

    // Discovery sidecar.
    let sidecar = sidecar_path();
    if let Some(parent) = sidecar.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(
        &sidecar,
        serde_json::to_string_pretty(&json!({
            "port": port,
            "token": token,
            "pid": std::process::id(),
        }))
        .unwrap_or_default(),
    );

    let running_thread = running.clone();
    let token_thread = token.clone();
    let server_thread = server.clone();
    let handle = std::thread::Builder::new()
        .name("handy-mcp-server".into())
        .spawn(move || {
            while running_thread.load(Ordering::SeqCst) {
                match server_thread.recv_timeout(Duration::from_millis(500)) {
                    Ok(Some(request)) => {
                        let app = app.clone();
                        let token = token_thread.clone();
                        // Handle each request on its own thread so a long
                        // model_test doesn't block the accept loop / other tools.
                        let _ = std::thread::Builder::new()
                            .name("handy-mcp-req".into())
                            .spawn(move || handle_request(&app, &token, request));
                    }
                    Ok(None) => continue,
                    Err(_) => break,
                }
            }
        })
        .map_err(|e| format!("Failed to spawn server thread: {}", e))?;

    if let Ok(mut guard) = SERVER.lock() {
        *guard = Some(ServerState {
            running,
            port,
            server,
            handle: Some(handle),
        });
    }
    log::info!("MCP/CLI server listening on 127.0.0.1:{}", port);
    Ok(())
}

/// Stop the server if running and remove the discovery sidecar. Unblocks and
/// joins the accept thread so the listening socket is fully released before any
/// subsequent rebind on the same port (avoids EADDRINUSE on restart).
pub fn stop() {
    let state = SERVER.lock().ok().and_then(|mut g| g.take());
    if let Some(mut state) = state {
        state.running.store(false, Ordering::SeqCst);
        state.server.unblock();
        if let Some(handle) = state.handle.take() {
            let _ = handle.join();
        }
        log::info!("MCP/CLI server stopped");
        // `state` (and its Arc<Server>) drops here, closing the socket.
    }
    let _ = std::fs::remove_file(sidecar_path());
}

fn json_response(status: u16, body: String) -> Response<std::io::Cursor<Vec<u8>>> {
    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
    Response::from_string(body)
        .with_status_code(status)
        .with_header(header)
}

fn authorized(request: &tiny_http::Request, token: &str) -> bool {
    // `equiv` requires a 'static str, so the header name is a literal.
    let auth = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .map(|h| h.value.as_str());
    match auth {
        Some(v) => v.strip_prefix("Bearer ").map(str::trim) == Some(token),
        None => false,
    }
}

fn handle_request(app: &AppHandle, token: &str, mut request: tiny_http::Request) {
    let method = request.method().clone();
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or("").to_string();

    // Health is unauthenticated so the CLI can probe liveness.
    if method == Method::Get && path == "/health" {
        let _ = request.respond(json_response(
            200,
            json!({ "ok": true, "version": VERSION }).to_string(),
        ));
        return;
    }

    if !authorized(&request, token) {
        let _ = request.respond(json_response(
            401,
            json!({ "error": "unauthorized" }).to_string(),
        ));
        return;
    }

    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);

    if method == Method::Post && path == "/mcp" {
        let (status, resp) = handle_mcp(app, &body);
        let _ = request.respond(json_response(status, resp));
        return;
    }

    if method == Method::Post {
        if let Some(tool) = path.strip_prefix("/cli/") {
            let args: Value = serde_json::from_str(&body).unwrap_or(json!({}));
            let (status, resp) = match tools::call_tool(app, tool, &args) {
                Ok(v) => (200, json!({ "ok": true, "result": v })),
                Err(e) => (200, json!({ "ok": false, "error": e })),
            };
            let _ = request.respond(json_response(status, resp.to_string()));
            return;
        }
    }

    let _ = request.respond(json_response(
        404,
        json!({ "error": "not found" }).to_string(),
    ));
}

/// Handle the MCP Streamable-HTTP body. Returns (http_status, json_body).
/// A notification (no `id`) yields HTTP 202 with an empty body.
fn handle_mcp(app: &AppHandle, body: &str) -> (u16, String) {
    let parsed: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            return (
                400,
                json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": { "code": -32700, "message": format!("parse error: {}", e) }
                })
                .to_string(),
            );
        }
    };

    // Batch support: an array of messages.
    if let Some(arr) = parsed.as_array() {
        let responses: Vec<Value> = arr
            .iter()
            .filter_map(|m| handle_mcp_message(app, m))
            .collect();
        if responses.is_empty() {
            return (202, String::new());
        }
        return (200, Value::Array(responses).to_string());
    }

    match handle_mcp_message(app, &parsed) {
        Some(resp) => (200, resp.to_string()),
        None => (202, String::new()),
    }
}

/// Handle one JSON-RPC message. Returns `None` for notifications (no `id`).
fn handle_mcp_message(app: &AppHandle, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(json!({}));

    // Notifications carry no id and expect no response.
    if id.is_none() {
        return None;
    }
    let id = id.unwrap();

    let result: Result<Value, (i64, String)> = match method {
        "initialize" => {
            let protocol = params
                .get("protocolVersion")
                .and_then(|v| v.as_str())
                .unwrap_or(DEFAULT_PROTOCOL)
                .to_string();
            Ok(json!({
                "protocolVersion": protocol,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "handy", "version": VERSION }
            }))
        }
        "tools/list" => Ok(json!({ "tools": tools::tool_specs() })),
        "tools/call" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
            match tools::call_tool(app, name, &arguments) {
                Ok(v) => {
                    let text = serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string());
                    Ok(json!({
                        "content": [{ "type": "text", "text": text }],
                        "isError": false
                    }))
                }
                // Tool execution errors are reported in-band per the MCP spec.
                Err(e) => Ok(json!({
                    "content": [{ "type": "text", "text": e }],
                    "isError": true
                })),
            }
        }
        "ping" => Ok(json!({})),
        other => Err((-32601, format!("method not found: {}", other))),
    };

    Some(match result {
        Ok(v) => json!({ "jsonrpc": "2.0", "id": id, "result": v }),
        Err((code, message)) => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
        }
    })
}
