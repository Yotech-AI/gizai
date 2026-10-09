//! A fake authorization server and MCP server on 127.0.0.1, modelled on Otus OS, for the MCP sign-in tests (GA-39):
//! the MCP at /api/mcp answers 401 with `WWW-Authenticate: Bearer resource_metadata="…"` without a valid token, and
//! initialize and tools/list (JSON over Streamable HTTP POST) with one; protected resource metadata (RFC 9728),
//! authorization server metadata (RFC 8414, or OpenID discovery), dynamic registration for public clients with loopback
//! redirects only (RFC 7591), the authorization code flow with PKCE S256 checked for real (RFC 7636), the resource
//! checked on every token request (RFC 8707), a refresh token replaced on every use (an old one is invalid_grant), and
//! revocation (RFC 7009). It counts and records what it got. Only std and serde_json, so both the agents crate's and
//! the app's tests can include it with `#[path]`. The "browser" here is a plain HTTP GET: nothing opens a real one.
#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// How the fake behaves; `Config::otus()` is Otus OS as it is.
#[derive(Debug, Clone)]
pub struct Config {
    /// Offers dynamic client registration.
    pub registration: bool,
    /// Offers token revocation.
    pub revocation: bool,
    /// Paths where the protected resource metadata is served.
    pub prm_paths: Vec<String>,
    /// The path the MCP's 401 names as `resource_metadata`; None: a bare `Bearer` challenge.
    pub header_prm: Option<String>,
    /// Paths where the authorization server metadata is served.
    pub as_paths: Vec<String>,
    /// Lists S256 in code_challenge_methods_supported (else only "plain").
    pub s256: bool,
    /// expires_in of the access tokens it gives, in seconds.
    pub access_ttl: u64,
    /// How long a refresh takes (widens the window for callers racing each other).
    pub refresh_delay: Duration,
    /// A client id it knows without registration (one you would enter), allowed any loopback redirect.
    pub known_client: Option<String>,
}

impl Config {
    pub fn otus() -> Config {
        Config {
            registration: true,
            revocation: true,
            prm_paths: vec!["/.well-known/oauth-protected-resource".into()],
            header_prm: Some("/.well-known/oauth-protected-resource".into()),
            as_paths: vec!["/.well-known/oauth-authorization-server".into()],
            s256: true,
            access_ttl: 3600,
            refresh_delay: Duration::ZERO,
            known_client: None,
        }
    }
}

/// What the fake got.
#[derive(Debug, Clone, Default)]
pub struct Log {
    /// "GET /path" of every request, in order.
    pub requests: Vec<String>,
    /// The JSON bodies of the registration requests.
    pub registrations: Vec<Value>,
    /// The query parameters of each visit to the sign-in page.
    pub authorizations: Vec<HashMap<String, String>>,
    /// The form fields of each token request.
    pub token_requests: Vec<HashMap<String, String>>,
    /// How many grant_type=refresh_token requests came in.
    pub refreshes: usize,
    /// The most refresh requests it was working on at the same time.
    pub max_refreshes_at_once: usize,
    /// (token, token_type_hint) of each revocation.
    pub revoked: Vec<(String, String)>,
    /// The bearer tokens the MCP accepted, in order.
    pub mcp_tokens: Vec<String>,
    /// How many MCP requests it refused with 401.
    pub mcp_refused: usize,
}

struct Code {
    client_id: String,
    redirect_uri: String,
    challenge: String,
    resource: String,
}

struct Inner {
    cfg: Config,
    log: Log,
    /// client id → its registered redirect uris.
    clients: HashMap<String, Vec<String>>,
    codes: HashMap<String, Code>,
    /// access token → (client id, runs out at)
    access: HashMap<String, (String, Instant)>,
    /// valid refresh token → client id
    refresh: HashMap<String, String>,
    n: u64,
    nonce: String,
    refreshing: usize,
}

impl Inner {
    fn next(&mut self, kind: &str) -> String {
        self.n += 1;
        format!("otus-{kind}-{}-{}", self.nonce, self.n)
    }

    fn mint(&mut self, client_id: &str) -> (String, String) {
        let access = self.next("at");
        let refresh = self.next("rt");
        let ttl = Duration::from_secs(self.cfg.access_ttl);
        self.access.insert(access.clone(), (client_id.to_string(), Instant::now() + ttl));
        self.refresh.insert(refresh.clone(), client_id.to_string());
        (access, refresh)
    }
}

/// The fake, listening on 127.0.0.1 until the test process ends.
#[derive(Clone)]
pub struct FakeOtus {
    /// "http://127.0.0.1:<port>"
    pub base: String,
    inner: Arc<Mutex<Inner>>,
}

