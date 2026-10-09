//! GA-59: Bitbucket Cloud's REST API without Bitbucket. One local server per test binary stands in for
//! api.bitbucket.org (GIZAI_BITBUCKET_API, set once when it starts). It answers by the request's path (the repository)
//! and the login in its Authorization header, so tests running at the same time don't meet, and it keeps every request
//! it got, so each test can find its own. Nothing here reaches bitbucket.org or github.com.
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde_json::{json, Value};

use gizai_agents::bitbucket::{self, Login, SCOPES};
use gizai_agents::github::{Commit, PullRequest};
use gizai_agents::secrets::{Keychain, MemoryKeychain};

/// Logins the fake API knows, as (email, API token). Every token holds SECRET, so no error text may.
const GOOD: (&str, &str) = ("jeffrey@yotech.ai", "ATATT3xFfGF0goodSECRET");
/// Refused everywhere (401).
const REFUSED: (&str, &str) = ("nobody@yotech.ai", "ATATT3xFfGF0refusedSECRET");
/// May not read the account (403 on /user).
const NO_SCOPE: (&str, &str) = ("noscope@yotech.ai", "ATATT3xFfGF0scopeSECRET");
/// The server hangs up without an answer.
const HANG_UP: (&str, &str) = ("hangup@yotech.ai", "ATATT3xFfGF0hangupSECRET");

/// The latest commit of the open pull request #5, as Bitbucket gives it (12 characters), and in full.
const OPEN_HEAD: &str = "a1b2c3d4e5f6";
const OPEN_TIP: &str = "a1b2c3d4e5f67890abcdef0123456789abcdef01";

fn login((email, token): (&str, &str)) -> Login {
    Login { email: email.into(), token: token.into() }
}

/// Standard base64 with padding, written here so the test doesn't take the app's word for it.
fn b64(bytes: &[u8]) -> String {
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(char::from(ABC[(n >> 18) as usize & 63]));
        out.push(char::from(ABC[(n >> 12) as usize & 63]));
        out.push(if chunk.len() > 1 { char::from(ABC[(n >> 6) as usize & 63]) } else { '=' });
        out.push(if chunk.len() > 2 { char::from(ABC[n as usize & 63]) } else { '=' });
    }
    out
}

/// The Authorization header a login should go in.
fn basic((email, token): (&str, &str)) -> String {
    format!("Basic {}", b64(format!("{email}:{token}").as_bytes()))
}

/// No token, nor anything of one, in `text`.
fn no_secret(text: &str) {
    for bit in ["SECRET", "ATATT3x", GOOD.1, REFUSED.1, NO_SCOPE.1, HANG_UP.1] {
        assert!(!text.contains(bit), "{bit} shows in: {text}");
    }
}

/// A request the fake API got.
#[derive(Debug, Clone)]
struct Seen {
    method: String,
    /// Without the query.
    path: String,
    /// As sent (URL-encoded).
    query: String,
    authorization: String,
    body: String,
}

static SEEN: Mutex<Vec<Seen>> = Mutex::new(Vec::new());

/// The requests that `keep` keeps, in the order they came.
fn requests(keep: impl Fn(&Seen) -> bool) -> Vec<Seen> {
    SEEN.lock().unwrap_or_else(|e| e.into_inner()).iter().filter(|r| keep(r)).cloned().collect()
}

/// The fake API's base ("http://127.0.0.1:<port>/2.0"), started once for this test binary.
fn server() -> &'static str {
    static BASE: OnceLock<String> = OnceLock::new();
    BASE.get_or_init(|| {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/2.0", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if let Ok(s) = stream {
                    serve(s);
                }
            }
        });
        // SAFETY: set once, before any test of this binary talks to the API (each one that does calls server() first)
        unsafe { std::env::set_var("GIZAI_BITBUCKET_API", &base) };
        base
    })
}

