//! Outgoing HTTP(S) and the proxy the user chose (Advanced > Proxy).
//!
//! Every HTTP client Handy builds for the internet comes from here
//! (`client_builder`, `client`, `configure`, `ureq_agent`), so model downloads,
//! update checks, LLM providers and the online transcription engines all take
//! the same route.
//!
//! On Windows a proxy that wants a sign-in is common: browsers answer it with
//! the Windows account (NTLM / Negotiate) or a saved password, while reqwest and
//! ureq can do neither (they read the proxy's address, never how to sign in to
//! it). So on Windows, when a proxy is in play, HTTPS goes through a small
//! bridge on 127.0.0.1 (`bridge.rs`): it picks the proxy for each site the way
//! Windows does (static proxy, bypass list, PAC script), signs in to it (the
//! Windows account through SSPI, or the user name and password from the
//! settings) and then just relays the encrypted bytes - TLS stays end to end
//! between Handy's client and the site.
//!
//! Elsewhere (macOS, Linux) nothing changes: reqwest's own proxy handling
//! (environment variables, system settings) as before.

#[cfg(windows)]
mod bridge;
#[cfg(windows)]
mod sspi;
#[cfg(windows)]
mod system;

use crate::settings::{AppSettings, ProxyAuth, ProxyMode};
use std::sync::{OnceLock, RwLock};

/// What the user set, with the password readable (it is stored encrypted).
#[derive(Clone, Default, PartialEq)]
pub struct ProxyConfig {
    pub mode: ProxyMode,
    pub host: String,
    pub port: u16,
    pub bypass: String,
    pub auth: ProxyAuth,
    pub username: String,
    pub password: String,
    /// The password as stored, so it is decrypted only when it changes.
    stored_password: String,
}

/// Never prints the password.
impl std::fmt::Debug for ProxyConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProxyConfig")
            .field("mode", &self.mode)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("bypass", &self.bypass)
            .field("auth", &self.auth)
            .field("username", &self.username)
            .field("password", &(!self.password.is_empty()).then_some("***"))
            .finish()
    }
}

impl ProxyConfig {
    fn from_settings(settings: &AppSettings, previous: &ProxyConfig) -> Self {
        let mut host = clean_host(&settings.proxy_host);
        let mut port = settings.proxy_port;
        // "proxy.corp:3128" typed into the address field: its port wins.
        if let Some((name, typed)) = host.rsplit_once(':')
            && !name.contains(':')
            && let Ok(typed) = typed.parse::<u16>()
        {
            port = typed;
            host = name.to_string();
        }
        ProxyConfig {
            mode: settings.proxy_mode,
            host,
            port,
            bypass: settings.proxy_bypass.clone(),
            auth: settings.proxy_auth,
            username: settings.proxy_username.trim().to_string(),
            password: if settings.proxy_password == previous.stored_password {
                previous.password.clone()
            } else {
                secret::reveal(&settings.proxy_password)
            },
            stored_password: settings.proxy_password.clone(),
        }
    }

    /// The user name and password to sign in with, if the user chose that.
    pub fn credentials(&self) -> Option<(&str, &str)> {
        (self.auth == ProxyAuth::Password && !self.username.is_empty())
            .then_some((self.username.as_str(), self.password.as_str()))
    }
}

fn config_cell() -> &'static RwLock<ProxyConfig> {
    static CONFIG: OnceLock<RwLock<ProxyConfig>> = OnceLock::new();
    CONFIG.get_or_init(|| RwLock::new(ProxyConfig::default()))
}

