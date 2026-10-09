//! GA-59 QA: a Bitbucket project's new cards start from its main branch just fetched through the checkout's remote for
//! the project's link, also when that remote uses another form of the link (git@bitbucket.org:…, ssh://git@…, an
//! `upstream` beside a fork as origin, https with a user name), and the run prompt says it was just fetched from
//! Bitbucket (from GitHub for a GitHub project, from "the project's repository" for another git URL). "Bitbucket" and
//! "GitHub" are local bare repositories behind a fake ssh as your own GIT_SSH_COMMAND; the clones allow no https, so
//! nothing reaches bitbucket.org or github.com. Runs use the fake Claude Code (FAKE_TEMP=1 makes it print its prompt).
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use gizai_core::clis::Cli;
use gizai_core::model::{AgentInput, ProjectInput};
use gizai_lib::git::{START_FETCH_LIMIT, remote_for, start_point};

const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

/// Writes an executable script through a child `sh`, so this process never holds it open for writing (a leaked
/// handle would make running it fail with "Text file busy").
fn write_script(path: &Path, script: &str) {
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path).stdin(Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

/// The fake ssh, set up once for this test binary (every test calls this first, before it starts any process):
/// base/ssh-args (one line per call); "Bitbucket" at base/bitbucket/<workspace>/<repo>.git and "GitHub" at
/// base/github/<owner>/<name>.git (found with or without .git). Bitbucket's API points at a closed local port, so
/// nothing here could ever ask api.bitbucket.org.
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
host=""
for a in "$@"; do
  case "$a" in
    git-receive-pack*|git-upload-pack*) cmd="$a" ;;
    *@bitbucket.org) host=bitbucket ;;
    *@github.com) host=github ;;
  esac
done
if [ -z "$cmd" ]; then
  echo "authenticated via ssh key." >&2; exit 0
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
        // SAFETY: set once, before this test binary starts any process (every test calls fake_ssh first)
        unsafe {
            std::env::set_var("GIT_SSH_COMMAND", &ssh);
            std::env::set_var("GIZAI_BITBUCKET_API", "http://127.0.0.1:9");
        }
        base
    })
}

fn ssh_calls() -> Vec<String> {
    std::fs::read_to_string(fake_ssh().join("ssh-args")).unwrap_or_default().lines().map(String::from).collect()
}

/// "Bitbucket" (or "GitHub", `host`) at base/<host>/<repo>.git, fed from `src` (branch `branch`), and the project's local
/// clone of it, which allows no https. Its remotes are set by the caller.
struct Hosted {
    src: PathBuf,
    bare: PathBuf,
    local: PathBuf,
    branch: String,
}

