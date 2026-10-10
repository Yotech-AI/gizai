//! GA-59 QA: Settings → Bitbucket (status, Save login, Remove login, Check connection), the Bitbucket remote offered
//! by the repository check, and the Team Lead's project tools with a Bitbucket link, end to end without Bitbucket.
//!
//! Process-wide fakes, set up once per test binary (`fakes`):
//! - a local "Bitbucket API" over plain HTTP on 127.0.0.1 (GIZAI_BITBUCKET_API): `GET /2.0/user` answers 200 for
//!   jeffrey@yotech.ai with the token good-token-123 (HTTP Basic), 401 for anything else; every request is recorded;
//! - a fake ssh as GIT_SSH_COMMAND that serves "Bitbucket" from local bare repositories at base/bitbucket/<workspace>/
//!   <repository>(.git) and answers `ssh -T git@bitbucket.org` the way Bitbucket does (exit code 1). A file
//!   fake-ssh-mode with "publickey" in a bare repository makes ssh refuse the key for it.
//!
//! Nothing here reaches bitbucket.org, api.bitbucket.org or github.com.
// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use gizai_core::model::{AgentInput, ProjectInput};
use gizai_lib::AppState;
use gizai_lib::bitbucket::{self, BitbucketStatus};
use gizai_lib::tools;
use serde_json::{Value, json};

const EMAIL: &str = "jeffrey@yotech.ai";
const GOOD: &str = "good-token-123";
const BAD: &str = "bad-token-456";
const KEY: &str = "bitbucket/login";
const ACCOUNT: &str = "Jeffrey Sevinga (jefsev)";
const SCOPES: [&str; 3] = ["read:user:bitbucket", "read:pullrequest:bitbucket", "write:pullrequest:bitbucket"];

// ---- the fakes ----

/// One request the fake API got.
#[derive(Debug, Clone)]
struct Seen {
    /// "GET /2.0/user HTTP/1.1"
    line: String,
    auth: Option<String>,
    /// The request line and every header, as sent.
    head: String,
}

static SEEN: Mutex<Vec<Seen>> = Mutex::new(Vec::new());

fn seen() -> Vec<Seen> {
    SEEN.lock().unwrap().clone()
}

/// Standard base64 with padding (RFC 4648), written here on its own (not the app's).
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let n = (u32::from(chunk[0]) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        out.push(char::from(ABC[((n >> 18) & 63) as usize]));
        out.push(char::from(ABC[((n >> 12) & 63) as usize]));
        out.push(if chunk.len() > 1 { char::from(ABC[((n >> 6) & 63) as usize]) } else { '=' });
        out.push(if chunk.len() > 2 { char::from(ABC[(n & 63) as usize]) } else { '=' });
    }
    out
}

/// The Authorization header for `email` and `token`, as Bitbucket's API takes an API token.
fn basic(email: &str, token: &str) -> String {
    format!("Basic {}", base64(format!("{email}:{token}").as_bytes()))
}

/// One HTTP/1.1 request to the fake API: recorded, then answered and closed.
fn serve(mut s: TcpStream) {
    let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
    let (mut head, mut byte) = (Vec::new(), [0u8; 1]);
    while !head.ends_with(b"\r\n\r\n") && s.read(&mut byte).unwrap_or(0) == 1 {
        head.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&head).to_string();
    let line = head.split("\r\n").next().unwrap_or("").to_string();
    let header = |name: &str| head.split("\r\n").skip(1).find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.trim().eq_ignore_ascii_case(name).then(|| v.trim().to_string())
    });
    let auth = header("authorization");
    let len: usize = header("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut body = vec![0u8; len];
    let _ = s.read_exact(&mut body);
    SEEN.lock().unwrap().push(Seen { line: line.clone(), auth: auth.clone(), head: head.clone() });
    let path = line.split_whitespace().nth(1).unwrap_or("").split('?').next().unwrap_or("");
    let (status, json) = if line.starts_with("GET ") && path == "/2.0/user" && auth.as_deref() == Some(basic(EMAIL, GOOD).as_str()) {
        ("200 OK", r#"{"display_name":"Jeffrey Sevinga","username":"jefsev","type":"user","uuid":"{0f1e2d3c}"}"#)
    } else {
        ("401 Unauthorized", r#"{"type":"error","error":{"message":"Unauthorized"}}"#)
    };
    let reply = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}", json.len());
    let _ = s.write_all(reply.as_bytes());
    let _ = s.flush();
}

/// Writes an executable script through a child `sh`, so this process never holds it open for writing (a leaked
/// handle would make running it fail with "Text file busy").
fn write_script(path: &Path, script: &str) {
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path).stdin(Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

/// The fake ssh. Every call's arguments go to base/ssh-args.
const FAKE_SSH: &str = r#"#!/bin/sh
base='@BASE@'
[ "$1" = "-G" ] && exit 1
echo "$*" >> "$base/ssh-args"
host=""
cmd=""
for a in "$@"; do
  case "$a" in
    git-receive-pack*|git-upload-pack*) cmd="$a" ;;
    *@*) host="$a" ;;
  esac