impl FakeOtus {
    pub fn start(cfg: Config) -> FakeOtus {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().subsec_nanos();
        let mut clients = HashMap::new();
        if let Some(id) = &cfg.known_client {
            clients.insert(id.clone(), vec![]);
        }
        let inner = Arc::new(Mutex::new(Inner {
            cfg, log: Log::default(), clients, codes: HashMap::new(), access: HashMap::new(), refresh: HashMap::new(), n: 0, refreshing: 0,
            nonce: format!("{port}x{nanos:x}"),
        }));
        let base = format!("http://127.0.0.1:{port}");
        let fake = FakeOtus { base, inner };
        let me = fake.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let me = me.clone();
                std::thread::spawn(move || me.serve(stream));
            }
        });
        fake
    }

    pub fn mcp_url(&self) -> String {
        format!("{}/api/mcp", self.base)
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A copy of what it got so far.
    pub fn log(&self) -> Log {
        self.lock().log.clone()
    }

    pub fn refreshes(&self) -> usize {
        self.lock().log.refreshes
    }

    pub fn set_access_ttl(&self, secs: u64) {
        self.lock().cfg.access_ttl = secs;
    }

    pub fn set_refresh_delay(&self, d: Duration) {
        self.lock().cfg.refresh_delay = d;
    }

    /// Tokens for `client_id` as if it had signed in (the client is known from then on): (access, refresh).
    pub fn mint(&self, client_id: &str) -> (String, String) {
        let mut i = self.lock();
        i.clients.entry(client_id.to_string()).or_default();
        i.mint(client_id)
    }

    /// Whether the MCP takes `token` now.
    pub fn accepts(&self, token: &str) -> bool {
        self.lock().access.get(token).is_some_and(|(_, until)| *until > Instant::now())
    }

    /// Whether `refresh_token` can still be used.
    pub fn refresh_valid(&self, refresh_token: &str) -> bool {
        self.lock().refresh.contains_key(refresh_token)
    }

    /// Forgets every access token, the way a server that dropped its sessions would: the MCP answers 401 invalid_token.
    pub fn drop_access_tokens(&self) {
        self.lock().access.clear();
    }

    /// Withdraws every refresh token, the way an admin ending all sessions would: the next refresh is invalid_grant.
    pub fn withdraw_refresh_tokens(&self) {
        self.lock().refresh.clear();
    }

    // ---- the server side ----

    fn serve(&self, mut stream: TcpStream) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
        let Some(req) = read_request(&mut stream) else { return };
        let resp = self.handle(&req);
        let mut head = format!("HTTP/1.1 {} {}\r\n", resp.status, reason(resp.status));
        for (k, v) in &resp.headers {
            head.push_str(&format!("{k}: {v}\r\n"));
        }
        head.push_str(&format!("Content-Length: {}\r\nConnection: close\r\n\r\n", resp.body.len()));
        let _ = stream.write_all(head.as_bytes()).and_then(|()| stream.write_all(resp.body.as_bytes())).and_then(|()| stream.flush());
    }

    fn handle(&self, req: &Request) -> Resp {
        let base = self.base.clone();
        let mut i = self.lock();
        i.log.requests.push(format!("{} {}", req.method, req.path));
        let path = req.path.as_str();
        if req.method == "GET" && i.cfg.prm_paths.iter().any(|p| p == path) {
            return Resp::json(200, &json!({
                "resource": format!("{base}/api/mcp"),
                "authorization_servers": [base],
                "bearer_methods_supported": ["header"],
                "resource_name": "OTUS",
            }));
        }
        if req.method == "GET" && i.cfg.as_paths.iter().any(|p| p == path) {
            let mut doc = json!({
                "issuer": base,
                "authorization_endpoint": format!("{base}/oauth/authorize"),
                "token_endpoint": format!("{base}/oauth/token"),
                "response_types_supported": ["code"],
                "grant_types_supported": ["authorization_code", "refresh_token"],
                "code_challenge_methods_supported": if i.cfg.s256 { json!(["S256"]) } else { json!(["plain"]) },
                "token_endpoint_auth_methods_supported": ["none"],
            });
            if i.cfg.registration {
                doc["registration_endpoint"] = json!(format!("{base}/oauth/register"));
            }
            if i.cfg.revocation {
                doc["revocation_endpoint"] = json!(format!("{base}/oauth/revoke"));
            }
            return Resp::json(200, &doc);
        }
        match (req.method.as_str(), path) {
            ("POST", "/oauth/register") if i.cfg.registration => register(&mut i, req),
            ("GET", "/oauth/authorize") => sign_in_page(&mut i, req, &base),
            ("POST", "/oauth/token") => {
                let fields = parse_query(&String::from_utf8_lossy(&req.body));
                i.log.token_requests.push(fields.clone());
                if fields.get("grant_type").map(String::as_str) == Some("refresh_token") {
                    i.log.refreshes += 1;
                    i.refreshing += 1;
                    i.log.max_refreshes_at_once = i.log.max_refreshes_at_once.max(i.refreshing);
                    let delay = i.cfg.refresh_delay;
                    drop(i);
                    std::thread::sleep(delay);
                    let mut i = self.lock();
                    i.refreshing -= 1;
                    return refresh(&mut i, &fields, &base);
                }
                code_grant(&mut i, &fields, &base)
            }
            ("POST", "/oauth/revoke") if i.cfg.revocation => {
                let fields = parse_query(&String::from_utf8_lossy(&req.body));
                let token = fields.get("token").cloned().unwrap_or_default();
                i.log.revoked.push((token.clone(), fields.get("token_type_hint").cloned().unwrap_or_default()));
                i.refresh.remove(&token);
                i.access.remove(&token);
                Resp { status: 200, headers: vec![], body: String::new() }
            }
            ("POST", "/api/mcp") => mcp(&mut i, req, &base),
            _ => Resp::text(404, "Not found"),
        }
    }
}

