//! The proxy Windows is set to use (Settings > Network & internet > Proxy),
//! read the way WinHTTP reads it, and the proxy a PAC script picks for a site.

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{ERROR_IO_PENDING, GlobalFree, HGLOBAL};
use windows::Win32::Networking::WinHttp::{
    WINHTTP_ACCESS_TYPE_NO_PROXY, WINHTTP_AUTOPROXY_CONFIG_URL, WINHTTP_AUTOPROXY_OPTIONS,
    WINHTTP_CALLBACK_FLAG_GETPROXYFORURL_COMPLETE, WINHTTP_CALLBACK_FLAG_REQUEST_ERROR,
    WINHTTP_CALLBACK_STATUS_GETPROXYFORURL_COMPLETE, WINHTTP_CALLBACK_STATUS_REQUEST_ERROR,
    WINHTTP_CURRENT_USER_IE_PROXY_CONFIG, WINHTTP_FLAG_ASYNC, WINHTTP_INTERNET_SCHEME_HTTP,
    WINHTTP_INTERNET_SCHEME_HTTPS, WINHTTP_PROXY_RESULT, WinHttpCloseHandle,
    WinHttpCreateProxyResolver, WinHttpFreeProxyResult, WinHttpGetIEProxyConfigForCurrentUser,
    WinHttpGetProxyForUrlEx, WinHttpGetProxyResult, WinHttpOpen, WinHttpSetStatusCallback,
};
use windows::core::{HSTRING, PCWSTR, PWSTR};

/// Windows' proxy settings for this user.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SystemProxy {
    /// "proxy:8080" or "http=a:80;https=b:443" (empty = none).
    pub proxy: String,
    /// "<local>;*.corp" (empty = none).
    pub bypass: String,
    /// The PAC script's address (empty = none).
    pub pac_url: String,
    /// "Automatically detect settings" (WPAD). Not followed: see uses_proxy().
    pub auto_detect: bool,
}

impl SystemProxy {
    /// A proxy Handy follows: a static one or a PAC script. "Automatically
    /// detect settings" alone is on by default on every PC and would mean a
    /// WPAD lookup before the first request everywhere, so it is not followed
    /// (reqwest never did either).
    pub fn uses_proxy(&self) -> bool {
        !self.pac_url.is_empty() || self.static_proxy_for_https().is_some()
    }

    /// The static proxy for HTTPS: the `https=` entry of a per-protocol list,
    /// else the one proxy for everything.
    pub fn static_proxy_for_https(&self) -> Option<(String, u16)> {
        static_proxy_for_https(&self.proxy)
    }
}

/// "a:80" / "http=a:80;https=b:443" / "http://a:80" -> the HTTPS proxy.
pub fn static_proxy_for_https(list: &str) -> Option<(String, u16)> {
    let entries: Vec<&str> = list
        .split([';', ' '])
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .collect();
    let pick = if entries.iter().any(|e| e.contains('=')) {
        entries.iter().find_map(|e| {
            let (scheme, value) = e.split_once('=')?;
            scheme.trim().eq_ignore_ascii_case("https").then_some(value)
        })?
    } else {
        entries.first()?
    };
    parse_host_port(pick)
}

/// "http://host:port/", "host:port", "host" (port 80).
pub fn parse_host_port(text: &str) -> Option<(String, u16)> {
    let text = text.trim();
    // A SOCKS entry ("socks=...", "socks5://..."): Handy speaks HTTP proxies only.
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("socks=") || (lower.starts_with("socks") && lower.contains("://")) {
        return None;
    }
    let text = text
        .strip_prefix("http://")
        .or_else(|| text.strip_prefix("https://"))
        .unwrap_or(text)
        .trim_end_matches('/');
    if text.is_empty() {
        return None;
    }
    if let Some((host, port)) = text.rsplit_once(':')
        && (!host.is_empty() && !host.contains(':') || host.starts_with('[') && host.ends_with(']'))
        && let Ok(port) = port.parse::<u16>()
    {
        return Some((host.to_string(), port));
    }
    Some((text.to_string(), 80))
}

fn take_wide(ptr: PWSTR) -> String {
    if ptr.is_null() {
        return String::new();
    }
    // SAFETY: WinHTTP returned a NUL-terminated string allocated with
    // GlobalAlloc; read it once and free it once.
    unsafe {
        let text = ptr.to_string().unwrap_or_default();
        let _ = GlobalFree(Some(HGLOBAL(ptr.0 as _)));
        text
    }
}

