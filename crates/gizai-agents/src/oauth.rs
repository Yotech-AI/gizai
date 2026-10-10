//! Signing in to MCP servers that ask for it, the way the MCP authorization spec (2025-06-18) has it, and keeping those
//! sign-ins fresh.
//! - `discover`: the MCP server's protected resource metadata (RFC 9728) names its authorization server, whose metadata
//!   (RFC 8414, else OpenID discovery) gives the addresses to sign in with.
//! - `begin` and `finish`: Gizai registers itself where the server lets it (RFC 7591, as a public client without a secret),
//!   your browser opens the server's sign-in page (authorization code flow with PKCE S256 and a random state), and the
//!   answer comes back to a listener on 127.0.0.1. Every request names the MCP server as the resource (RFC 8707), so its
//!   tokens only work there.
//! - `TokenStore`: the tokens in your keychain, renewed by one caller at a time per server (a server may replace the
//!   refresh token on every use), and withdrawn on sign-out where the server offers that (RFC 7009).
//!
//! Only https is used, except for addresses on this computer (127.0.0.1, localhost, [::1]). Tokens and codes never appear
//! in errors, Debug output or logs. Everything here blocks: call it from `spawn_blocking`.
use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use ureq::ResponseExt;

use crate::secrets::Keychain;

/// How long one request to a server may take.
const HTTP_LIMIT: Duration = Duration::from_secs(30);
/// The MCP version whose authorization this follows, sent along while discovering.
const MCP_VERSION: &str = "2025-06-18";
/// The most of an answer Gizai reads.
const BODY_LIMIT: u64 = 1 << 20;

/// What a server without dynamic client registration needs from you.
pub const NO_REGISTRATION: &str = "This server doesn't let Gizai register itself: enter a client id for Gizai from the service's settings";

/// What a `WWW-Authenticate: Bearer …` header (on an MCP server's 401 or 403) says about signing in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WwwAuth {
    /// Where the server's protected resource metadata is (RFC 9728).
    pub resource_metadata: Option<String>,
    /// The access to ask for, space-separated.
    pub scope: Option<String>,
    /// Why the server refused, like `invalid_token` or `insufficient_scope`.
    pub error: Option<String>,
}

/// Reads a `WWW-Authenticate` header, which may hold several challenges joined by commas: the Bearer challenge's
/// parameters, else the first of each name in any challenge.
pub fn parse_www_authenticate(header: &str) -> WwwAuth {
    let c: Vec<char> = header.chars().collect();
    // (in the Bearer challenge, name, value)
    let mut params: Vec<(bool, String, String)> = vec![];
    let (mut i, mut bearer, mut item_start) = (0, false, true);
    // The last value was unquoted: a loose word after it (`scope=read write`) still belongs to it.
    let mut open = false;
    while i < c.len() {
        if c[i] == ',' {
            (item_start, open) = (true, false);
            i += 1;
            continue;
        }
        if c[i].is_whitespace() {
            i += 1;
            continue;
        }
        if c[i] == '"' {
            quoted(&c, &mut i);
            (item_start, open) = (false, false);
            continue;
        }
        let start = i;
        while i < c.len() && !matches!(c[i], ',' | '=' | '"') && !c[i].is_whitespace() {
            i += 1;
        }
        if i == start {
            // A stray '='.
            i += 1;
            continue;
        }
        let name: String = c[start..i].iter().collect();
        let mut j = i;
        while j < c.len() && c[j].is_whitespace() {
            j += 1;
        }
        if c.get(j) == Some(&'=') {
            i = j + 1;
            while i < c.len() && c[i].is_whitespace() {
                i += 1;
            }
            let value = if c.get(i) == Some(&'"') {
                open = false;
                quoted(&c, &mut i)
            } else {
                let from = i;
                while i < c.len() && c[i] != ',' && !c[i].is_whitespace() {
                    i += 1;
                }
                open = true;
                c[from..i].iter().collect()
            };
            params.push((bearer, name.to_ascii_lowercase(), value));
        } else if item_start {
            bearer = name.eq_ignore_ascii_case("bearer");
            open = false;
        } else if open && let Some(last) = params.last_mut() {
            last.2.push(' ');
            last.2.push_str(&name);
        }
        item_start = false;
    }
    let find = |name: &str| {
        let pick = |only_bearer: bool| params.iter().find(|(b, n, v)| (*b || !only_bearer) && n == name && !v.trim().is_empty());
        pick(true).or_else(|| pick(false)).map(|(_, _, v)| v.trim().to_string())
    };
    WwwAuth { resource_metadata: find("resource_metadata"), scope: find("scope"), error: find("error") }
}

/// A quoted string from `c[*i] == '"'` on, its backslash escapes undone; `*i` ends after the closing quote.
fn quoted(c: &[char], i: &mut usize) -> String {
    let mut out = String::new();
    *i += 1;
    while *i < c.len() {
        match c[*i] {
            '\\' => {
                if let Some(&next) = c.get(*i + 1) {
                    out.push(next);
                }
                *i += 2;
            }
            '"' => {
                *i += 1;
                break;
            }
            ch => {
                out.push(ch);
                *i += 1;
            }
        }
    }
    out
}