/// The proxy settings now in force (a copy).
pub fn current() -> ProxyConfig {
    config_cell()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// Takes the proxy settings from `settings`. Called at startup and on every
/// settings write, so a change applies to the next request.
pub fn apply_settings(settings: &AppSettings) {
    let mut config = config_cell()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let next = ProxyConfig::from_settings(settings, &config);
    if *config != next {
        log::info!(
            "Proxy: {:?}{}",
            next.mode,
            if next.mode == ProxyMode::Manual {
                format!(" {}:{}", next.host, next.port)
            } else {
                String::new()
            }
        );
        *config = next;
    }
}

/// "http://proxy.corp:8080/" or " proxy.corp " -> "proxy.corp".
fn clean_host(host: &str) -> String {
    let host = host.trim();
    let host = host
        .strip_prefix("http://")
        .or_else(|| host.strip_prefix("https://"))
        .unwrap_or(host);
    host.trim_end_matches('/').to_string()
}

/// localhost, 127.x, ::1: local servers (FLM, Ollama, LM Studio) never go
/// through a proxy.
pub fn is_loopback_host(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<std::net::IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

/// How a request leaves Handy.
#[derive(Debug, Clone, PartialEq)]
enum Route {
    /// reqwest's own handling (environment variables, the Windows proxy
    /// without sign-in): what Handy always did.
    Default,
    /// Straight to the site.
    Direct,
    /// HTTPS through the bridge on 127.0.0.1 at this port, with this token.
    #[allow(dead_code)]
    Bridge { port: u16, token: String },
}

#[cfg(not(windows))]
fn route() -> Route {
    Route::Default
}

#[cfg(windows)]
fn route() -> Route {
    let config = current();
    match config.mode {
        ProxyMode::Off => return Route::Direct,
        ProxyMode::Manual if !config.host.is_empty() && config.port != 0 => {}
        // Manual without an address: as if it were the Windows setting.
        ProxyMode::Manual | ProxyMode::System => {
            // HTTPS_PROXY & co. still win, as they always did.
            if proxy_env_set() || !system::current().uses_proxy() {
                return Route::Default;
            }
        }
    }
    match bridge::ensure() {
        Some((port, token)) => Route::Bridge { port, token },
        None => Route::Default,
    }
}

#[cfg(windows)]
fn proxy_env_set() -> bool {
    [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ]
    .iter()
    .any(|name| std::env::var(name).is_ok_and(|value| !value.trim().is_empty()))
}

/// Sends a request through the bridge: HTTPS to anywhere but this PC. Plain
/// HTTP goes direct (the bridge only relays tunnels); Handy's own internet
/// traffic is all HTTPS. With Windows' settings a NO_PROXY variable still
/// sends its sites direct, as reqwest did on its own.
fn through_bridge(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    if url.scheme() != "https" || is_loopback_host(host) {
        return false;
    }
    let config = current();
    let manual = config.mode == ProxyMode::Manual && !config.host.is_empty() && config.port != 0;
    manual || !env_no_proxy(host)
}

fn env_no_proxy(host: &str) -> bool {
    let list = std::env::var("NO_PROXY")
        .or_else(|_| std::env::var("no_proxy"))
        .unwrap_or_default();
    no_proxy_matches(host, &list)
}

/// NO_PROXY: "*", or names and IPs separated by commas, each covering itself
/// and everything under it ("corp.example", ".corp.example", "*.corp.example").
fn no_proxy_matches(host: &str, list: &str) -> bool {
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    list.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .any(|entry| {
            if entry == "*" {
                return true;
            }
            let entry = entry
                .trim_start_matches("*.")
                .trim_start_matches('.')
                .to_ascii_lowercase();
            let entry = match entry.rsplit_once(':') {
                Some((name, port))
                    if !name.contains(':') && port.chars().all(|c| c.is_ascii_digit()) =>
                {
                    name.to_string()
                }
                _ => entry,
            };
            let entry = entry.trim_start_matches('[').trim_end_matches(']');
            if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                return ip_matches(ip, entry);
            }
            host == entry || host.ends_with(&format!(".{entry}"))
        })
}

/// "10.0.0.0/8", "fd00::/8" or one address.
fn ip_matches(ip: std::net::IpAddr, entry: &str) -> bool {
    use std::net::IpAddr;
    let (address, bits) = match entry.split_once('/') {
        Some((address, bits)) => match bits.parse::<u32>() {
            Ok(bits) => (address, Some(bits)),
            Err(_) => return false,
        },
        None => (entry, None),
    };
    let Ok(network) = address.parse::<IpAddr>() else {
        return false;
    };
    match (ip, network) {
        (IpAddr::V4(ip), IpAddr::V4(network)) => {
            let bits = bits.unwrap_or(32).min(32);
            let mask = if bits == 0 {
                0
            } else {
                u32::MAX << (32 - bits)
            };
            u32::from(ip) & mask == u32::from(network) & mask
        }
        (IpAddr::V6(ip), IpAddr::V6(network)) => {
            let bits = bits.unwrap_or(128).min(128);
            let mask = if bits == 0 {
                0
            } else {
                u128::MAX << (128 - bits)
            };
            u128::from(ip) & mask == u128::from(network) & mask
        }
        _ => false,
    }
}

/// Points a reqwest client at the chosen proxy.
pub fn configure(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    match route() {
        Route::Default => builder,
        Route::Direct => builder.no_proxy(),
        Route::Bridge { port, token } => {
            let bridge = format!("http://127.0.0.1:{port}");
            let proxy =
                reqwest::Proxy::custom(move |url| through_bridge(url).then(|| bridge.clone()))
                    .basic_auth("handy", &token);
            builder.proxy(proxy)
        }
    }
}

/// `reqwest::Client::builder()` with the chosen proxy.
pub fn client_builder() -> reqwest::ClientBuilder {
    configure(reqwest::Client::builder())
}

/// `reqwest::Client::new()` with the chosen proxy.
pub fn client() -> reqwest::Client {
    client_builder().build().unwrap_or_else(|error| {
        log::warn!(
            "HTTP client with the proxy settings failed ({}), using the default one",
            crate::utils::error_chain(&error)
        );
        reqwest::Client::new()
    })
}

/// A ureq agent for `url` with the chosen proxy.
pub fn ureq_agent(url: &str) -> ureq::Agent {
    let builder = ureq::AgentBuilder::new();
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return builder.build();
    };
    match route() {
        Route::Bridge { port, token } if through_bridge(&parsed) => {
            match ureq::Proxy::new(format!("http://handy:{token}@127.0.0.1:{port}")) {
                Ok(proxy) => builder.proxy(proxy).build(),
                Err(_) => builder.build(),
            }
        }
        _ => builder.build(),
    }
}