fn read() -> SystemProxy {
    let mut config = WINHTTP_CURRENT_USER_IE_PROXY_CONFIG::default();
    // SAFETY: the API fills `config`; take_wide frees each string it returned.
    if unsafe { WinHttpGetIEProxyConfigForCurrentUser(&mut config) }.is_err() {
        return SystemProxy::default();
    }
    SystemProxy {
        auto_detect: config.fAutoDetect.as_bool(),
        pac_url: take_wide(config.lpszAutoConfigUrl).trim().to_string(),
        proxy: take_wide(config.lpszProxy).trim().to_string(),
        bypass: take_wide(config.lpszProxyBypass).trim().to_string(),
    }
}

/// Windows' proxy settings, re-read at most every 30 s.
pub fn current() -> SystemProxy {
    static CACHE: OnceLock<Mutex<Option<(Instant, SystemProxy)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((at, value)) = cache.as_ref()
        && at.elapsed() < Duration::from_secs(30)
    {
        return value.clone();
    }
    let value = read();
    *cache = Some((Instant::now(), value.clone()));
    value
}

/// One way to a site: through a proxy, or direct.
#[derive(Clone, Debug, PartialEq)]
pub enum Hop {
    Direct,
    Proxy(String, u16),
}

/// The ways a PAC script names for one site, in the order to try them
/// (empty = direct).
type PacAnswer = Vec<Hop>;

/// Runs the PAC script at `pac_url` for `https://host:port/` (WinHTTP
/// downloads and runs it, signing in with the Windows account if its server
/// asks). Answers are kept 5 minutes per site and port; Err = the script could
/// not be used.
pub fn pac_for(pac_url: &str, host: &str, port: u16) -> Result<PacAnswer, String> {
    type Cache = HashMap<(String, String, u16), (Instant, PacAnswer)>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (pac_url.to_string(), host.to_ascii_lowercase(), port);
    if let Some((at, answer)) = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&key)
        && at.elapsed() < Duration::from_secs(300)
    {
        return Ok(answer.clone());
    }
    let answer = run_pac(pac_url, &pac_target(host, port))?;
    cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(key, (Instant::now(), answer.clone()));
    Ok(answer)
}

/// The address the script is asked about: `https://host/`, with the port
/// when it is not 443.
fn pac_target(host: &str, port: u16) -> String {
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    if port == 443 {
        format!("https://{host}/")
    } else {
        format!("https://{host}:{port}/")
    }
}

/// What the resolver's callback reports back (shared with WinHTTP's thread).
struct PacWait {
    done: Mutex<Option<bool>>,
    signal: Condvar,
}

unsafe extern "system" fn pac_callback(
    _handle: *mut core::ffi::c_void,
    context: usize,
    status: u32,
    _info: *mut core::ffi::c_void,
    _length: u32,
) {
    if context == 0 {
        return;
    }
    let ok = status == WINHTTP_CALLBACK_STATUS_GETPROXYFORURL_COMPLETE;
    if !ok && status != WINHTTP_CALLBACK_STATUS_REQUEST_ERROR {
        return;
    }
    // SAFETY: context is the PacWait kept alive by run_pac for as long as the
    // request can call back (leaked if it gives up waiting).
    let wait = unsafe { &*(context as *const PacWait) };
    let mut done = wait
        .done
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *done = Some(ok);
    wait.signal.notify_all();
}