/// How to sign in to one MCP server, as `discover` found it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Discovery {
    /// The MCP url, named as the resource in every request (RFC 8707). Its scheme and host are in lowercase, without a
    /// fragment, and spelled the way the server's metadata spells it when that is the same address.
    pub resource: String,
    /// The authorization server: the first in the metadata's `authorization_servers`, else the MCP url's origin.
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    /// None: Gizai can't register itself, so it needs a client id from the service.
    pub registration_endpoint: Option<String>,
    /// None: signing out can only forget the tokens.
    pub revocation_endpoint: Option<String>,
    /// The access to ask for, space-separated: the 401's `scope`, else the metadata's `scopes_supported`.
    pub scope: Option<String>,
    /// The name the server gives itself (`resource_name`), like "OTUS".
    pub service_name: Option<String>,
}

/// Finds how to sign in to the MCP server at `mcp_url`; `www_authenticate` is its 401's header, when there was one.
/// The protected resource metadata comes from the header's `resource_metadata`, else from the well-known address with the
/// MCP url's path, then without; when there is none, the MCP url's origin is taken as the authorization server.
pub fn discover(mcp_url: &str, www_authenticate: Option<&str>) -> Result<Discovery, String> {
    let mcp = checked(mcp_url)?;
    let origin = mcp.origin();
    let www = www_authenticate.map(parse_www_authenticate).unwrap_or_default();
    let mut places = vec![];
    if let Some(url) = &www.resource_metadata {
        places.push(if url.starts_with('/') { format!("{origin}{url}") } else { url.clone() });
    }
    let path = mcp.path.trim_end_matches('/');
    if !path.is_empty() {
        places.push(format!("{origin}/.well-known/oauth-protected-resource{path}"));
    }
    places.push(format!("{origin}/.well-known/oauth-protected-resource"));
    let mut metadata = None;
    for url in &places {
        if let Some(doc) = json_at(url)? {
            metadata = Some(doc);
            break;
        }
    }

    let mut resource = mcp.canonical();
    let mut issuer = origin;
    let mut scope = www.scope.clone();
    let mut service_name = None;
    if let Some(doc) = &metadata {
        if let Some(declared) = text(doc, "resource") {
            if !under(&resource, &declared) {
                return Err(format!("{}'s sign-in details are for another address ({}), not {}", mcp.host(), shown(&declared), shown(mcp_url)));
            }
            if same_address(&resource, &declared) {
                resource = declared;
            }
        }
        let servers = doc.get("authorization_servers").and_then(Value::as_array);
        if let Some(first) = servers.and_then(|all| all.iter().find_map(|s| s.as_str().map(str::trim).filter(|s| !s.is_empty()))) {
            issuer = first.to_string();
        }
        if scope.is_none() {
            let all: Vec<&str> = doc.get("scopes_supported").and_then(Value::as_array)
                .map(|all| all.iter().filter_map(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).collect())
                .unwrap_or_default();
            scope = (!all.is_empty()).then(|| all.join(" "));
        }
        service_name = text(doc, "resource_name");
    }

    let meta = authorization_server(&issuer)?;
    let needed = |key: &str, what: &str| text(&meta, key).ok_or_else(|| format!("{}'s OAuth details have no {what}", host_of(&issuer)));
    let authorization_endpoint = needed("authorization_endpoint", "sign-in page (authorization_endpoint)")?;
    let token_endpoint = needed("token_endpoint", "token address (token_endpoint)")?;
    checked(&authorization_endpoint)?;
    checked(&token_endpoint)?;
    let registration_endpoint = text(&meta, "registration_endpoint");
    if let Some(url) = &registration_endpoint {
        checked(url)?;
    }
    // Only used to withdraw tokens on sign-out: one Gizai can't use safely is left out rather than stopping the sign-in.
    let revocation_endpoint = text(&meta, "revocation_endpoint").filter(|url| checked(url).is_ok());
    if let Some(methods) = meta.get("code_challenge_methods_supported").and_then(Value::as_array)
        && !methods.iter().any(|m| m.as_str() == Some("S256")) {
        return Err(format!("{} doesn't support PKCE with S256, which Gizai needs to sign in safely", host_of(&issuer)));
    }
    Ok(Discovery {
        resource, issuer, authorization_endpoint, token_endpoint, registration_endpoint, revocation_endpoint, scope, service_name,
    })
}

/// The authorization server's metadata: RFC 8414 first, then OpenID discovery, with the issuer's path where it has one.
fn authorization_server(issuer: &str) -> Result<Value, String> {
    let a = checked(issuer)?;
    let (origin, path) = (a.origin(), a.path.trim_end_matches('/'));
    let mut places = vec![
        format!("{origin}/.well-known/oauth-authorization-server{path}"),
        format!("{origin}/.well-known/openid-configuration{path}"),
    ];
    if !path.is_empty() {
        places.push(format!("{origin}{path}/.well-known/openid-configuration"));
    }
    for url in &places {
        if let Some(doc) = json_at(url)? {
            return Ok(doc);
        }
    }
    Err(format!("Couldn't find how to sign in to {}: it publishes no OAuth details (authorization server metadata) for {}", a.host(), shown(issuer)))
}

/// A sign-in under way: the address to open in your browser, and the listener on 127.0.0.1 waiting for the answer.
pub struct Pending {
    pub authorize_url: String,
    pub redirect_uri: String,
    pub client_id: String,
    listener: TcpListener,
    state: String,
    verifier: String,
    discovery: Discovery,
    cancelled: Arc<AtomicBool>,
}

impl Pending {
    /// Ends `finish`'s wait early, e.g. when you close the sign-in.
    pub fn cancel_handle(&self) -> CancelHandle {
        CancelHandle(self.cancelled.clone())
    }
}