done
if [ "$host" != "git@bitbucket.org" ]; then
  echo "fake ssh: this test only knows git@bitbucket.org, not $host" >&2; exit 255
fi
if [ -z "$cmd" ]; then
  echo "authenticated via ssh key."
  echo ""
  echo "You can use git to connect to Bitbucket. Shell access is disabled."
  exit 1
fi
path="${cmd#* }"
path=$(printf '%s' "$path" | tr -d "'")
path="${path#/}"
repo="$base/bitbucket/$path"
[ -d "$repo" ] || repo="$repo.git"
case "$(cat "$repo/fake-ssh-mode" 2>/dev/null)" in
  publickey) echo "git@bitbucket.org: Permission denied (publickey)." >&2; exit 255 ;;
esac
verb="${cmd%% *}"
exec git "${verb#git-}" "$repo"
"#;

/// The fake ssh's folder (base/ssh-args, base/bitbucket/…). Sets GIZAI_BITBUCKET_API and GIT_SSH_COMMAND once for
/// this test binary; every test calls it first.
fn fakes() -> &'static Path {
    static BASE: OnceLock<PathBuf> = OnceLock::new();
    BASE.get_or_init(|| {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let api = format!("http://{}/2.0", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for s in listener.incoming().flatten() {
                std::thread::spawn(move || serve(s));
            }
        });
        let base = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap().keep();
        let ssh = base.join("bin/ssh");
        std::fs::create_dir_all(ssh.parent().unwrap()).unwrap();
        write_script(&ssh, &FAKE_SSH.replace("@BASE@", &base.display().to_string()));
        // SAFETY: set once, before any test of this binary starts a process (every test calls fakes first, and waits
        // here while it runs)
        unsafe {
            std::env::set_var("GIZAI_BITBUCKET_API", &api);
            std::env::set_var("GIT_SSH_COMMAND", &ssh);
            // the in-memory keychain of test_state, never a file
            std::env::remove_var("GIZAI_FAKE_KEYCHAIN");
        }
        base
    })
}

fn ssh_calls() -> Vec<String> {
    std::fs::read_to_string(fakes().join("ssh-args")).unwrap_or_default().lines().map(String::from).collect()
}

// ---- helpers ----

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

fn has_branch(repo: &Path, branch: &str) -> bool {
    Command::new("git").args(["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).current_dir(repo).status().unwrap().success()
}

/// A new git repository at `dir` with one commit on main.
fn repo_with_a_commit(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["-c", "user.email=t@t", "-c", "user.name=t", "-c", "commit.gpgsign=false", "commit", "-q", "--allow-empty", "-m", "init"]);
    dir.to_path_buf()
}

/// "Bitbucket" at git@bitbucket.org:<repo> behind the fake ssh (a bare repository), and a local clone of it whose
/// origin is `origin`. The clone allows no https at all, so nothing can reach bitbucket.org.
fn bitbucket_repo(tmp: &Path, repo: &str, origin: &str) -> (PathBuf, PathBuf) {
    let name = repo.replace('/', "-");
    let src = repo_with_a_commit(&tmp.join(format!("src-{name}")));
    let bare = fakes().join(format!("bitbucket/{repo}.git"));
    git(tmp, &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
    let local = tmp.join(&name);
    git(tmp, &["clone", "-q", bare.to_str().unwrap(), local.to_str().unwrap()]);
    git(&local, &["remote", "set-url", "origin", origin]);
    git(&local, &["config", "protocol.https.allow", "never"]);
    (bare, local)
}

fn project(st: &AppState, name: &str, key: &str, repo: &Path, link: &str, status: Option<&str>) -> String {
    gizai_core::projects::create(&st.db, &st.you_id, ProjectInput {
        name: name.into(), key: key.into(), repo_path: Some(repo.display().to_string()), repo_url: Some(link.into()),
        default_branch: Some("main".into()), status: status.map(String::from), ..Default::default()
    }).unwrap()
}

/// Every file under `dir` that holds `needle`.
fn files_with(dir: &Path, needle: &[u8]) -> Vec<PathBuf> {
    let mut found = vec![];
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            found.extend(files_with(&p, needle));
        } else if std::fs::read(&p).is_ok_and(|b| b.windows(needle.len()).any(|w| w == needle)) {
            found.push(p);
        }
    }
    found
}