fn register(i: &mut Inner, req: &Request) -> Resp {
    let body: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
    i.log.registrations.push(body.clone());
    if body.get("token_endpoint_auth_method").and_then(Value::as_str) != Some("none") {
        return oauth_error(400, "invalid_client_metadata", "only public clients (token_endpoint_auth_method none)");
    }
    let uris: Vec<String> = body.get("redirect_uris").and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_default();
    if uris.is_empty() || !uris.iter().all(|u| is_loopback_redirect(u)) {
        return oauth_error(400, "invalid_redirect_uri", "only http loopback redirects");
    }
    let client_id = i.next("client");
    i.clients.insert(client_id.clone(), uris.clone());
    Resp::json(201, &json!({
        "client_id": client_id,
        "client_name": body.get("client_name").cloned().unwrap_or(Value::Null),
        "redirect_uris": uris,
        "token_endpoint_auth_method": "none",
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
    }))
}

fn is_loopback_redirect(u: &str) -> bool {
    u.starts_with("http://127.0.0.1:") || u.starts_with("http://localhost:") || u.starts_with("http://[::1]:")
}

/// The sign-in page: checks what Otus checks, then sends the "browser" back to the redirect with a code and the state.
fn sign_in_page(i: &mut Inner, req: &Request, base: &str) -> Resp {
    let q = parse_query(&req.query);
    i.log.authorizations.push(q.clone());
    let get = |k: &str| q.get(k).cloned().unwrap_or_default();
    let (client_id, redirect_uri, state, challenge) = (get("client_id"), get("redirect_uri"), get("state"), get("code_challenge"));
    let refused = |why: &str| Resp::text(400, &format!("refused: {why}"));
    if get("response_type") != "code" {
        return refused("response_type must be code");
    }
    let Some(allowed) = i.clients.get(&client_id) else { return refused("unknown client_id") };
    let known = i.cfg.known_client.as_deref() == Some(client_id.as_str());
    if !(allowed.contains(&redirect_uri) || (known && is_loopback_redirect(&redirect_uri))) {
        return refused("redirect_uri not registered for this client");
    }
    if get("code_challenge_method") != "S256" || challenge.len() != 43 {
        return refused("PKCE S256 required");
    }
    if state.is_empty() {
        return refused("state required");
    }
    if get("resource") != format!("{base}/api/mcp") {
        return refused("resource must be the MCP address");
    }
    let code = i.next("code");
    i.codes.insert(code.clone(), Code { client_id, redirect_uri: redirect_uri.clone(), challenge, resource: get("resource") });
    let location = format!("{redirect_uri}?code={}&state={}", pct(&code), pct(&state));
    Resp { status: 302, headers: vec![("Location".into(), location)], body: String::new() }
}