impl std::fmt::Debug for Pending {
    // No state or code verifier, and the sign-in address without its query.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pending").field("authorize_url", &shown(&self.authorize_url)).field("redirect_uri", &self.redirect_uri)
            .field("client_id", &self.client_id).finish_non_exhaustive()
    }
}

/// Cancels the sign-in `finish` waits for: it returns "The sign-in was cancelled" within a moment.
#[derive(Debug, Clone)]
pub struct CancelHandle(Arc<AtomicBool>);

impl CancelHandle {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// Binds 127.0.0.1 on a free port, registers Gizai when `client_id` is None (a server without registration gives
/// `NO_REGISTRATION`), and builds the sign-in address with PKCE (S256), a random state, the resource and the scope.
pub fn begin(d: &Discovery, client_id: Option<&str>) -> Result<Pending, String> {
    checked(&d.authorization_endpoint)?;
    checked(&d.token_endpoint)?;
    let no_port = |e: std::io::Error| format!("Couldn't open a port on 127.0.0.1 for the sign-in's answer: {e}");
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(no_port)?;
    let port = listener.local_addr().map_err(no_port)?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");
    let client_id = match client_id.map(str::trim).filter(|id| !id.is_empty()) {
        Some(id) => id.to_string(),
        None => register(d, &redirect_uri)?,
    };
    let verifier = random_code()?;
    let challenge = b64url(&Sha256::digest(verifier.as_bytes()));
    let state = random_code()?;
    let mut fields = vec![
        ("response_type", "code"),
        ("client_id", client_id.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
        ("state", state.as_str()),
        ("resource", d.resource.as_str()),
    ];
    if let Some(scope) = d.scope.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        fields.push(("scope", scope));
    }
    let base = d.authorization_endpoint.trim().split('#').next().unwrap_or_default();
    let authorize_url = format!("{base}{}{}", if base.contains('?') { "&" } else { "?" }, form(&fields));
    Ok(Pending {
        authorize_url, redirect_uri, client_id, listener, state, verifier, discovery: d.clone(), cancelled: Arc::new(AtomicBool::new(false)),
    })
}

/// Registers Gizai as a public client, without a secret (RFC 7591), and returns its client id.
fn register(d: &Discovery, redirect_uri: &str) -> Result<String, String> {
    let Some(url) = d.registration_endpoint.as_deref().filter(|url| !url.trim().is_empty()) else {
        return Err(NO_REGISTRATION.to_string());
    };
    let mut body = json!({
        "client_name": "Gizai",
        "redirect_uris": [redirect_uri],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
    });
    if let Some(scope) = d.scope.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        body["scope"] = json!(scope);
    }
    let answer = post_json(url, &body)?;
    let host = host_of(url);
    if !answer.ok() {
        let hint = if matches!(answer.status, 401 | 403) { ": enter a client id for Gizai from the service's settings" } else { "" };
        return Err(format!("{host} didn't let Gizai register itself ({}){hint}", refusal(&answer)));
    }
    serde_json::from_str::<Value>(&answer.body).ok().and_then(|doc| text(&doc, "client_id"))
        .ok_or_else(|| format!("{host} registered Gizai but gave no client id"))
}

/// Waits for ONE answer on the listener: `GET /callback?code=…&state=…` (or `?error=…`) with the right state. Anything
/// else, like a wrong state or another path, is answered 400 and the wait goes on. The right answer closes the listener;
/// its code is exchanged at the token endpoint (with the code verifier, redirect uri, client id and resource), and the
/// browser gets a short page saying how that went ("Gizai is signed in. You can close this tab." or the error).
/// Gives up after `timeout` (callers pass 10 minutes): "The sign-in wasn't finished within 10 minutes".
pub fn finish(p: Pending, timeout: Duration) -> Result<Saved, String> {
    let deadline = Instant::now().checked_add(timeout);
    let broken = |e: std::io::Error| format!("The sign-in's listener on 127.0.0.1 stopped: {e}");
    p.listener.set_nonblocking(true).map_err(broken)?;
    let (mut stream, answer) = loop {
        if p.cancelled.load(Ordering::SeqCst) {
            return Err("The sign-in was cancelled".to_string());
        }
        let left = deadline.map(|d| d.saturating_duration_since(Instant::now()));
        if left.is_some_and(|left| left.is_zero()) {
            return Err(format!("The sign-in wasn't finished within {}", span(timeout)));
        }
        match p.listener.accept() {
            Ok((stream, _)) => {
                if let Some(found) = callback(stream, &p.state, left) {
                    break found;
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => std::thread::sleep(left.unwrap_or(Duration::MAX).min(Duration::from_millis(50))),
            Err(e) if matches!(e.kind(), ErrorKind::Interrupted | ErrorKind::ConnectionAborted) => {}
            Err(e) => return Err(broken(e)),
        }
    };
    // The answer is in: nothing else may come in on this port.
    drop(p.listener);
    let result = answer
        .map_err(|why| format!("{} didn't sign Gizai in: {why}", host_of(&p.discovery.authorization_endpoint)))
        .and_then(|code| exchange(&p.discovery, &p.client_id, &p.redirect_uri, &p.verifier, &code));
    let page_text = match &result {
        Ok(_) => "Gizai is signed in. You can close this tab.".to_string(),
        Err(e) => format!("Gizai couldn't sign in. {e}. You can close this tab and try again in Gizai."),
    };
    respond(&mut stream, "200 OK", &page(&page_text));
    result
}

/// One connection to the listener. The sign-in's answer, its state checked: the connection and the code, or why the
/// sign-in failed. Anything else is answered 400 here, and None keeps the wait going.
fn callback(mut stream: TcpStream, state: &str, left: Option<Duration>) -> Option<(TcpStream, Result<String, String>)> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(left.unwrap_or(Duration::MAX).clamp(Duration::from_millis(100), Duration::from_secs(5))));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let head = read_head(&mut stream)?;
    let mut words = head.lines().next().unwrap_or_default().split_whitespace();
    let (method, target) = (words.next().unwrap_or_default(), words.next().unwrap_or_default());
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if method != "GET" || path != "/callback" {
        respond(&mut stream, "400 Bad Request", &page("This address is only for Gizai's sign-in."));
        return None;
    }
    let q = query_params(query);
    if !q.get("state").is_some_and(|s| same_secret(s, state)) {
        respond(&mut stream, "400 Bad Request", &page("This answer isn't for the sign-in Gizai is waiting for. Go back to Gizai and try again."));
        return None;
    }
    let answer = match (q.get("code").filter(|c| !c.is_empty()), q.get("error")) {
        (_, Some(error)) => Err(explain(&clean_code(error), q.get("error_description").map(|d| clean(d)).as_deref())),
        (Some(code), None) => Ok(code.clone()),
        (None, None) => Err("its answer had no sign-in code".to_string()),
    };
    Some((stream, answer))
}