fn keys(v: &Value) -> Vec<String> {
    let mut k: Vec<String> = v.as_object().unwrap_or_else(|| panic!("not an object: {v}")).keys().cloned().collect();
    k.sort();
    k
}

fn saved(st: &AppState) -> Option<String> {
    st.keychain.get(KEY).unwrap()
}

/// What Settings → Bitbucket shows without a login.
fn assert_no_login(s: &BitbucketStatus) {
    assert_eq!((s.email.as_deref(), s.has_token, s.account.as_deref()), (None, false, None), "{s:?}");
    let p = s.account_problem.as_ref().expect("a reason");
    assert_eq!(p.what, "No Bitbucket login yet");
    let fix = p.fix.as_deref().expect("what to do");
    for scope in SCOPES {
        assert!(fix.contains(scope), "the fix names {scope}: {fix}");
    }
}

// ---- a. status without a login ----

#[tokio::test]
async fn without_a_login_settings_says_so_and_which_scopes_a_token_needs() {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    assert_eq!(gizai_agents::bitbucket::KEY, KEY, "the login's place in the keychain");
    let s = bitbucket::status(&st).await;
    assert_no_login(&s);
    assert!(s.account_problem.unwrap().fix.unwrap().contains("API token"));
}

// ---- b. a login Bitbucket refuses, or one filled in wrong ----

#[tokio::test]
async fn a_login_bitbucket_refuses_or_one_filled_in_wrong_is_not_saved() {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());

    let e = bitbucket::save_login(&st, EMAIL.into(), BAD.into()).await.unwrap_err();
    assert!(e.contains("Bitbucket refused"), "{e}");
    assert!(e.ends_with("Nothing was saved."), "{e}");
    assert!(!e.contains(BAD), "the token is never in an error: {e}");
    assert_eq!(saved(&st), None, "nothing in the keychain");
    assert!(seen().iter().any(|r| r.auth.as_deref() == Some(basic(EMAIL, BAD).as_str())), "Bitbucket was asked first");
    assert_no_login(&bitbucket::status(&st).await);

    // filled in wrong: Bitbucket isn't even asked, and nothing is saved
    for (email, token, says) in [
        ("", "never-sent-1", "email"),
        ("   ", "never-sent-2", "email"),
        ("jeffrey.yotech.ai", "never-sent-3", "email"),
        (EMAIL, "never-sent 4", "space"),
        (EMAIL, "", "API token"),
    ] {
        let e = bitbucket::save_login(&st, email.into(), token.into()).await.unwrap_err();
        assert!(e.contains(says), "{email:?}/{token:?}: {e}");
        assert!(!e.contains("never-sent"), "the token is never in an error: {e}");
        assert_eq!(saved(&st), None, "{email:?}/{token:?}: nothing saved");
        let header = basic(email.trim(), token.trim());
        assert!(!seen().iter().any(|r| r.auth.as_deref() == Some(header.as_str())), "{email:?}/{token:?}: no request reached the API");
    }

    // a refused login never replaces the one that is saved
    bitbucket::save_login(&st, EMAIL.into(), GOOD.into()).await.unwrap();
    let before = saved(&st).expect("the good login");
    let e = bitbucket::save_login(&st, EMAIL.into(), BAD.into()).await.unwrap_err();
    assert!(e.ends_with("Nothing was saved."), "{e}");
    assert_eq!(saved(&st), Some(before), "the saved login stays as it was");
    assert_eq!(bitbucket::status(&st).await.account.as_deref(), Some(ACCOUNT));
}