fn code_grant(i: &mut Inner, f: &HashMap<String, String>, base: &str) -> Resp {
    let get = |k: &str| f.get(k).cloned().unwrap_or_default();
    if get("grant_type") != "authorization_code" {
        return oauth_error(400, "unsupported_grant_type", "");
    }
    // A code is used once, right or wrong.
    let Some(code) = i.codes.remove(&get("code")) else { return oauth_error(400, "invalid_grant", "unknown or used code") };
    if code.client_id != get("client_id") || code.redirect_uri != get("redirect_uri") {
        return oauth_error(400, "invalid_grant", "client or redirect_uri mismatch");
    }
    if get("resource") != code.resource || code.resource != format!("{base}/api/mcp") {
        return oauth_error(400, "invalid_target", "resource mismatch");
    }
    if b64url(&sha256(get("code_verifier").as_bytes())) != code.challenge {
        return oauth_error(400, "invalid_grant", "PKCE verification failed");
    }
    let (access, refresh) = i.mint(&code.client_id);
    tokens(i, &access, &refresh)
}

fn refresh(i: &mut Inner, f: &HashMap<String, String>, base: &str) -> Resp {
    let get = |k: &str| f.get(k).cloned().unwrap_or_default();
    let old = get("refresh_token");
    let Some(client) = i.refresh.get(&old).cloned() else { return oauth_error(400, "invalid_grant", "refresh token unknown, used or revoked") };
    if client != get("client_id") {
        return oauth_error(400, "invalid_grant", "client mismatch");
    }
    if get("resource") != format!("{base}/api/mcp") {
        return oauth_error(400, "invalid_target", "resource mismatch");
    }
    // Replaced on every use.
    i.refresh.remove(&old);
    let (access, refresh) = i.mint(&client);
    tokens(i, &access, &refresh)
}

fn tokens(i: &Inner, access: &str, refresh: &str) -> Resp {
    Resp::json(200, &json!({
        "access_token": access, "token_type": "Bearer", "expires_in": i.cfg.access_ttl, "refresh_token": refresh,
    }))
}

fn mcp(i: &mut Inner, req: &Request, base: &str) -> Resp {
    let token = req.header("authorization").and_then(|v| v.strip_prefix("Bearer ")).map(str::trim).unwrap_or_default().to_string();
    let valid = i.access.get(&token).is_some_and(|(_, until)| *until > Instant::now());
    if !valid {
        i.log.mcp_refused += 1;
        let mut challenge = match &i.cfg.header_prm {
            Some(p) => format!("Bearer resource_metadata=\"{base}{p}\""),
            None => "Bearer".to_string(),
        };
        if !token.is_empty() {
            challenge.push_str(if i.cfg.header_prm.is_some() { ", error=\"invalid_token\"" } else { " error=\"invalid_token\"" });
        }
        return Resp { status: 401, headers: vec![("WWW-Authenticate".into(), challenge)], body: String::new() };
    }
    i.log.mcp_tokens.push(token);
    let msg: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
    let Some(id) = msg.get("id").cloned() else {
        return Resp { status: 202, headers: vec![], body: String::new() };
    };
    let result = match msg.get("method").and_then(Value::as_str).unwrap_or_default() {
        "initialize" => json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "otus-os", "version": "1.4.0"},
        }),
        "tools/list" => json!({"tools": [
            {"name": "list_tasks", "description": "Lists your tasks in Otus OS", "inputSchema": {"type": "object", "properties": {}},
             "annotations": {"readOnlyHint": true}},
            {"name": "create_task", "description": "Creates a task in Otus OS", "inputSchema": {"type": "object", "properties": {"title": {"type": "string"}}}},
        ]}),
        _ => return Resp::json(200, &json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "Method not found"}})),
    };
    Resp::json(200, &json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn oauth_error(status: u16, code: &str, description: &str) -> Resp {
    Resp::json(status, &json!({"error": code, "error_description": description}))
}

struct Request {
    method: String,
    path: String,
    query: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

struct Resp {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Resp {
    fn json(status: u16, v: &Value) -> Resp {
        Resp { status, headers: vec![("Content-Type".into(), "application/json".into()), ("Cache-Control".into(), "no-store".into())], body: v.to_string() }
    }

    fn text(status: u16, s: &str) -> Resp {
        Resp { status, headers: vec![("Content-Type".into(), "text/plain".into())], body: s.to_string() }
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        302 => "Found",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "Other",
    }
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut r = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    r.read_line(&mut line).ok()?;
    let mut words = line.split_whitespace();
    let (method, target) = (words.next()?.to_string(), words.next()?.to_string());
    let mut headers = vec![];
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).ok()? == 0 {
            break;
        }
        let h = h.trim_end_matches(['\r', '\n']);
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let req_header = |n: &str| headers.iter().find(|(k, _): &&(String, String)| k.eq_ignore_ascii_case(n)).map(|(_, v)| v.clone());
    let mut body = vec![];
    if let Some(len) = req_header("content-length").and_then(|l| l.parse::<usize>().ok()) {
        body = vec![0; len];
        r.read_exact(&mut body).ok()?;
    } else if req_header("transfer-encoding").is_some_and(|t| t.eq_ignore_ascii_case("chunked")) {
        loop {
            let mut size = String::new();
            r.read_line(&mut size).ok()?;
            let n = usize::from_str_radix(size.trim().split(';').next().unwrap_or("0"), 16).ok()?;
            let mut chunk = vec![0; n + 2];
            r.read_exact(&mut chunk).ok()?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..n]);
        }
    }
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.clone(), String::new()),
    };
    Some(Request { method, path, query, headers, body })
}