impl Hosted {
    fn new(tmp: &Path, host: &str, repo: &str, branch: &str) -> Hosted {
        let base = fake_ssh();
        let src = tmp.join("src");
        std::fs::create_dir(&src).unwrap();
        git(&src, &["init", "-q", "-b", branch]);
        git(&src, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init"]);
        let bare = base.join(format!("{host}/{repo}.git"));
        git(tmp, &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
        let local = tmp.join("local");
        git(tmp, &["clone", "-q", bare.to_str().unwrap(), local.to_str().unwrap()]);
        git(&local, &["config", "protocol.https.allow", "never"]);
        Hosted { src, bare, local, branch: branch.into() }
    }

    /// A new commit on the hosted main branch, which the local clone hasn't fetched.
    fn push(&self, msg: &str) -> String {
        git(&self.src, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", msg]);
        git(&self.src, &["push", "-q", self.bare.to_str().unwrap(), &format!("HEAD:{}", self.branch)]);
        git(&self.bare, &["rev-parse", &self.branch])
    }
}

/// Project Kade (with a card, KADE-1) on `local`, linked to `link` with main branch `branch`. gh points nowhere, so
/// nothing here could ever ask github.com.
fn project(st: &gizai_lib::AppState, local: &Path, link: &str, branch: &str) -> (String, gizai_core::model::Project) {
    let task = gizai_lib::test_task(st, local.to_str().unwrap(), "backend");
    let p = gizai_core::projects::list(&st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    gizai_core::projects::update(&st.db, &st.you_id, &p.id, ProjectInput { name: p.name.clone(), key: p.key.clone(),
        repo_path: p.repo_path.clone(), repo_url: Some(link.into()), default_branch: Some(branch.into()), ..Default::default() }).unwrap();
    gizai_core::settings::set(&st.db, "gh_bin", &"/nonexistent/gh".to_string()).unwrap();
    (task, gizai_core::projects::get(&st.db, &p.id).unwrap())
}

#[tokio::test]
async fn a_bitbucket_project_starts_from_main_fetched_through_an_origin_written_as_git_at_bitbucket_org() {
    fake_ssh();
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let h = Hosted::new(tmp.path(), "bitbucket", "acme/site", "main");
    git(&h.local, &["remote", "set-url", "origin", "git@bitbucket.org:acme/site.git"]);
    let (_, p) = project(&st, &h.local, "https://bitbucket.org/acme/site", "main");
    assert_eq!(p.repo_url.as_deref(), Some("https://bitbucket.org/acme/site"));
    let old = git(&h.local, &["rev-parse", "origin/main"]);
    let new = h.push("on bitbucket");
    assert_ne!(old, new);

    let start = start_point(&p, &h.local, Some(START_FETCH_LIMIT)).unwrap();
    assert_eq!(start, "refs/remotes/origin/main", "through the checkout's own remote");
    assert_eq!(git(&h.local, &["rev-parse", &start]), new, "Bitbucket's latest main");
    assert!(ssh_calls().iter().any(|l| l.contains("git@bitbucket.org") && l.contains("git-upload-pack 'acme/site.git'")), "{:?}", ssh_calls());
    assert_eq!(git(&h.local, &["remote", "get-url", "origin"]), "git@bitbucket.org:acme/site.git", "the remote is left as it was");

    // without a fetch: the same ref, as last fetched
    let newer = h.push("not fetched yet");
    assert_eq!(start_point(&p, &h.local, None).unwrap(), "refs/remotes/origin/main");
    assert_eq!(git(&h.local, &["rev-parse", "refs/remotes/origin/main"]), new);
    assert_eq!(start_point(&p, &h.local, Some(START_FETCH_LIMIT)).unwrap(), "refs/remotes/origin/main");
    assert_eq!(git(&h.local, &["rev-parse", "refs/remotes/origin/main"]), newer);
}

#[tokio::test]
async fn a_bitbucket_project_with_a_fork_as_origin_starts_from_master_fetched_through_upstream_written_as_ssh_url() {
    fake_ssh();
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let h = Hosted::new(tmp.path(), "bitbucket", "acme/shop-start", "master");
    // origin is your fork (never fetched: https is off), upstream the project's repository
    git(&h.local, &["remote", "set-url", "origin", "https://jefsev@bitbucket.org/jefsev/shop-start-fork.git"]);
    git(&h.local, &["remote", "add", "upstream", "ssh://git@bitbucket.org/acme/shop-start.git"]);
    let (_, p) = project(&st, &h.local, "https://bitbucket.org/acme/shop-start/src/master/", "master");
    let new = h.push("on bitbucket's master");

    let start = start_point(&p, &h.local, Some(START_FETCH_LIMIT)).unwrap();
    assert_eq!(start, "refs/remotes/upstream/master");
    assert_eq!(git(&h.local, &["rev-parse", &start]), new);
    assert!(ssh_calls().iter().any(|l| l.contains("git@bitbucket.org") && l.contains("git-upload-pack '/acme/shop-start.git'")
            || l.contains("git@bitbucket.org") && l.contains("git-upload-pack 'acme/shop-start.git'")), "{:?}", ssh_calls());
}

#[test]
fn remote_for_finds_the_remote_for_a_bitbucket_link_in_any_of_its_forms() {
    fake_ssh();
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["remote", "add", "origin", "https://bitbucket.org/acme/site"]);
    let link = "https://bitbucket.org/acme/site";
    for form in ["https://jefsev@bitbucket.org/acme/site.git", "ssh://git@bitbucket.org/acme/site.git", "git@bitbucket.org:acme/site.git",
                 "https://bitbucket.org/acme/site", "https://bitbucket.org/acme/site.git", "https://www.bitbucket.org/acme/site/",
                 "https://bitbucket.org/ACME/Site.git"] {
        git(&repo, &["remote", "set-url", "origin", form]);
        assert_eq!(remote_for(&repo, link).as_deref(), Some("origin"), "origin = {form}");
    }
    for other in ["https://bitbucket.org/acme/site-other", "git@bitbucket.org:other/site.git", "https://github.com/acme/site"] {
        git(&repo, &["remote", "set-url", "origin", other]);
        assert_eq!(remote_for(&repo, link), None, "origin = {other}");
    }
    // a fork as origin and the project's repository as upstream
    git(&repo, &["remote", "set-url", "origin", "https://jefsev@bitbucket.org/jefsev/site.git"]);
    git(&repo, &["remote", "add", "upstream", "git@bitbucket.org:acme/site.git"]);
    assert_eq!(remote_for(&repo, link).as_deref(), Some("upstream"));
    // both match: origin first
    git(&repo, &["remote", "set-url", "origin", "ssh://git@bitbucket.org/acme/site.git"]);
    assert_eq!(remote_for(&repo, link).as_deref(), Some("origin"));
}

// ---------------------------------------------------------------------------------------------------------------
// The run prompt
// ---------------------------------------------------------------------------------------------------------------

/// Adds a CLI in Settings → Coding CLIs and returns its id.
fn add_cli(st: &gizai_lib::AppState, name: &str, kind: &str, command: &str, env: &[&str]) -> String {
    let mut list: Vec<Cli> = gizai_core::clis::list(&st.db).unwrap();
    list.push(Cli { name: name.into(), kind: kind.into(), command: command.into(), env: env.iter().map(|e| e.to_string()).collect(), ..Default::default() });
    gizai_lib::clis::save(st, list).unwrap().into_iter().find(|c| c.cli.name == name).unwrap().cli.id
}

/// Puts the backend agent on the CLI `cli`.
fn put_agent_on(st: &gizai_lib::AppState, cli: &str) {
    let (_, agent) = gizai_core::team::all_agents(&st.db).unwrap().into_iter().find(|(_, m)| m.role_key == "backend").unwrap();
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, AgentInput { name: agent.name.clone(), role_key: "backend".into(),
        adapter: cli.into(), ..Default::default() }).unwrap();
}

/// The prompt the fake Claude Code was given (it writes it to stderr with FAKE_TEMP=1).
fn prompt_of(run: &gizai_core::model::Run) -> String {
    let err = std::fs::read_to_string(Path::new(&run.log_path).with_extension("stderr.log")).unwrap();
    let start = err.find("prompt>>").unwrap_or_else(|| panic!("no prompt in {err}")) + "prompt>>".len();
    let end = err[start..].find("<<prompt").unwrap() + start;
    err[start..end].to_string()
}

/// The "## Your branch" section of a prompt (up to the next section).
fn your_branch(prompt: &str) -> String {
    let at = prompt.find("## Your branch").unwrap_or_else(|| panic!("no Your branch section in {prompt}"));
    let rest = &prompt[at..];
    let end = rest[3..].find("\n## ").map(|e| e + 3).unwrap_or(rest.len());
    rest[..end].trim().to_string()
}

/// Runs card KADE-1 of a project linked to `link`, whose clone `h.local` has its remotes as the caller set them, on the
/// fake Claude Code after a new commit on the hosted main branch. Returns the run's prompt and checks the run started
/// from that commit.
async fn run_prompt(st: &gizai_lib::AppState, h: &Hosted, link: &str) -> String {
    let claude = add_cli(st, "Claude Code (temp)", "claude_code", FAKE_CLAUDE, &["FAKE_TEMP=1"]);
    let (task, _) = project(st, &h.local, link, &h.branch);
    put_agent_on(st, &claude);
    let new = h.push("on the host, not fetched yet");
    let s = gizai_lib::runs::run_once(st, &task, None, None).await.unwrap();
    assert_eq!(s.status, "succeeded", "{:?}", s.error);
    let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
    assert_eq!(run.base_sha.as_deref(), Some(new.as_str()), "the run started from the hosted main branch's latest commit");
    assert_eq!(git(Path::new(run.worktree_path.as_deref().unwrap()), &["rev-parse", "HEAD"]), new);
    prompt_of(&run)
}

#[tokio::test]
async fn a_bitbucket_cards_run_prompt_says_its_main_branch_was_just_fetched_from_bitbucket() {
    fake_ssh();
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let h = Hosted::new(tmp.path(), "bitbucket", "acme/site-run", "main");
    git(&h.local, &["remote", "set-url", "origin", "git@bitbucket.org:acme/site-run.git"]);
    let prompt = run_prompt(&st, &h, "https://bitbucket.org/acme/site-run").await;
    assert_eq!(your_branch(&prompt), "## Your branch\n\nThe main branch is origin/main, just fetched from Bitbucket.");
    assert!(ssh_calls().iter().any(|l| l.contains("git-upload-pack 'acme/site-run.git'")), "{:?}", ssh_calls());
}

#[tokio::test]
async fn a_bitbucket_cards_run_through_upstream_names_upstream_and_bitbucket() {
    fake_ssh();
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let h = Hosted::new(tmp.path(), "bitbucket", "acme/site-up", "master");
    git(&h.local, &["remote", "set-url", "origin", "https://jefsev@bitbucket.org/jefsev/site-up.git"]);
    git(&h.local, &["remote", "add", "upstream", "ssh://git@bitbucket.org/acme/site-up.git"]);
    let prompt = run_prompt(&st, &h, "https://bitbucket.org/acme/site-up").await;
    assert_eq!(your_branch(&prompt), "## Your branch\n\nThe main branch is upstream/master, just fetched from Bitbucket.");
}

#[tokio::test]
async fn a_github_cards_run_prompt_still_says_github() {
    fake_ssh();
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let h = Hosted::new(tmp.path(), "github", "acme/site-gh", "main");
    git(&h.local, &["remote", "set-url", "origin", "git@github.com:acme/site-gh.git"]);
    let prompt = run_prompt(&st, &h, "https://github.com/acme/site-gh").await;
    assert_eq!(your_branch(&prompt), "## Your branch\n\nThe main branch is origin/main, just fetched from GitHub.");
}

#[tokio::test]
async fn another_git_urls_run_prompt_says_the_projects_repository() {
    fake_ssh();
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let h = Hosted::new(tmp.path(), "elsewhere", "acme/site-git", "main");
    // the clone's origin is the bare repository's path, and so is the project's link
    let prompt = run_prompt(&st, &h, h.bare.to_str().unwrap()).await;
    assert_eq!(your_branch(&prompt), "## Your branch\n\nThe main branch is origin/main, just fetched from the project's repository.");
}
