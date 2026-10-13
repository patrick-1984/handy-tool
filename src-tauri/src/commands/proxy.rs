//! Advanced > Proxy: its settings, what Windows is set to, and a test.

use crate::settings::{ProxyAuth, ProxyMode, update_settings};
use serde::Serialize;
use specta::Type;
use std::time::Duration;
use tauri::AppHandle;

/// What the test fetches: small, and on the server updates come from.
const TEST_URL: &str = "https://github.com/";

#[tauri::command]
#[specta::specta]
pub fn change_proxy_mode_setting(app: AppHandle, mode: String) -> Result<(), String> {
    let mode = match mode.as_str() {
        "system" => ProxyMode::System,
        "manual" => ProxyMode::Manual,
        "off" => ProxyMode::Off,
        other => return Err(format!("Unknown proxy mode '{other}'")),
    };
    update_settings(&app, |s| s.proxy_mode = mode);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_proxy_host_setting(app: AppHandle, host: String) -> Result<(), String> {
    update_settings(&app, |s| s.proxy_host = host.trim().to_string());
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_proxy_port_setting(app: AppHandle, port: u16) -> Result<(), String> {
    update_settings(&app, |s| s.proxy_port = port);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_proxy_bypass_setting(app: AppHandle, bypass: String) -> Result<(), String> {
    update_settings(&app, |s| s.proxy_bypass = bypass.trim().to_string());
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_proxy_auth_setting(app: AppHandle, auth: String) -> Result<(), String> {
    let auth = match auth.as_str() {
        "windows" => ProxyAuth::Windows,
        "password" => ProxyAuth::Password,
        other => return Err(format!("Unknown proxy sign-in '{other}'")),
    };
    update_settings(&app, |s| s.proxy_auth = auth);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_proxy_username_setting(app: AppHandle, username: String) -> Result<(), String> {
    update_settings(&app, |s| s.proxy_username = username.trim().to_string());
    Ok(())
}

/// Saves the proxy password encrypted (empty = forget it). It is never sent
/// back to the window.
#[tauri::command]
#[specta::specta]
pub fn set_proxy_password(app: AppHandle, password: String) -> Result<(), String> {
    let stored = crate::net::secret::protect(&password);
    if !password.is_empty() && stored.is_empty() {
        return Err("The password could not be encrypted".to_string());
    }
    update_settings(&app, |s| s.proxy_password = stored);
    Ok(())
}

/// The proxy Windows is set to use (Settings > Network & internet > Proxy).
#[derive(Serialize, Type)]
pub struct SystemProxyInfo {
    /// "proxy.corp:8080" for HTTPS.
    pub proxy: Option<String>,
    /// The setup script's address.
    pub script: Option<String>,
    /// "<local>;*.corp".
    pub bypass: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub fn get_system_proxy() -> SystemProxyInfo {
    let (proxy, script, bypass) = crate::net::system_proxy();
    SystemProxyInfo {
        proxy,
        script,
        bypass,
    }
}

#[derive(Serialize, Type)]
pub struct ProxyTestResult {
    pub ok: bool,
    /// The HTTP status the site answered with.
    pub status: Option<u16>,
    /// "direct" | "bypassed" | "environment" | "proxy" | "error".
    pub route: String,
    /// The proxy's address (route "proxy").
    pub proxy: Option<String>,
    /// How Handy signed in to it: "Negotiate", "NTLM", "Basic"; None = it did
    /// not ask.
    pub sign_in: Option<String>,
    pub error: Option<String>,
}

/// Fetches a page through the proxy settings now in force.
#[tauri::command]
#[specta::specta]
pub async fn test_proxy_connection() -> ProxyTestResult {
    let (route, proxy) = crate::net::route_for(TEST_URL);
    let (route, mut proxy) = (route.to_string(), proxy);
    let mut error = None;
    if route == "error" {
        error = proxy.take();
    }
    let client = crate::net::client_builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30))
        .build();
    let outcome = match client {
        Ok(client) => client.get(TEST_URL).send().await,
        Err(build_error) => {
            return ProxyTestResult {
                ok: false,
                status: None,
                route,
                proxy,
                sign_in: None,
                error: Some(crate::utils::error_chain(&build_error)),
            };
        }
    };
    let via_proxy = route == "proxy";
    match outcome {
        Ok(response) => ProxyTestResult {
            ok: true,
            status: Some(response.status().as_u16()),
            route,
            proxy,
            sign_in: if via_proxy {
                crate::net::last_sign_in()
            } else {
                None
            },
            error,
        },
        Err(request_error) => {
            // The client only learns "the tunnel failed"; the bridge knows why.
            let detail = crate::net::last_tunnel_error()
                .filter(|_| via_proxy)
                .unwrap_or_else(|| crate::utils::error_chain(&request_error));
            log::warn!("Proxy test failed: {detail}");
            ProxyTestResult {
                ok: false,
                status: None,
                route,
                proxy,
                sign_in: None,
                error: Some(detail),
            }
        }
    }
}