// ---- the "browser" and other plain HTTP from the test ----

/// A plain HTTP/1.1 request to an http://127.0.0.1 address: (status, headers, body). Never follows redirects.
pub fn http(method: &str, url: &str, headers: &[(&str, &str)], body: &str) -> (u16, Vec<(String, String)>, String) {
    let rest = url.strip_prefix("http://").expect("only plain http on this computer");
    let (authority, target) = match rest.find('/') {
        Some(at) => (&rest[..at], &rest[at..]),
        None => (rest, "/"),
    };
    let mut s = TcpStream::connect(authority).unwrap_or_else(|e| panic!("connect {authority}: {e}"));
    s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    let mut req = format!("{method} {target} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\nContent-Length: {}\r\n", body.len());
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(body);
    s.write_all(req.as_bytes()).unwrap();
    let mut raw = vec![];
    let _ = s.read_to_end(&mut raw);
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let mut lines = head.lines();
    let status = lines.next().and_then(|l| l.split_whitespace().nth(1)).and_then(|s| s.parse().ok()).unwrap_or(0);
    let headers = lines.filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect();
    (status, headers, body.to_string())
}

pub fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

/// The browser on the sign-in page: the address the server sends it back to (the redirect with code and state).
pub fn authorize(authorize_url: &str) -> String {
    let (status, headers, body) = http("GET", authorize_url, &[], "");
    assert_eq!(status, 302, "the sign-in page refused: {body}");
    header(&headers, "location").expect("a redirect back").to_string()
}

/// The whole browser part of a sign-in: the sign-in page, then back to Gizai's listener. (status, page) of the latter.
pub fn play_browser(authorize_url: &str) -> (u16, String) {
    let back = authorize(authorize_url);
    let (status, _, page) = http("GET", &back, &[], "");
    (status, page)
}

/// The MCP's answer status to tools/list with `token`.
pub fn mcp_status(mcp_url: &str, token: &str) -> u16 {
    let auth = format!("Bearer {token}");
    http("POST", mcp_url, &[("Authorization", auth.as_str()), ("Content-Type", "application/json")],
         r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#).0
}

/// The query parameters of an address (or a form body), decoded.
pub fn query_of(url: &str) -> HashMap<String, String> {
    parse_query(url.split_once('?').map_or("", |(_, q)| q))
}

pub fn parse_query(q: &str) -> HashMap<String, String> {
    let mut all = HashMap::new();
    for pair in q.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        all.entry(unpct(k)).or_insert_with(|| unpct(v));
    }
    all
}

fn unpct(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        let hex = b.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
        match (b[i], hex) {
            (b'+', _) => {
                out.push(b' ');
                i += 1;
            }
            (b'%', Some(v)) => {
                out.push(v);
                i += 3;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn pct(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

// ---- PKCE S256, done here so the check doesn't lean on Gizai's own code ----

/// Base64url without padding (RFC 4648 §5).
pub fn b64url(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..=chunk.len() {
            out.push(char::from(ABC[((n >> (18 - 6 * i)) & 63) as usize]));
        }
    }
    out
}

/// SHA-256 (FIPS 180-4).
pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for t in 0..16 {
            w[t] = u32::from_be_bytes([chunk[4 * t], chunk[4 * t + 1], chunk[4 * t + 2], chunk[4 * t + 3]]);
        }
        for t in 16..64 {
            let s0 = w[t - 15].rotate_right(7) ^ w[t - 15].rotate_right(18) ^ (w[t - 15] >> 3);
            let s1 = w[t - 2].rotate_right(17) ^ w[t - 2].rotate_right(19) ^ (w[t - 2] >> 10);
            w[t] = w[t - 16].wrapping_add(s0).wrapping_add(w[t - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for t in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[t]).wrapping_add(w[t]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 32];
    for (i, v) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}
