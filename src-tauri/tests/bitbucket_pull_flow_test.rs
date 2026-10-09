//! GA-59 QA: review on Bitbucket Cloud, end to end without bitbucket.org. "Bitbucket" is a fake REST API on 127.0.0.1
//! (GIZAI_BITBUCKET_API) that keeps each repository's pull requests and records every request, and a fake ssh as your
//! own GIT_SSH_COMMAND that logs its arguments and serves git@bitbucket.org:<workspace>/<repo> from local bare
//! repositories. The project's local clone has its origin at an https://bitbucket.org/ address and allows no https at
//! all, so a push that doesn't go over ssh fails. Covers Open pull request on a Bitbucket card in Review (push over SSH,
//! one pull request into the project's main branch, or the open one kept), the PR check (merge → Deploy or Done and
//! clean-up, declined and superseded → closed, draft) and the missing login.
// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

use gizai_agents::bitbucket::Login;
use gizai_lib::pulls::{self, PullInfo};
use serde_json::{Value, json};

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const EMAIL: &str = "qa@acme.test";
const TOKEN: &str = "ATATT3xFfGF0-qa-token";
/// "Basic " and base64 of "qa@acme.test:ATATT3xFfGF0-qa-token" (worked out with coreutils' base64, not Gizai's).
const AUTH: &str = "Basic cWFAYWNtZS50ZXN0OkFUQVRUM3hGZkdGMC1xYS10b2tlbg==";

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