/// A request's head, up to the blank line (at most 16 KiB); None when nothing could be read.
fn read_head(stream: &mut TcpStream) -> Option<String> {
    let mut head = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        head.extend_from_slice(&chunk[..n]);
        if head.windows(4).any(|w| w == b"\r\n\r\n") || head.windows(2).any(|w| w == b"\n\n") || head.len() > 16 * 1024 {
            break;
        }
    }
    (!head.is_empty()).then(|| String::from_utf8_lossy(&head).into_owned())
}

/// Answers the browser with `body` and ends the connection.
fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let head = format!("HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\n\
                        Referrer-Policy: no-referrer\r\nConnection: close\r\n\r\n", body.len());
    let _ = stream.write_all(head.as_bytes()).and_then(|()| stream.write_all(body.as_bytes())).and_then(|()| stream.flush());
}

/// A short plain page with one paragraph.
fn page(text: &str) -> String {
    format!("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Gizai</title></head>\
             <body style=\"background:#15171c;color:#e9eaee;font:18px/1.5 system-ui,sans-serif;padding:3em\"><p>{}</p></body></html>",
            html(text))
}

/// Trades the sign-in's code for tokens at the token endpoint.
fn exchange(d: &Discovery, client_id: &str, redirect_uri: &str, verifier: &str, code: &str) -> Result<Saved, String> {
    let fields = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("code_verifier", verifier),
        ("resource", d.resource.as_str()),
    ];
    let answer = post_form(&d.token_endpoint, &fields)?;
    let t = tokens(&answer).map_err(|(_, why)| format!("{} didn't sign Gizai in: {why}", host_of(&d.token_endpoint)))?;
    Ok(Saved {
        access_token: t.access_token,
        refresh_token: t.refresh_token,
        expires_at: t.expires_at,
        client_id: client_id.to_string(),
        token_endpoint: d.token_endpoint.clone(),
        revocation_endpoint: d.revocation_endpoint.clone(),
        resource: d.resource.clone(),
        issuer: d.issuer.clone(),
        scope: t.scope.or_else(|| d.scope.clone()),
    })
}

/// What the keychain keeps for a signed-in server (as JSON under one key).
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// When the access token runs out, in ms since the epoch; None when the server didn't say.
    pub expires_at: Option<i64>,
    pub client_id: String,
    pub token_endpoint: String,
    pub revocation_endpoint: Option<String>,
    /// The MCP url the tokens are for (sent again with every refresh).
    pub resource: String,
    pub issuer: String,
    pub scope: Option<String>,
}

impl Saved {
    /// Whether the access token is good for at least `min_valid` more (one without an expiry always is).
    pub fn lasts(&self, min_valid: Duration) -> bool {
        let min = i64::try_from(min_valid.as_millis()).unwrap_or(i64::MAX);
        self.expires_at.is_none_or(|at| at.saturating_sub(now_ms()) >= min)
    }
}

impl std::fmt::Debug for Saved {
    // Never the tokens.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Saved")
            .field("access_token", &format_args!("•••"))
            .field("refresh_token", &self.refresh_token.as_ref().map(|_| format_args!("•••")))
            .field("expires_at", &self.expires_at)
            .field("client_id", &self.client_id)
            .field("token_endpoint", &self.token_endpoint)
            .field("revocation_endpoint", &self.revocation_endpoint)
            .field("resource", &self.resource)
            .field("issuer", &self.issuer)
            .field("scope", &self.scope)
            .finish()
    }
}

/// Why a refresh didn't work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshError {
    /// invalid_grant and the like: sign in again.
    Refused(String),
    /// The server couldn't be reached or answered strangely: try again later.
    Failed(String),
}

impl std::fmt::Display for RefreshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RefreshError::Refused(why) | RefreshError::Failed(why) => f.write_str(why),
        }
    }
}

