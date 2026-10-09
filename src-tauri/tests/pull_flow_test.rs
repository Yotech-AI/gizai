//! Review on GitHub, end to end with a fake gh and "GitHub" as local bare repositories: Open pull request on a card
//! in Review, a pull request an agent opened, and the PR check that moves a merged card to Done.
use std::path::{Path, PathBuf};
use std::process::Command;

use gizai_lib::pulls::{self, PullInfo};
use serde_json::json;

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const LINK: &str = "https://github.com/acme/shop";

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

fn has_branch(repo: &Path, branch: &str) -> bool {
    Command::new("git").args(["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).current_dir(repo).status().unwrap().success()
}

/// "GitHub" (bare repositories under tmp/github) and the project's local clone of https://github.com/acme/shop. The
/// clone's git config sends https://github.com/acme/ there and allows no transport but local files, so nothing here
/// can reach the real GitHub.
fn github(tmp: &Path) -> (PathBuf, PathBuf) {
    let src = tmp.join("src");
    std::fs::create_dir(&src).unwrap();
    git(&src, &["init", "-q", "-b", "main"]);
    git(&src, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init"]);
    let bare = tmp.join("github/acme/shop");
    git(tmp, &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
    let local = tmp.join("local");
    let rewrite = format!("url.{}/.insteadOf=https://github.com/acme/", tmp.join("github/acme").display());
    git(tmp, &["clone", "-q", "-c", &rewrite, "-c", "protocol.allow=never", "-c", "protocol.file.allow=always",
               "-c", "user.email=t@t", "-c", "user.name=t", LINK, local.to_str().unwrap()]);
    (bare, local)
}

/// Writes an executable script through a child `sh`, so this process never holds it open for writing: such a
/// handle leaks into any process another test thread starts at that moment, and running the script would then
/// fail with "Text file busy" (ETXTBSY).
fn write_script(path: &Path, script: &str) {
    use std::io::Write;
    let mut sh = std::process::Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path)
        .stdin(std::process::Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

/// A fake gh: each call's arguments go to dir/gh-args (one line each); `pr list` answers with dir/list.json;
/// `pr create` keeps its stdin in dir/gh-stdin and fails with dir/create.err, else answers with dir/create.out.
fn fake_gh(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let gh = dir.join("gh");
    write_script(&gh, &format!(r#"#!/bin/sh
d='{d}'
echo "$*" >> "$d/gh-args"
case "$1 $2" in
  "pr list") [ -f "$d/list.json" ] && cat "$d/list.json"; exit 0 ;;
  "pr create")
    cat > "$d/gh-stdin"
    if [ -f "$d/create.err" ]; then cat "$d/create.err" >&2; exit 1; fi
    cat "$d/create.out"; exit 0 ;;
esac
echo "unknown command: $*" >&2; exit 1
"#, d = dir.display()));
    gh
}

fn gh_calls(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("gh-args")).unwrap_or_default().lines().map(String::from).collect()
}

/// gh's answer to `pr list` for one pull request.
fn list(dir: &Path, number: u64, state: &str, draft: bool, head: &str) {
    let pr = json!([{"number": number, "url": format!("{LINK}/pull/{number}"), "state": state, "isDraft": draft, "headRefOid": head,
                     "commits": [{"oid": head}]}]);
    std::fs::write(dir.join("list.json"), pr.to_string()).unwrap();
}

struct Card { st: gizai_lib::AppState, task: String, bare: PathBuf, local: PathBuf, wt: PathBuf, branch: String, gh: PathBuf }

/// Card KADE-1 of a project linked to https://github.com/acme/shop, after an agent's run (its worktree and branch,
/// with one commit); in Testing. gh is the fake.
async fn worked_card(tmp: &Path) -> Card {
    let st = gizai_lib::test_state(tmp);
    let (bare, local) = github(tmp);
    let task = gizai_lib::test_task(&st, local.to_str().unwrap(), "backend");
    let p = gizai_core::projects::list(&st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    gizai_core::projects::update(&st.db, &st.you_id, &p.id, gizai_core::model::ProjectInput { name: p.name.clone(), key: p.key.clone(),
        repo_path: p.repo_path.clone(), repo_url: Some(LINK.into()), default_branch: Some("main".into()), ..Default::default() }).unwrap();
    let gh = tmp.join("gh");
    gizai_core::settings::set(&st.db, "gh_bin", &fake_gh(&gh).to_string_lossy().to_string()).unwrap();
    gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    let run = gizai_core::runs::list_for_task(&st.db, &task).unwrap().remove(0);
    let (wt, branch) = (PathBuf::from(run.worktree_path.unwrap()), run.branch.unwrap());
    assert!(wt.starts_with(st.data_dir.join("worktrees")), "{wt:?}");
    commit(&wt, "invoices.csv");
    Card { st, task, bare, local, wt, branch, gh }
}

fn commit(dir: &Path, name: &str) {
    std::fs::write(dir.join(name), name).unwrap();
    git(dir, &["add", name]);
    git(dir, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", name]);
}

fn to_review(c: &Card) {
    let team = gizai_core::team::get(&c.st.db, &gizai_core::team::list(&c.st.db).unwrap()[0].id).unwrap();
    let review = team.states.iter().find(|s| s.category == "review").unwrap().id.clone();
    gizai_core::tasks::move_to(&c.st.db, &c.st.you_id, &c.task, &review, "").unwrap();
}

fn task(c: &Card) -> gizai_core::model::Task { gizai_core::tasks::get(&c.st.db, &c.task).unwrap() }

/// GA-49: in the seed Review's next column is Deploy; a team without a deploy step links Review to Done.
fn review_then_done(c: &Card) {
    let team = gizai_core::team::get(&c.st.db, &gizai_core::team::list(&c.st.db).unwrap()[0].id).unwrap();
    let id = |category: &str| team.states.iter().find(|s| s.category == category).unwrap().id.clone();
    gizai_core::columns::set_column(&c.st.db, &c.st.you_id, &id("review"),
        gizai_core::columns::ColumnInput { next_state_id: Some(id("done")), ..Default::default() }).unwrap();
}

#[tokio::test]
async fn open_pull_request_pushes_the_cards_branch_and_opens_its_pull_request_with_gh() {
    let tmp = tempfile::tempdir().unwrap();
    let c = worked_card(tmp.path()).await;
    gizai_core::tasks::update(&c.st.db, &c.st.you_id, &c.task, gizai_core::model::TaskPatch {
        description_md: Some("Download all invoices as one CSV file.".into()), acceptance_md: Some("- [ ] One row per invoice".into()), ..Default::default() }).unwrap();
    // in Testing: no button, so no pull request, and the PR check doesn't ask GitHub about it
    let e = pulls::open(&c.st, &c.task).await.unwrap_err();
    assert!(e.contains("KADE-1 isn't in Review"), "{e}");
    assert!(pulls::check_all(&c.st).await.is_empty());
    assert!(gh_calls(&c.gh).is_empty(), "gh never started");
    assert!(!has_branch(&c.bare, &c.branch), "nothing pushed");

    to_review(&c);
    std::fs::write(c.gh.join("create.out"), format!("{LINK}/pull/7\n")).unwrap();
    let pr = pulls::open(&c.st, &c.task).await.unwrap();
    assert_eq!(pr, PullInfo { url: format!("{LINK}/pull/7"), number: Some(7), state: "open".into(), note: None });
    assert_eq!(git(&c.bare, &["rev-parse", &c.branch]), git(&c.wt, &["rev-parse", "HEAD"]), "the branch is on GitHub with its commit");
    assert_eq!(gh_calls(&c.gh), [
        format!("pr list --repo acme/shop --head {} --state all --limit 20 --json number,url,state,isDraft,headRefOid,commits", c.branch),
        format!("pr create --repo acme/shop --head {} --base main --title KADE-1: Export invoices as CSV --body-file -", c.branch),
    ]);
    assert_eq!(std::fs::read_to_string(c.gh.join("gh-stdin")).unwrap(),
               "Download all invoices as one CSV file.\n\n## Acceptance criteria\n\n- [ ] One row per invoice\n\nFrom Gizai card KADE-1.");
    let t = task(&c);
    assert_eq!((t.pr_url.as_deref(), t.pr_state.as_deref(), t.state_name.as_str()), (Some(format!("{LINK}/pull/7").as_str()), Some("open"), "Review"));
    let last = gizai_core::tasks::activity(&c.st.db, &c.task).unwrap().pop().unwrap();
    assert!(last.actor_name.is_some(), "opened by you");
    assert_eq!(last.diff, json!({"pullRequest": format!("{LINK}/pull/7"), "prState": "open", "opened": true}));
}

#[tokio::test]
async fn a_pull_request_an_agent_opened_shows_on_the_card_and_push_branch_adds_to_it() {
    let tmp = tempfile::tempdir().unwrap();
    let c = worked_card(tmp.path()).await;
    to_review(&c);
    git(&c.local, &["push", "-q", "origin", &c.branch]); // the agent pushed it and opened a pull request
    list(&c.gh, 4, "OPEN", false, &git(&c.wt, &["rev-parse", "HEAD"]));
    let pr = pulls::check(&c.st, &c.task).await.unwrap();
    assert_eq!(pr, Some(PullInfo { url: format!("{LINK}/pull/4"), number: Some(4), state: "open".into(), note: None }));
    let t = task(&c);
    assert_eq!((t.pr_url.as_deref(), t.pr_state.as_deref()), (Some(format!("{LINK}/pull/4").as_str()), Some("open")));
    let last = gizai_core::tasks::activity(&c.st.db, &c.task).unwrap().pop().unwrap();
    assert_eq!((last.actor_name, last.diff), (None, json!({"pullRequest": format!("{LINK}/pull/4"), "prState": "open"})), "seen by Gizai, not opened by you");
    // the agent marks it as a draft: the PR check (every two minutes) sees it
    list(&c.gh, 4, "OPEN", true, &git(&c.wt, &["rev-parse", "HEAD"]));
    let checked = pulls::check_all(&c.st).await;
    assert_eq!((checked.len(), checked[0].changed, checked[0].moved_to.clone()), (1, true, None));
    assert_eq!(task(&c).pr_state.as_deref(), Some("draft"));
    // Push branch: new commits go to the same pull request, nothing new is opened
    commit(&c.wt, "fix.csv");
    std::fs::write(c.wt.join("scratch.txt"), "not committed").unwrap();
    let pr = pulls::open(&c.st, &c.task).await.unwrap();
    assert_eq!((pr.url.as_str(), pr.number, pr.state.as_str()), (format!("{LINK}/pull/4").as_str(), Some(4), "draft"));
    assert_eq!(pr.note.as_deref(), Some("Its worktree has 1 uncommitted change, which the pull request doesn't have"));
    assert_eq!(git(&c.bare, &["rev-parse", &c.branch]), git(&c.wt, &["rev-parse", "HEAD"]));
    assert!(!gh_calls(&c.gh).iter().any(|l| l.starts_with("pr create")), "{:?}", gh_calls(&c.gh));
    assert!(!gizai_core::tasks::activity(&c.st.db, &c.task).unwrap().iter().any(|e| e.diff.get("opened").is_some()));
}

#[tokio::test]
async fn a_merge_on_github_moves_the_card_to_done_and_removes_its_worktree_on_the_next_check() {
    let tmp = tempfile::tempdir().unwrap();
    let c = worked_card(tmp.path()).await;
    review_then_done(&c);
    to_review(&c);
    std::fs::write(c.gh.join("create.out"), format!("{LINK}/pull/7\n")).unwrap();
    pulls::open(&c.st, &c.task).await.unwrap();
    // still open on GitHub: nothing moves
    list(&c.gh, 7, "OPEN", false, &git(&c.wt, &["rev-parse", "HEAD"]));
    let checked = pulls::check_all(&c.st).await;
    assert_eq!((checked.len(), checked[0].changed), (1, false));
    assert_eq!(task(&c).state_name, "Review");
    // merged on GitHub
    list(&c.gh, 7, "MERGED", false, &git(&c.wt, &["rev-parse", "HEAD"]));
    let checked = pulls::check_all(&c.st).await;
    assert_eq!(checked.len(), 1);
    assert_eq!((checked[0].moved_to.as_deref(), checked[0].changed), (Some("Done"), true));
    let said = format!("removed its worktree and deleted branch {} after the merge", c.branch);
    assert_eq!(checked[0].pull, Some(PullInfo { url: format!("{LINK}/pull/7"), number: Some(7), state: "merged".into(), note: Some(said.clone()) }));
    let t = task(&c);
    assert_eq!((t.state_name.as_str(), t.pr_state.as_deref()), ("Done", Some("merged")));
    assert!(!c.wt.exists(), "its worktree is removed");
    assert!(!has_branch(&c.local, &c.branch), "its local branch is deleted");
    assert!(has_branch(&c.bare, &c.branch), "GitHub's copy is GitHub's");
    let a = gizai_core::tasks::activity(&c.st.db, &c.task).unwrap();
    assert!(a.iter().any(|e| e.diff == json!({"column": ["Review", "Done"]})), "{a:?}");
    assert!(a.iter().any(|e| e.actor_name.is_none() && e.diff == json!({"cleanup": said})), "{a:?}");
    // Done: the next checks leave it alone and don't ask GitHub
    let calls = gh_calls(&c.gh).len();
    assert!(pulls::check_all(&c.st).await.is_empty());
    assert_eq!(gh_calls(&c.gh).len(), calls);
}

#[tokio::test]
async fn a_merged_card_keeps_a_worktree_with_uncommitted_work() {
    let tmp = tempfile::tempdir().unwrap();
    let c = worked_card(tmp.path()).await;
    review_then_done(&c);
    to_review(&c);
    std::fs::write(c.gh.join("create.out"), format!("{LINK}/pull/7\n")).unwrap();
    pulls::open(&c.st, &c.task).await.unwrap();
    std::fs::write(c.wt.join("notes.txt"), "not committed").unwrap();
    list(&c.gh, 7, "MERGED", false, &git(&c.wt, &["rev-parse", "HEAD"]));
    let checked = pulls::check_all(&c.st).await;
    assert_eq!(checked[0].moved_to.as_deref(), Some("Done"));
    assert_eq!(checked[0].pull.as_ref().unwrap().note.as_deref(),
               Some(format!("kept its worktree {} (it has uncommitted changes) after the merge", c.wt.display()).as_str()));
    assert!(c.wt.join("notes.txt").exists() && has_branch(&c.local, &c.branch), "nothing lost");
    assert_eq!(task(&c).state_name, "Done");
}

#[tokio::test]
async fn an_older_merged_pull_request_does_not_move_a_card_with_newer_work() {
    let tmp = tempfile::tempdir().unwrap();
    let c = worked_card(tmp.path()).await;
    to_review(&c);
    // the branch's pull request #3 was merged before this card's latest commit
    list(&c.gh, 3, "MERGED", false, &git(&c.local, &["rev-parse", "main"]));
    pulls::check_all(&c.st).await;
    let t = task(&c);
    assert_eq!(t.state_name, "Review", "stays for your review");
    assert!(c.wt.exists() && has_branch(&c.local, &c.branch));
}

#[tokio::test]
async fn gh_problems_are_said_plainly_and_change_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let c = worked_card(tmp.path()).await;
    to_review(&c);
    // not logged in
    std::fs::write(c.gh.join("create.err"), "To get started with GitHub CLI, please run:  gh auth login\n").unwrap();
    let e = pulls::open(&c.st, &c.task).await.unwrap_err();
    assert_eq!(e, "The GitHub CLI isn't logged in: run gh auth login in a terminal");
    assert_eq!(task(&c).pr_url, None);
    // gh moved away since it was set
    gizai_core::settings::set(&c.st.db, "gh_bin", &"/nonexistent/gh".to_string()).unwrap();
    let e = pulls::open(&c.st, &c.task).await.unwrap_err();
    assert!(e.contains("The GitHub CLI isn't at /nonexistent/gh any more"), "{e}");
    assert!(pulls::check(&c.st, &c.task).await.is_err());
    // a project without its GitHub link
    let p = gizai_core::projects::list(&c.st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    gizai_core::projects::update(&c.st.db, &c.st.you_id, &p.id, gizai_core::model::ProjectInput { name: p.name.clone(), key: p.key.clone(),
        repo_path: p.repo_path.clone(), repo_url: Some(String::new()), ..Default::default() }).unwrap();
    let e = pulls::open(&c.st, &c.task).await.unwrap_err();
    assert!(e.contains("Link Kade to its GitHub repository first"), "{e}");
    assert_eq!(task(&c).state_name, "Review");
}

#[tokio::test]
async fn the_github_cli_is_a_setting_and_empty_means_find_it_when_needed() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let mut s = gizai_lib::runs::get_settings(&st);
    assert_eq!(s.gh_bin, None);
    s.gh_bin = Some("  /opt/gh/bin/gh ".into());
    gizai_lib::runs::save_settings(&st, &s).unwrap();
    assert_eq!(gizai_lib::runs::get_settings(&st).gh_bin.as_deref(), Some("/opt/gh/bin/gh"));
    s.gh_bin = Some("   ".into());
    gizai_lib::runs::save_settings(&st, &s).unwrap();
    assert_eq!(gizai_lib::runs::get_settings(&st).gh_bin, None);
}

#[tokio::test]
async fn with_a_deploy_column_a_merge_moves_the_card_to_deploy_and_nothing_starts_on_it() {
    // GA-32: Deploy (merged, not deployed yet) after Review, in the seed since GA-49 (Review's next column, Manual); the
    // DevOps Agent is on it and has the card.
    let tmp = tempfile::tempdir().unwrap();
    let c = worked_card(tmp.path()).await;
    let team = gizai_core::team::get(&c.st.db, &gizai_core::team::list(&c.st.db).unwrap()[0].id).unwrap();
    let ops = gizai_core::team::add_agent(&c.st.db, &c.st.you_id, &team.id, gizai_core::model::AgentInput { name: "DevOps Agent".into(),
        role_key: "devops".into(), wakeup: "on_assign".into(), ..Default::default() }).unwrap();
    to_review(&c);
    gizai_core::tasks::update(&c.st.db, &c.st.you_id, &c.task, gizai_core::model::TaskPatch { assignee_id: Some(ops.clone()), ..Default::default() }).unwrap();
    std::fs::write(c.gh.join("create.out"), format!("{LINK}/pull/7\n")).unwrap();
    pulls::open(&c.st, &c.task).await.unwrap();
    let runs_before = gizai_core::runs::list_for_task(&c.st.db, &c.task).unwrap().len();
    list(&c.gh, 7, "MERGED", false, &git(&c.wt, &["rev-parse", "HEAD"]));
    let checked = pulls::check_all(&c.st).await;
    assert_eq!((checked.len(), checked[0].moved_to.as_deref(), checked[0].changed), (1, Some("Deploy"), true));
    let t = task(&c);
    assert_eq!((t.state_name.as_str(), t.state_category.as_str(), t.pr_state.as_deref(), t.assignee_id.as_deref()),
               ("Deploy", "deploy", Some("merged"), Some(ops.as_str())));
    let a = gizai_core::tasks::activity(&c.st.db, &c.task).unwrap();
    assert!(a.iter().any(|e| e.diff == json!({"column": ["Review", "Deploy"]})), "{a:?}");
    // the rest of the merge handling is as before
    assert!(!c.wt.exists(), "its worktree is removed");
    assert!(!has_branch(&c.local, &c.branch), "its local branch is deleted");
    // nothing starts on it, not even the DevOps Agent it is assigned to
    gizai_lib::runs::dispatch(&c.st, &c.task).await;
    gizai_lib::runs::pull(&c.st).await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(gizai_lib::runs::live(&c.st).is_empty());
    assert_eq!(gizai_core::runs::list_for_task(&c.st.db, &c.task).unwrap().len(), runs_before);
    // Deploy: the next checks leave it alone and don't ask GitHub
    let calls = gh_calls(&c.gh).len();
    assert!(pulls::check_all(&c.st).await.is_empty());
    assert_eq!(gh_calls(&c.gh).len(), calls);
    assert_eq!(task(&c).state_name, "Deploy", "until a person drags it to Done");
}