/// Runs the script with WinHttpGetProxyForUrlEx: unlike WinHttpGetProxyForUrl
/// it returns the script's whole ordered list, DIRECT entries included, so a
/// "PROXY a; DIRECT" script can fall back to direct when `a` is down.
fn run_pac(pac_url: &str, target: &str) -> Result<PacAnswer, String> {
    let failed = |what: &str, code: u32| {
        format!("the proxy script {pac_url} could not be used: {what} (error {code})")
    };
    // SAFETY: an async WinHTTP session used only for this lookup, closed below.
    let session = unsafe {
        WinHttpOpen(
            windows::core::w!("Handy"),
            WINHTTP_ACCESS_TYPE_NO_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            WINHTTP_FLAG_ASYNC,
        )
    };
    if session.is_null() {
        return Err("WinHTTP is not available".to_string());
    }
    let mut resolver: *mut core::ffi::c_void = std::ptr::null_mut();
    // SAFETY: valid session; resolver receives a new handle.
    let created = unsafe { WinHttpCreateProxyResolver(session, &mut resolver) };
    if created != 0 || resolver.is_null() {
        // SAFETY: closing the session opened above.
        let _ = unsafe { WinHttpCloseHandle(session) };
        return Err(failed("no proxy resolver", created));
    }
    let wait = Arc::new(PacWait {
        done: Mutex::new(None),
        signal: Condvar::new(),
    });
    let context = Arc::into_raw(wait.clone()) as usize;
    let pac = HSTRING::from(pac_url);
    let url = HSTRING::from(target);
    let options = WINHTTP_AUTOPROXY_OPTIONS {
        dwFlags: WINHTTP_AUTOPROXY_CONFIG_URL,
        lpszAutoConfigUrl: PCWSTR(pac.as_ptr()),
        fAutoLogonIfChallenged: true.into(),
        ..Default::default()
    };
    // SAFETY: the callback only reads `context` (a leaked Arc, see below).
    unsafe {
        WinHttpSetStatusCallback(
            resolver,
            Some(pac_callback),
            WINHTTP_CALLBACK_FLAG_REQUEST_ERROR | WINHTTP_CALLBACK_FLAG_GETPROXYFORURL_COMPLETE,
            0,
        );
    }
    // SAFETY: strings and options outlive the call (the API copies them).
    let started = unsafe { WinHttpGetProxyForUrlEx(resolver, &url, &options, Some(context)) };
    let outcome = if started == ERROR_IO_PENDING.0 {
        let done = wait
            .done
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (done, _) = wait
            .signal
            .wait_timeout_while(done, Duration::from_secs(20), |done| done.is_none())
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *done
    } else {
        Some(false)
    };
    let answer = match outcome {
        Some(true) => {
            let mut result = WINHTTP_PROXY_RESULT::default();
            // SAFETY: the lookup completed; the result is freed below.
            let read = unsafe { WinHttpGetProxyResult(resolver, &mut result) };
            if read == 0 {
                let mut hops = Vec::new();
                let mut skipped = 0;
                for i in 0..result.cEntries as usize {
                    // SAFETY: cEntries entries at pEntries, valid until freed.
                    let entry = unsafe { &*result.pEntries.add(i) };
                    if !entry.fProxy.as_bool() || entry.fBypass.as_bool() {
                        hops.push(Hop::Direct);
                    } else if entry.ProxyScheme != WINHTTP_INTERNET_SCHEME_HTTP
                        && entry.ProxyScheme != WINHTTP_INTERNET_SCHEME_HTTPS
                    {
                        // SOCKS: Handy speaks HTTP proxies only.
                        skipped += 1;
                    } else if !entry.pwszProxy.is_null() {
                        // SAFETY: a NUL-terminated string owned by `result`.
                        let host = unsafe { entry.pwszProxy.to_string() }.unwrap_or_default();
                        if !host.is_empty() {
                            hops.push(Hop::Proxy(host, entry.ProxyPort));
                        }
                    }
                }
                // SAFETY: freeing the result read above, once.
                unsafe { WinHttpFreeProxyResult(&mut result) };
                if hops.is_empty() && skipped > 0 {
                    // Empty would mean "go direct", which the script did not say.
                    Err(format!(
                        "the proxy script {pac_url} names only SOCKS proxies, which Handy cannot use"
                    ))
                } else {
                    Ok(hops)
                }
            } else {
                Err(failed("no result", read))
            }
        }
        Some(false) => Err(failed("the script failed", started)),
        None => Err(failed("timed out", 0)),
    };
    // SAFETY: closing the handles opened above. Closing the resolver cancels a
    // lookup still running; its callback may still come, so the PacWait it
    // points at is only reclaimed when the lookup has finished.
    unsafe {
        let _ = WinHttpCloseHandle(resolver);
        let _ = WinHttpCloseHandle(session);
    }
    if outcome.is_some() && started == ERROR_IO_PENDING.0 {
        // SAFETY: the callback has run (it set `done`) and will not run
        // again for this finished lookup; this is the Arc leaked above.
        drop(unsafe { Arc::from_raw(context as *const PacWait) });
    } else if started != ERROR_IO_PENDING.0 {
        // SAFETY: the lookup never started, so nothing can call back.
        drop(unsafe { Arc::from_raw(context as *const PacWait) });
    }
    answer
}