/// Where a request to `url` goes: ("direct" | "bypassed" | "environment" |
/// "proxy" | "error", the proxy's address or the error).
#[cfg(windows)]
pub fn route_for(url: &str) -> (&'static str, Option<String>) {
    let host = reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_default();
    match route() {
        Route::Default if proxy_env_set() => ("environment", None),
        Route::Default | Route::Direct => ("direct", None),
        Route::Bridge { .. } if !reqwest::Url::parse(url).is_ok_and(|u| through_bridge(&u)) => {
            ("bypassed", None)
        }
        Route::Bridge { .. } => match bridge::upstream_for(&host, 443) {
            Ok(hops) => match hops.first() {
                Some(system::Hop::Proxy(proxy_host, proxy_port)) => {
                    ("proxy", Some(format!("{proxy_host}:{proxy_port}")))
                }
                _ => ("bypassed", None),
            },
            Err(error) => ("error", Some(error)),
        },
    }
}

#[cfg(not(windows))]
pub fn route_for(_url: &str) -> (&'static str, Option<String>) {
    ("direct", None)
}

/// The proxy Windows is set to use: (static proxy for HTTPS, PAC script
/// address, bypass list), each None when not set.
#[cfg(windows)]
pub fn system_proxy() -> (Option<String>, Option<String>, Option<String>) {
    let windows = system::current();
    let some = |s: &str| (!s.is_empty()).then(|| s.to_string());
    (
        windows
            .static_proxy_for_https()
            .map(|(host, port)| format!("{host}:{port}")),
        some(&windows.pac_url),
        some(&windows.bypass),
    )
}

#[cfg(not(windows))]
pub fn system_proxy() -> (Option<String>, Option<String>, Option<String>) {
    (None, None, None)
}

/// How the last tunnel signed in to the proxy ("NTLM", "Negotiate", "Basic").
#[cfg(windows)]
pub fn last_sign_in() -> Option<String> {
    bridge::last_sign_in()
}