// ---- c. the good login ----

#[tokio::test]
async fn the_login_bitbucket_accepts_is_kept_in_the_keychain_and_nowhere_else() {
    let base = fakes();
    assert_eq!((base64(b"Man"), base64(b"Ma"), base64(b"M")), ("TWFu".into(), "TWE=".into(), "TQ==".into()), "the test's own base64");
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());

    let s = bitbucket::save_login(&st, EMAIL.into(), GOOD.into()).await.unwrap();
    assert_eq!(s, BitbucketStatus { email: Some(EMAIL.into()), has_token: true, account: Some(ACCOUNT.into()), account_problem: None });
    let kept = saved(&st).expect("the login is in the keychain");
    assert!(kept.contains(EMAIL) && kept.contains(GOOD), "the keychain holds the email and the token");

    let again = bitbucket::status(&st).await;
    assert_eq!(again, s, "the status shows the same account");
    let json = serde_json::to_string(&again).unwrap();
    assert!(!json.contains(GOOD) && !json.contains(&base64(format!("{EMAIL}:{GOOD}").as_bytes())), "the token never goes to the screen: {json}");

    // the token went to the API only in the Authorization header
    let ours: Vec<Seen> = seen().into_iter().filter(|r| r.auth.as_deref() == Some(basic(EMAIL, GOOD).as_str())).collect();
    assert!(!ours.is_empty(), "the API was asked with the login");
    for r in &ours {
        assert_eq!(r.line, "GET /2.0/user HTTP/1.1", "{r:?}");
    }
    for r in seen() {
        assert!(!r.line.contains(GOOD), "never in a path or query: {}", r.line);
        assert!(!r.head.contains(GOOD), "never in plain text in a header: {}", r.head);
    }

    // never in Gizai's database (and its -wal), settings or logs, never in a process's arguments
    assert!(st.data_dir.starts_with(tmp.path()));
    assert_eq!(files_with(tmp.path(), GOOD.as_bytes()), Vec::<PathBuf>::new());
    assert_eq!(files_with(tmp.path(), base64(format!("{EMAIL}:{GOOD}").as_bytes()).as_bytes()), Vec::<PathBuf>::new());
    assert!(!ssh_calls().iter().any(|l| l.contains(GOOD)), "{:?}", ssh_calls());
    assert!(!std::fs::read_to_string(base.join("ssh-args")).unwrap_or_default().contains(GOOD));
}

// ---- d. remove login ----

#[tokio::test]
async fn remove_login_takes_it_out_of_the_keychain_and_can_be_repeated() {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    bitbucket::save_login(&st, EMAIL.into(), GOOD.into()).await.unwrap();
    assert!(saved(&st).is_some());

    let s = bitbucket::remove_login(&st).await.unwrap();
    assert_no_login(&s);
    assert_eq!(saved(&st), None);
    assert_no_login(&bitbucket::status(&st).await);

    let s = bitbucket::remove_login(&st).await.expect("removing again is fine");
    assert_no_login(&s);
    assert_eq!(saved(&st), None);
}

// ---- e. the JSON the settings screen reads ----

#[tokio::test]
async fn the_json_shapes_are_the_contract_with_the_settings_screen() {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());

    let v = serde_json::to_value(bitbucket::status(&st).await).unwrap();
    assert_eq!(keys(&v), ["account", "accountProblem", "email", "hasToken"], "{v}");
    assert_eq!((&v["email"], &v["hasToken"], &v["account"]), (&Value::Null, &json!(false), &Value::Null), "{v}");
    assert_eq!(v["accountProblem"]["what"], "No Bitbucket login yet");
    assert!(v["accountProblem"]["fix"].is_string(), "{v}");

    let v = serde_json::to_value(bitbucket::save_login(&st, EMAIL.into(), GOOD.into()).await.unwrap()).unwrap();
    assert_eq!(v, json!({"email": EMAIL, "hasToken": true, "account": ACCOUNT, "accountProblem": null}));
    let v = serde_json::to_value(bitbucket::status(&st).await).unwrap();
    assert_eq!(v, json!({"email": EMAIL, "hasToken": true, "account": ACCOUNT, "accountProblem": null}));

    let c = serde_json::to_value(bitbucket::check(&st).await).unwrap();
    assert_eq!(keys(&c), ["checks", "ok", "pushOver"], "{c}");
    assert_eq!(c["pushOver"], "ssh");
    assert_eq!(c["ok"], true, "{c}");
    let checks = c["checks"].as_array().unwrap();
    assert!(!checks.is_empty());
    for line in checks {
        assert_eq!(keys(line), ["fix", "name", "projectId", "repo", "result", "text"], "{line}");
    }
}

