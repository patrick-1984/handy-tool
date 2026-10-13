//! The proxy bridge: a tiny HTTP CONNECT proxy on 127.0.0.1 that Handy's own
//! clients use for HTTPS when a proxy is in play (see net/mod.rs).
//!
//! For each tunnel it picks the route the way Windows does (the manual proxy
//! and its bypass list, or Windows' PAC script / static proxy / bypass list),
//! opens the tunnel through that proxy - signing in with Negotiate or NTLM
//! (SSPI: the Windows account, or the user name and password from the settings)
//! or Basic - answers the client "200" and relays bytes both ways. It never
//! sees plain text: the client's TLS runs inside the tunnel.
//!
//! Only Handy can use it: it listens on loopback and every request must carry
//! the random token of this run (Proxy-Authorization: Basic handy:<token>), so
//! another program on the PC cannot borrow the user's proxy sign-in.

use super::system::Hop;
use super::{bypassed, is_loopback_host, sspi, system};
use crate::managers::openrouter_transcription::base64_encode;
use crate::settings::ProxyMode;
use log::{info, warn};
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_HEAD: usize = 32 * 1024;
const MAX_BODY: usize = 1024 * 1024;

/// The bridge's port and token, started on first use. None = it could not
/// start (Handy then uses reqwest's own proxy handling, as before).
pub fn ensure() -> Option<(u16, String)> {
    static BRIDGE: OnceLock<Option<(u16, String)>> = OnceLock::new();
    BRIDGE.get_or_init(start).clone()
}

fn start() -> Option<(u16, String)> {
    let listener = match TcpListener::bind(("127.0.0.1", 0)) {
        Ok(listener) => listener,
        Err(error) => {
            warn!("Proxy bridge could not start: {error}");
            return None;
        }
    };
    let port = listener.local_addr().ok()?.port();
    let token = uuid::Uuid::new_v4().simple().to_string();
    // Clients differ in the scheme's case (reqwest "Basic", ureq "basic").
    let expected = base64_encode(format!("handy:{token}").as_bytes());
    let spawned = std::thread::Builder::new()
        .name("proxy-bridge".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let expected = expected.clone();
                let _ = std::thread::Builder::new()
                    .name("proxy-tunnel".into())
                    .spawn(move || handle(stream, &expected));
            }
        });
    if let Err(error) = spawned {
        warn!("Proxy bridge could not start: {error}");
        return None;
    }
    info!("Proxy bridge listening on 127.0.0.1:{port}");
    Some((port, token))
}

#[derive(Default)]
struct Last {
    sign_in: Option<String>,
    error: Option<String>,
}

fn last() -> &'static Mutex<Last> {
    static LAST: OnceLock<Mutex<Last>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(Last::default()))
}

fn record(sign_in: Option<String>, error: Option<String>) {
    let mut last = last()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    last.sign_in = sign_in;
    last.error = error;
}

pub fn last_sign_in() -> Option<String> {
    last()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .sign_in
        .clone()
}

pub fn last_error() -> Option<String> {
    last()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .error
        .clone()
}

/// The ways for a tunnel to host:port, in the order to try them (a PAC script
/// may name backups, or DIRECT); empty = go direct.
pub fn upstream_for(host: &str, port: u16) -> Result<Vec<Hop>, String> {
    if is_loopback_host(host) {
        return Ok(Vec::new());
    }
    let config = super::current();
    match config.mode {
        ProxyMode::Off => Ok(Vec::new()),
        ProxyMode::Manual if !config.host.is_empty() && config.port != 0 => {
            if bypassed(host, &config.bypass) {
                Ok(Vec::new())
            } else {
                Ok(vec![Hop::Proxy(config.host, config.port)])
            }
        }
        ProxyMode::Manual | ProxyMode::System => {
            let windows = system::current();
            if !windows.pac_url.is_empty() {
                match system::pac_for(&windows.pac_url, host, port) {
                    Ok(proxies) => return Ok(proxies),
                    // A broken script: the static proxy below, if any.
                    Err(error) => warn!("Proxy: {error}"),
                }
            }
            if bypassed(host, &windows.bypass) {
                return Ok(Vec::new());
            }
            Ok(windows
                .static_proxy_for_https()
                .map(|(proxy, port)| Hop::Proxy(proxy, port))
                .into_iter()
                .collect())
        }
    }
}