#[cfg(not(windows))]
pub fn last_sign_in() -> Option<String> {
    None
}

/// Why the last tunnel failed, if it did (the client only sees "502").
#[cfg(windows)]
pub fn last_tunnel_error() -> Option<String> {
    bridge::last_error()
}

#[cfg(not(windows))]
pub fn last_tunnel_error() -> Option<String> {
    None
}

/// Does `host` match a bypass list ("<local>; *.corp; 10.*", commas or
/// semicolons or spaces between entries, `*` as a wildcard, case-insensitive)?
/// `<local>` is a host name without a dot. The port is ignored.
pub fn bypassed(host: &str, list: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    list.split([';', ',', ' ', '\n', '\r', '\t'])
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .any(|entry| {
            if entry.eq_ignore_ascii_case("<local>") {
                return !host.contains('.') && !host.contains(':');
            }
            let entry = entry
                .strip_prefix("http://")
                .or_else(|| entry.strip_prefix("https://"))
                .unwrap_or(entry);
            // "host:port" entries: compare the host part only.
            let entry = match entry.rsplit_once(':') {
                Some((name, port))
                    if !name.contains(':') && port.chars().all(|c| c.is_ascii_digit()) =>
                {
                    name
                }
                _ => entry,
            };
            // ".corp.local" means the domain and everything under it.
            if let Some(domain) = entry.strip_prefix('.') {
                let host = host.to_ascii_lowercase();
                let domain = domain.to_ascii_lowercase();
                return host == domain || host.ends_with(&format!(".{domain}"));
            }
            wildcard_match(&entry.to_ascii_lowercase(), &host.to_ascii_lowercase())
        })
}

