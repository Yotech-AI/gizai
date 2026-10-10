//! GA-92 QA: an agent's own `git push` in a run goes to the project's repository the way Gizai's push after the run goes
//! (Settings → GitHub → Push over), through git settings in the run's environment only. End to end with fake CLIs and a
//! fake ssh (the run's own GIT_SSH_COMMAND, as yours would be) that serves "GitHub" and "Bitbucket" from local bare
//! repositories, so nothing here reaches github.com or bitbucket.org. The project's clone allows no https, and only its
//! main checkout reaches the bare repository through a rewrite of its own (`git config --worktree`): Gizai's fetch of
//! main and its push after the run go there, while the run's worktree sees origin as it is written.
// Linux and macOS only: these tests run shell scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use gizai_agents::stream::RunEvent;
use gizai_core::clis::Cli;
use gizai_core::model::{AgentInput, TaskPatch};

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const FAKE_CLI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-cli.sh");
const LINK: &str = "https://github.com/acme/shop";
const TOKEN: &str = "gho_FAKEtoken0123456789";

/// What the agent's git sees, appended to "$seen" by each run: the push and fetch address of origin and the run's
/// GIT_CONFIG_* environment.
const LOOK: &str = r#"{
  echo "push=$(git remote get-url --push origin)"
  echo "fetch=$(git remote get-url origin)"
  env | grep '^GIT_CONFIG_' | sort | sed 's/^/env: /'
} >> "$seen" 2>&1"#;

/// The agent commits and pushes its branch itself, the way QA does before `gh pr create`, and writes down whether the
/// remote-tracking branch gh looks at is now its HEAD.
const PUSH: &str = r#"git -c user.email=t@t -c user.name=t commit -q --allow-empty -m "Pushed by the agent" || exit 1
if out=$(git push -q origin HEAD 2>&1); then
  echo "pushed: tracking=$(git rev-parse "refs/remotes/origin/$(git branch --show-current)") head=$(git rev-parse HEAD)" >> "$seen"
else
  echo "push failed: $out" >> "$seen"
fi"#;

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