fn handle(mut client: TcpStream, expected: &str) {
    let _ = client.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
    let Ok(head) = read_head(&mut client) else {
        return;
    };
    let request = Head::parse(&head);
    let authorized = request
        .header("proxy-authorization")
        .and_then(|value| value.trim().split_once(' '))
        .is_some_and(|(scheme, credentials)| {
            scheme.eq_ignore_ascii_case("basic") && credentials.trim() == expected
        });
    if !authorized {
        let _ = client.write_all(
            b"HTTP/1.1 407 Proxy Authentication Required\r\n\
              Proxy-Authenticate: Basic realm=\"handy\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        return;
    }
    let mut parts = request.first_line.split_whitespace();
    let (method, authority) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let target = method
        .eq_ignore_ascii_case("CONNECT")
        .then(|| split_authority(authority))
        .flatten();
    let Some((host, port)) = target else {
        let _ = client.write_all(
            b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        return;
    };
    match open_tunnel(&host, port) {
        Ok(upstream) => {
            if client
                .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                .is_ok()
            {
                relay(client, upstream);
            }
        }
        Err(error) => {
            warn!("Proxy: no tunnel to {host}:{port}: {error}");
            record(None, Some(error));
            let _ = client.write_all(
                b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    }
}

/// "host:443" / "[::1]:443" -> (host, port).
fn split_authority(authority: &str) -> Option<(String, u16)> {
    let (host, port) = authority.rsplit_once(':')?;
    let port = port.parse::<u16>().ok()?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    (!host.is_empty()).then(|| (host.to_string(), port))
}

fn authority(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

fn connect(host: &str, port: u16) -> io::Result<TcpStream> {
    let mut last_error = io::Error::new(io::ErrorKind::NotFound, "no address");
    for address in (host, port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                return Ok(stream);
            }
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

fn open_tunnel(host: &str, port: u16) -> Result<TcpStream, String> {
    let proxies = upstream_for(host, port)?;
    if proxies.is_empty() {
        let stream = connect(host, port).map_err(|e| format!("cannot reach {host}: {e}"))?;
        record(None, None);
        return Ok(stream);
    }
    // The next proxy only when one cannot be reached at all (as browsers do
    // with a PAC script's list); a refused sign-in is an answer.
    let mut unreachable = Vec::new();
    for hop in &proxies {
        let (proxy_host, proxy_port) = match hop {
            Hop::Proxy(proxy_host, proxy_port) => (proxy_host, proxy_port),
            Hop::Direct => match connect(host, port) {
                Ok(stream) => {
                    record(None, None);
                    return Ok(stream);
                }
                Err(error) => {
                    unreachable.push(format!("cannot reach {host}: {error}"));
                    continue;
                }
            },
        };
        let first = match connect(proxy_host, *proxy_port) {
            Ok(stream) => stream,
            Err(error) => {
                unreachable.push(format!(
                    "cannot reach the proxy {proxy_host}:{proxy_port}: {error}"
                ));
                continue;
            }
        };
        let (stream, sign_in) = through_proxy_on(first, proxy_host, *proxy_port, host, port)?;
        record(sign_in, None);
        return Ok(stream);
    }
    Err(unreachable.join("; "))
}

/// A response's (or request's) head: first line + headers.
struct Head {
    first_line: String,
    headers: Vec<(String, String)>,
}

impl Head {
    fn parse(raw: &[u8]) -> Head {
        let text = String::from_utf8_lossy(raw);
        let mut lines = text.split("\r\n");
        let first_line = lines.next().unwrap_or("").to_string();
        let headers = lines
            .filter_map(|line| {
                let (name, value) = line.split_once(':')?;
                Some((name.trim().to_ascii_lowercase(), value.trim().to_string()))
            })
            .collect();
        Head {
            first_line,
            headers,
        }
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    fn all(&self, name: &str) -> impl Iterator<Item = &str> {
        self.headers
            .iter()
            .filter(move |(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    fn status(&self) -> u16 {
        self.first_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }

    /// The sign-in methods the proxy offers, lowercase ("negotiate", "ntlm",
    /// "basic"), from every Proxy-Authenticate header.
    fn schemes(&self) -> Vec<String> {
        let mut schemes = Vec::new();
        for value in self.all("proxy-authenticate") {
            for part in value.split(',') {
                let word = part.split_whitespace().next().unwrap_or("");
                let word = word.to_ascii_lowercase();
                if matches!(word.as_str(), "negotiate" | "ntlm" | "basic")
                    && !schemes.contains(&word)
                {
                    schemes.push(word);
                }
            }
        }
        schemes
    }

    /// The proxy's challenge for `scheme` ("NTLM <base64>"), decoded.
    /// Several challenges may share one header (`NTLM <token>, Basic
    /// realm="x"`): the NTLM/Negotiate one is the scheme and a single token.
    fn challenge(&self, scheme: &str) -> Option<Vec<u8>> {
        self.all("proxy-authenticate")
            .flat_map(|value| value.split(','))
            .find_map(|part| {
                let mut words = part.split_whitespace();
                let name = words.next()?;
                let data = words.next()?;
                if !name.eq_ignore_ascii_case(scheme) || words.next().is_some() {
                    return None;
                }
                super::secret::base64_decode(data).filter(|d| !d.is_empty())
            })
    }
}

fn read_head(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut head = Vec::with_capacity(512);
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() >= MAX_HEAD {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "head too long"));
        }
        if stream.read(&mut byte)? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        head.push(byte[0]);
    }
    Ok(head)
}

fn read_line(stream: &mut TcpStream) -> io::Result<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    while !line.ends_with(b"\r\n") {
        if line.len() >= MAX_HEAD {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "line too long"));
        }
        if stream.read(&mut byte)? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        line.push(byte[0]);
    }
    Ok(String::from_utf8_lossy(&line[..line.len() - 2]).to_string())
}

fn skip(stream: &mut TcpStream, mut count: usize) -> io::Result<()> {
    let mut buffer = [0u8; 4096];
    while count > 0 {
        let n = stream.read(&mut buffer[..count.min(4096)])?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        count -= n;
    }
    Ok(())
}

/// Reads a non-2xx answer's body so the connection can carry the next step.
/// Returns whether the connection can be used again.
fn drain_body(stream: &mut TcpStream, head: &Head) -> bool {
    let http11 = head.first_line.starts_with("HTTP/1.1");
    let closing = head
        .all("connection")
        .chain(head.all("proxy-connection"))
        .any(|v| v.to_ascii_lowercase().contains("close"));
    let keep_alive = head
        .all("connection")
        .chain(head.all("proxy-connection"))
        .any(|v| v.to_ascii_lowercase().contains("keep-alive"));
    if closing || !(http11 || keep_alive) {
        return false;
    }
    if head
        .header("transfer-encoding")
        .is_some_and(|v| v.to_ascii_lowercase().contains("chunked"))
    {
        let mut total = 0usize;
        loop {
            let Ok(line) = read_line(stream) else {
                return false;
            };
            let size = usize::from_str_radix(line.split(';').next().unwrap_or("").trim(), 16);
            let Ok(size) = size else { return false };
            if size == 0 {
                // Trailers up to the empty line.
                loop {
                    match read_line(stream) {
                        Ok(line) if line.is_empty() => return true,
                        Ok(_) => continue,
                        Err(_) => return false,
                    }
                }
            }
            total += size;
            if total > MAX_BODY || skip(stream, size + 2).is_err() {
                return false;
            }
        }
    }
    match head.header("content-length").map(|v| v.parse::<usize>()) {
        Some(Ok(length)) if length <= MAX_BODY => skip(stream, length).is_ok(),
        _ => false,
    }
}

fn send_connect(
    stream: &mut TcpStream,
    target: &str,
    authorization: Option<&str>,
) -> io::Result<()> {
    let mut request = format!(
        "CONNECT {target} HTTP/1.1\r\nHost: {target}\r\nUser-Agent: Handy\r\nProxy-Connection: Keep-Alive\r\n"
    );
    if let Some(value) = authorization {
        request.push_str(&format!("Proxy-Authorization: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes())
}

/// One CONNECT exchange: the answer's head and whether the connection can be
/// used again.
fn exchange(
    stream: &mut TcpStream,
    target: &str,
    authorization: Option<&str>,
) -> Result<(Head, bool), String> {
    send_connect(stream, target, authorization).map_err(|e| format!("proxy connection: {e}"))?;
    let head = Head::parse(&read_head(stream).map_err(|e| format!("proxy answer: {e}"))?);
    if (200..300).contains(&head.status()) {
        return Ok((head, true));
    }
    let reusable = drain_body(stream, &head);
    Ok((head, reusable))
}

/// A tunnel to host:port through the proxy, signed in if it asks. Returns the
/// stream and the sign-in method used.
#[cfg(test)]
fn through_proxy(
    proxy_host: &str,
    proxy_port: u16,
    host: &str,
    port: u16,
) -> Result<(TcpStream, Option<String>), String> {
    let first = connect(proxy_host, proxy_port).map_err(|e| e.to_string())?;
    through_proxy_on(first, proxy_host, proxy_port, host, port)
}

/// As through_proxy, on an already open connection to the proxy.
fn through_proxy_on(
    first: TcpStream,
    proxy_host: &str,
    proxy_port: u16,
    host: &str,
    port: u16,
) -> Result<(TcpStream, Option<String>), String> {
    let proxy = format!("{proxy_host}:{proxy_port}");
    let target = authority(host, port);
    let open = || -> Result<TcpStream, String> {
        let stream = connect(proxy_host, proxy_port)
            .map_err(|e| format!("cannot reach the proxy {proxy}: {e}"))?;
        let _ = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
        Ok(stream)
    };

    let mut stream = first;
    let _ = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
    let (head, mut reusable) = exchange(&mut stream, &target, None)?;
    if (200..300).contains(&head.status()) {
        return Ok((finish(stream), None));
    }
    if head.status() != 407 {
        return Err(format!(
            "the proxy {proxy} answered \"{}\"",
            head.first_line
        ));
    }
    let offered = head.schemes();
    let config = super::current();
    let credentials = config.credentials();
    let mut failure = None;
    for scheme in ["negotiate", "ntlm", "basic"] {
        if !offered.iter().any(|s| s == scheme) {
            continue;
        }
        if scheme == "basic" && credentials.is_none() {
            failure.get_or_insert_with(|| {
                format!(
                    "the proxy {proxy} wants a user name and password; enter them in Advanced > Proxy"
                )
            });
            continue;
        }
        if !reusable {
            stream = open()?;
        }
        let attempt = if scheme == "basic" {
            let (user, password) = credentials.unwrap_or_default();
            sign_in_basic(&mut stream, &target, user, password)
        } else {
            sign_in_sspi(&mut stream, &target, proxy_host, scheme, credentials)
        };
        match attempt {
            Ok(()) => {
                let name = match scheme {
                    "negotiate" => "Negotiate",
                    "ntlm" => "NTLM",
                    _ => "Basic",
                };
                return Ok((finish(stream), Some(name.to_string())));
            }
            Err((error, can_reuse)) => {
                warn!("Proxy {proxy}: {scheme} sign-in failed: {error}");
                failure = Some(error);
                reusable = can_reuse;
            }
        }
    }
    Err(failure.unwrap_or_else(|| {
        format!(
            "the proxy {proxy} asks for a sign-in Handy cannot do ({})",
            head.all("proxy-authenticate")
                .collect::<Vec<_>>()
                .join(", ")
        )
    }))
}

fn finish(stream: TcpStream) -> TcpStream {
    let _ = stream.set_read_timeout(None);
    stream
}

type Attempt = Result<(), (String, bool)>;

fn sign_in_basic(stream: &mut TcpStream, target: &str, user: &str, password: &str) -> Attempt {
    let value = format!(
        "Basic {}",
        base64_encode(format!("{user}:{password}").as_bytes())
    );
    let (head, reusable) = exchange(stream, target, Some(&value)).map_err(|e| (e, false))?;
    match head.status() {
        200..=299 => Ok(()),
        407 => Err((
            "the proxy did not accept the user name or password".to_string(),
            reusable,
        )),
        _ => Err((
            format!("the proxy answered \"{}\"", head.first_line),
            reusable,
        )),
    }
}

fn sign_in_sspi(
    stream: &mut TcpStream,
    target: &str,
    proxy_host: &str,
    scheme: &str,
    credentials: Option<(&str, &str)>,
) -> Attempt {
    let name = if scheme == "negotiate" {
        "Negotiate"
    } else {
        "NTLM"
    };
    let who = if credentials.is_some() {
        "the user name and password"
    } else {
        "the Windows account"
    };
    let mut context = sspi::Context::new(name, proxy_host, credentials).map_err(|e| (e, true))?;
    let mut challenge: Option<Vec<u8>> = None;
    for _ in 0..5 {
        let (token, _) = context
            .step(challenge.as_deref())
            .map_err(|e| (format!("{name} with {who}: {e}"), true))?;
        let value = format!("{name} {}", base64_encode(&token));
        let (head, reusable) = exchange(stream, target, Some(&value)).map_err(|e| (e, false))?;
        match head.status() {
            200..=299 => return Ok(()),
            407 => match head.challenge(name) {
                Some(next) if reusable => challenge = Some(next),
                Some(_) => {
                    return Err((
                        format!(
                            "the proxy closed the connection in the middle of the {name} sign-in"
                        ),
                        false,
                    ));
                }
                None => {
                    return Err((
                        format!("the proxy did not accept {name} with {who}"),
                        reusable,
                    ));
                }
            },
            _ => {
                return Err((
                    format!("the proxy answered \"{}\"", head.first_line),
                    reusable,
                ));
            }
        }
    }
    Err((format!("{name} sign-in did not finish"), false))
}

/// How long the answer may take after the client finished sending.
const AFTER_CLIENT_DONE: Duration = Duration::from_secs(120);

/// Relays bytes both ways until either side closes.
fn relay(client: TcpStream, upstream: TcpStream) {
    relay_with(client, upstream, AFTER_CLIENT_DONE);
}

/// Relays bytes both ways on one thread with non-blocking sockets: a
/// would-block read or write moves nothing (unlike a timed-out blocking send,
/// which leaves a Windows socket in an unknown state), so the deadline after
/// the client finished sending is always enforced, whether the server went
/// quiet or the client stopped reading. It waits 1 ms after moving data and up
/// to 50 ms when idle.
fn relay_with(mut client: TcpStream, mut upstream: TcpStream, after_client_done: Duration) {
    if client.set_nonblocking(true).is_err() || upstream.set_nonblocking(true).is_err() {
        return;
    }
    let would_block = |error: &io::Error| {
        matches!(
            error.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
        )
    };
    let mut buffer = vec![0u8; 64 * 1024];
    // Read but not yet written, each way.
    let mut upward: Vec<u8> = Vec::new();
    let mut downward: Vec<u8> = Vec::new();
    let (mut client_eof, mut upstream_eof) = (false, false);
    let mut client_done: Option<Instant> = None;
    let mut idle_ms = 0u64;
    'relay: loop {
        let mut moved = false;

        // Client -> server.
        if !client_eof && upward.is_empty() {
            match client.read(&mut buffer) {
                Ok(0) => client_eof = true,
                Ok(n) => {
                    upward.extend_from_slice(&buffer[..n]);
                    moved = true;
                }
                Err(error) if would_block(&error) => {}
                Err(_) => break 'relay,
            }
        }
        while !upward.is_empty() {
            match upstream.write(&upward) {
                Ok(0) => break 'relay,
                Ok(n) => {
                    upward.drain(..n);
                    moved = true;
                }
                Err(error) if would_block(&error) => break,
                Err(_) => break 'relay,
            }
        }
        if client_eof && upward.is_empty() && client_done.is_none() {
            // Pass the client's end on; the answer may still come back.
            let _ = upstream.shutdown(Shutdown::Write);
            client_done = Some(Instant::now());
        }

        // Server -> client.
        if !upstream_eof && downward.is_empty() {
            match upstream.read(&mut buffer) {
                Ok(0) => upstream_eof = true,
                Ok(n) => {
                    downward.extend_from_slice(&buffer[..n]);
                    moved = true;
                }
                Err(error) if would_block(&error) => {}
                Err(_) => break 'relay,
            }
        }
        while !downward.is_empty() {
            match client.write(&downward) {
                Ok(0) => break 'relay,
                Ok(n) => {
                    downward.drain(..n);
                    moved = true;
                }
                Err(error) if would_block(&error) => break,
                Err(_) => break 'relay,
            }
        }

        // The server is done and everything reached the client.
        if upstream_eof && downward.is_empty() {
            break;
        }
        if client_done.is_some_and(|at| at.elapsed() >= after_client_done) {
            break;
        }
        if moved {
            idle_ms = 0;
        } else {
            idle_ms = (idle_ms * 2).clamp(1, 50);
            std::thread::sleep(Duration::from_millis(idle_ms));
        }
    }
    let _ = client.shutdown(Shutdown::Both);
    let _ = upstream.shutdown(Shutdown::Both);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use windows::Win32::Foundation::{SEC_E_OK, SEC_I_CONTINUE_NEEDED};
    use windows::Win32::Security::Authentication::Identity::{
        ASC_REQ_ALLOCATE_MEMORY, ASC_REQ_CONNECTION, AcceptSecurityContext,
        AcquireCredentialsHandleW, FreeContextBuffer, SECBUFFER_TOKEN, SECBUFFER_VERSION,
        SECPKG_CRED_INBOUND, SECURITY_NATIVE_DREP, SecBuffer, SecBufferDesc,
    };
    use windows::Win32::Security::Credentials::SecHandle;

    /// The server side of an SSPI sign-in, as a proxy would run it. (Handles
    /// are leaked: test only.)
    struct Acceptor {
        credentials: SecHandle,
        context: Option<SecHandle>,
    }

    impl Acceptor {
        fn new(package: &str) -> Acceptor {
            let mut credentials = SecHandle::default();
            unsafe {
                AcquireCredentialsHandleW(
                    None,
                    &windows::core::HSTRING::from(package),
                    SECPKG_CRED_INBOUND,
                    None,
                    None,
                    None,
                    None,
                    &mut credentials,
                    None,
                )
                .unwrap();
            }
            Acceptor {
                credentials,
                context: None,
            }
        }

        /// (answer token, signed in)
        fn step(&mut self, token: &[u8]) -> (Vec<u8>, bool) {
            let mut input_buffer = SecBuffer {
                cbBuffer: token.len() as u32,
                BufferType: SECBUFFER_TOKEN,
                pvBuffer: token.as_ptr() as *mut _,
            };
            let input = SecBufferDesc {
                ulVersion: SECBUFFER_VERSION,
                cBuffers: 1,
                pBuffers: &mut input_buffer,
            };
            let mut output_buffer = SecBuffer {
                cbBuffer: 0,
                BufferType: SECBUFFER_TOKEN,
                pvBuffer: std::ptr::null_mut(),
            };
            let mut output = SecBufferDesc {
                ulVersion: SECBUFFER_VERSION,
                cBuffers: 1,
                pBuffers: &mut output_buffer,
            };
            let mut new_context = self.context.unwrap_or_default();
            let mut attributes = 0u32;
            let status = unsafe {
                AcceptSecurityContext(
                    Some(&self.credentials),
                    self.context.as_ref().map(|c| c as *const SecHandle),
                    Some(&input),
                    ASC_REQ_ALLOCATE_MEMORY | ASC_REQ_CONNECTION,
                    SECURITY_NATIVE_DREP,
                    Some(&mut new_context),
                    Some(&mut output),
                    &mut attributes,
                    None,
                )
            };
            assert!(
                status == SEC_E_OK || status == SEC_I_CONTINUE_NEEDED,
                "server-side sign-in failed: {status:?}"
            );
            self.context = Some(new_context);
            let answer = if output_buffer.pvBuffer.is_null() {
                Vec::new()
            } else {
                unsafe {
                    let bytes = std::slice::from_raw_parts(
                        output_buffer.pvBuffer as *const u8,
                        output_buffer.cbBuffer as usize,
                    )
                    .to_vec();
                    let _ = FreeContextBuffer(output_buffer.pvBuffer);
                    bytes
                }
            };
            (answer, status == SEC_E_OK)
        }
    }

    /// A proxy that only lets in a Windows account signed in with `package`.
    /// Returns its port; the thread answers whether the sign-in succeeded.
    fn windows_sign_in_proxy(
        package: &'static str,
        offers: &'static str,
    ) -> (u16, std::thread::JoinHandle<bool>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let head = Head::parse(&read_head(&mut s).unwrap());
            assert!(head.header("proxy-authorization").is_none());
            s.write_all(
                format!("HTTP/1.1 407 Sign in\r\n{offers}Content-Length: 2\r\n\r\nno").as_bytes(),
            )
            .unwrap();
            let mut acceptor = Acceptor::new(package);
            for _ in 0..5 {
                let Ok(raw) = read_head(&mut s) else {
                    return false;
                };
                let head = Head::parse(&raw);
                let value = head.header("proxy-authorization").unwrap_or("").to_string();
                let Some((scheme, data)) = value.split_once(' ') else {
                    return false;
                };
                assert_eq!(scheme, package);
                let token = crate::net::secret::base64_decode(data).unwrap();
                let (answer, done) = acceptor.step(&token);
                if done {
                    s.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                        .unwrap();
                    return true;
                }
                s.write_all(
                    format!(
                        "HTTP/1.1 407 Continue\r\nProxy-Authenticate: {package} {}\r\nContent-Length: 0\r\n\r\n",
                        base64_encode(&answer)
                    )
                    .as_bytes(),
                )
                .unwrap();
            }
            false
        });
        (port, server)
    }

    #[test]
    fn windows_account_signs_in_with_ntlm() {
        let _settings = crate::net::TEST_CONFIG_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (port, server) = windows_sign_in_proxy("NTLM", "Proxy-Authenticate: NTLM\r\n");
        let (_stream, sign_in) = through_proxy("127.0.0.1", port, "site.example", 443).unwrap();
        assert_eq!(sign_in.as_deref(), Some("NTLM"));
        assert!(server.join().unwrap());
    }

    #[test]
    fn windows_account_signs_in_with_negotiate_first() {
        let _settings = crate::net::TEST_CONFIG_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (port, server) = windows_sign_in_proxy(
            "Negotiate",
            "Proxy-Authenticate: Negotiate\r\nProxy-Authenticate: NTLM\r\n",
        );
        let (_stream, sign_in) = through_proxy("127.0.0.1", port, "site.example", 443).unwrap();
        assert_eq!(sign_in.as_deref(), Some("Negotiate"));
        assert!(server.join().unwrap());
    }

    #[test]
    fn authority_splits_and_joins() {
        assert_eq!(
            split_authority("github.com:443"),
            Some(("github.com".into(), 443))
        );
        assert_eq!(split_authority("[::1]:8443"), Some(("::1".into(), 8443)));
        assert_eq!(split_authority("github.com"), None);
        assert_eq!(authority("::1", 443), "[::1]:443");
        assert_eq!(authority("a.b", 443), "a.b:443");
    }

    #[test]
    fn schemes_and_challenges_are_read() {
        let head = Head::parse(
            b"HTTP/1.1 407 Proxy Authentication Required\r\n\
              Proxy-Authenticate: Negotiate\r\nProxy-Authenticate: NTLM\r\n\
              Proxy-Authenticate: Basic realm=\"corp, inc\"\r\nContent-Length: 0\r\n\r\n",
        );
        assert_eq!(head.status(), 407);
        assert_eq!(head.schemes(), vec!["negotiate", "ntlm", "basic"]);
        let challenge =
            Head::parse(b"HTTP/1.1 407 x\r\nProxy-Authenticate: NTLM TlRMTVNTUAAC\r\n\r\n");
        assert_eq!(challenge.challenge("NTLM").unwrap(), b"NTLMSSP\0\x02");
        assert!(challenge.challenge("Negotiate").is_none());
        let combined = Head::parse(
            b"HTTP/1.1 407 x\r\nProxy-Authenticate: Basic realm=\"a, b\", NTLM TlRMTVNTUAAC, Negotiate\r\n\r\n",
        );
        assert_eq!(combined.challenge("NTLM").unwrap(), b"NTLMSSP\0\x02");
        assert!(combined.challenge("Negotiate").is_none());
        assert_eq!(combined.schemes(), vec!["basic", "ntlm", "negotiate"]);
    }

    /// A fake proxy that wants Basic sign-in, then relays to an echo server.
    #[test]
    fn basic_sign_in_opens_a_tunnel_that_relays() {
        let echo = TcpListener::bind("127.0.0.1:0").unwrap();
        let echo_port = echo.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut s, _) = echo.accept().unwrap();
            let mut buf = [0u8; 5];
            s.read_exact(&mut buf).unwrap();
            s.write_all(&buf).unwrap();
        });
        let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy_port = proxy.local_addr().unwrap().port();
        let expected = format!("Basic {}", base64_encode(b"user:pw"));
        std::thread::spawn(move || {
            let (mut s, _) = proxy.accept().unwrap();
            let head = Head::parse(&read_head(&mut s).unwrap());
            assert!(head.header("proxy-authorization").is_none());
            s.write_all(
                b"HTTP/1.1 407 Need\r\nProxy-Authenticate: Basic realm=\"x\"\r\nContent-Length: 4\r\n\r\nnope",
            )
            .unwrap();
            let head = Head::parse(&read_head(&mut s).unwrap());
            assert_eq!(head.header("proxy-authorization"), Some(expected.as_str()));
            let up = TcpStream::connect(("127.0.0.1", echo_port)).unwrap();
            s.write_all(b"HTTP/1.1 200 OK\r\n\r\n").unwrap();
            relay(s, up.try_clone().unwrap());
            let _ = up.shutdown(Shutdown::Both);
        });
        let mut stream = TcpStream::connect(("127.0.0.1", proxy_port)).unwrap();
        let (head, reusable) = exchange(&mut stream, "example.com:443", None).unwrap();
        assert_eq!(head.status(), 407);
        assert!(reusable);
        assert!(sign_in_basic(&mut stream, "example.com:443", "user", "pw").is_ok());
        stream.write_all(b"hello").unwrap();
        let mut back = [0u8; 5];
        stream.read_exact(&mut back).unwrap();
        assert_eq!(&back, b"hello");
    }

    /// The client stops sending and waits: the late answer still reaches it.
    #[test]
    fn relay_delivers_the_answer_after_the_client_finished_sending() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let server_port = server.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut s, _) = server.accept().unwrap();
            let mut request = Vec::new();
            s.read_to_end(&mut request).unwrap();
            assert_eq!(request, b"question");
            std::thread::sleep(Duration::from_millis(1500));
            s.write_all(b"answer").unwrap();
        });
        let front = TcpListener::bind("127.0.0.1:0").unwrap();
        let front_port = front.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (client_side, _) = front.accept().unwrap();
            let upstream = TcpStream::connect(("127.0.0.1", server_port)).unwrap();
            relay(client_side, upstream);
        });
        let mut client = TcpStream::connect(("127.0.0.1", front_port)).unwrap();
        client.write_all(b"question").unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        let mut answer = Vec::new();
        client.read_to_end(&mut answer).unwrap();
        assert_eq!(answer, b"answer");
    }

    /// A client that finished sending and then stopped reading cannot hold
    /// the tunnel past the deadline, even with a write blocked on it.
    #[test]
    fn relay_gives_up_on_a_client_that_stopped_reading() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let server_port = server.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut s, _) = server.accept().unwrap();
            let mut request = Vec::new();
            let _ = s.read_to_end(&mut request);
            let chunk = vec![7u8; 1 << 20];
            for _ in 0..200 {
                if s.write_all(&chunk).is_err() {
                    break;
                }
            }
        });
        let front = TcpListener::bind("127.0.0.1:0").unwrap();
        let front_port = front.local_addr().unwrap().port();
        let (ended_tx, ended_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (client_side, _) = front.accept().unwrap();
            let upstream = TcpStream::connect(("127.0.0.1", server_port)).unwrap();
            relay_with(client_side, upstream, Duration::from_secs(2));
            let _ = ended_tx.send(());
        });
        let client = TcpStream::connect(("127.0.0.1", front_port)).unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        // Never reads.
        assert!(
            ended_rx.recv_timeout(Duration::from_secs(20)).is_ok(),
            "the relay is still stuck"
        );
        drop(client);
    }

    #[test]
    fn chunked_407_body_is_drained() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let _ = read_head(&mut s).unwrap();
            s.write_all(
                b"HTTP/1.1 407 x\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nabcd\r\n0\r\n\r\n",
            )
            .unwrap();
            let _ = read_head(&mut s).unwrap();
            s.write_all(b"HTTP/1.1 200 OK\r\n\r\n").unwrap();
        });
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let (head, reusable) = exchange(&mut stream, "a:443", None).unwrap();
        assert_eq!(head.status(), 407);
        assert!(reusable);
        let (head, _) = exchange(&mut stream, "a:443", Some("Basic x")).unwrap();
        assert_eq!(head.status(), 200);
    }

    #[test]
    fn closing_407_is_not_reused() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let _ = read_head(&mut s).unwrap();
            s.write_all(b"HTTP/1.1 407 x\r\nConnection: close\r\n\r\n")
                .unwrap();
        });
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let (_, reusable) = exchange(&mut stream, "a:443", None).unwrap();
        assert!(!reusable);
    }
}
