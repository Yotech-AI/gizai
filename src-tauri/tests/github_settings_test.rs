//! Settings → GitHub and Open pull request over SSH or HTTPS, end to end without GitHub: a fake ssh as your own
//! GIT_SSH_COMMAND (it logs its arguments and serves "GitHub" from local bare repositories), a fake gh, and a local
//! server that refuses every HTTPS login. Every test in this file runs with that fake ssh.
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use gizai_lib::github;
use gizai_lib::pulls::{self, PullInfo};

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const TOKEN: &str = "gho_FAKEtoken0123456789";

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

fn has_branch(repo: &Path, branch: &str) -> bool {
    Command::new("git").args(["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).current_dir(repo).status().unwrap().success()
}

/// Writes an executable script through a child `sh`, so this process never holds it open for writing (a leaked
/// handle would make running it fail with "Text file busy").
fn write_script(path: &Path, script: &str) {
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path).stdin(Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

/// The fake ssh's folder: base/ssh-args (one line per call) and "GitHub" at base/github/<owner>/<name>.git (found with
/// or without .git, as GitHub does). A file fake-ssh-mode in a bare repository with "publickey" makes ssh fail for it.
/// `ssh -T git@github.com` answers the way GitHub does for octocat's key.
fn fake_ssh() -> &'static Path {
    static BASE: OnceLock<PathBuf> = OnceLock::new();
    BASE.get_or_init(|| {
        let base = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap().keep();
        let ssh = base.join("bin/ssh");
        std::fs::create_dir_all(ssh.parent().unwrap()).unwrap();
        write_script(&ssh, &format!(r#"#!/bin/sh
base='{base}'
[ "$1" = "-G" ] && exit 1
echo "$*" >> "$base/ssh-args"
cmd=""
for a in "$@"; do case "$a" in git-receive-pack*|git-upload-pack*) cmd="$a" ;; esac; done
if [ -z "$cmd" ]; then
  echo "Hi octocat! You've successfully authenticated, but GitHub does not provide shell access." >&2; exit 1
fi
repo="$base/github/$(printf '%s' "${{cmd#* }}" | tr -d "'" | sed 's|^/||')"
[ -d "$repo" ] || repo="$repo.git"
case "$(cat "$repo/fake-ssh-mode" 2>/dev/null)" in
  publickey) echo "git@github.com: Permission denied (publickey)." >&2; exit 255 ;;
esac
verb="${{cmd%% *}}"
exec git "${{verb#git-}}" "$repo"
"#, base = base.display()));
        // SAFETY: set once, before this test binary starts any process (every test calls fake_ssh first)
        unsafe { std::env::set_var("GIT_SSH_COMMAND", &ssh) };
        base
    })
}

fn ssh_calls() -> Vec<String> {
    std::fs::read_to_string(fake_ssh().join("ssh-args")).unwrap_or_default().lines().map(String::from).collect()
}

/// A fake gh in `dir`: each call's arguments go to dir/gh-args. Logged in when dir/logged-in holds an account name.
/// `auth login` shows a one-time code and a link, then waits until dir/code-entered holds the account to log in as.
/// `auth git-credential get` answers with a token when logged in. `pr list` answers [], `pr create` with
/// dir/create.out.
fn fake_gh(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let gh = dir.join("gh");
    write_script(&gh, &format!(r#"#!/bin/sh
d=$(dirname "$0")
echo "$*" >> "$d/gh-args"
case "$*" in
  --version) echo "gh version 2.62.0 (2024-11-14)"; exit 0 ;;
  "auth status"*)
    if [ -f "$d/logged-in" ]; then
      echo "github.com"
      echo "  ✓ Logged in to github.com account $(cat "$d/logged-in") (keyring)"
      echo "  - Token: gho_************************************"
      exit 0
    fi
    echo "You are not logged into any GitHub hosts. To log in, run: gh auth login" >&2; exit 1 ;;
  "auth login"*)
    echo "! First copy your one-time code: 4F2A-9C1B" >&2
    echo "Open this URL to continue in your web browser: https://github.com/login/device" >&2
    while [ ! -f "$d/code-entered" ]; do sleep 0.1; done
    cp "$d/code-entered" "$d/logged-in"
    echo "✓ Authentication complete." >&2
    echo "✓ Logged in as $(cat "$d/logged-in")" >&2
    exit 0 ;;
  "auth git-credential"*)
    cat > /dev/null
    [ -f "$d/logged-in" ] || exit 1
    printf 'protocol=http\nhost=github.com\nusername=x-access-token\npassword={TOKEN}\n'; exit 0 ;;
  "pr list"*) echo "[]"; exit 0 ;;
  "pr create"*) cat > /dev/null; cat "$d/create.out"; exit 0 ;;
esac
echo "unknown command: $*" >&2; exit 1
"#));
    gh
}