/// "a:80; b:8080; DIRECT" -> [a:80, b:8080, direct], in order. ("PROXY"
/// keywords, if WinHTTP leaves them, are skipped.)
fn parse_proxy_list(list: &str) -> PacAnswer {
    list.split([';', ' ', '\t'])
        .map(str::trim)
        .filter(|e| !e.is_empty() && !e.eq_ignore_ascii_case("proxy"))
        .filter_map(|entry| {
            if entry.eq_ignore_ascii_case("direct") {
                Some(Hop::Direct)
            } else {
                parse_host_port(entry).map(|(host, port)| Hop::Proxy(host, port))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_proxy_lists_parse() {
        assert_eq!(
            static_proxy_for_https("proxy.corp:8080"),
            Some(("proxy.corp".into(), 8080))
        );
        assert_eq!(
            static_proxy_for_https("http=a:80;https=b.corp:443;ftp=c:21"),
            Some(("b.corp".into(), 443))
        );
        assert_eq!(static_proxy_for_https("http=a:80;ftp=c:21"), None);
        assert_eq!(
            static_proxy_for_https("http://proxy:3128/"),
            Some(("proxy".into(), 3128))
        );
        assert_eq!(static_proxy_for_https("proxy"), Some(("proxy".into(), 80)));
        assert_eq!(static_proxy_for_https(""), None);
        assert_eq!(static_proxy_for_https("socks=s:1080"), None);
    }

    #[test]
    fn pac_lists_keep_every_proxy_in_order() {
        assert_eq!(
            parse_proxy_list("a.corp:8080; b.corp:3128"),
            vec![
                Hop::Proxy("a.corp".into(), 8080),
                Hop::Proxy("b.corp".into(), 3128)
            ]
        );
        assert_eq!(
            parse_proxy_list("PROXY a.corp:8080; DIRECT"),
            vec![Hop::Proxy("a.corp".into(), 8080), Hop::Direct]
        );
        assert_eq!(
            static_proxy_for_https("socksproxy.corp:8080"),
            Some(("socksproxy.corp".into(), 8080))
        );
        assert_eq!(static_proxy_for_https("socks5://s:1080"), None);
        assert!(parse_proxy_list("").is_empty());
        assert_eq!(pac_target("svc.corp", 443), "https://svc.corp/");
        assert_eq!(pac_target("svc.corp", 8443), "https://svc.corp:8443/");
        assert_eq!(pac_target("::1", 8443), "https://[::1]:8443/");
    }

    /// The real WinHTTP: a script answering "PROXY 127.0.0.1:9; DIRECT" must
    /// keep its DIRECT fallback.
    #[test]
    fn pac_script_keeps_its_direct_fallback() {
        use std::io::{Read, Write};
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in server.incoming().take(8) {
                let Ok(mut s) = stream else { continue };
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                let socks = buf.windows(6).any(|w| w == b"/socks");
                let body = if socks {
                    "function FindProxyForURL(url, host) { return \"SOCKS 127.0.0.1:9; DIRECT\"; }"
                } else {
                    "function FindProxyForURL(url, host) { return \"PROXY 127.0.0.1:9; DIRECT\"; }"
                };
                let _ = s.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/x-ns-proxy-autoconfig\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                );
            }
        });
        let pac = format!("http://127.0.0.1:{port}/proxy.pac");
        let hops = run_pac(&pac, "https://example.com/").unwrap();
        assert_eq!(hops, vec![Hop::Proxy("127.0.0.1".into(), 9), Hop::Direct]);
        // A SOCKS entry is skipped; its DIRECT fallback stays.
        let socks = format!("http://127.0.0.1:{port}/socks.pac");
        let hops = run_pac(&socks, "https://example.com/").unwrap();
        assert_eq!(hops, vec![Hop::Direct]);
    }

    #[test]
    fn auto_detect_alone_is_not_followed() {
        let only_wpad = SystemProxy {
            auto_detect: true,
            ..Default::default()
        };
        assert!(!only_wpad.uses_proxy());
        let with_proxy = SystemProxy {
            proxy: "p:8080".into(),
            ..Default::default()
        };
        assert!(with_proxy.uses_proxy());
        let with_pac = SystemProxy {
            pac_url: "http://wpad/proxy.pac".into(),
            ..Default::default()
        };
        assert!(with_pac.uses_proxy());
    }
}