fn has_branch(repo: &Path, branch: &str) -> bool {
    Command::new("git").args(["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).current_dir(repo).output().unwrap().status.success()
}

/// Writes an executable script through a child `sh`, so this process never holds it open for writing (a leaked
/// handle would make running it fail with "Text file busy").
fn write_script(path: &Path, script: &str) {
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path).stdin(Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

// ---------------------------------------------------------------------------------------------------------------
// The fake Bitbucket REST API
// ---------------------------------------------------------------------------------------------------------------

/// One request the fake API got: its method, its path and query (percent-decoded), its Authorization and its body.
#[derive(Debug, Clone)]
struct Req {
    method: String,
    target: String,
    auth: Option<String>,
    body: String,
}

#[derive(Default)]
struct Api {
    /// "workspace/repo" → its pull requests as Bitbucket lists them (newest first).
    pulls: HashMap<String, Vec<Value>>,
    /// "workspace/repo#id" → the full hashes of that pull request's commits.
    commits: HashMap<String, Vec<String>>,
    requests: Vec<Req>,
}

fn api() -> &'static Mutex<Api> {
    static API: OnceLock<Mutex<Api>> = OnceLock::new();
    API.get_or_init(|| Mutex::new(Api::default()))
}

/// %XX and '+' decoded, so assertions read the query the way it was meant.
fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let (mut out, mut i) = (Vec::with_capacity(b.len()), 0);
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                match std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(v) => { out.push(v); i += 3; continue; }
                    None => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// What the fake API answers, as Bitbucket Cloud's REST API 2.0 would (only what Gizai uses).
fn answer(method: &str, target: &str, body: &str, auth: Option<String>) -> (u16, Value) {
    let mut guard = api().lock().unwrap();
    let a = &mut *guard;
    a.requests.push(Req { method: method.to_string(), target: decode(target), auth, body: body.to_string() });
    let path = target.split('?').next().unwrap_or(target);
    let segs: Vec<&str> = path.trim_matches('/').split('/').collect();
    match (method, segs.as_slice()) {
        ("GET", ["user"]) => (200, json!({"display_name": "QA Tester", "username": "qa-tester"})),
        ("GET", ["repositories", ws, repo, "pullrequests"]) => {
            (200, json!({"values": a.pulls.get(&format!("{ws}/{repo}")).cloned().unwrap_or_default(), "pagelen": 20}))
        }
        ("POST", ["repositories", ws, repo, "pullrequests"]) => {
            let sent: Value = serde_json::from_str(body).unwrap_or(Value::Null);
            let list = a.pulls.entry(format!("{ws}/{repo}")).or_default();
            let id = list.iter().filter_map(|p| p["id"].as_u64()).max().unwrap_or(0) + 1;
            let pr = json!({
                "id": id, "type": "pullrequest", "title": sent["title"], "description": sent["description"], "state": "OPEN", "draft": false,
                "links": {"html": {"href": format!("https://bitbucket.org/{ws}/{repo}/pull-requests/{id}")}},
                "source": sent["source"], "destination": sent["destination"],
            });
            list.insert(0, pr.clone());
            (201, pr)
        }
        ("GET", ["repositories", ws, repo, "pullrequests", id, "commits"]) => {
            let hashes = a.commits.get(&format!("{ws}/{repo}#{id}")).cloned().unwrap_or_default();
            (200, json!({"values": hashes.iter().map(|h| json!({"hash": h, "type": "commit"})).collect::<Vec<_>>(), "pagelen": 100}))
        }
        _ => (404, json!({"type": "error", "error": {"message": format!("{method} {path} isn't here")}})),
    }
}

fn serve(stream: TcpStream) {
    let mut out = stream.try_clone().unwrap();
    let mut r = BufReader::new(stream);
    let mut line = String::new();
    if r.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
    let mut headers: HashMap<String, String> = HashMap::new();
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).unwrap_or(0) == 0 {
            break;
        }
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    let mut body = vec![];
    if let Some(n) = headers.get("content-length").and_then(|n| n.parse::<usize>().ok()) {
        body = vec![0; n];
        r.read_exact(&mut body).unwrap();
    } else if headers.get("transfer-encoding").is_some_and(|t| t.contains("chunked")) {
        loop {
            let mut size = String::new();
            r.read_line(&mut size).unwrap();
            let n = usize::from_str_radix(size.trim().split(';').next().unwrap_or("0"), 16).unwrap_or(0);
            let mut rest = vec![0; n + 2]; // the chunk and its CRLF (the last one: just CRLF)
            r.read_exact(&mut rest).unwrap();
            if n == 0 {
                break;
            }
            body.extend_from_slice(&rest[..n]);
        }
    }
    let (status, reply) = answer(&method, &target, &String::from_utf8_lossy(&body), headers.get("authorization").cloned());
    let text = reply.to_string();
    let reason = match status { 200 => "OK", 201 => "Created", _ => "Not Found" };
    let _ = write!(out, "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
    let _ = out.flush();
}

/// The fake ssh and the fake API, set up once for this test binary: every test calls this first, before it starts any
/// process (GIT_SSH_COMMAND and GIZAI_BITBUCKET_API are the whole process's). Returns the fake ssh's folder:
/// base/ssh-args (one line per call) and "Bitbucket" at base/bitbucket/<workspace>/<repo>.git (found with or without
/// .git).
fn fakes() -> &'static Path {
    static BASE: OnceLock<PathBuf> = OnceLock::new();
    BASE.get_or_init(|| {
        let base = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap().keep();
        let ssh = base.join("bin/ssh");
        std::fs::create_dir_all(ssh.parent().unwrap()).unwrap();
        std::fs::create_dir_all(base.join("bitbucket")).unwrap();
        write_script(&ssh, &format!(r#"#!/bin/sh
base='{base}'
[ "$1" = "-G" ] && exit 1
echo "$*" >> "$base/ssh-args"
cmd=""
host=""
for a in "$@"; do
  case "$a" in
    git-receive-pack*|git-upload-pack*) cmd="$a" ;;
    *@bitbucket.org) host=bitbucket ;;
    *@github.com) host=github ;;
  esac
done
if [ -z "$cmd" ]; then
  echo "authenticated via ssh key." >&2
  echo "You can use git to connect to Bitbucket. Shell access is disabled" >&2; exit 0
fi
[ -n "$host" ] || {{ echo "ssh: Could not resolve hostname" >&2; exit 255; }}
path="${{cmd#* }}"
path="$(printf '%s' "$path" | tr -d "'")"
path="${{path#/}}"
repo="$base/$host/$path"
[ -d "$repo" ] || repo="$repo.git"
verb="${{cmd%% *}}"
exec git "${{verb#git-}}" "$repo"
"#, base = base.display()));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                std::thread::spawn(move || serve(stream));
            }
        });
        // SAFETY: set once, before this test binary starts any process (every test calls fakes first)
        unsafe {
            std::env::set_var("GIT_SSH_COMMAND", &ssh);
            std::env::set_var("GIZAI_BITBUCKET_API", format!("http://127.0.0.1:{port}"));
        }
        base
    })
}