fn gh_calls(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("gh-args")).unwrap_or_default().lines().map(String::from).collect()
}

/// "GitHub" at git@github.com:<repo> behind the fake ssh, and the project's local clone of https://github.com/<repo>
/// (its remote origin). While the clone's own config sends that address to the bare repository too (a card's run
/// fetches main from it), the clone allows no https at all, so nothing can reach github.com.
fn github(tmp: &Path, repo: &str) -> (PathBuf, PathBuf) {
    let base = fake_ssh();
    let src = tmp.join("src");
    std::fs::create_dir(&src).unwrap();
    git(&src, &["init", "-q", "-b", "main"]);
    git(&src, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init"]);
    let bare = base.join(format!("github/{repo}.git"));
    git(tmp, &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
    let local = tmp.join("local");
    git(tmp, &["clone", "-q", bare.to_str().unwrap(), local.to_str().unwrap()]);
    git(&local, &["remote", "set-url", "origin", &format!("https://github.com/{repo}")]);
    git(&local, &["config", "protocol.https.allow", "never"]);
    git(&local, &["config", &format!("url.{}.insteadOf", bare.display()), &format!("https://github.com/{repo}")]);
    (bare, local)
}

/// Takes the clone's own rewrite away: from now on only Gizai decides where https://github.com/<repo> goes.
fn unlink_local(local: &Path, bare: &Path) {
    git(local, &["config", "--unset", &format!("url.{}.insteadOf", bare.display())]);
}

fn link_project(st: &gizai_lib::AppState, link: &str) {
    let p = gizai_core::projects::list(&st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    gizai_core::projects::update(&st.db, &st.you_id, &p.id, gizai_core::model::ProjectInput { name: p.name.clone(), key: p.key.clone(),
        repo_path: p.repo_path.clone(), repo_url: Some(link.into()), default_branch: Some("main".into()), ..Default::default() }).unwrap();
}

struct Card { st: gizai_lib::AppState, task: String, bare: PathBuf, local: PathBuf, wt: PathBuf, branch: String, gh: PathBuf }

/// Card KADE-1 of project Kade, linked to https://github.com/<repo>, after an agent's run (its worktree and branch,
/// with one commit), in Review. gh is the fake.
async fn review_card(tmp: &Path, repo: &str) -> Card {
    let st = gizai_lib::test_state(tmp);
    let (bare, local) = github(tmp, repo);
    let task = gizai_lib::test_task(&st, local.to_str().unwrap(), "backend");
    link_project(&st, &format!("https://github.com/{repo}"));
    let gh = tmp.join("gh");
    gizai_core::settings::set(&st.db, "gh_bin", &fake_gh(&gh).to_string_lossy().to_string()).unwrap();
    gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    let run = gizai_core::runs::list_for_task(&st.db, &task).unwrap().remove(0);
    let (wt, branch) = (PathBuf::from(run.worktree_path.unwrap()), run.branch.unwrap());
    std::fs::write(wt.join("invoices.csv"), "id\n").unwrap();
    git(&wt, &["add", "invoices.csv"]);
    git(&wt, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "invoices.csv"]);
    let team = gizai_core::team::get(&st.db, &gizai_core::team::list(&st.db).unwrap()[0].id).unwrap();
    let review = team.states.iter().find(|s| s.category == "review").unwrap().id.clone();
    gizai_core::tasks::move_to(&st.db, &st.you_id, &task, &review, "").unwrap();
    Card { st, task, bare, local, wt, branch, gh }
}

fn task(c: &Card) -> gizai_core::model::Task { gizai_core::tasks::get(&c.st.db, &c.task).unwrap() }

#[tokio::test]
async fn open_pull_request_pushes_over_ssh_to_git_at_github_com_and_the_branch_arrives() {
    let tmp = tempfile::tempdir().unwrap();
    let c = review_card(tmp.path(), "owner/name").await;
    unlink_local(&c.local, &c.bare);
    let config = std::fs::read_to_string(c.local.join(".git/config")).unwrap();
    assert_eq!(gizai_lib::runs::get_settings(&c.st).push_over, "ssh", "SSH is the default");
    std::fs::write(c.gh.join("create.out"), "https://github.com/owner/name/pull/7\n").unwrap();

    let pr = pulls::open(&c.st, &c.task).await.unwrap();
    assert_eq!(pr, PullInfo { url: "https://github.com/owner/name/pull/7".into(), number: Some(7), state: "open".into(), note: None });
    assert!(ssh_calls().iter().any(|l| l.contains("git@github.com") && l.contains("git-receive-pack 'owner/name'")), "{:?}", ssh_calls());
    assert_eq!(git(&c.bare, &["rev-parse", &c.branch]), git(&c.wt, &["rev-parse", "HEAD"]), "the branch is in the bare repository behind git@github.com:owner/name");
    // the remote keeps its https URL, and the repository's config is as it was
    assert_eq!(git(&c.local, &["remote", "get-url", "origin"]), "https://github.com/owner/name");
    assert_eq!(std::fs::read_to_string(c.local.join(".git/config")).unwrap(), config);
    assert_eq!(task(&c).pr_url.as_deref(), Some("https://github.com/owner/name/pull/7"));
}

#[tokio::test]
async fn a_push_ssh_refuses_says_what_to_check_and_records_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let c = review_card(tmp.path(), "owner/locked").await;
    unlink_local(&c.local, &c.bare);
    std::fs::write(c.bare.join("fake-ssh-mode"), "publickey").unwrap();
    let before = gizai_core::tasks::activity(&c.st.db, &c.task).unwrap().len();
    let e = pulls::open(&c.st, &c.task).await.unwrap_err();
    assert!(e.contains(&format!("Couldn't push {} to origin: GitHub didn't accept your SSH key", c.branch)), "{e}");
    assert!(e.contains("github.com/settings/keys") && e.contains("ssh-add"), "{e}");
    assert!(!has_branch(&c.bare, &c.branch));
    let t = task(&c);
    assert_eq!((t.pr_url, t.pr_state, t.state_name.as_str()), (None, None, "Review"));
    assert_eq!(gizai_core::tasks::activity(&c.st.db, &c.task).unwrap().len(), before, "nothing recorded");
    assert!(!gh_calls(&c.gh).iter().any(|l| l.starts_with("pr ")), "no pull request asked for");
}

#[tokio::test]
async fn settings_says_gh_is_not_logged_in_then_a_login_shows_the_account_and_check_connection_passes() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let (bare, local) = github(tmp.path(), "owner/check");
    unlink_local(&local, &bare);
    gizai_lib::test_task(&st, local.to_str().unwrap(), "backend");
    link_project(&st, "https://github.com/owner/check");
    let dir = tmp.path().join("gh");
    let gh = fake_gh(&dir);
    gizai_core::settings::set(&st.db, "gh_bin", &gh.to_string_lossy().to_string()).unwrap();

    // not logged in: Settings says so and how to log in
    let s = github::status(&st).await;
    assert_eq!((s.gh_path.as_deref(), s.gh_version.as_deref(), s.gh_problem.as_ref()), (Some(gh.to_str().unwrap()), Some("2.62.0"), None));
    assert_eq!(s.account, None);
    let p = s.account_problem.unwrap();
    assert_eq!(p.what, "The GitHub CLI isn't logged in");
    assert!(p.fix.unwrap().contains("Log in with GitHub"));
    assert_eq!(s.login_command, Some(format!("{} auth login --hostname github.com --web", gh.display())));
    assert_eq!((s.push_over.as_str(), s.login), ("ssh", None));
    let checked = github::check(&st).await;
    assert!(!checked.ok);
    let account = checked.checks.iter().find(|c| c.name == "Account").unwrap();
    assert_eq!((account.result.as_str(), account.text.as_str()), ("failed", "The GitHub CLI isn't logged in"));
    assert!(account.fix.is_some());

    // Log in with GitHub: gh's code and link show in the app
    let code = github::login(&st).await.unwrap();
    assert_eq!((code.code.as_str(), code.url.as_str()), ("4F2A-9C1B", "https://github.com/login/device"));
    assert_eq!(github::status(&st).await.login, Some(code.clone()), "the code stays on screen while gh waits");
    assert_eq!(github::login(&st).await.unwrap(), code, "a second click shows the same login");
    assert_eq!(gh_calls(&dir).iter().filter(|l| l.starts_with("auth login")).count(), 1, "one gh login at a time");
    std::fs::write(dir.join("code-entered"), "octocat").unwrap(); // you enter the code on GitHub
    assert_eq!(github::login_wait(&st).await, Some(Ok(Some("octocat".into()))));

    // the status shows the account, and Check connection passes
    let s = github::status(&st).await;
    assert_eq!((s.account.as_deref(), s.account_problem, s.login), (Some("octocat"), None, None));
    let checked = github::check(&st).await;
    let lines: Vec<(&str, &str, &str)> = checked.checks.iter().map(|c| (c.name.as_str(), c.result.as_str(), c.text.as_str())).collect();
    assert_eq!(lines, [
        ("GitHub CLI", "ok", format!("gh 2.62.0 at {}", gh.display()).as_str()),
        ("Account", "ok", "Logged in to GitHub as octocat"),
        ("SSH", "ok", "git@github.com accepts your SSH key, as octocat"),
        ("Kade", "ok", "You can push to owner/check"),
    ]);
    assert!(checked.ok, "{checked:?}");
    assert_eq!(checked.checks[3].repo.as_deref(), Some("owner/check"));
    assert!(checked.checks[3].project_id.is_some());
    assert!(ssh_calls().iter().any(|l| l.contains("git-receive-pack 'owner/check'")), "the project's check went over ssh");
}