/// One HTTP/1.1 request: its line, its headers up to the blank line, its body (by Content-Length, or chunked).
fn serve(mut s: TcpStream) {
    let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
    let mut r = BufReader::new(s.try_clone().unwrap());
    let mut line = String::new();
    if r.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = line.trim_end().splitn(3, ' ');
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();
    let mut headers: Vec<(String, String)> = vec![];
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).unwrap_or(0) == 0 {
            break;
        }
        let h = h.trim_end_matches(['\r', '\n']);
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    let header = |k: &str| headers.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    let mut body = Vec::new();
    if let Some(n) = header("content-length").and_then(|v| v.parse::<usize>().ok()) {
        body.resize(n, 0);
        let _ = r.read_exact(&mut body);
    } else if header("transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) {
        loop {
            let mut size = String::new();
            if r.read_line(&mut size).unwrap_or(0) == 0 {
                break;
            }
            let n = usize::from_str_radix(size.trim().split(';').next().unwrap_or("0"), 16).unwrap_or(0);
            let mut chunk = vec![0; n];
            let _ = r.read_exact(&mut chunk);
            body.extend(chunk);
            let mut end = String::new();
            let _ = r.read_line(&mut end);
            if n == 0 {
                break;
            }
        }
    }
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.clone(), String::new()),
    };
    let seen = Seen { method, path, query, authorization: header("authorization").unwrap_or_default(), body: String::from_utf8_lossy(&body).into() };
    SEEN.lock().unwrap_or_else(|e| e.into_inner()).push(seen.clone());
    if let Some((status, json)) = answer(&seen) {
        let _ = s.write_all(format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
                                    json.len()).as_bytes());
        let _ = s.flush();
    }
    // dropped: the connection closes (without an answer for HANG_UP)
}

fn error(message: &str) -> String {
    json!({"type": "error", "error": {"message": message}}).to_string()
}

fn pull(id: u64, state: &str, draft: bool, hash: &str) -> Value {
    json!({
        "id": id, "title": format!("SH-{id}"), "state": state, "draft": draft,
        "links": {"html": {"href": format!("https://bitbucket.org/acme/shop/pull-requests/{id}")}},
        "source": {"branch": {"name": "gizai/ga-1-x"}, "commit": {"hash": hash}},
        "destination": {"branch": {"name": "master"}, "commit": {"hash": "0f0f0f0f0f0f"}},
    })
}

/// What the fake API answers: None hangs up.
fn answer(r: &Seen) -> Option<(u16, String)> {
    let who = r.authorization.as_str();
    if who == basic(HANG_UP) {
        return None;
    }
    let (good, no_scope) = (who == basic(GOOD), who == basic(NO_SCOPE));
    if !good && !no_scope {
        return Some((401, error("Unauthorized")));
    }
    let Some(path) = r.path.strip_prefix("/2.0") else { return Some((404, error("Not found"))) };
    Some(match (r.method.as_str(), path) {
        ("GET", "/user") if no_scope => (403, error("Your credentials lack one or more required privilege scopes.")),
        ("GET", "/user") => (200, json!({"display_name": "Jeffrey Sevinga", "username": "jefsev", "type": "user",
                                         "uuid": "{6f9b2c1e-0000-4000-8000-000000000001}"}).to_string()),
        ("GET", "/repositories/acme/shop/pullrequests") => (200, json!({"pagelen": 20, "page": 1, "size": 5, "values": [
            pull(5, "OPEN", false, OPEN_HEAD),
            pull(4, "OPEN", true, "b4b4b4b4b4b4"),
            pull(3, "MERGED", false, &"c3".repeat(6)),
            pull(2, "DECLINED", false, "d2d2d2d2d2d2"),
            pull(1, "SUPERSEDED", false, "e1e1e1e1e1e1"),
        ]}).to_string()),
        ("GET", "/repositories/acme/shop/pullrequests/3/commits") => (200, json!({"pagelen": 100, "values": [
            {"hash": format!("{}{}", "c3".repeat(6), "d4".repeat(14)), "message": "the last one"},
            {"hash": "e5".repeat(20), "message": "an earlier one"},
        ]}).to_string()),
        ("GET", "/repositories/acme/gone/pullrequests") => (404, error("Repository acme/gone not found")),
        ("GET", "/repositories/acme/locked/pullrequests") => (403, error("Your credentials lack one or more required privilege scopes.")),
        ("POST", "/repositories/acme/bad/pullrequests") => (400, error("There are no changes to be pulled")),
        ("POST", p) if p.starts_with("/repositories/") && p.ends_with("/pullrequests") => {
            let repo = p.trim_start_matches("/repositories/").trim_end_matches("/pullrequests");
            (201, json!({"id": 7, "state": "OPEN", "links": {"html": {"href": format!("https://bitbucket.org/{repo}/pull-requests/7")}}}).to_string())
        }
        _ => (404, error("Not found")),
    })
}

/// A URL query's decoded pairs.
fn pairs(query: &str) -> Vec<(String, String)> {
    fn decode(s: &str) -> String {
        let b = s.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b'%' if i + 2 < b.len() && u8::from_str_radix(&s[i + 1..i + 3], 16).is_ok() => {
                    out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
                    i += 3;
                    continue;
                }
                b'+' => out.push(b' '),
                c => out.push(c),
            }
            i += 1;
        }
        String::from_utf8(out).unwrap()
    }
    query.split('&').filter(|p| !p.is_empty())
        .map(|p| p.split_once('=').map(|(k, v)| (decode(k), decode(v))).unwrap_or_else(|| (decode(p), String::new()))).collect()
}