/// grant_type=refresh_token with client_id and resource. A new refresh token replaces the old; without one the old stays.
pub fn refresh(s: &Saved) -> Result<Saved, RefreshError> {
    let host = host_of(&s.token_endpoint);
    let Some(old) = s.refresh_token.as_deref().filter(|t| !t.is_empty()) else {
        return Err(RefreshError::Refused(format!("The sign-in to {host} has run out and can't be renewed: sign in again")));
    };
    let fields = [
        ("grant_type", "refresh_token"),
        ("refresh_token", old),
        ("client_id", s.client_id.as_str()),
        ("resource", s.resource.as_str()),
    ];
    let answer = post_form(&s.token_endpoint, &fields).map_err(RefreshError::Failed)?;
    match tokens(&answer) {
        Ok(t) => Ok(Saved {
            access_token: t.access_token,
            refresh_token: t.refresh_token.or_else(|| s.refresh_token.clone()),
            expires_at: t.expires_at,
            scope: t.scope.or_else(|| s.scope.clone()),
            ..s.clone()
        }),
        Err((code, why)) if answer.status == 401 || code.as_deref().is_some_and(sign_in_again) => {
            Err(RefreshError::Refused(format!("{host} no longer accepts the sign-in ({why}): sign in again")))
        }
        Err((_, why)) => Err(RefreshError::Failed(format!("{host} didn't renew the sign-in: {why}"))),
    }
}

/// OAuth errors that mean the sign-in itself is no good any more.
fn sign_in_again(code: &str) -> bool {
    matches!(code, "invalid_grant" | "invalid_client" | "unauthorized_client" | "invalid_scope" | "invalid_target" | "access_denied")
}

/// RFC 7009 revocation of the refresh token (else the access token) when the server offers it; Ok(false) when it has none.
pub fn revoke(s: &Saved) -> Result<bool, String> {
    let Some(url) = s.revocation_endpoint.as_deref().filter(|url| !url.trim().is_empty()) else { return Ok(false) };
    let (token, hint) = match s.refresh_token.as_deref().filter(|t| !t.is_empty()) {
        Some(refresh_token) => (refresh_token, "refresh_token"),
        None => (s.access_token.as_str(), "access_token"),
    };
    let answer = post_form(url, &[("token", token), ("token_type_hint", hint), ("client_id", s.client_id.as_str())])?;
    if answer.ok() {
        Ok(true)
    } else {
        Err(format!("{} didn't withdraw the sign-in: {}", host_of(url), refusal(&answer)))
    }
}

/// Why `TokenStore` has no access token to give.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenProblem {
    /// Never signed in, or signed out.
    SignedOut,
    /// The server refused the refresh: sign in again.
    SignInAgain(String),
    /// Keychain or network trouble.
    Failed(String),
}

impl std::fmt::Display for TokenProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenProblem::SignedOut => f.write_str("Not signed in to this server: sign in first"),
            TokenProblem::SignInAgain(why) | TokenProblem::Failed(why) => f.write_str(why),
        }
    }
}

/// Tokens per server in the keychain (key `mcp/<server id>/oauth`), with one refresh at a time per server.
pub struct TokenStore {
    keychain: Arc<dyn Keychain>,
    /// One lock per server id.
    locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl std::fmt::Debug for TokenStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenStore").finish_non_exhaustive()
    }
}

impl TokenStore {
    pub fn new(keychain: Arc<dyn Keychain>) -> Self {
        TokenStore { keychain, locks: Mutex::new(HashMap::new()) }
    }

    /// "mcp/<server id>/oauth"
    pub fn key(server_id: &str) -> String {
        format!("mcp/{server_id}/oauth")
    }

    /// The server's sign-in; None when there is none.
    pub fn load(&self, server_id: &str) -> Result<Option<Saved>, String> {
        self.read(server_id).map_err(|p| p.to_string())
    }

    pub fn save(&self, server_id: &str, s: &Saved) -> Result<(), String> {
        let text = serde_json::to_string(s).map_err(|_| "Couldn't write the sign-in down for the keychain".to_string())?;
        self.keychain.set(&Self::key(server_id), &text)
    }

    pub fn forget(&self, server_id: &str) -> Result<(), String> {
        self.keychain.delete(&Self::key(server_id))
    }

    /// A valid access token for at least `min_valid`: the stored one, else refreshed. Under the server's lock: reads the
    /// stored sign-in again after taking the lock (another caller may have just refreshed it); refreshes; SAVES the new
    /// sign-in in the keychain BEFORE returning the new access token; when the refresh is refused, reads the stored sign-in
    /// once more and uses it if it changed meanwhile and is valid, else SignInAgain.
    pub fn access_token(&self, server_id: &str, min_valid: Duration) -> Result<String, TokenProblem> {
        let saved = self.read(server_id)?.ok_or(TokenProblem::SignedOut)?;
        if saved.lasts(min_valid) {
            return Ok(saved.access_token);
        }
        self.renew(server_id, min_valid, None)
    }

    /// For when the MCP server refused `rejected` (a 401 with `invalid_token`) though it looked valid: a refreshed access
    /// token for at least `min_valid`, or the one another caller got meanwhile. As `access_token` otherwise.
    pub fn renewed_token(&self, server_id: &str, rejected: &str, min_valid: Duration) -> Result<String, TokenProblem> {
        self.renew(server_id, min_valid, Some(rejected))
    }

    /// Revokes where the server offers it (a failed revocation doesn't stop the sign-out) and forgets the tokens.
    pub fn sign_out(&self, server_id: &str) -> Result<(), String> {
        let lock = self.lock(server_id);
        let _one_at_a_time = lock.lock().unwrap_or_else(PoisonError::into_inner);
        if let Ok(Some(saved)) = self.read(server_id) {
            let _ = revoke(&saved);
        }
        self.forget(server_id)
    }