/// `*` matches any run of characters, everything else itself.
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while t < text.len() {
        if p < pattern.len() && pattern[p] != '*' && pattern[p] == text[t] {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            mark = t;
            p += 1;
        } else if let Some(s) = star {
            p = s + 1;
            mark += 1;
            t = mark;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

/// The proxy password at rest: on Windows encrypted for this Windows account
/// (DPAPI) and stored as base64, so the settings file (and a backup of it) does
/// not hold it readable. Elsewhere it is kept as typed.
pub mod secret {
    #[cfg(windows)]
    const PREFIX: &str = "dpapi:";

    /// Text for the settings file. Empty stays empty.
    pub fn protect(password: &str) -> String {
        if password.is_empty() {
            return String::new();
        }
        #[cfg(windows)]
        {
            match dpapi::protect(password.as_bytes()) {
                Some(blob) => format!(
                    "{PREFIX}{}",
                    crate::managers::openrouter_transcription::base64_encode(&blob)
                ),
                None => {
                    log::warn!("Proxy: could not encrypt the password; it is not saved");
                    String::new()
                }
            }
        }
        #[cfg(not(windows))]
        {
            password.to_string()
        }
    }

    /// The password from the settings file ("" if it cannot be read, e.g. a
    /// backup restored under another Windows account).
    pub fn reveal(stored: &str) -> String {
        if stored.is_empty() {
            return String::new();
        }
        #[cfg(windows)]
        {
            let Some(encoded) = stored.strip_prefix(PREFIX) else {
                return String::new();
            };
            let Some(blob) = base64_decode(encoded) else {
                return String::new();
            };
            match dpapi::unprotect(&blob).and_then(|bytes| String::from_utf8(bytes).ok()) {
                Some(password) => password,
                None => {
                    log::warn!(
                        "Proxy: the saved password cannot be read under this Windows account; enter it again"
                    );
                    String::new()
                }
            }
        }
        #[cfg(not(windows))]
        {
            stored.to_string()
        }
    }

    #[cfg(windows)]
    pub(super) fn base64_decode(text: &str) -> Option<Vec<u8>> {
        fn value(c: u8) -> Option<u32> {
            Some(match c {
                b'A'..=b'Z' => (c - b'A') as u32,
                b'a'..=b'z' => (c - b'a' + 26) as u32,
                b'0'..=b'9' => (c - b'0' + 52) as u32,
                b'+' => 62,
                b'/' => 63,
                _ => return None,
            })
        }
        let bytes: Vec<u8> = text
            .bytes()
            .filter(|c| !c.is_ascii_whitespace() && *c != b'=')
            .collect();
        let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
        for chunk in bytes.chunks(4) {
            if chunk.len() == 1 {
                return None;
            }
            let mut acc = 0u32;
            for (i, c) in chunk.iter().enumerate() {
                acc |= value(*c)? << (18 - 6 * i);
            }
            out.push((acc >> 16) as u8);
            if chunk.len() > 2 {
                out.push((acc >> 8) as u8);
            }
            if chunk.len() > 3 {
                out.push(acc as u8);
            }
        }
        Some(out)
    }

    #[cfg(windows)]
    mod dpapi {
        use windows::Win32::Foundation::{HLOCAL, LocalFree};
        use windows::Win32::Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        };

        fn take(blob: CRYPT_INTEGER_BLOB) -> Vec<u8> {
            // SAFETY: the API returned this buffer of cbData bytes; we copy it
            // out and free it once.
            unsafe {
                let bytes = std::slice::from_raw_parts(blob.pbData, blob.cbData as usize).to_vec();
                let _ = LocalFree(Some(HLOCAL(blob.pbData as _)));
                bytes
            }
        }

        pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
            let input = CRYPT_INTEGER_BLOB {
                cbData: data.len() as u32,
                pbData: data.as_ptr() as *mut u8,
            };
            let mut output = CRYPT_INTEGER_BLOB::default();
            // SAFETY: input points at `data` for the call; output is filled by
            // the API and freed in take().
            unsafe {
                CryptProtectData(
                    &input,
                    windows::core::w!("Handy proxy"),
                    None,
                    None,
                    None,
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
                .ok()?;
            }
            Some(take(output))
        }

        pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
            let input = CRYPT_INTEGER_BLOB {
                cbData: data.len() as u32,
                pbData: data.as_ptr() as *mut u8,
            };
            let mut output = CRYPT_INTEGER_BLOB::default();
            // SAFETY: as in protect().
            unsafe {
                CryptUnprotectData(
                    &input,
                    None,
                    None,
                    None,
                    None,
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
                .ok()?;
            }
            Some(take(output))
        }
    }
}

/// Tests that change or depend on the proxy settings in force run one at a time.
#[cfg(test)]
pub(crate) static TEST_CONFIG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypass_list_matches_like_windows() {
        let list = "<local>; *.corp.example; 10.*, .internal.net localhost:8080";
        assert!(bypassed("intranet", list));
        assert!(bypassed("wiki.corp.example", list));
        assert!(!bypassed("corp.example.com", list));
        assert!(bypassed("10.1.2.3", list));
        assert!(!bypassed("110.1.2.3", list));
        assert!(bypassed("internal.net", list));
        assert!(bypassed("a.b.internal.net", list));
        assert!(bypassed("localhost", list));
        assert!(!bypassed("github.com", list));
        assert!(!bypassed("github.com", ""));
        assert!(bypassed("GitHub.COM", "*.com"));
    }

    #[test]
    fn no_proxy_variable_matches_like_reqwest() {
        let list = "corp.example, .internal.net,*.lab.test, 10.0.0.5, svc:8443";
        assert!(no_proxy_matches("corp.example", list));
        assert!(no_proxy_matches("api.corp.example", list));
        assert!(!no_proxy_matches("notcorp.example", list));
        assert!(no_proxy_matches("a.internal.net", list));
        assert!(no_proxy_matches("x.lab.test", list));
        assert!(no_proxy_matches("10.0.0.5", list));
        assert!(no_proxy_matches("svc", list));
        assert!(!no_proxy_matches("github.com", list));
        assert!(no_proxy_matches("github.com", "*"));
        assert!(no_proxy_matches("10.1.2.3", "10.0.0.0/8"));
        assert!(!no_proxy_matches("11.1.2.3", "10.0.0.0/8"));
        assert!(no_proxy_matches("192.168.1.9", "192.168.1.0/24"));
        assert!(no_proxy_matches("[fd00::1]", "fd00::/8"));
        assert!(!no_proxy_matches("10.0.0.50", "10.0.0.5"));
        assert!(!no_proxy_matches("github.com", ""));
    }

    /// ureq writes "basic" in lower case; the bridge must still let it in.
    #[cfg(windows)]
    #[test]
    fn ureq_requests_pass_the_bridge_token() {
        let _settings = TEST_CONFIG_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        use std::io::{Read, Write};
        let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy_port = proxy.local_addr().unwrap().port();
        let seen = std::thread::spawn(move || {
            let (mut s, _) = proxy.accept().unwrap();
            let mut head = Vec::new();
            let mut b = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                s.read_exact(&mut b).unwrap();
                head.push(b[0]);
            }
            s.write_all(b"HTTP/1.1 200 OK\r\n\r\n").unwrap();
            String::from_utf8(head).unwrap()
        });
        *config_cell().write().unwrap() = ProxyConfig {
            mode: ProxyMode::Manual,
            host: "127.0.0.1".into(),
            port: proxy_port,
            ..Default::default()
        };
        let url = "https://handy-ureq-test.example/v1";
        let result = ureq_agent(url)
            .post(url)
            .timeout(std::time::Duration::from_secs(10))
            .call();
        *config_cell().write().unwrap() = ProxyConfig::default();
        assert!(result.is_err(), "the fake proxy speaks no TLS");
        let head = seen.join().unwrap();
        assert!(head.starts_with("CONNECT handy-ureq-test.example:443 HTTP/1.1"));
    }

    #[test]
    fn loopback_hosts_never_use_a_proxy() {
        assert!(is_loopback_host("localhost"));
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("127.5.5.5"));
        assert!(is_loopback_host("[::1]"));
        assert!(!is_loopback_host("github.com"));
        assert!(!is_loopback_host("192.168.1.10"));
    }

    #[test]
    fn host_is_cleaned() {
        assert_eq!(clean_host(" http://proxy.corp:8080/ "), "proxy.corp:8080");
        assert_eq!(clean_host("proxy.corp"), "proxy.corp");
    }

    #[test]
    fn only_remote_https_goes_through_the_bridge() {
        let url = |s: &str| reqwest::Url::parse(s).unwrap();
        assert!(through_bridge(&url("https://github.com/x")));
        assert!(!through_bridge(&url("http://github.com/x")));
        assert!(!through_bridge(&url("https://127.0.0.1:52625/v1")));
        assert!(!through_bridge(&url("https://localhost/v1")));
    }

    /// The whole chain: a reqwest client from `client_builder` -> the bridge
    /// (token) -> a proxy that wants Basic sign-in. The fake proxy sees the
    /// tunnel request with the user's credentials, then hangs up (no TLS).
    #[cfg(windows)]
    #[tokio::test]
    async fn requests_reach_a_signing_in_proxy_through_the_bridge() {
        use std::io::{Read, Write};
        let _settings = TEST_CONFIG_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy_port = proxy.local_addr().unwrap().port();
        let seen = std::thread::spawn(move || {
            let (mut s, _) = proxy.accept().unwrap();
            let read_head = |s: &mut std::net::TcpStream| {
                let mut head = Vec::new();
                let mut b = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") {
                    s.read_exact(&mut b).unwrap();
                    head.push(b[0]);
                }
                String::from_utf8(head).unwrap()
            };
            let first = read_head(&mut s);
            s.write_all(
                b"HTTP/1.1 407 x\r\nProxy-Authenticate: Basic realm=\"p\"\r\nContent-Length: 0\r\n\r\n",
            )
            .unwrap();
            let second = read_head(&mut s);
            s.write_all(b"HTTP/1.1 200 OK\r\n\r\n").unwrap();
            (first, second)
        });
        *config_cell().write().unwrap() = ProxyConfig {
            mode: ProxyMode::Manual,
            host: "127.0.0.1".into(),
            port: proxy_port,
            bypass: String::new(),
            auth: ProxyAuth::Password,
            username: "user".into(),
            password: "pw".into(),
            ..Default::default()
        };
        let client = client_builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap();
        let result = client.get("https://handy-proxy-test.example/").send().await;
        *config_cell().write().unwrap() = ProxyConfig::default();
        assert!(result.is_err(), "the fake proxy speaks no TLS");
        let (first, second) = seen.join().unwrap();
        assert!(first.starts_with("CONNECT handy-proxy-test.example:443 HTTP/1.1"));
        assert!(!first.to_ascii_lowercase().contains("proxy-authorization"));
        // base64("user:pw")
        assert!(second.contains("Proxy-Authorization: Basic dXNlcjpwdw=="));
        assert_eq!(last_sign_in().as_deref(), Some("Basic"));
    }

    /// Real HTTPS (needs the internet, so not in the normal run): reqwest ->
    /// bridge -> a local proxy that tunnels for real -> github.com.
    #[cfg(windows)]
    #[tokio::test]
    #[ignore]
    async fn real_https_works_through_the_bridge() {
        let _settings = TEST_CONFIG_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        use std::io::{Read, Write};
        let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy_port = proxy.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in proxy.incoming() {
                let Ok(mut s) = stream else { continue };
                std::thread::spawn(move || {
                    let mut head = Vec::new();
                    let mut b = [0u8; 1];
                    while !head.ends_with(b"\r\n\r\n") {
                        if s.read_exact(&mut b).is_err() {
                            return;
                        }
                        head.push(b[0]);
                    }
                    let head = String::from_utf8_lossy(&head).to_string();
                    let target = head.split_whitespace().nth(1).unwrap().to_string();
                    let up = std::net::TcpStream::connect(target).unwrap();
                    s.write_all(b"HTTP/1.1 200 OK\r\n\r\n").unwrap();
                    let (mut a, mut b2) = (s.try_clone().unwrap(), up.try_clone().unwrap());
                    std::thread::spawn(move || {
                        let _ = std::io::copy(&mut a, &mut b2);
                    });
                    let (mut up, mut s) = (up, s);
                    let _ = std::io::copy(&mut up, &mut s);
                });
            }
        });
        *config_cell().write().unwrap() = ProxyConfig {
            mode: ProxyMode::Manual,
            host: "127.0.0.1".into(),
            port: proxy_port,
            ..Default::default()
        };
        let client = client_builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap();
        let response = client
            .get("https://github.com/patrick-1984/handy-tool/releases/latest/download/latest.json")
            .send()
            .await;
        *config_cell().write().unwrap() = ProxyConfig::default();
        let response = response.unwrap();
        assert!(response.status().is_success(), "{}", response.status());
        let body = response.text().await.unwrap();
        assert!(body.contains("\"version\""), "{body}");
    }

    #[cfg(windows)]
    #[test]
    fn the_bridge_refuses_requests_without_its_token() {
        use std::io::{Read, Write};
        let (port, _) = bridge::ensure().unwrap();
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(b"CONNECT github.com:443 HTTP/1.1\r\nHost: github.com:443\r\n\r\n")
            .unwrap();
        let mut answer = String::new();
        s.read_to_string(&mut answer).unwrap();
        assert!(answer.starts_with("HTTP/1.1 407"));
    }

    #[cfg(windows)]
    #[test]
    fn password_round_trips_encrypted() {
        let stored = secret::protect("p@ss:word");
        assert!(stored.starts_with("dpapi:"));
        assert!(!stored.contains("p@ss"));
        assert_eq!(secret::reveal(&stored), "p@ss:word");
        assert_eq!(secret::reveal(""), "");
        assert_eq!(secret::reveal("dpapi:not base64!"), "");
        assert_eq!(secret::protect(""), "");
    }

    #[cfg(windows)]
    #[test]
    fn base64_decodes_what_the_encoder_wrote() {
        for data in [
            &b""[..],
            b"a",
            b"ab",
            b"abc",
            b"abcd",
            &[0, 255, 7, 128, 64],
        ] {
            let encoded = crate::managers::openrouter_transcription::base64_encode(data);
            assert_eq!(secret::base64_decode(&encoded).unwrap(), data);
        }
    }
}