// ---- f. check connection ----

#[tokio::test]
async fn check_connection_has_the_account_ssh_and_a_dry_run_push_per_bitbucket_project() {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    bitbucket::save_login(&st, EMAIL.into(), GOOD.into()).await.unwrap();
    let (shop_bare, shop) = bitbucket_repo(tmp.path(), "acme/shop", "https://bitbucket.org/acme/shop");
    let (web_bare, web) = bitbucket_repo(tmp.path(), "acme/web", "git@bitbucket.org:acme/web.git");
    let site = repo_with_a_commit(&tmp.path().join("site"));
    let old = repo_with_a_commit(&tmp.path().join("old"));
    let shop_id = project(&st, "Shop", "SHOP", &shop, "https://bitbucket.org/acme/shop/src/master/", None);
    let web_id = project(&st, "Web", "WEB", &web, "git@bitbucket.org:acme/web.git", None);
    project(&st, "Site", "SITE", &site, "https://github.com/acme/site", None);
    project(&st, "Old", "OLD", &old, "https://bitbucket.org/acme/old", Some("archived"));
    assert_eq!(gizai_core::projects::get(&st.db, &shop_id).unwrap().repo_url.as_deref(), Some("https://bitbucket.org/acme/shop"));
    assert_eq!(gizai_core::projects::get(&st.db, &web_id).unwrap().repo_url.as_deref(), Some("https://bitbucket.org/acme/web"));
    let configs = [std::fs::read_to_string(shop.join(".git/config")).unwrap(), std::fs::read_to_string(web.join(".git/config")).unwrap()];

    let c = bitbucket::check(&st).await;
    assert_eq!(c.push_over, "ssh");
    let names: Vec<&str> = c.checks.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(&names[..2], ["Account", "SSH"], "{c:#?}");
    assert_eq!((c.checks[0].result.as_str(), c.checks[1].result.as_str()), ("ok", "ok"), "{c:#?}");
    assert!(c.checks[0].text.contains(ACCOUNT), "{}", c.checks[0].text);
    assert!(c.checks[1].text.contains("git@bitbucket.org"), "{}", c.checks[1].text);
    let mut projects: Vec<(&str, &str, &str, Option<&str>, Option<&str>)> = c.checks[2..].iter()
        .map(|l| (l.name.as_str(), l.result.as_str(), l.text.as_str(), l.project_id.as_deref(), l.repo.as_deref())).collect();
    projects.sort();
    assert_eq!(projects, [
        ("Shop", "ok", "You can push to acme/shop", Some(shop_id.as_str()), Some("acme/shop")),
        ("Web", "ok", "You can push to acme/web", Some(web_id.as_str()), Some("acme/web")),
    ], "one line per Bitbucket project, none for GitHub or an archived one: {c:#?}");
    assert!(c.checks.iter().all(|l| l.fix.is_none()), "{c:#?}");
    assert!(c.ok, "{c:#?}");
    // the projects' checks went over ssh to git@bitbucket.org, as a dry run: nothing arrived, nothing changed
    let calls = ssh_calls();
    assert!(calls.iter().any(|l| l.contains("-T git@bitbucket.org")), "{calls:?}");
    assert!(calls.iter().any(|l| l.contains("git@bitbucket.org") && l.contains("git-receive-pack 'acme/shop'")), "{calls:?}");
    assert!(calls.iter().any(|l| l.contains("git@bitbucket.org") && l.contains("git-receive-pack 'acme/web.git'")), "{calls:?}");
    assert!(!calls.iter().any(|l| l.contains("acme/site") || l.contains("acme/old")), "{calls:?}");
    assert!(!has_branch(&shop_bare, "gizai-connection-check") && !has_branch(&web_bare, "gizai-connection-check"), "a dry run sends nothing");
    assert_eq!([std::fs::read_to_string(shop.join(".git/config")).unwrap(), std::fs::read_to_string(web.join(".git/config")).unwrap()], configs);
    assert_eq!(git(&shop, &["remote", "get-url", "origin"]), "https://bitbucket.org/acme/shop");

    // Bitbucket refuses the SSH key for acme/web: its line says what to do
    std::fs::write(web_bare.join("fake-ssh-mode"), "publickey").unwrap();
    let c = bitbucket::check(&st).await;
    assert!(!c.ok, "{c:#?}");
    let line = c.checks.iter().find(|l| l.name == "Web").unwrap();
    assert_eq!(line.result, "failed", "{line:?}");
    assert!(line.text.contains("Bitbucket") && line.text.contains("SSH key"), "{line:?}");
    assert!(line.fix.as_deref().unwrap_or_default().contains("Bitbucket → Personal settings → SSH keys"), "{line:?}");
    assert_eq!((line.project_id.as_deref(), line.repo.as_deref()), (Some(web_id.as_str()), Some("acme/web")));
    let shop_line = c.checks.iter().find(|l| l.name == "Shop").unwrap();
    assert_eq!(shop_line.result, "ok", "{shop_line:?}");
    assert!(!has_branch(&web_bare, "gizai-connection-check"));
}