    fn lock(&self, server_id: &str) -> Arc<Mutex<()>> {
        let mut all = self.locks.lock().unwrap_or_else(PoisonError::into_inner);
        all.entry(server_id.to_string()).or_default().clone()
    }

    /// The stored sign-in. One that can't be read (never shown: it holds tokens) means signing in again.
    fn read(&self, server_id: &str) -> Result<Option<Saved>, TokenProblem> {
        match self.keychain.get(&Self::key(server_id)).map_err(TokenProblem::Failed)? {
            None => Ok(None),
            Some(text) => serde_json::from_str(&text).map(Some)
                .map_err(|_| TokenProblem::SignInAgain("The saved sign-in for this server can't be read: sign in again".to_string())),
        }
    }

    /// The refresh, one caller at a time per server. `rejected`: a token the server refused, refreshed even when it
    /// looks valid.
    fn renew(&self, server_id: &str, min_valid: Duration, rejected: Option<&str>) -> Result<String, TokenProblem> {
        let lock = self.lock(server_id);
        let _one_at_a_time = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let usable = |s: &Saved| s.lasts(min_valid) && rejected != Some(s.access_token.as_str());
        // Another caller may have refreshed it while this one waited for the lock.
        let saved = self.read(server_id)?.ok_or(TokenProblem::SignedOut)?;
        if usable(&saved) {
            return Ok(saved.access_token);
        }
        match refresh(&saved) {
            Ok(new) => {
                // Saved before it is used: the server may already have replaced the refresh token this one had.
                self.save(server_id, &new).map_err(TokenProblem::Failed)?;
                Ok(new.access_token)
            }
            Err(RefreshError::Failed(why)) => Err(TokenProblem::Failed(why)),
            Err(RefreshError::Refused(why)) => match self.read(server_id)? {
                // Refreshed meanwhile outside this store (another Gizai process), using up the refresh token this one had.
                Some(now) if (now.access_token != saved.access_token || now.refresh_token != saved.refresh_token) && usable(&now) => {
                    Ok(now.access_token)
                }
                _ => Err(TokenProblem::SignInAgain(why)),
            },
        }
    }
}

/// The tokens in a token endpoint's answer.
struct Tokens {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<i64>,
    scope: Option<String>,
}

/// A token endpoint's answer: the tokens, or the OAuth error code (when there is one) and why, in plain words.
fn tokens(a: &Answer) -> Result<Tokens, (Option<String>, String)> {
    let error = || match oauth_error(&a.body) {
        Some((code, why)) => (Some(code), why),
        None => (None, format!("it answered with HTTP status {}", a.status)),
    };
    if !a.ok() {
        return Err(error());
    }
    let Ok(doc) = serde_json::from_str::<Value>(&a.body) else {
        return Err((None, "its answer wasn't the JSON OAuth expects".to_string()));
    };
    let Some(access_token) = text(&doc, "access_token") else {
        // Some servers answer an error with 200.
        return Err(if doc.get("error").is_some() { error() } else { (None, "its answer had no access token".to_string()) });
    };
    if let Some(kind) = text(&doc, "token_type").filter(|k| !k.eq_ignore_ascii_case("bearer")) {
        return Err((None, format!("it gave a {} token, and Gizai only uses Bearer tokens", clean(&kind))));
    }
    let expires_in = match doc.get("expires_in") {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    };
    let expires_at = expires_in.filter(|s| s.is_finite() && *s > 0.0).map(|s| now_ms().saturating_add((s * 1000.0) as i64));
    Ok(Tokens { access_token, refresh_token: text(&doc, "refresh_token"), expires_at, scope: text(&doc, "scope") })
}

/// An OAuth error answer (`{"error": "invalid_grant", "error_description": "…"}`): its code, and what it means in plain words.
fn oauth_error(body: &str) -> Option<(String, String)> {
    let doc: Value = serde_json::from_str(body).ok()?;
    let code = clean_code(doc.get("error")?.as_str()?);
    if code.is_empty() {
        return None;
    }
    let description = doc.get("error_description").and_then(Value::as_str).map(clean).filter(|d| !d.is_empty());
    let why = explain(&code, description.as_deref());
    Some((code, why))
}

/// An OAuth error code in plain words, with the server's own description after it when it gave one.
fn explain(code: &str, description: Option<&str>) -> String {
    let plain = match code {
        "invalid_grant" => "the sign-in has expired or was withdrawn",
        "invalid_client" => "the server doesn't know Gizai's client id",
        "unauthorized_client" => "Gizai isn't allowed to sign in this way",
        "access_denied" => "the sign-in was declined",
        "invalid_scope" => "the server doesn't offer the access Gizai asked for",
        "invalid_target" => "the server doesn't accept this MCP address as the resource",
        "invalid_request" => "the server found the request incomplete or wrong",
        "unsupported_grant_type" | "unsupported_response_type" => "the server doesn't support this way of signing in",
        "invalid_redirect_uri" => "the server doesn't accept Gizai's return address on 127.0.0.1",
        "invalid_client_metadata" => "the server refused Gizai's registration details",
        "unsupported_token_type" => "the server can't withdraw this kind of token",
        "server_error" => "the server ran into an error",
        "temporarily_unavailable" => "the server is busy or down for now",
        _ => "",
    };
    match (plain, description) {
        ("", Some(d)) => d.to_string(),
        ("", None) => format!("it answered {code}"),
        (p, Some(d)) => format!("{p} ({d})"),
        (p, None) => p.to_string(),
    }
}