fn ssh_calls() -> Vec<String> {
    std::fs::read_to_string(fakes().join("ssh-args")).unwrap_or_default().lines().map(String::from).collect()
}

/// The requests the fake API got about `repo` ("workspace/repo").
fn requests(repo: &str) -> Vec<Req> {
    let at = format!("/repositories/{repo}/");
    api().lock().unwrap().requests.iter().filter(|r| r.target.starts_with(&at)).cloned().collect()
}

/// The pull requests the fake API lists for `repo` from now on (newest first).
fn set_pulls(repo: &str, pulls: Vec<Value>) {
    api().lock().unwrap().pulls.insert(repo.to_string(), pulls);
}

fn set_commits(repo: &str, id: u64, hashes: &[&str]) {
    api().lock().unwrap().commits.insert(format!("{repo}#{id}"), hashes.iter().map(|h| h.to_string()).collect());
}

/// A pull request as Bitbucket's API gives it; `hash` is its latest commit, short (12 characters) as Bitbucket gives it.
fn pr(repo: &str, id: u64, state: &str, draft: bool, branch: &str, hash: Option<&str>) -> Value {
    let mut source = json!({"branch": {"name": branch}});
    if let Some(h) = hash {
        source["commit"] = json!({"hash": h, "type": "commit"});
    }
    json!({"id": id, "type": "pullrequest", "title": "t", "state": state, "draft": draft,
           "links": {"html": {"href": format!("https://bitbucket.org/{repo}/pull-requests/{id}")}},
           "source": source, "destination": {"branch": {"name": "master"}}})
}

fn url(repo: &str, id: u64) -> String {
    format!("https://bitbucket.org/{repo}/pull-requests/{id}")
}

// ---------------------------------------------------------------------------------------------------------------
// The card
// ---------------------------------------------------------------------------------------------------------------