#[tokio::test]
async fn a_login_can_be_cancelled_from_the_app() {
    fake_ssh();
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let gh = fake_gh(&tmp.path().join("gh"));
    gizai_core::settings::set(&st.db, "gh_bin", &gh.to_string_lossy().to_string()).unwrap();
    github::login(&st).await.unwrap();
    github::login_cancel(&st);
    let ended = github::login_wait(&st).await.unwrap().unwrap_err();
    assert_eq!(ended.what, "Login cancelled");
    let s = github::status(&st).await;
    assert_eq!((s.login, s.account), (None, None));
    // without gh: the error says where to get it
    gizai_core::settings::set(&st.db, "gh_bin", &"/nonexistent/gh".to_string()).unwrap();
    let e = github::login(&st).await.unwrap_err();
    assert!(e.contains("cli.github.com"), "{e}");
    let s = github::status(&st).await;
    let p = s.gh_problem.unwrap();
    assert_eq!(p.what, "Not found at /nonexistent/gh");
    assert!(p.fix.unwrap().contains("cli.github.com"));
}

#[tokio::test]
async fn push_over_is_a_setting_ssh_by_default_or_https() {
    fake_ssh();
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let gh = fake_gh(&tmp.path().join("gh"));
    gizai_core::settings::set(&st.db, "gh_bin", &gh.to_string_lossy().to_string()).unwrap();
    let mut s = gizai_lib::runs::get_settings(&st);
    assert_eq!(s.push_over, "ssh");
    s.push_over = "https".into();
    gizai_lib::runs::save_settings(&st, &s).unwrap();
    assert_eq!(gizai_lib::runs::get_settings(&st).push_over, "https");
    assert_eq!(github::status(&st).await.push_over, "https");
    // over HTTPS, Check connection doesn't need ssh
    let checked = github::check(&st).await;
    assert_eq!(checked.push_over, "https");
    let ssh = checked.checks.iter().find(|c| c.name == "SSH").unwrap();
    assert_eq!((ssh.result.as_str(), ssh.text.as_str()), ("skipped", "Not used: pushes go over HTTPS with gh's login"));
    // only ssh or https
    s.push_over = "ftp".into();
    assert!(gizai_lib::runs::save_settings(&st, &s).is_err());
    assert_eq!(gizai_lib::runs::get_settings(&st).push_over, "https");
}