/// Why a server refused, in plain words: its OAuth error, else its HTTP status.
fn refusal(a: &Answer) -> String {
    oauth_error(&a.body).map(|(_, why)| why).unwrap_or_else(|| format!("it answered with HTTP status {}", a.status))
}

/// An error code from a server, safe to show: letters, digits, `_`, `-` and `.`, at most 64.
fn clean_code(code: &str) -> String {
    code.trim().chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')).take(64).collect()
}

/// A server's text for a message: one line of at most 300 characters.
fn clean(s: &str) -> String {
    let line: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let line = line.trim();
    if line.chars().count() > 300 { format!("{}…", line.chars().take(300).collect::<String>()) } else { line.to_string() }
}

/// A server's answer: its status and body.
struct Answer {
    status: u16,
    body: String,
}

impl Answer {
    fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// An agent for one address: 30 s per request, HTTP errors as answers, `redirects` redirects at most (none for a POST,
/// whose body may hold a code or token), and no proxy for this computer's own addresses.
fn agent(a: &Address, redirects: u32) -> ureq::Agent {
    let mut config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(HTTP_LIMIT))
        .max_redirects(redirects)
        .save_redirect_history(redirects > 0)
        .user_agent(format!("Gizai/{}", env!("CARGO_PKG_VERSION")));
    if is_loopback(&a.host()) {
        config = config.proxy(None);
    }
    config.build().into()
}

/// A JSON object at `url`; None when there is none there (any other answer). Redirects are followed when every address
/// on the way is one Gizai signs in with.
fn json_at(url: &str) -> Result<Option<Value>, String> {
    let a = checked(url)?;
    let mut response = agent(&a, 5).get(url).header("Accept", "application/json").header("MCP-Protocol-Version", MCP_VERSION)
        .call().map_err(|e| failed(url, e))?;
    for hop in response.get_redirect_history().unwrap_or_default() {
        checked(&hop.to_string())?;
    }
    let answer = read(&mut response, url)?;
    if !answer.ok() {
        return Ok(None);
    }
    Ok(serde_json::from_str::<Value>(&answer.body).ok().filter(Value::is_object))
}

/// POSTs `fields` form-encoded.
fn post_form(url: &str, fields: &[(&str, &str)]) -> Result<Answer, String> {
    let a = checked(url)?;
    let mut response = agent(&a, 0).post(url).header("Accept", "application/json").content_type("application/x-www-form-urlencoded")
        .send(form(fields)).map_err(|e| failed(url, e))?;
    read(&mut response, url)
}

/// POSTs `body` as JSON.
fn post_json(url: &str, body: &Value) -> Result<Answer, String> {
    let a = checked(url)?;
    let mut response = agent(&a, 0).post(url).header("Accept", "application/json").send_json(body).map_err(|e| failed(url, e))?;
    read(&mut response, url)
}

fn read(response: &mut ureq::http::Response<ureq::Body>, url: &str) -> Result<Answer, String> {
    let status = response.status().as_u16();
    let body = response.body_mut().with_config().limit(BODY_LIMIT).lossy_utf8(true).read_to_string().map_err(|e| failed(url, e))?;
    Ok(Answer { status, body })
}

/// A request that failed on the way, in plain words.
fn failed(url: &str, e: ureq::Error) -> String {
    let why = match &e {
        ureq::Error::Timeout(_) => format!("it gave none within {}", span(HTTP_LIMIT)),
        ureq::Error::Io(io) if io.kind() == ErrorKind::TimedOut => format!("it gave none within {}", span(HTTP_LIMIT)),
        ureq::Error::Io(io) if io.kind() == ErrorKind::ConnectionRefused => "it refused the connection".to_string(),
        ureq::Error::HostNotFound => "the address wasn't found".to_string(),
        ureq::Error::ConnectionFailed => "the connection failed".to_string(),
        ureq::Error::BodyExceedsLimit(_) => "its answer was too large".to_string(),
        ureq::Error::Tls(_) | ureq::Error::Rustls(_) => format!("the secure connection (TLS) failed: {e}"),
        _ => e.to_string(),
    };
    format!("Couldn't get an answer from {}: {why}", host_of(url))
}

/// An http(s) address in parts, `scheme://authority/path?query`, without its fragment.
struct Address<'a> {
    scheme: &'a str,
    authority: &'a str,
    path: &'a str,
    query: Option<&'a str>,
}

impl Address<'_> {
    /// In lowercase, IPv6 in brackets: "os.oranjeuil.nl", "[::1]".
    fn host(&self) -> String {
        let a = self.authority;
        let host = if a.starts_with('[') { a.find(']').map_or(a, |end| &a[..=end]) } else { a.split(':').next().unwrap_or(a) };
        host.to_ascii_lowercase()
    }

    /// The port, or the scheme's own.
    fn port(&self) -> u16 {
        let a = self.authority;
        let host_end = if a.starts_with('[') { a.find(']').map_or(a.len(), |end| end + 1) } else { a.find(':').unwrap_or(a.len()) };
        a[host_end..].strip_prefix(':').and_then(|p| p.parse().ok()).unwrap_or(if self.is_https() { 443 } else { 80 })
    }

    fn is_https(&self) -> bool {
        self.scheme.eq_ignore_ascii_case("https")
    }

    /// "https://os.oranjeuil.nl", in lowercase.
    fn origin(&self) -> String {
        format!("{}://{}", self.scheme.to_ascii_lowercase(), self.authority.to_ascii_lowercase())
    }

    /// The address with its scheme and host in lowercase (the MCP spec's canonical form).
    fn canonical(&self) -> String {
        match self.query {
            Some(q) => format!("{}{}?{q}", self.origin(), self.path),
            None => format!("{}{}", self.origin(), self.path),
        }
    }
}