/// Writes an executable script through a child `sh`, so this process never holds it open for writing (a leaked
/// handle would make running it fail with "Text file busy").
fn write_script(path: &Path, script: &str) {
    use std::io::Write;
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path).stdin(Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

/// The fake ssh at tmp/bin/ssh: each call goes to tmp/ssh-args, and git@github.com:<path> and git@bitbucket.org:<path>
/// are served from tmp/hosted/github/<path> and tmp/hosted/bitbucket/<path> (with or without .git).
fn fake_ssh(tmp: &Path) -> PathBuf {
    let ssh = tmp.join("bin/ssh");
    write_script(&ssh, &r#"#!/bin/sh
base='BASE'
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
[ -n "$cmd" ] || { echo "authenticated via ssh key." >&2; exit 0; }
[ -n "$host" ] || { echo "ssh: Could not resolve hostname" >&2; exit 255; }
path="${cmd#* }"
path="$(printf '%s' "$path" | tr -d "'")"
path="${path#/}"
repo="$base/hosted/$host/$path"
[ -d "$repo" ] || repo="$repo.git"
verb="${cmd%% *}"
exec git "${verb#git-}" "$repo"
"#.replace("BASE", &tmp.display().to_string()));
    ssh
}

fn ssh_calls(tmp: &Path) -> Vec<String> {
    std::fs::read_to_string(tmp.join("ssh-args")).unwrap_or_default().lines().map(String::from).collect()
}

/// "GitHub" or "Bitbucket" (`host`): a bare repository at tmp/hosted/<host>/<repo>.git with one commit on main, and the
/// project's clone of it at tmp/local, whose origin is `origin` as written. Only the clone's main checkout (on main)
/// reaches the bare repository, through a rewrite of `origin` and of the project's link `link` in tmp/main-only.gitconfig
/// (`includeIf "onbranch:main"`; a card's worktree is on its own branch, and `git worktree add` would copy a
/// config.worktree). The clone allows no https at all.
fn hosted(tmp: &Path, host: &str, repo: &str, origin: &str, link: &str) -> (PathBuf, PathBuf) {
    let src = tmp.join("src");
    std::fs::create_dir(&src).unwrap();
    git(&src, &["init", "-q", "-b", "main"]);
    git(&src, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init"]);
    let bare = tmp.join(format!("hosted/{host}/{repo}.git"));
    git(tmp, &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
    let local = tmp.join("local");
    git(tmp, &["clone", "-q", bare.to_str().unwrap(), local.to_str().unwrap()]);
    git(&local, &["remote", "set-url", "origin", origin]);
    git(&local, &["config", "protocol.https.allow", "never"]);
    let main_only = tmp.join("main-only.gitconfig");
    for from in [origin, link] {
        git(tmp, &["config", "-f", main_only.to_str().unwrap(), "--add", &format!("url.{}.insteadOf", bare.display()), from]);
    }
    git(&local, &["config", "includeIf.onbranch:main.path", main_only.to_str().unwrap()]);
    (bare, local)
}

/// Card KADE-1 on the clone `local`, in To do and assigned to the Backend Agent; its project linked to `link`.
fn card(st: &gizai_lib::AppState, local: &Path, link: Option<&str>) -> String {
    let task = gizai_lib::test_task(st, local.to_str().unwrap(), "backend");
    if let Some(link) = link {
        let p = gizai_core::projects::list(&st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
        gizai_core::projects::update(&st.db, &st.you_id, &p.id, gizai_core::model::ProjectInput { name: p.name.clone(), key: p.key.clone(),
            repo_path: p.repo_path.clone(), repo_url: Some(link.into()), default_branch: Some("main".into()), ..Default::default() }).unwrap();
    }
    task
}

/// The agent: a script that runs `body` in the run's worktree with the fake ssh as its GIT_SSH_COMMAND and "$seen" at
/// tmp/seen, then goes on as `fake` with the same arguments and stdin.
fn agent(tmp: &Path, name: &str, body: &str, fake: &str) -> String {
    let path = tmp.join(name);
    write_script(&path, &format!("#!/bin/bash\nexport GIT_SSH_COMMAND='{ssh}' GIT_TERMINAL_PROMPT=0\nseen='{seen}'\n{body}\nexec bash '{fake}' \"$@\"\n",
                                 ssh = fake_ssh(tmp).display(), seen = tmp.join("seen").display()));
    path.to_string_lossy().to_string()
}

fn seen(tmp: &Path) -> String {
    std::fs::read_to_string(tmp.join("seen")).unwrap_or_default()
}

/// Every value of `key=` in what the agent saw, in order.
fn values(tmp: &Path, key: &str) -> Vec<String> {
    seen(tmp).lines().filter_map(|l| l.strip_prefix(&format!("{key}="))).map(String::from).collect()
}

/// The run's GIT_CONFIG_* environment lines (of its first look).
fn git_env(tmp: &Path) -> Vec<String> {
    seen(tmp).lines().filter_map(|l| l.strip_prefix("env: ")).map(String::from).collect()
}

/// GIT_CONFIG_* of this test's own environment, which a run without settings of its own passes on as it is.
fn own_git_env() -> Vec<String> {
    let mut v: Vec<String> = std::env::vars().filter(|(k, _)| k.starts_with("GIT_CONFIG_")).map(|(k, v)| format!("{k}={v}")).collect();
    v.sort();
    v
}

/// The agent's own push went: GitHub (or Bitbucket) has its branch at the run's HEAD, and the remote-tracking branch
/// that `gh pr create` looks at is that HEAD too.
fn assert_pushed_itself(tmp: &Path, bare: &Path, wt: &Path, branch: &str) {
    let s = seen(tmp);
    let line = s.lines().filter(|l| l.starts_with("pushed: ") || l.starts_with("push failed")).last().unwrap_or_else(|| panic!("no push in {s}"));
    let head = git(wt, &["rev-parse", "HEAD"]);
    assert_eq!(line, format!("pushed: tracking={head} head={head}"), "{s}");
    assert_eq!(git(bare, &["rev-parse", &format!("refs/heads/{branch}")]), head, "the branch is on the hosted repository");
}

/// A run's notes from Gizai, as Show output reads them back from its log.
fn notes(st: &gizai_lib::AppState, run_id: &str) -> Vec<String> {
    gizai_lib::runs::events_for(st, run_id).into_iter().filter_map(|e| match e.event { RunEvent::Note { text } => Some(text), _ => None }).collect()
}

fn run_of(st: &gizai_lib::AppState, run_id: &str) -> (gizai_core::model::Run, PathBuf, String) {
    let run = gizai_core::runs::get(&st.db, run_id).unwrap();
    let (wt, branch) = (PathBuf::from(run.worktree_path.clone().unwrap()), run.branch.clone().unwrap());
    (run, wt, branch)
}

fn stderr_of(run: &gizai_core::model::Run) -> String {
    std::fs::read_to_string(Path::new(&run.log_path).with_extension("stderr.log")).unwrap_or_default()
}

/// Adds a CLI in Settings → Coding CLIs and puts the Backend Agent on it.
fn backend_on_cli(st: &gizai_lib::AppState, name: &str, kind: &str, command: &str, env: &[&str]) {
    let mut list: Vec<Cli> = gizai_core::clis::list(&st.db).unwrap();
    list.push(Cli { name: name.into(), kind: kind.into(), command: command.into(), env: env.iter().map(|e| e.to_string()).collect(), ..Default::default() });
    let cli = gizai_lib::clis::save(st, list).unwrap().into_iter().find(|c| c.cli.name == name).unwrap().cli.id;
    let (_, agent) = gizai_core::team::all_agents(&st.db).unwrap().into_iter().find(|(_, m)| m.role_key == "backend").unwrap();
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, AgentInput { name: agent.name.clone(), role_key: "backend".into(),
        adapter: cli, ..Default::default() }).unwrap();
}

/// Makes In progress Manual, so the queue doesn't start a card that stayed there again by itself.
fn in_progress_manual(st: &gizai_lib::AppState) {
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let state = gizai_core::team::get(&st.db, &team_id).unwrap().states.into_iter().find(|s| s.category == "in_progress").unwrap().id;
    gizai_core::columns::set_column(&st.db, &st.you_id, &state, gizai_core::columns::ColumnInput { auto: Some(false), ..Default::default() }).unwrap();
}

/// A fake gh in tmp/gh, logged in as octocat: `auth git-credential get` answers with TOKEN. Each call's arguments go to
/// tmp/gh/gh-args.
fn fake_gh(tmp: &Path) -> PathBuf {
    let gh = tmp.join("gh/gh");
    write_script(&gh, &format!(r#"#!/bin/sh
d=$(dirname "$0")
echo "$*" >> "$d/gh-args"
case "$*" in
  --version) echo "gh version 2.62.0 (2024-11-14)"; exit 0 ;;
  "auth status"*) echo "github.com"; echo "  ✓ Logged in to github.com account octocat (keyring)"; exit 0 ;;
  "auth git-credential"*) cat > /dev/null; printf 'protocol=https\nhost=github.com\nusername=x-access-token\npassword={TOKEN}\n'; exit 0 ;;
esac
exit 1
"#));
    gh
}

/// The git config files of the clone (shared and the main checkout's own), to see that a run changed none of them.
fn config_files(local: &Path) -> (String, String, String) {
    (std::fs::read_to_string(local.join(".git/config")).unwrap(), std::fs::read_to_string(local.join("../main-only.gitconfig")).unwrap(),
     git(local, &["remote", "-v"]))
}

#[tokio::test]
async fn over_ssh_the_agents_own_push_reaches_github_and_nothing_else_changes_in_the_run() {
    let tmp = tempfile::tempdir().unwrap();
    let tmp = tmp.path();
    let st = gizai_lib::test_state(tmp);
    let (bare, local) = hosted(tmp, "github", "acme/shop", LINK, LINK);
    let task = card(&st, &local, Some(LINK));
    // a test's own "GitHub" in a folder, as GA's tests have it: https://github.com/acme/ goes to tmp/testhub/acme/
    let testhub = tmp.join("testhub/acme");
    git(tmp, &["clone", "-q", "--bare", bare.to_str().unwrap(), testhub.join("other").to_str().unwrap()]);
    let config = config_files(&local);

    let body = format!(r#"{LOOK}
echo "other-fetch=$(git ls-remote --get-url https://github.com/acme/other)" >> "$seen"
# another GitHub repository, as the remote of a repository of its own
git init -q "$TMPDIR/elsewhere" && git -C "$TMPDIR/elsewhere" remote add origin https://github.com/acme/other
echo "other-push=$(git -C "$TMPDIR/elsewhere" remote get-url --push origin)" >> "$seen"
# a test's local rewrite: its clone of https://github.com/acme/other pushes to the folder
git clone -q -c 'url.{testhub}/.insteadOf=https://github.com/acme/' -c protocol.allow=never -c protocol.file.allow=always \
  https://github.com/acme/other "$TMPDIR/test-clone" >> "$seen" 2>&1
echo "test-push=$(git -C "$TMPDIR/test-clone" remote get-url --push origin)" >> "$seen"
echo "test-fetch=$(git -C "$TMPDIR/test-clone" remote get-url origin)" >> "$seen"
git -C "$TMPDIR/test-clone" -c user.email=t@t -c user.name=t commit -q --allow-empty -m "From a test" \
  && git -C "$TMPDIR/test-clone" push -q origin HEAD:refs/heads/from-a-test >> "$seen" 2>&1 && echo "test-pushed=yes" >> "$seen"
{PUSH}"#, testhub = testhub.display());
    let s = gizai_lib::runs::run_once(&st, &task, None, Some(agent(tmp, "claude", &body, FAKE))).await.unwrap();
    assert_eq!((s.status.as_str(), s.outcome.as_deref()), ("succeeded", Some("ready_for_testing")), "{:?}: {}", s.error, seen(tmp));
    let (_, wt, branch) = run_of(&st, &s.run_id);

    // the project's own repository: pushed over SSH, fetched as written
    assert_eq!(values(tmp, "push"), ["git@github.com:acme/shop.git"], "{}", seen(tmp));
    assert_eq!(values(tmp, "fetch"), [LINK]);
    assert!(git_env(tmp).iter().any(|l| l.starts_with("GIT_CONFIG_COUNT=")), "{:?}", git_env(tmp));
    assert!(git_env(tmp).iter().all(|l| !l.contains("insteadOf") || l.contains("pushInsteadOf")), "push rules only: {:?}", git_env(tmp));
    assert_pushed_itself(tmp, &bare, &wt, &branch);
    assert!(ssh_calls(tmp).iter().any(|l| l.contains("git@github.com") && l.contains("git-receive-pack 'acme/shop.git'")), "{:?}", ssh_calls(tmp));

    // every other address stays: another GitHub repository, and a test's local rewrite, whose push lands in its folder
    assert_eq!(values(tmp, "other-fetch"), ["https://github.com/acme/other"]);
    assert_eq!(values(tmp, "other-push"), ["https://github.com/acme/other"]);
    let to = testhub.join("other").display().to_string();
    assert_eq!((values(tmp, "test-push"), values(tmp, "test-fetch")), (vec![to.clone()], vec![to]));
    assert_eq!(values(tmp, "test-pushed"), ["yes"], "{}", seen(tmp));
    assert_eq!(git(&testhub.join("other"), &["log", "-1", "--format=%s", "from-a-test"]), "From a test");
    assert!(ssh_calls(tmp).iter().all(|l| !l.contains("acme/other")), "nothing for acme/other went over ssh: {:?}", ssh_calls(tmp));

    // Gizai's push after the run finds nothing left to push, the card moves on, and no git config or remote changed
    assert!(notes(&st, &s.run_id).is_empty(), "{:?}", notes(&st, &s.run_id));
    let t = gizai_core::tasks::get(&st.db, &task).unwrap();
    assert_eq!((t.state_name.as_str(), t.hold), ("Testing", None));
    assert_eq!(config_files(&local), config);
    let wt_git = PathBuf::from(git(&wt, &["rev-parse", "--absolute-git-dir"]));
    assert!(!wt_git.join("config.worktree").exists() && !wt_git.join("config").exists(), "the run's worktree has no git config of its own");
    assert_eq!(git(&wt, &["remote", "get-url", "--push", "origin"]), LINK, "outside the run, origin pushes as it is written");
}

#[tokio::test]
async fn over_https_the_agents_git_gets_its_github_login_from_gh_and_from_no_other_helper() {
    let tmp = tempfile::tempdir().unwrap();
    let tmp = tmp.path();
    let st = gizai_lib::test_state(tmp);
    let (_bare, local) = hosted(tmp, "github", "acme/shop", "git@github.com:acme/shop.git", LINK);
    let task = card(&st, &local, Some(LINK));
    gizai_core::settings::set(&st.db, "github_push_over", &"https").unwrap();
    let gh = fake_gh(tmp);
    gizai_core::settings::set(&st.db, "gh_bin", &gh.to_string_lossy().to_string()).unwrap();
    // your own credential helper, in the repository's config
    let own = tmp.join("own-helper");
    write_script(&own, &format!("#!/bin/sh\necho \"$*\" >> '{}'\nprintf 'username=me\\npassword=own-secret\\n'\n", tmp.join("own-helper-ran").display()));
    git(&local, &["config", "credential.helper", &format!("!{}", own.display())]);
    let config = config_files(&local);

    let body = format!(r#"{LOOK}
printf 'protocol=https\nhost=github.com\npath=acme/shop.git\n\n' | git credential fill 2>&1 | sed 's/^/github: /' >> "$seen"
printf 'protocol=https\nhost=gitlab.com\n\n' | git credential fill 2>&1 | sed 's/^/gitlab: /' >> "$seen""#);
    let s = gizai_lib::runs::run_once(&st, &task, None, Some(agent(tmp, "claude", &body, FAKE))).await.unwrap();
    assert_eq!((s.status.as_str(), s.outcome.as_deref()), ("succeeded", Some("ready_for_testing")), "{:?}: {}", s.error, seen(tmp));
    // an ssh origin of the project pushes over https, and fetches as written
    assert_eq!(values(tmp, "push"), ["https://github.com/acme/shop.git"], "{}", seen(tmp));
    assert_eq!(values(tmp, "fetch"), ["git@github.com:acme/shop.git"]);
    // github.com's login comes from gh, and your own helper isn't asked for it
    let s_ = seen(tmp);
    assert!(s_.contains(&format!("github: password={TOKEN}")), "{s_}");
    let calls = std::fs::read_to_string(tmp.join("gh/gh-args")).unwrap_or_default();
    assert!(calls.lines().any(|l| l == "auth git-credential get"), "{calls}");
    let own_ran = std::fs::read_to_string(tmp.join("own-helper-ran")).unwrap_or_default();
    assert_eq!(own_ran.lines().count(), 1, "your own helper ran once, for gitlab.com only: {own_ran}");
    assert!(s_.contains("gitlab: password=own-secret"), "another host keeps your own helper: {s_}");
    assert!(git_env(tmp).iter().any(|l| l.contains("credential.https://github.com.helper")), "{:?}", git_env(tmp));
    assert!(notes(&st, &s.run_id).is_empty(), "{:?}", notes(&st, &s.run_id));
    assert_eq!(config_files(&local), config);
}

#[tokio::test]
async fn to_bitbucket_the_agents_own_push_goes_over_ssh_to_git_at_bitbucket_org() {
    let tmp = tempfile::tempdir().unwrap();
    let tmp = tmp.path();
    let st = gizai_lib::test_state(tmp);
    // origin as Bitbucket's Clone button gives it, with a user name in it
    let origin = "https://jefsev@bitbucket.org/acme/site.git";
    let (bare, local) = hosted(tmp, "bitbucket", "acme/site", origin, "https://bitbucket.org/acme/site");
    let task = card(&st, &local, Some("https://bitbucket.org/acme/site"));
    gizai_core::settings::set(&st.db, "gh_bin", &"/nonexistent/gh".to_string()).unwrap();
    let config = config_files(&local);

    let body = format!("{LOOK}\n{PUSH}");
    let s = gizai_lib::runs::run_once(&st, &task, None, Some(agent(tmp, "claude", &body, FAKE))).await.unwrap();
    assert_eq!((s.status.as_str(), s.outcome.as_deref()), ("succeeded", Some("ready_for_testing")), "{:?}: {}", s.error, seen(tmp));
    let (_, wt, branch) = run_of(&st, &s.run_id);
    assert_eq!(values(tmp, "push"), ["git@bitbucket.org:acme/site.git"], "{}", seen(tmp));
    assert_eq!(values(tmp, "fetch"), [origin]);
    assert_pushed_itself(tmp, &bare, &wt, &branch);
    assert!(ssh_calls(tmp).iter().any(|l| l.contains("git@bitbucket.org") && l.contains("git-receive-pack 'acme/site.git'")), "{:?}", ssh_calls(tmp));
    assert!(git_env(tmp).iter().all(|l| !l.contains("credential.")), "no credential helper for Bitbucket: {:?}", git_env(tmp));
    assert!(notes(&st, &s.run_id).is_empty(), "{:?}", notes(&st, &s.run_id));
    assert_eq!(config_files(&local), config);
}

#[tokio::test]
async fn codex_gemini_and_other_clis_push_the_same_way() {
    for kind in ["codex", "gemini", "other"] {
        let tmp = tempfile::tempdir().unwrap();
        let tmp = tmp.path();
        let st = gizai_lib::test_state(tmp);
        let (bare, local) = hosted(tmp, "github", "acme/shop", LINK, LINK);
        let task = card(&st, &local, Some(LINK));
        let cli = agent(tmp, kind, &format!("{LOOK}\n{PUSH}"), FAKE_CLI);
        backend_on_cli(&st, kind, kind, &cli, &[&format!("FAKE_KIND={kind}")]);
        let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
        assert_eq!((s.status.as_str(), s.outcome.as_deref()), ("succeeded", Some("ready_for_testing")), "{kind}: {:?}: {}", s.error, seen(tmp));
        let (run, wt, branch) = run_of(&st, &s.run_id);
        assert_eq!(values(tmp, "push"), ["git@github.com:acme/shop.git"], "{kind}: {}", seen(tmp));
        assert_eq!(values(tmp, "fetch"), [LINK], "{kind}");
        assert_pushed_itself(tmp, &bare, &wt, &branch);
        if kind == "codex" {
            // also in Codex's shell environment policy, for the commands it runs
            let err = stderr_of(&run);
            for want in ["-c shell_environment_policy.set.GIT_CONFIG_COUNT=", "-c shell_environment_policy.set.GIT_CONFIG_KEY_0=\"url.git@github.com:acme/shop.git.pushInsteadOf\""] {
                assert!(err.contains(want), "{want} missing in {err}");
            }
        }
        assert!(notes(&st, &s.run_id).is_empty(), "{kind}: {:?}", notes(&st, &s.run_id));
    }
}

#[tokio::test]
async fn gizais_nudge_and_a_continued_run_get_the_same_settings() {
    let tmp = tempfile::tempdir().unwrap();
    let tmp = tmp.path();
    let st = gizai_lib::test_state(tmp);
    let (bare, local) = hosted(tmp, "github", "acme/shop", LINK, LINK);
    let task = card(&st, &local, Some(LINK));
    in_progress_manual(&st);
    // every run ends without its result line, so Gizai nudges the first; each run pushes a commit of its own
    let cli = agent(tmp, "claude-waits", &format!("{LOOK}\n{PUSH}"), FAKE);
    backend_on_cli(&st, "Claude Code (waits)", "claude_code", &cli, &["FAKE_NO_RESULT=1"]);
    gizai_core::tasks::update(&st.db, &st.you_id, &task, TaskPatch { description_md: Some("Push it.".into()), ..Default::default() }).unwrap();
    gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    let t0 = Instant::now();
    let runs = loop {
        let mut runs = gizai_core::runs::list_for_task(&st.db, &task).unwrap();
        runs.sort_by_key(|r| r.created_at);
        if runs.len() >= 2 && runs.iter().all(|r| r.ended_at.is_some()) && gizai_lib::runs::live(&st).iter().all(|l| l.task_id != task) {
            break runs;
        }
        assert!(t0.elapsed() < Duration::from_secs(30), "the nudge: {:?}", runs.iter().map(|r| (&r.trigger, &r.status)).collect::<Vec<_>>());
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(runs[1].trigger, "result_nudge", "{:?}", runs.iter().map(|r| &r.trigger).collect::<Vec<_>>());
    // then Continue on the nudged run, by hand
    let (id, done) = gizai_lib::runs::continue_run(&st, &runs[1].id, None).await.unwrap();
    done.await.unwrap();
    let (cont, wt, branch) = run_of(&st, &id);
    assert!(stderr_of(&cont).contains("--resume"), "a continued session: {}", stderr_of(&cont));

    assert_eq!(values(tmp, "push"), ["git@github.com:acme/shop.git"; 3], "new, nudged and continued: {}", seen(tmp));
    assert_eq!(values(tmp, "fetch"), [LINK; 3]);
    let s = seen(tmp);
    let pushed: Vec<&str> = s.lines().filter(|l| l.starts_with("pushed: ") || l.starts_with("push failed")).map(|l| &l[..6]).collect();
    assert_eq!(pushed, ["pushed"; 3], "{s}");
    assert_pushed_itself(tmp, &bare, &wt, &branch);
}

#[tokio::test]
async fn a_project_without_a_link_or_with_another_git_url_gets_no_settings() {
    for link in [None, Some("ssh://git.example.com/acme/shop.git")] {
        let tmp = tempfile::tempdir().unwrap();
        let tmp = tmp.path();
        let st = gizai_lib::test_state(tmp);
        let origin = link.unwrap_or(LINK);
        let (_bare, local) = hosted(tmp, "github", "acme/shop", origin, origin);
        let task = card(&st, &local, link);
        let s = gizai_lib::runs::run_once(&st, &task, None, Some(agent(tmp, "claude", LOOK, FAKE))).await.unwrap();
        assert_eq!((s.status.as_str(), s.outcome.as_deref()), ("succeeded", Some("ready_for_testing")), "{link:?}: {:?}: {}", s.error, seen(tmp));
        assert_eq!(values(tmp, "push"), [origin], "{link:?}: as written: {}", seen(tmp));
        assert_eq!(git_env(tmp), own_git_env(), "{link:?}: nothing added to the run's environment");
        assert!(notes(&st, &s.run_id).is_empty(), "{link:?}: {:?}", notes(&st, &s.run_id));
    }
}

#[tokio::test]
async fn over_https_without_gh_the_run_starts_without_the_settings_and_its_log_says_why() {
    let tmp = tempfile::tempdir().unwrap();
    let tmp = tmp.path();
    let st = gizai_lib::test_state(tmp);
    let (_bare, local) = hosted(tmp, "github", "acme/shop", LINK, LINK);
    let task = card(&st, &local, Some(LINK));
    gizai_core::settings::set(&st.db, "github_push_over", &"https").unwrap();
    let missing = tmp.join("no-gh-here");
    gizai_core::settings::set(&st.db, "gh_bin", &missing.to_string_lossy().to_string()).unwrap();
    let s = gizai_lib::runs::run_once(&st, &task, None, Some(agent(tmp, "claude", LOOK, FAKE))).await.unwrap();
    assert_eq!((s.status.as_str(), s.outcome.as_deref()), ("succeeded", Some("ready_for_testing")), "{:?}: {}", s.error, seen(tmp));
    assert_eq!(values(tmp, "push"), [LINK], "{}", seen(tmp));
    assert_eq!(git_env(tmp), own_git_env(), "nothing added to the run's environment");
    let said = notes(&st, &s.run_id);
    assert!(said.iter().any(|n| n.contains("Couldn't set up this run's own git push over HTTPS")
        && n.contains(&format!("The GitHub CLI isn't at {} any more", missing.display()))), "{said:?}");
    let log = std::fs::read_to_string(&gizai_core::runs::get(&st.db, &s.run_id).unwrap().log_path).unwrap();
    assert!(log.lines().any(|l| l.contains("\"gizai_note\"") && l.contains("Couldn't set up this run's own git push")), "in the run's log: {log}");
}