#[tokio::test]
async fn check_connection_without_a_login_says_what_to_do() {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let c = bitbucket::check(&st).await;
    assert!(!c.ok, "{c:#?}");
    assert_eq!(c.push_over, "ssh");
    let lines: Vec<(&str, &str)> = c.checks.iter().map(|l| (l.name.as_str(), l.result.as_str())).collect();
    assert_eq!(lines, [("Account", "failed"), ("SSH", "ok")], "{c:#?}");
    assert_eq!(c.checks[0].text, "No Bitbucket login yet");
    let fix = c.checks[0].fix.as_deref().expect("what to do");
    for scope in SCOPES {
        assert!(fix.contains(scope), "{fix}");
    }
}

// ---- g. the repository check offers the Bitbucket remote ----

#[test]
fn the_repository_check_offers_the_bitbucket_remote_next_to_the_github_one_origin_first() {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    use gizai_lib::git::repo_check;

    let both_bitbucket = tmp.path().join("bb");
    std::fs::create_dir(&both_bitbucket).unwrap();
    git(&both_bitbucket, &["init", "-q", "-b", "main"]);
    git(&both_bitbucket, &["remote", "add", "upstream", "git@bitbucket.org:acme/shop.git"]);
    git(&both_bitbucket, &["remote", "add", "origin", "https://jefsev@bitbucket.org/acme/web.git"]);
    let r = repo_check(&both_bitbucket);
    assert!(r.is_git);
    assert_eq!((r.bitbucket.as_deref(), r.github.as_deref()), (Some("https://bitbucket.org/acme/web"), None), "origin first");

    let mixed = tmp.path().join("mixed");
    std::fs::create_dir(&mixed).unwrap();
    git(&mixed, &["init", "-q", "-b", "main"]);
    git(&mixed, &["remote", "add", "origin", "git@github.com:acme/shop.git"]);
    git(&mixed, &["remote", "add", "upstream", "https://bitbucket.org/acme/shop.git"]);
    let r = repo_check(&mixed);
    assert_eq!((r.github.as_deref(), r.bitbucket.as_deref()), (Some("https://github.com/acme/shop"), Some("https://bitbucket.org/acme/shop")));

    let none = tmp.path().join("none");
    std::fs::create_dir(&none).unwrap();
    git(&none, &["init", "-q", "-b", "main"]);
    let r = repo_check(&none);
    assert!(r.is_git);
    assert_eq!((r.github, r.bitbucket), (None, None));

    // the JSON the project form reads
    let v = serde_json::to_value(repo_check(&both_bitbucket)).unwrap();
    assert_eq!(v["bitbucket"], "https://bitbucket.org/acme/web");
    assert_eq!(v["github"], Value::Null);
}

// ---- h. the Team Lead's project tools ----