fn values<'a>(pairs: &'a [(String, String)], key: &str) -> Vec<&'a str> {
    pairs.iter().filter(|(k, _)| k == key).map(|(_, v)| v.as_str()).collect()
}

// ---- the login ----

#[test]
fn the_tests_own_base64_is_right() {
    // RFC 4648 §10 and RFC 7617's example
    for (plain, want) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="),
                          ("foobar", "Zm9vYmFy"), ("Aladdin:open sesame", "QWxhZGRpbjpvcGVuIHNlc2FtZQ==")] {
        assert_eq!(b64(plain.as_bytes()), want, "{plain}");
    }
}

#[test]
fn a_good_login_gets_its_account_and_goes_only_in_the_authorization_header() {
    server();
    assert_eq!(bitbucket::account(&login(GOOD)), Ok("Jeffrey Sevinga (jefsev)".to_string()));
    let mine = requests(|r| r.path == "/2.0/user" && r.authorization == basic(GOOD));
    assert_eq!(mine.len(), 1, "{mine:?}");
    assert_eq!(mine[0].method, "GET");
    // HTTP Basic with email:token
    assert_eq!(mine[0].authorization, format!("Basic {}", b64(b"jeffrey@yotech.ai:ATATT3xFfGF0goodSECRET")));
    assert!(mine[0].authorization.starts_with("Basic amVmZnJleUB5b3RlY2guYWk6"), "base64 of \"jeffrey@yotech.ai:\": {}", mine[0].authorization);
    no_secret(&format!("{} {}", mine[0].path, mine[0].query));
}

#[test]
fn a_refused_login_or_one_without_the_scopes_says_what_to_do_and_never_shows_the_token() {
    server();
    assert_eq!(SCOPES, ["read:user:bitbucket", "read:pullrequest:bitbucket", "write:pullrequest:bitbucket"]);
    // 401: the email and token aren't accepted
    let refused = bitbucket::account(&login(REFUSED)).unwrap_err();
    assert_eq!(refused.what, "Bitbucket refused the email and API token");
    assert!(refused.fix.as_deref().is_some_and(|f| f.contains("token")), "a fix: {refused:?}");
    // 403: the token may not read the account: which scopes it needs
    let scopes = bitbucket::account(&login(NO_SCOPE)).unwrap_err();
    for s in SCOPES {
        assert!(scopes.to_string().contains(s), "{s} missing in {scopes}");
    }
    // no answer at all: can't reach Bitbucket, check the connection
    let hung = bitbucket::account(&login(HANG_UP)).unwrap_err();
    assert!(hung.what.starts_with("Can't reach Bitbucket (127.0.0.1:"), "{hung:?}");
    assert!(hung.fix.as_deref().is_some_and(|f| f.contains("internet connection")), "{hung:?}");
    for p in [&refused, &scopes, &hung] {
        no_secret(&p.what);
        no_secret(p.fix.as_deref().unwrap_or_default());
        no_secret(&p.to_string());
        no_secret(&format!("{p:?}"));
    }
    // a login's Debug never shows the token
    for who in [GOOD, REFUSED, NO_SCOPE, HANG_UP] {
        let shown = format!("{:?} {:#?}", login(who), login(who));
        assert!(shown.contains(who.0), "{shown}");
        no_secret(&shown);
    }
    // each went to /user with its own login
    for who in [REFUSED, NO_SCOPE, HANG_UP] {
        assert!(!requests(|r| r.path == "/2.0/user" && r.authorization == basic(who)).is_empty(), "{who:?} asked /user");
    }
}

#[test]
fn the_login_is_kept_in_the_keychain_under_bitbucket_login() {
    let kc = MemoryKeychain::default();
    assert_eq!(bitbucket::KEY, "bitbucket/login");
    assert_eq!(bitbucket::saved_login(&kc), Ok(None), "nothing saved yet");
    bitbucket::save_login(&kc, &login(GOOD)).unwrap();
    let raw = kc.get("bitbucket/login").unwrap().expect("saved under bitbucket/login");
    assert!(raw.contains(GOOD.0) && raw.contains(GOOD.1), "{raw}");
    assert_eq!(bitbucket::saved_login(&kc), Ok(Some(login(GOOD))));
    // another login takes its place
    bitbucket::save_login(&kc, &login(NO_SCOPE)).unwrap();
    assert_eq!(bitbucket::saved_login(&kc), Ok(Some(login(NO_SCOPE))));
    no_secret(&format!("{kc:?}"));
    // removed, and removing again is fine
    assert_eq!(bitbucket::remove_login(&kc), Ok(()));
    assert_eq!(kc.get("bitbucket/login"), Ok(None));
    assert_eq!(bitbucket::saved_login(&kc), Ok(None));
    assert_eq!(bitbucket::remove_login(&kc), Ok(()));
    // something unreadable there: log in again, without repeating what's there
    kc.set("bitbucket/login", "not json ATATT3xFfGF0junkSECRET").unwrap();
    let e = bitbucket::saved_login(&kc).unwrap_err();
    assert!(e.contains("log in to Bitbucket again"), "{e}");
    no_secret(&e);
}