/// "Bitbucket" at git@bitbucket.org:<repo> behind the fake ssh (its main branch is master), and the project's local
/// clone with its origin at git@bitbucket.org:<repo>.git (so a card's run fetches master over the fake ssh). The clone
/// allows no https at all, so nothing can reach bitbucket.org.
fn bitbucket(tmp: &Path, repo: &str) -> (PathBuf, PathBuf) {
    let base = fakes();
    let src = tmp.join("src");
    std::fs::create_dir(&src).unwrap();
    git(&src, &["init", "-q", "-b", "master"]);
    git(&src, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init"]);
    let bare = base.join(format!("bitbucket/{repo}.git"));
    git(tmp, &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
    let local = tmp.join("local");
    git(tmp, &["clone", "-q", bare.to_str().unwrap(), local.to_str().unwrap()]);
    git(&local, &["remote", "set-url", "origin", &format!("git@bitbucket.org:{repo}.git")]);
    git(&local, &["config", "protocol.https.allow", "never"]);
    (bare, local)
}

fn save_login(st: &gizai_lib::AppState) {
    gizai_agents::bitbucket::save_login(st.keychain.as_ref(), &Login { email: EMAIL.into(), token: TOKEN.into() }).unwrap();
}

struct Card { st: gizai_lib::AppState, task: String, bare: PathBuf, local: PathBuf, wt: PathBuf, branch: String }

/// Card KADE-1 of project Kade, linked to https://bitbucket.org/<repo>/src/master/ (stored tidy) with main branch
/// master, after an agent's run (its worktree and branch, with one commit); in Testing. After the run the clone's
/// origin is `origin` (an https form of the link): a push goes over ssh only when Gizai sends it there. With `login`,
/// your Bitbucket login is in the keychain.
async fn worked_card(tmp: &Path, repo: &str, origin: &str, login: bool) -> Card {
    fakes();
    let st = gizai_lib::test_state(tmp);
    let (bare, local) = bitbucket(tmp, repo);
    let task = gizai_lib::test_task(&st, local.to_str().unwrap(), "backend");
    let p = gizai_core::projects::list(&st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    gizai_core::projects::update(&st.db, &st.you_id, &p.id, gizai_core::model::ProjectInput { name: p.name.clone(), key: p.key.clone(),
        repo_path: p.repo_path.clone(), repo_url: Some(format!("https://bitbucket.org/{repo}/src/master/")), default_branch: Some("master".into()),
        ..Default::default() }).unwrap();
    let p = gizai_core::projects::get(&st.db, &p.id).unwrap();
    assert_eq!((p.repo_url.as_deref(), p.default_branch.as_str()), (Some(format!("https://bitbucket.org/{repo}").as_str()), "master"), "stored tidy");
    if login {
        save_login(&st);
    }
    gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    let run = gizai_core::runs::list_for_task(&st.db, &task).unwrap().remove(0);
    let (wt, branch) = (PathBuf::from(run.worktree_path.unwrap()), run.branch.unwrap());
    assert!(wt.starts_with(st.data_dir.join("worktrees")), "{wt:?}");
    assert_eq!(run.base_sha.as_deref(), Some(git(&bare, &["rev-parse", "master"]).as_str()), "started from Bitbucket's master");
    commit(&wt, "invoices.csv");
    git(&local, &["remote", "set-url", "origin", origin]);
    Card { st, task, bare, local, wt, branch }
}

fn commit(dir: &Path, name: &str) {
    std::fs::write(dir.join(name), name).unwrap();
    git(dir, &["add", name]);
    git(dir, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", name]);
}

fn team(st: &gizai_lib::AppState) -> gizai_core::team::Team {
    gizai_core::team::get(&st.db, &gizai_core::team::list(&st.db).unwrap()[0].id).unwrap()
}

fn state_id(st: &gizai_lib::AppState, category: &str) -> String {
    team(st).states.iter().find(|s| s.category == category).unwrap().id.clone()
}

fn to_review(c: &Card) {
    gizai_core::tasks::move_to(&c.st.db, &c.st.you_id, &c.task, &state_id(&c.st, "review"), "").unwrap();
}

/// GA-49: in the seed Review's next column is Deploy; a team without a deploy step links Review to Done.
fn review_then_done(c: &Card) {
    gizai_core::columns::set_column(&c.st.db, &c.st.you_id, &state_id(&c.st, "review"),
        gizai_core::columns::ColumnInput { next_state_id: Some(state_id(&c.st, "done")), ..Default::default() }).unwrap();
}

fn task(c: &Card) -> gizai_core::model::Task { gizai_core::tasks::get(&c.st.db, &c.task).unwrap() }

fn tip(c: &Card) -> String { git(&c.wt, &["rev-parse", "HEAD"]) }

// ---------------------------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------------------------

#[tokio::test]
async fn open_pull_request_pushes_over_ssh_to_bitbucket_and_opens_one_pull_request_into_the_main_branch() {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    let repo = "acme/shop-open";
    let c = worked_card(tmp.path(), repo, &format!("https://bitbucket.org/{repo}"), true).await;
    gizai_core::tasks::update(&c.st.db, &c.st.you_id, &c.task, gizai_core::model::TaskPatch {
        description_md: Some("Download all invoices as one CSV file.".into()), acceptance_md: Some("- [ ] One row per invoice".into()), ..Default::default() }).unwrap();
    // the fixture: without Gizai's rewrite, a push to origin (https) can't go anywhere
    let plain = Command::new("git").args(["push", "-q", "origin", &c.branch]).current_dir(&c.local).output().unwrap();
    assert!(!plain.status.success(), "the clone must not reach https://bitbucket.org on its own");
    assert!(!has_branch(&c.bare, &c.branch));
    let config = std::fs::read_to_string(c.local.join(".git/config")).unwrap();

    // in Testing: no pull request, and nothing asked
    let e = pulls::open(&c.st, &c.task).await.unwrap_err();
    assert!(e.contains("KADE-1 isn't in Review"), "{e}");
    assert!(requests(repo).is_empty(), "{:?}", requests(repo));

    to_review(&c);
    let pr = pulls::open(&c.st, &c.task).await.unwrap();
    assert_eq!(pr, PullInfo { url: url(repo, 1), number: Some(1), state: "open".into(), note: None });

    // pushed over SSH to git@bitbucket.org with your keys; the branch is in "Bitbucket" with its latest commit
    assert!(ssh_calls().iter().any(|l| l.contains("git@bitbucket.org") && l.contains(&format!("git-receive-pack '{repo}'"))), "{:?}", ssh_calls());
    assert_eq!(git(&c.bare, &["rev-parse", &c.branch]), tip(&c), "the branch arrived in the bare repository behind git@bitbucket.org:{repo}");
    // the remote keeps its https address and the repository's config is as it was
    assert_eq!(git(&c.local, &["remote", "get-url", "origin"]), format!("https://bitbucket.org/{repo}"));
    assert_eq!(std::fs::read_to_string(c.local.join(".git/config")).unwrap(), config);

    // Bitbucket's API: the branch's pull requests, then one new pull request
    let reqs = requests(repo);
    assert_eq!(reqs.iter().map(|r| r.method.as_str()).collect::<Vec<_>>(), ["GET", "POST"], "{reqs:?}");
    let list = &reqs[0].target;
    assert!(list.starts_with(&format!("/repositories/{repo}/pullrequests?")), "{list}");
    assert!(list.contains(&format!("q=source.branch.name = \"{}\"", c.branch)), "{list}");
    for s in ["state=OPEN", "state=MERGED", "state=DECLINED", "state=SUPERSEDED"] {
        assert!(list.contains(s), "{s} in {list}");
    }
    assert_eq!(reqs[1].target, format!("/repositories/{repo}/pullrequests"));
    let sent: Value = serde_json::from_str(&reqs[1].body).unwrap();
    assert_eq!(sent, json!({
        "title": "KADE-1: Export invoices as CSV",
        "description": "Download all invoices as one CSV file.\n\n## Acceptance criteria\n\n- [ ] One row per invoice\n\nFrom Gizai card KADE-1.",
        "source": {"branch": {"name": c.branch}},
        "destination": {"branch": {"name": "master"}},
    }));
    // the login goes as HTTP Basic in the Authorization header only, never in an address
    for r in &reqs {
        assert_eq!(r.auth.as_deref(), Some(AUTH), "{r:?}");
        assert!(!r.target.contains(TOKEN) && !r.target.contains(EMAIL), "{r:?}");
    }
    assert!(!ssh_calls().iter().any(|l| l.contains(TOKEN)));

    let t = task(&c);
    assert_eq!((t.pr_url.as_deref(), t.pr_state.as_deref(), t.state_name.as_str()), (Some(url(repo, 1).as_str()), Some("open"), "Review"));
    let last = gizai_core::tasks::activity(&c.st.db, &c.task).unwrap().pop().unwrap();
    assert!(last.actor_name.is_some(), "opened by you");
    assert_eq!(last.diff, json!({"pullRequest": url(repo, 1), "prState": "open", "opened": true}));
    // no token in Gizai's database, settings or logs
    assert_eq!(files_with(&c.st.data_dir, TOKEN.as_bytes()), Vec::<PathBuf>::new());
}

#[tokio::test]
async fn a_branch_with_an_open_pull_request_on_bitbucket_keeps_it_and_the_push_goes_over_ssh_from_a_clone_url_with_a_user_name() {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    let repo = "acme/shop-keep";
    // the address Bitbucket's Clone button gives
    let c = worked_card(tmp.path(), repo, &format!("https://jefsev@bitbucket.org/{repo}.git"), true).await;
    to_review(&c);
    let head = tip(&c);
    set_pulls(repo, vec![pr(repo, 4, "OPEN", false, &c.branch, Some(&head[..12]))]);

    let pr = pulls::open(&c.st, &c.task).await.unwrap();
    assert_eq!(pr, PullInfo { url: url(repo, 4), number: Some(4), state: "open".into(), note: None });
    let reqs = requests(repo);
    assert!(!reqs.iter().any(|r| r.method == "POST"), "no new pull request: {reqs:?}");
    assert!(ssh_calls().iter().any(|l| l.contains("git@bitbucket.org") && l.contains(&format!("git-receive-pack '{repo}.git'"))), "{:?}", ssh_calls());
    assert_eq!(git(&c.bare, &["rev-parse", &c.branch]), head);
    assert_eq!(git(&c.local, &["remote", "get-url", "origin"]), format!("https://jefsev@bitbucket.org/{repo}.git"));
    let t = task(&c);
    assert_eq!((t.pr_url.as_deref(), t.pr_state.as_deref()), (Some(url(repo, 4).as_str()), Some("open")));
    let last = gizai_core::tasks::activity(&c.st.db, &c.task).unwrap().pop().unwrap();
    assert_eq!((last.actor_name, last.diff), (None, json!({"pullRequest": url(repo, 4), "prState": "open"})), "seen by Gizai, not opened by you");

    // Push branch again with a new commit and an uncommitted change: the same pull request, nothing new opened
    commit(&c.wt, "fix.csv");
    std::fs::write(c.wt.join("scratch.txt"), "not committed").unwrap();
    let pr = pulls::open(&c.st, &c.task).await.unwrap();
    assert_eq!((pr.url.as_str(), pr.number), (url(repo, 4).as_str(), Some(4)));
    assert_eq!(pr.note.as_deref(), Some("Its worktree has 1 uncommitted change, which the pull request doesn't have"));
    assert_eq!(git(&c.bare, &["rev-parse", &c.branch]), tip(&c), "the new commit went to Bitbucket");
    assert!(!requests(repo).iter().any(|r| r.method == "POST"));
}

#[tokio::test]
async fn a_merge_on_bitbucket_moves_the_card_to_deploy_removes_its_worktree_and_nothing_starts_on_it() {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    let repo = "acme/shop-merge";
    let c = worked_card(tmp.path(), repo, &format!("https://bitbucket.org/{repo}"), true).await;
    let ops = gizai_core::team::add_agent(&c.st.db, &c.st.you_id, &team(&c.st).id, gizai_core::model::AgentInput { name: "DevOps Agent".into(),
        role_key: "devops".into(), wakeup: "on_assign".into(), ..Default::default() }).unwrap();
    to_review(&c);
    gizai_core::tasks::update(&c.st.db, &c.st.you_id, &c.task, gizai_core::model::TaskPatch { assignee_id: Some(ops.clone()), ..Default::default() }).unwrap();
    pulls::open(&c.st, &c.task).await.unwrap();
    let runs_before = gizai_core::runs::list_for_task(&c.st.db, &c.task).unwrap().len();

    // still open on Bitbucket: nothing moves
    let checked = pulls::check_all(&c.st).await;
    assert_eq!((checked.len(), checked[0].changed, checked[0].moved_to.clone()), (1, false, None));
    assert_eq!(task(&c).state_name, "Review");

    // merged on Bitbucket: its latest commit is the branch's (short, as Bitbucket gives it); no commit list
    let head = tip(&c);
    set_pulls(repo, vec![pr(repo, 1, "MERGED", false, &c.branch, Some(&head[..12]))]);
    let checked = pulls::check_all(&c.st).await;
    assert_eq!((checked.len(), checked[0].moved_to.as_deref(), checked[0].changed), (1, Some("Deploy"), true));
    let said = format!("removed its worktree and deleted branch {} after the merge", c.branch);
    assert_eq!(checked[0].pull, Some(PullInfo { url: url(repo, 1), number: Some(1), state: "merged".into(), note: Some(said.clone()) }));
    let t = task(&c);
    assert_eq!((t.state_name.as_str(), t.state_category.as_str(), t.pr_state.as_deref(), t.assignee_id.as_deref()),
               ("Deploy", "deploy", Some("merged"), Some(ops.as_str())));
    let a = gizai_core::tasks::activity(&c.st.db, &c.task).unwrap();
    assert!(a.iter().any(|e| e.diff == json!({"column": ["Review", "Deploy"]})), "{a:?}");
    assert!(a.iter().any(|e| e.actor_name.is_none() && e.diff == json!({"cleanup": said})), "{a:?}");
    assert!(!c.wt.exists(), "its worktree is removed");
    assert!(!has_branch(&c.local, &c.branch), "its local branch is deleted");
    assert!(has_branch(&c.bare, &c.branch), "Bitbucket's copy is Bitbucket's");
    assert!(requests(repo).iter().any(|r| r.method == "GET" && r.target.starts_with(&format!("/repositories/{repo}/pullrequests/1/commits"))),
            "a merged pull request's commits are asked for: {:?}", requests(repo));

    // nothing starts on it, not even the DevOps Agent it is assigned to
    gizai_lib::runs::dispatch(&c.st, &c.task).await;
    gizai_lib::runs::pull(&c.st).await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(gizai_lib::runs::live(&c.st).is_empty());
    assert_eq!(gizai_core::runs::list_for_task(&c.st.db, &c.task).unwrap().len(), runs_before);
    // Deploy: the next checks leave it alone and don't ask Bitbucket
    let asked = requests(repo).len();
    assert!(pulls::check_all(&c.st).await.is_empty());
    assert_eq!(requests(repo).len(), asked);
    assert_eq!(task(&c).state_name, "Deploy", "until a person drags it to Done");
}

#[tokio::test]
async fn check_now_follows_a_merge_an_agent_made_by_its_commits_and_an_older_merge_moves_nothing() {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    let repo = "acme/shop-done";
    let c = worked_card(tmp.path(), repo, &format!("https://bitbucket.org/{repo}"), true).await;
    review_then_done(&c);
    to_review(&c);
    let head = tip(&c);
    let master = git(&c.local, &["rev-parse", "master"]);

    // the branch's pull request #3 was merged before this card's latest commit: the card stays for your review
    set_pulls(repo, vec![pr(repo, 3, "MERGED", false, &c.branch, Some(&master[..12]))]);
    set_commits(repo, 3, &[&master]);
    let p = pulls::check(&c.st, &c.task).await.unwrap();
    assert_eq!(p.as_ref().map(|p| p.state.as_str()), Some("merged"));
    let t = task(&c);
    assert_eq!(t.state_name, "Review", "stays for your review");
    assert!(c.wt.exists() && has_branch(&c.local, &c.branch));

    // an agent pushed and Bitbucket merged #5: no latest commit given, its commits have the branch's tip
    set_pulls(repo, vec![pr(repo, 5, "MERGED", false, &c.branch, None)]);
    set_commits(repo, 5, &[&head, &master]);
    let p = pulls::check(&c.st, &c.task).await.unwrap();
    let said = format!("removed its worktree and deleted branch {} after the merge", c.branch);
    assert_eq!(p, Some(PullInfo { url: url(repo, 5), number: Some(5), state: "merged".into(), note: Some(said) }));
    let t = task(&c);
    assert_eq!((t.state_name.as_str(), t.pr_url.as_deref(), t.pr_state.as_deref()), ("Done", Some(url(repo, 5).as_str()), Some("merged")));
    assert!(!c.wt.exists() && !has_branch(&c.local, &c.branch));
    assert!(requests(repo).iter().any(|r| r.target == format!("/repositories/{repo}/pullrequests/5/commits?pagelen=100")), "{:?}", requests(repo));
}

#[tokio::test]
async fn declined_and_superseded_pull_requests_show_as_closed_and_a_draft_as_draft() {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    let repo = "acme/shop-states";
    let c = worked_card(tmp.path(), repo, &format!("https://bitbucket.org/{repo}"), true).await;
    to_review(&c);
    let head = tip(&c);
    let short = &head[..12];
    for (state, draft, shown) in [("OPEN", true, "draft"), ("DECLINED", false, "closed"), ("OPEN", false, "open"), ("SUPERSEDED", false, "closed")] {
        set_pulls(repo, vec![pr(repo, 2, state, draft, &c.branch, Some(short))]);
        let p = pulls::check(&c.st, &c.task).await.unwrap();
        assert_eq!(p, Some(PullInfo { url: url(repo, 2), number: Some(2), state: shown.into(), note: None }), "{state} draft={draft}");
        let t = task(&c);
        assert_eq!((t.pr_url.as_deref(), t.pr_state.as_deref(), t.state_name.as_str()), (Some(url(repo, 2).as_str()), Some(shown), "Review"),
                   "{state} draft={draft}");
    }
    assert!(c.wt.exists() && has_branch(&c.local, &c.branch), "a closed pull request cleans nothing up");
    // the PR check (every two minutes) sees the same
    let checked = pulls::check_all(&c.st).await;
    assert_eq!((checked.len(), checked[0].changed, checked[0].moved_to.clone()), (1, false, None));
    assert_eq!(checked[0].pull.as_ref().map(|p| p.state.as_str()), Some("closed"));
    // and the closed one keeps no commit request: only merged ones get their commits asked for
    assert!(!requests(repo).iter().any(|r| r.target.contains("/commits")), "{:?}", requests(repo));
}

/// A fake gh in `dir`: each call's arguments go to dir/gh-args; `pr list` answers [] (no pull requests).
fn fake_gh(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let gh = dir.join("gh");
    write_script(&gh, r#"#!/bin/sh
d=$(dirname "$0")
echo "$*" >> "$d/gh-args"
case "$1 $2" in
  "pr list") echo "[]"; exit 0 ;;
esac
echo "unknown command: $*" >&2; exit 1
"#);
    gh
}

#[tokio::test]
async fn without_a_bitbucket_login_open_pull_request_says_to_log_in_and_the_pr_check_still_follows_github_cards() {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    let repo = "acme/shop-nologin";
    let c = worked_card(tmp.path(), repo, &format!("https://bitbucket.org/{repo}"), false).await;
    to_review(&c);
    let ssh_before = ssh_calls().len();

    let e = pulls::open(&c.st, &c.task).await.unwrap_err();
    assert!(e.contains("Log in to Bitbucket first (Settings → Bitbucket)"), "{e}");
    assert!(!has_branch(&c.bare, &c.branch), "nothing pushed");
    assert!(!ssh_calls()[ssh_before..].iter().any(|l| l.contains(repo)), "{:?}", ssh_calls());
    assert!(requests(repo).is_empty(), "{:?}", requests(repo));
    let t = task(&c);
    assert_eq!((t.pr_url, t.pr_state, t.state_name.as_str()), (None, None, "Review"));
    let e = pulls::check(&c.st, &c.task).await.unwrap_err();
    assert!(e.contains("Log in to Bitbucket first (Settings → Bitbucket)"), "{e}");

    // a GitHub project's card in Review, with a branch: the PR check follows it with gh, and skips the Bitbucket one
    let hub_dir = tmp.path().join("hub");
    std::fs::create_dir(&hub_dir).unwrap();
    git(&hub_dir, &["init", "-q", "-b", "main"]);
    let hub = gizai_core::projects::create(&c.st.db, &c.st.you_id, gizai_core::model::ProjectInput { name: "Hub".into(), key: "HUB".into(),
        repo_path: Some(hub_dir.to_string_lossy().into()), repo_url: Some("https://github.com/acme/hub".into()), default_branch: Some("main".into()),
        ..Default::default() }).unwrap();
    let gh_task = gizai_core::tasks::create(&c.st.db, &c.st.you_id, gizai_core::model::TaskInput { project_id: hub, title: "Ship the hub".into(),
        state_id: Some(state_id(&c.st, "review")), ..Default::default() }).unwrap();
    c.st.db.write(None, |w| { w.conn().execute("UPDATE tasks SET branch=?2 WHERE id=?1", [gh_task.as_str(), "gizai/hub-1-ship-the-hub"])?; Ok(()) }).unwrap();
    let gh_dir = tmp.path().join("gh");
    gizai_core::settings::set(&c.st.db, "gh_bin", &fake_gh(&gh_dir).to_string_lossy().to_string()).unwrap();

    let checked = pulls::check_all(&c.st).await;
    assert_eq!(checked.iter().map(|c| c.task_id.as_str()).collect::<Vec<_>>(), [gh_task.as_str()], "{checked:?}");
    assert_eq!((checked[0].changed, checked[0].pull.clone()), (false, None));
    let gh_args = std::fs::read_to_string(gh_dir.join("gh-args")).unwrap_or_default();
    assert!(gh_args.contains("pr list --repo acme/hub --head gizai/hub-1-ship-the-hub"), "{gh_args}");
    assert!(requests(repo).is_empty(), "{:?}", requests(repo));

    // with a login, the next round follows both
    save_login(&c.st);
    let checked = pulls::check_all(&c.st).await;
    let mut ids: Vec<&str> = checked.iter().map(|c| c.task_id.as_str()).collect();
    ids.sort();
    let mut want = vec![gh_task.as_str(), c.task.as_str()];
    want.sort();
    assert_eq!(ids, want, "{checked:?}");
    assert_eq!(requests(repo).len(), 1, "{:?}", requests(repo));
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