/// `url` in parts when it is an http(s) address without a user or password in it.
fn split(url: &str) -> Option<Address<'_>> {
    let url = url.trim();
    let url = url.split('#').next().unwrap_or(url);
    let (scheme, rest) = url.split_once("://")?;
    if !(scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http")) {
        return None;
    }
    let (authority, rest) = rest.split_at(rest.find(['/', '?']).unwrap_or(rest.len()));
    if authority.is_empty() || authority.contains(['@', '\\']) || authority.contains(char::is_whitespace) {
        return None;
    }
    let (path, query) = match rest.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (rest, None),
    };
    Some(Address { scheme, authority, path, query })
}

/// Addresses on this computer, the only ones Gizai talks to over plain http.
fn is_loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

/// `url` when it is https, or http on this computer (127.0.0.1, localhost, [::1]): the only addresses Gizai signs in with.
fn checked(url: &str) -> Result<Address<'_>, String> {
    let a = split(url).ok_or_else(|| format!("{} isn't a web address (https://…)", shown(url)))?;
    if a.is_https() || is_loopback(&a.host()) {
        Ok(a)
    } else {
        Err(format!("{} isn't secure (http): Gizai only signs in over https", shown(url)))
    }
}

/// The host of `url` for a message.
fn host_of(url: &str) -> String {
    split(url).map(|a| a.host()).unwrap_or_else(|| "the server".to_string())
}

/// An address for a message: without a user or password, query or fragment (they may hold a key), at most 200 characters.
fn shown(url: &str) -> String {
    let base = url.trim().split(['?', '#']).next().unwrap_or_default();
    let base = match base.split_once("://") {
        Some((scheme, rest)) => {
            let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
            format!("{scheme}://{}{path}", authority.rsplit('@').next().unwrap_or(authority))
        }
        None => base.to_string(),
    };
    if base.chars().count() > 200 { format!("{}…", base.chars().take(200).collect::<String>()) } else { base }
}

fn same_origin(a: &Address, b: &Address) -> bool {
    a.scheme.eq_ignore_ascii_case(b.scheme) && a.host() == b.host() && a.port() == b.port()
}

/// Whether the MCP url `wanted` is the address a server's metadata declares or lies under it (RFC 9728's resource check):
/// the same scheme, host and port, and its path the same or below, a trailing slash aside.
fn under(wanted: &str, declared: &str) -> bool {
    let (Some(w), Some(d)) = (split(wanted), split(declared)) else { return false };
    same_origin(&w, &d) && format!("{}/", w.path.trim_end_matches('/')).starts_with(&format!("{}/", d.path.trim_end_matches('/')))
}

/// Whether two addresses are the same, but for the case of scheme and host and a trailing slash.
fn same_address(a: &str, b: &str) -> bool {
    let (Some(a), Some(b)) = (split(a), split(b)) else { return false };
    same_origin(&a, &b) && a.path.trim_end_matches('/') == b.path.trim_end_matches('/') && a.query == b.query
}

/// A text field of a JSON object, trimmed; None when it is missing or empty.
fn text(doc: &Value, key: &str) -> Option<String> {
    doc.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

/// `a=1&b=2`, every name and value percent-encoded: a form body or a query.
fn form(fields: &[(&str, &str)]) -> String {
    fields.iter().map(|(name, value)| format!("{}={}", pct(name), pct(value))).collect::<Vec<_>>().join("&")
}

/// Percent-encodes all but RFC 3986's unreserved characters.
fn pct(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 15)]));
        }
    }
    out
}

/// Undoes percent-encoding, and `+` for a space, in a query part.
fn unpct(s: &str) -> String {
    let b = s.as_bytes();
    let hex = |i: usize| b.get(i).and_then(|c| char::from(*c).to_digit(16));
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if hex(i + 1).is_some() && hex(i + 2).is_some() => {
                out.push((hex(i + 1).unwrap_or(0) * 16 + hex(i + 2).unwrap_or(0)) as u8);
                i += 3;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A query's parameters, decoded; the first of each name counts.
fn query_params(query: &str) -> HashMap<String, String> {
    let mut all = HashMap::new();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        all.entry(unpct(name)).or_insert_with(|| unpct(value));
    }
    all
}

/// Text for an HTML page.
fn html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Whether two secrets are equal, compared in constant time.
fn same_secret(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

/// 32 random bytes from the system as base64url: a PKCE code verifier (43 characters) or a state.
fn random_code() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| format!("Couldn't make a random code for the sign-in: {e}"))?;
    Ok(b64url(&bytes))
}

/// Base64url without padding (RFC 4648 §5).
fn b64url(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..=chunk.len() {
            out.push(char::from(ABC[((n >> (18 - 6 * i)) & 63) as usize]));
        }
    }
    out
}

/// Now, in ms since the epoch.
fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// "10 minutes", "a minute", "30 seconds".
fn span(d: Duration) -> String {
    match d.as_secs() {
        0 => format!("{} milliseconds", d.as_millis()),
        1 => "a second".to_string(),
        60 => "a minute".to_string(),
        s if s > 60 && s % 60 == 0 => format!("{} minutes", s / 60),
        s => format!("{s} seconds"),
    }
}