// ---- pull requests ----

#[test]
fn a_branchs_pull_requests_in_every_state_as_gizai_names_them() {
    server();
    let pulls = bitbucket::pulls_for_branch(&login(GOOD), "acme/shop", "gizai/ga-1-x").unwrap();
    let got: Vec<(u64, &str, bool, &str)> = pulls.iter().map(|p| (p.number, p.state.as_str(), p.is_draft, p.state())).collect();
    assert_eq!(got, [(5, "OPEN", false, "open"), (4, "OPEN", true, "draft"), (3, "MERGED", false, "merged"), (2, "CLOSED", false, "closed"),
                     (1, "CLOSED", false, "closed")]);
    assert_eq!(pulls[0].url, "https://bitbucket.org/acme/shop/pull-requests/5");
    assert_eq!(pulls[0].head_ref_oid, OPEN_HEAD);

    // the request: every state, this branch, URL-encoded, with the login
    let list = requests(|r| r.method == "GET" && r.path == "/2.0/repositories/acme/shop/pullrequests");
    assert_eq!(list.len(), 1, "{list:?}");
    let q = pairs(&list[0].query);
    assert_eq!(values(&q, "q"), ["source.branch.name = \"gizai/ga-1-x\""], "{}", list[0].query);
    assert_eq!(values(&q, "state"), ["OPEN", "MERGED", "DECLINED", "SUPERSEDED"], "{}", list[0].query);
    assert!(!list[0].query.contains(' ') && !list[0].query.contains('"'), "URL-encoded: {}", list[0].query);
    assert_eq!(list[0].authorization, basic(GOOD));

    // only the merged one's commits are asked, and kept in full
    let asked: Vec<String> = requests(|r| r.method == "GET" && r.path.starts_with("/2.0/repositories/acme/shop/pullrequests/"))
        .into_iter().map(|r| r.path).collect();
    assert_eq!(asked, ["/2.0/repositories/acme/shop/pullrequests/3/commits"]);
    let merged_tip = format!("{}{}", "c3".repeat(6), "d4".repeat(14));
    assert_eq!(pulls[2].commits, vec![Commit { oid: merged_tip.clone() }, Commit { oid: "e5".repeat(20) }]);
    assert!(pulls.iter().filter(|p| p.number != 3).all(|p| p.commits.is_empty()), "{pulls:?}");

    // contains: the branch tip from Bitbucket's short hash, or one of a merged pull request's commits
    assert_eq!(OPEN_TIP.len(), 40);
    assert!(OPEN_TIP.starts_with(OPEN_HEAD));
    assert!(pulls[0].contains(OPEN_TIP), "the open one's tip, by its 12-character hash");
    assert!(pulls[2].contains(&merged_tip) && pulls[2].contains(&"e5".repeat(20)), "the merged one's commits");
    assert!(!pulls[0].contains(&"f0".repeat(20)) && !pulls[2].contains(OPEN_TIP), "another commit");
    assert!(!pulls[0].contains(""), "an empty sha never matches");
    assert!(!pulls[0].contains("a1b2c3d"), "a shorter sha than the pull request's doesn't match it");
    let six = PullRequest { head_ref_oid: "a1b2c3".into(), commits: vec![Commit { oid: "a1b2c3".into() }], ..pulls[0].clone() };
    assert!(!six.contains(OPEN_TIP), "a hash shorter than 7 characters never matches by prefix");
    let seven = PullRequest { head_ref_oid: "a1b2c3d".into(), ..pulls[0].clone() };
    assert!(seven.contains(OPEN_TIP), "7 characters do");
    no_secret(&format!("{pulls:?}"));
}