#[tokio::test]
async fn the_team_leads_project_tools_take_a_bitbucket_link_as_repository_and_github_stays_its_alias() {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let lead = gizai_core::team::add_agent(&st.db, &st.you_id, &team_id, AgentInput {
        name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap();
    let call = |name: &'static str, args: Value| {
        let (st, lead) = (st.clone(), lead.clone());
        async move { tools::call(&st, &lead, name, args).await.unwrap_or_else(|e| panic!("{name} failed: {e}")) }
    };
    let sts = repo_with_a_commit(&tmp.path().join("sts"));

    let r = call("create_project", json!({"name": "STS Dias", "key": "STS", "repo_path": sts.display().to_string(),
                                          "repository": "https://bitbucket.org/snxo2021/sts_dias/src/master/"})).await;
    let id = r["project"]["id"].as_str().unwrap().to_string();
    assert_eq!(gizai_core::projects::get(&st.db, &id).unwrap().repo_url.as_deref(), Some("https://bitbucket.org/snxo2021/sts_dias"));
    let p = call("get_project", json!({"project": "STS"})).await;
    assert_eq!((&p["project"]["repository"], &p["project"]["provider"]), (&json!("https://bitbucket.org/snxo2021/sts_dias"), &json!("bitbucket")), "{p}");

    // the old name still works, and moves the project to GitHub
    let u = call("update_project", json!({"project": "STS", "github": "git@github.com:acme/shop.git"})).await;
    assert_eq!((&u["project"]["repository"], &u["project"]["provider"]), (&json!("https://github.com/acme/shop"), &json!("github")), "{u}");
    assert_eq!(gizai_core::projects::get(&st.db, &id).unwrap().repo_url.as_deref(), Some("https://github.com/acme/shop"));
    let p = call("get_project", json!({"project": "STS"})).await;
    assert_eq!((&p["project"]["repository"], &p["project"]["provider"]), (&json!("https://github.com/acme/shop"), &json!("github")), "{p}");

    // back to Bitbucket with repository, then an empty one clears it
    let u = call("update_project", json!({"project": "STS", "repository": "https://jefsev@bitbucket.org/snxo2021/sts_dias.git"})).await;
    assert_eq!((&u["project"]["repository"], &u["project"]["provider"]), (&json!("https://bitbucket.org/snxo2021/sts_dias"), &json!("bitbucket")), "{u}");
    let u = call("update_project", json!({"project": "STS", "repository": ""})).await;
    assert_eq!((&u["project"]["repository"], &u["project"]["provider"]), (&Value::Null, &Value::Null), "{u}");
    assert_eq!(gizai_core::projects::get(&st.db, &id).unwrap().repo_url, None);
    let p = call("get_project", json!({"project": "STS"})).await;
    assert_eq!((&p["project"]["repository"], &p["project"]["provider"]), (&Value::Null, &Value::Null), "{p}");

    // create_project with the old name and a Bitbucket link
    let other = repo_with_a_commit(&tmp.path().join("other"));
    let r = call("create_project", json!({"name": "Old alias", "key": "OLDA", "repo_path": other.display().to_string(),
                                          "github": "git@bitbucket.org:acme/old-alias.git"})).await;
    let id = r["project"]["id"].as_str().unwrap().to_string();
    assert_eq!(gizai_core::projects::get(&st.db, &id).unwrap().repo_url.as_deref(), Some("https://bitbucket.org/acme/old-alias"));
    let p = call("get_project", json!({"project": "OLDA"})).await;
    assert_eq!((&p["project"]["repository"], &p["project"]["provider"]), (&json!("https://bitbucket.org/acme/old-alias"), &json!("bitbucket")), "{p}");
}

#[test]
fn the_tool_catalog_offers_repository_for_create_and_update_project() {
    let cat = tools::catalog();
    for name in ["create_project", "update_project"] {
        let t = cat.iter().find(|t| t.name == name).unwrap_or_else(|| panic!("no {name}"));
        let props = &t.input_schema["properties"];
        assert_eq!(props["repository"]["type"], "string", "{name}: {props}");
        let desc = props["repository"]["description"].as_str().unwrap_or_default();
        assert!(desc.contains("Bitbucket") && desc.contains("GitHub"), "{name}: {desc}");
        assert!(props["github"].is_object(), "{name} keeps github as the old name: {props}");
    }
    let get = cat.iter().find(|t| t.name == "get_project").unwrap();
    assert!(get.read_only);
}