/// A local "GitHub" over plain HTTP that answers every request with 401 (a login it refuses).
fn refusing_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let (mut head, mut byte) = (Vec::new(), [0u8; 1]);
            while !head.ends_with(b"\r\n\r\n") && s.read(&mut byte).unwrap_or(0) == 1 {
                head.push(byte[0]);
            }
            let _ = s.write_all(b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"GitHub\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
    });
    url
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

#[tokio::test]
async fn over_https_open_pull_request_uses_ghs_login_and_no_token_is_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let c = review_card(tmp.path(), "owner/https").await;
    unlink_local(&c.local, &c.bare);
    // here https://github.com/owner/ is a local server that refuses every login
    git(&c.local, &["config", "--unset", "protocol.https.allow"]);
    git(&c.local, &["config", &format!("url.{}owner/.insteadOf", refusing_server()), "https://github.com/owner/"]);
    let mut s = gizai_lib::runs::get_settings(&c.st);
    s.push_over = "https".into();
    gizai_lib::runs::save_settings(&c.st, &s).unwrap();
    std::fs::write(c.gh.join("logged-in"), "octocat").unwrap();
    let ssh_before = ssh_calls().len();

    let e = pulls::open(&c.st, &c.task).await.unwrap_err();
    assert!(e.contains("GitHub refused gh's login for HTTPS"), "{e}");
    assert!(!e.contains(TOKEN), "{e}");
    assert!(gh_calls(&c.gh).iter().any(|l| l == "auth git-credential get"), "{:?}", gh_calls(&c.gh));
    assert!(!ssh_calls()[ssh_before..].iter().any(|l| l.contains("owner/https")), "no ssh over HTTPS");
    assert_eq!(task(&c).pr_url, None, "nothing recorded");
    // the login is gh's: no token in Gizai's database, settings or logs
    assert_eq!(files_with(&c.st.data_dir, TOKEN.as_bytes()), Vec::<PathBuf>::new());
}