#[test]
fn a_refused_login_a_missing_repository_and_missing_scopes_say_what_to_do() {
    server();
    let e = bitbucket::pulls_for_branch(&login(REFUSED), "acme/refused", "gizai/ga-1-x").unwrap_err();
    assert!(e.contains("log in to Bitbucket again in Settings → Bitbucket"), "{e}");
    no_secret(&e);
    let e = bitbucket::pulls_for_branch(&login(GOOD), "acme/gone", "gizai/ga-1-x").unwrap_err();
    assert!(e.contains("acme/gone") && e.contains("check the project's Bitbucket link"), "{e}");
    no_secret(&e);
    let e = bitbucket::pulls_for_branch(&login(GOOD), "acme/locked", "gizai/ga-1-x").unwrap_err();
    assert!(e.contains("Your credentials lack one or more required privilege scopes."), "Bitbucket's own message: {e}");
    for s in SCOPES {
        assert!(e.contains(s), "{s} missing in {e}");
    }
    no_secret(&e);
    assert_eq!(requests(|r| r.path.starts_with("/2.0/repositories/acme/gone/")).len(), 1);
}

#[test]
fn opening_a_pull_request_posts_its_title_description_and_branches_and_returns_its_link() {
    server();
    let body = "## What\n\nHe said \"ship it\" — it's $HOME\n\n- `one`\n".repeat(50);
    let url = bitbucket::create_pull(&login(GOOD), "acme/shop", "gizai/ga-59-bitbucket", "master", "GA-59: Bitbucket \"repositories\"", &body).unwrap();
    assert_eq!(url, "https://bitbucket.org/acme/shop/pull-requests/7");
    let posts = requests(|r| r.method == "POST" && r.path == "/2.0/repositories/acme/shop/pullrequests");
    assert_eq!(posts.len(), 1, "{posts:?}");
    assert_eq!(posts[0].authorization, basic(GOOD));
    let sent: Value = serde_json::from_str(&posts[0].body).unwrap_or_else(|e| panic!("JSON: {e}: {}", posts[0].body));
    assert_eq!(sent["title"], "GA-59: Bitbucket \"repositories\"");
    assert_eq!(sent["description"], body.as_str(), "the body untouched");
    assert_eq!(sent["source"]["branch"]["name"], "gizai/ga-59-bitbucket");
    assert_eq!(sent["destination"]["branch"]["name"], "master");
    no_secret(&posts[0].body);
    no_secret(&format!("{} {}", posts[0].path, posts[0].query));
}

#[test]
fn a_title_longer_than_255_characters_is_cut_to_255() {
    server();
    let title = format!("GA-60: {}", "Ü".repeat(300));
    let url = bitbucket::create_pull(&login(GOOD), "acme/long", "gizai/ga-60-x", "main", &title, "b").unwrap();
    assert_eq!(url, "https://bitbucket.org/acme/long/pull-requests/7");
    let posts = requests(|r| r.method == "POST" && r.path == "/2.0/repositories/acme/long/pullrequests");
    assert_eq!(posts.len(), 1, "{posts:?}");
    let sent: Value = serde_json::from_str(&posts[0].body).unwrap();
    let sent_title = sent["title"].as_str().unwrap();
    assert_eq!(sent_title.chars().count(), 255, "{sent_title}");
    assert_eq!(sent_title, title.chars().take(255).collect::<String>());
    assert_eq!(sent["destination"]["branch"]["name"], "main");
}

#[test]
fn bitbucket_refusing_a_pull_request_gives_its_own_message_without_the_token() {
    server();
    let e = bitbucket::create_pull(&login(GOOD), "acme/bad", "gizai/ga-1-x", "master", "t", "b").unwrap_err();
    assert!(e.contains("There are no changes to be pulled"), "{e}");
    no_secret(&e);
    let e = bitbucket::create_pull(&login(REFUSED), "acme/refused-create", "gizai/ga-1-x", "master", "t", "b").unwrap_err();
    assert!(e.contains("log in to Bitbucket again in Settings → Bitbucket"), "{e}");
    no_secret(&e);
}

// ---- GitHub's pull requests as before ----

#[test]
fn githubs_pull_requests_still_match_full_hashes() {
    let (head, earlier) = ("9f".repeat(20), "8e".repeat(20));
    let pr: PullRequest = serde_json::from_str(&format!(
        r#"{{"number":1,"url":"https://github.com/acme/shop/pull/1","state":"OPEN","isDraft":false,"headRefOid":"{head}","commits":[{{"oid":"{earlier}"}},{{"oid":"{head}"}}]}}"#)).unwrap();
    assert!(pr.contains(&head) && pr.contains(&earlier), "its latest commit and an earlier one");
    assert!(!pr.contains(&"7d".repeat(20)) && !pr.contains(""), "another commit, or none");
    assert!(!pr.contains(&head[..12]), "a short sha doesn't match a full hash");
    assert_eq!(pr.state(), "open");
}
