//! GA-86: the Team Lead's merge_pull_request, end to end with a fake gh and "GitHub" as local bare repositories. A pull
//! request that meets every rule is merged with a merge commit, the card gets the Team Lead's comment, moves to Deploy
//! and its worktree is cleaned up, and a notification and the activity say the Team Lead merged it. Each rule that fails
//! refuses with its reason and merges nothing. A board check may merge; a chat answer after an outside tool may not.
//! Only a person switches a project's Team Lead may merge.
// Linux and macOS only: these tests run shell scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use gizai_core::model::*;
use gizai_core::{board as core_board, comments, projects, runs as core_runs, settings, tasks, team};
use gizai_lib::notifications::{self, Desktop, Notice};
use gizai_lib::{AppState, tools};
use serde_json::{Value, json};

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const LINK: &str = "https://github.com/acme/shop";

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

/// "GitHub" (a bare repository under tmp/github) and the project's local clone of https://github.com/acme/shop. The
/// clone's git config sends https://github.com/acme/ there and allows no transport but local files, so nothing here
/// can reach the real GitHub.
fn github(tmp: &Path) -> PathBuf {
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
    local
}

/// Writes an executable script through a child `sh`, so this process never holds it open for writing (ETXTBSY).
fn write_script(path: &Path, script: &str) {
    use std::io::Write;
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path)
        .stdin(std::process::Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

/// A fake gh: each call's arguments go to dir/gh-args (one line each). `pr list` answers with dir/list.json, `pr view`
/// with dir/view.json. `pr merge` fails with dir/merge.err, else GitHub has merged it: dir/merged.json becomes the list.
fn fake_gh(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let gh = dir.join("gh");
    write_script(&gh, &format!(r#"#!/bin/sh
d='{d}'
echo "$*" >> "$d/gh-args"
case "$1 $2" in
  "pr list") [ -f "$d/list.json" ] && cat "$d/list.json"; exit 0 ;;
  "pr view") cat "$d/view.json"; exit 0 ;;
  "pr merge")
    if [ -f "$d/merge.err" ]; then cat "$d/merge.err" >&2; exit 1; fi
    cat "$d/merged.json" > "$d/list.json"
    echo "Merged pull request #7"; exit 0 ;;
esac
echo "unknown command: $*" >&2; exit 1
"#, d = dir.display()));
    gh
}

/// gh's `pr list` answer for pull request #7 from the card's branch.
fn list_json(state: &str, head: &str) -> String {
    json!([{"number": 7, "url": format!("{LINK}/pull/7"), "state": state, "isDraft": false, "headRefOid": head, "commits": [{"oid": head}]}]).to_string()
}

/// A check run of GitHub Actions, as `gh pr view --json statusCheckRollup` gives it.
fn job(name: &str, status: &str, conclusion: Value) -> Value {
    json!({"__typename": "CheckRun", "name": name, "workflowName": "CI", "status": status, "conclusion": conclusion,
           "startedAt": "2026-10-10T10:00:00Z", "completedAt": "2026-10-10T10:06:00Z", "detailsUrl": "https://github.com/acme/shop/actions/runs/1/job/2"})
}

struct T {
    st: AppState,
    task: String,
    lead: String,
    qa: String,
    project: String,
    wt: PathBuf,
    local: PathBuf,
    branch: String,
    /// The commit QA passed: the branch's latest, on "GitHub" too.
    head: String,
    gh: PathBuf,
    shown: Arc<Mutex<Vec<Notice>>>,
    _tmp: tempfile::TempDir,
}

/// Card KADE-1 of project KADE (linked to https://github.com/acme/shop, Team Lead may merge on) after an agent's run
/// (its worktree and branch, one commit, pushed); in Review with testing on, and its QA run (`qa`: an outcome, or none)
/// ended at the branch's latest commit. gh is the fake: pull request #7 is open, mergeable, and its three CI jobs and a
/// commit status succeeded. A Team Lead (Chat on) and a QA Agent; agents paused, so nothing else starts.
async fn setup_with(qa: Option<&str>) -> T {
    let tmp = real_tempdir();
    let mut st = gizai_lib::test_state(tmp.path());
    let shown: Arc<Mutex<Vec<Notice>>> = Arc::default();
    let s2 = shown.clone();
    st.desktop = Desktop { show: Arc::new(move |n| s2.lock().unwrap().push(n)), away: Arc::new(|| false) };
    let local = github(tmp.path());
    let task = gizai_lib::test_task(&st, local.to_str().unwrap(), "backend");
    let project = projects::list(&st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    projects::update(&st.db, &st.you_id, &project.id, ProjectInput { name: project.name.clone(), key: project.key.clone(),
        repo_path: project.repo_path.clone(), repo_url: Some(LINK.into()), default_branch: Some("main".into()), lead_may_merge: Some(true),
        ..Default::default() }).unwrap();
    let gh = tmp.path().join("gh");
    settings::set(&st.db, "gh_bin", &fake_gh(&gh).to_string_lossy().to_string()).unwrap();
    gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    let run = core_runs::list_for_task(&st.db, &task).unwrap().remove(0);
    let (wt, branch) = (PathBuf::from(run.worktree_path.unwrap()), run.branch.unwrap());
    commit(&wt, "invoices.csv");
    git(&local, &["push", "-q", "origin", &branch]);
    let head = git(&wt, &["rev-parse", "HEAD"]);
    settings::set(&st.db, "agents_paused", &true).unwrap();
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    let lead = team::add_agent(&st.db, &st.you_id, &team_id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(),
        chat_enabled: Some(true), ..Default::default() }).unwrap();
    let qa_agent = team::add_agent(&st.db, &st.you_id, &team_id, AgentInput { name: "QA Agent".into(), role_key: "qa".into(), ..Default::default() }).unwrap();
    let t = T { st, task, lead, qa: qa_agent, project: project.id, wt, local, branch, head, gh, shown, _tmp: tmp };
    t.move_to("review");
    if let Some(outcome) = qa {
        let head = t.head.clone();
        t.qa_run(outcome, &head);
    }
    std::fs::write(t.gh.join("list.json"), list_json("OPEN", &t.head)).unwrap();
    std::fs::write(t.gh.join("merged.json"), list_json("MERGED", &t.head)).unwrap();
    t.view(|_| {});
    t
}

async fn setup() -> T {
    setup_with(Some("qa_pass")).await
}

fn commit(dir: &Path, name: &str) {
    std::fs::write(dir.join(name), name).unwrap();
    git(dir, &["add", name]);
    git(dir, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", name]);
}

impl T {
    fn col(&self, category: &str) -> String {
        let team = team::get(&self.st.db, &team::list(&self.st.db).unwrap()[0].id).unwrap();
        team.states.iter().find(|s| s.category == category).unwrap().id.clone()
    }
    fn move_to(&self, category: &str) {
        tasks::move_to(&self.st.db, &self.st.you_id, &self.task, &self.col(category), "").unwrap();
    }
    fn task(&self) -> Task {
        tasks::get(&self.st.db, &self.task).unwrap()
    }
    /// A QA run on the card that ended with `outcome` at commit `head`.
    fn qa_run(&self, outcome: &str, head: &str) -> String {
        // created_at is in milliseconds: a later run gets a later time
        std::thread::sleep(std::time::Duration::from_millis(5));
        let wt = self.wt.display().to_string();
        let run = core_runs::create(&self.st.db, &self.qa, &self.task, "qa", "S-qa", &wt, &wt, &self.branch, "/tmp/qa.jsonl").unwrap();
        core_runs::set_head_sha(&self.st.db, &run, head).unwrap();
        core_runs::finish(&self.st.db, &run, "succeeded", Some(&Outcome { outcome: outcome.into(), summary: "Checked".into(), issues: vec![] }),
                          120_000, 0, 0, None).unwrap();
        run
    }
    /// gh's `pr view` answer for #7: open, from the card's branch into main at the commit QA passed, mergeable, every
    /// check green; then `f` changes it.
    fn view(&self, f: impl FnOnce(&mut Value)) {
        let mut v = json!({
            "number": 7, "url": format!("{LINK}/pull/7"), "state": "OPEN", "isDraft": false, "isCrossRepository": false,
            "headRefName": self.branch, "headRefOid": self.head, "baseRefName": "main", "mergeable": "MERGEABLE", "mergeStateStatus": "CLEAN",
            "statusCheckRollup": [
                job("ubuntu-24.04", "COMPLETED", json!("SUCCESS")), job("macos-14", "COMPLETED", json!("SUCCESS")),
                job("windows-latest", "COMPLETED", json!("SUCCESS")),
                {"__typename": "StatusContext", "context": "ci/legacy", "state": "SUCCESS", "startedAt": "2026-10-10T10:00:00Z", "targetUrl": null},
            ],
        });
        f(&mut v);
        std::fs::write(self.gh.join("view.json"), v.to_string()).unwrap();
    }
    async fn merge(&self) -> Result<Value, String> {
        tools::call(&self.st, &self.lead, "merge_pull_request", json!({"task": "KADE-1"})).await
    }
    fn gh_calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.gh.join("gh-args")).unwrap_or_default().lines().map(String::from).collect()
    }
    fn lead_comments(&self) -> Vec<String> {
        comments::list(&self.st.db, &self.task).unwrap().into_iter().filter(|c| c.author_id == self.lead).map(|c| c.body_md).collect()
    }
    fn set_switch(&self, on: bool) {
        let p = projects::get(&self.st.db, &self.project).unwrap();
        projects::update(&self.st.db, &self.st.you_id, &p.id, ProjectInput { name: p.name.clone(), key: p.key.clone(), repo_path: p.repo_path.clone(),
            repo_url: p.repo_url.clone(), default_branch: Some(p.default_branch.clone()), lead_may_merge: Some(on), ..Default::default() }).unwrap();
    }
    /// A refusal: it says `why` and that nothing changed, gh merged nothing, and the card is where it was, with no
    /// comment and no merge in its activity.
    fn refused(&self, e: &str, why: &str, column: &str) {
        assert!(e.contains(why), "expected {why:?} in: {e}");
        assert!(e.contains("Nothing changed"), "{e}");
        assert!(!self.gh_calls().iter().any(|c| c.starts_with("pr merge")), "gh merged nothing: {:?}", self.gh_calls());
        let t = self.task();
        assert_eq!((t.state_name.as_str(), t.pr_state.as_deref()), (column, None), "the card stays");
        assert!(self.lead_comments().is_empty(), "no comment: {:?}", self.lead_comments());
        assert!(!tasks::activity(&self.st.db, &self.task).unwrap().iter().any(|e| e.diff.get("merged").is_some()), "no merge in the activity");
        assert!(self.wt.exists(), "its worktree stays");
        assert!(self.shown.lock().unwrap().is_empty(), "no notification");
    }
}

// ---- a merge ----

#[tokio::test]
async fn a_pull_request_that_meets_every_rule_is_merged_with_a_merge_commit_and_its_card_moves_to_deploy() {
    let t = setup().await;
    let qa_run = core_runs::list_for_task(&t.st.db, &t.task).unwrap().remove(0);
    assert_eq!(qa_run.outcome.as_deref(), Some("qa_pass"));
    let r = t.merge().await.unwrap();
    assert_eq!((r["ok"].as_bool(), r["done"].as_str()), (Some(true), Some("merged")), "{r}");
    assert_eq!(r["pull_request"], json!({"number": 7, "url": format!("{LINK}/pull/7"), "into": "main", "head": t.head}));
    assert_eq!(r["card"], "moved to Deploy", "{r}");
    assert_eq!(r["qa_run"]["id"], qa_run.id.as_str());
    assert_eq!(r["checks_passed"], json!(["CI / ubuntu-24.04", "CI / macos-14", "CI / windows-latest", "ci/legacy"]));
    // gh: the card's pull request, read, then merged with a merge commit at the commit QA passed; nothing else
    let calls = t.gh_calls();
    assert_eq!(calls[..3], [
        format!("pr list --repo acme/shop --head {} --state all --limit 20 --json number,url,state,isDraft,headRefOid,commits", t.branch),
        format!("pr view 7 --repo acme/shop --json {}", gizai_agents::github::DETAIL_FIELDS),
        format!("pr merge 7 --repo acme/shop --merge --match-head-commit {}", t.head),
    ], "{calls:?}");
    assert_eq!(calls.iter().filter(|c| c.starts_with("pr merge")).count(), 1, "{calls:?}");
    for never in ["--squash", "--rebase", "--admin", "--auto", "--delete-branch", "-d "] {
        assert!(!calls.iter().any(|c| c.contains(never)), "{never}: {calls:?}");
    }
    // the Team Lead's comment: the pull request, its head commit, the QA run and the checks that passed
    let said = t.lead_comments();
    assert_eq!(said.len(), 1, "{said:?}");
    for part in [format!("[#7]({LINK}/pull/7)"), "into main with a merge commit".into(), format!("`{}`", t.head), "QA Agent".into(),
                 qa_run.id.clone(), "4 checks passed: CI / ubuntu-24.04, CI / macos-14, CI / windows-latest, ci/legacy".into()] {
        assert!(said[0].contains(&part), "{part:?} in: {}", said[0]);
    }
    // the card moved on as when you merge: Deploy, its pull request merged, its worktree and local branch cleaned up
    let card = t.task();
    assert_eq!((card.state_name.as_str(), card.state_category.as_str(), card.pr_state.as_deref(), card.pr_url.as_deref()),
               ("Deploy", "deploy", Some("merged"), Some(format!("{LINK}/pull/7").as_str())));
    assert!(!t.wt.exists(), "its worktree is removed");
    assert!(git(&t.local, &["branch", "--list", &t.branch]).is_empty(), "its local branch is deleted");
    // the activity says the Team Lead merged it
    let a = tasks::activity(&t.st.db, &t.task).unwrap();
    let by_lead = a.iter().find(|e| e.diff.get("merged").is_some()).expect("a merge in the activity");
    assert_eq!(by_lead.actor_name.as_deref(), Some("Team Lead"));
    assert_eq!(by_lead.diff, json!({"pullRequest": format!("{LINK}/pull/7"), "merged": true, "head": t.head}));
    assert!(a.iter().any(|e| e.diff == json!({"column": ["Review", "Deploy"]})), "{a:?}");
    // and so does a desktop notification
    let shown = t.shown.lock().unwrap().clone();
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!((shown[0].title.as_str(), shown[0].body.as_str(), shown[0].route.clone()),
               ("The Team Lead merged KADE-1's pull request #7", "Export invoices as CSV", format!("#/task/{}", t.task)));
    assert_eq!(shown[0].kind, notifications::Kind::LeadMerged);
    // merged: a second call is refused (not in Review) and merges nothing more
    let e = t.merge().await.unwrap_err();
    assert!(e.contains("is in Deploy, not Review"), "{e}");
    assert_eq!(t.gh_calls().iter().filter(|c| c.starts_with("pr merge")).count(), 1);
}

#[tokio::test]
async fn the_merge_notifies_once_the_inboxs_own_merged_and_waits_for_deploy_notice_does_not_fire_as_well() {
    let t = setup().await;
    // a card assigned to you is in your Inbox in Review, and would be again in Deploy
    tasks::update(&t.st.db, &t.st.you_id, &t.task, TaskPatch { assignee_id: Some(t.st.you_id.clone()), ..Default::default() }).unwrap();
    assert!(notifications::look(&t.st).is_empty(), "the first look takes in what is there");
    t.merge().await.unwrap();
    assert_eq!(t.task().state_name, "Deploy");
    assert!(notifications::look(&t.st).is_empty(), "the Inbox's deploy notice doesn't come as well");
    let titles: Vec<String> = t.shown.lock().unwrap().iter().map(|n| n.title.clone()).collect();
    assert_eq!(titles, ["The Team Lead merged KADE-1's pull request #7"]);
}

#[tokio::test]
async fn with_the_waiting_switch_off_the_merge_shows_no_notification_but_still_merges() {
    let t = setup().await;
    let mut all = gizai_lib::runs::get_settings(&t.st);
    all.notifications.waiting = false;
    gizai_lib::runs::save_settings(&t.st, &all).unwrap();
    t.merge().await.unwrap();
    assert_eq!(t.task().state_name, "Deploy");
    assert!(t.shown.lock().unwrap().is_empty());
    assert_eq!(t.lead_comments().len(), 1);
}

#[tokio::test]
async fn skipped_and_neutral_checks_count_as_passed() {
    let t = setup().await;
    t.view(|v| {
        v["statusCheckRollup"] = json!([job("ubuntu-24.04", "COMPLETED", json!("SUCCESS")), job("docs", "COMPLETED", json!("SKIPPED")),
                                        job("lint", "COMPLETED", json!("NEUTRAL"))]);
    });
    let r = t.merge().await.unwrap();
    assert_eq!(r["checks_passed"], json!(["CI / ubuntu-24.04", "CI / docs", "CI / lint"]));
    assert_eq!(t.task().state_name, "Deploy");
}

#[tokio::test]
async fn when_github_refuses_the_merge_gizai_changes_nothing_and_says_what_github_said() {
    let t = setup().await;
    std::fs::write(t.gh.join("merge.err"), "GraphQL: Head branch was modified. Review and try the merge again. (mergePullRequest)\n").unwrap();
    let e = t.merge().await.unwrap_err();
    assert!(e.contains("GitHub didn't merge pull request #7") && e.contains("Head branch was modified") && e.contains("Nothing changed"), "{e}");
    let t2 = t.task();
    assert_eq!((t2.state_name.as_str(), t2.pr_state.as_deref()), ("Review", None));
    assert!(t.lead_comments().is_empty());
    assert!(t.shown.lock().unwrap().is_empty());
    assert!(t.wt.exists());
}

// ---- each rule refuses with its reason and merges nothing ----

#[tokio::test]
async fn refused_when_the_projects_team_lead_may_merge_switch_is_off() {
    let t = setup().await;
    t.set_switch(false);
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "KADE doesn't let you merge: its Team Lead may merge switch is off", "Review");
    assert!(t.gh_calls().is_empty(), "gh never asked");
}

#[tokio::test]
async fn refused_when_the_card_is_not_in_review() {
    let t = setup().await;
    t.move_to("testing");
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "KADE-1 is in Testing, not Review", "Testing");
    t.move_to("ready");
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "KADE-1 is in To do, not Review", "To do");
    assert!(t.gh_calls().is_empty());
}

#[tokio::test]
async fn refused_when_the_card_has_testing_off() {
    let t = setup().await;
    tasks::update(&t.st.db, &t.st.you_id, &t.task, TaskPatch { testing: Some(false), ..Default::default() }).unwrap();
    assert!(!t.task().testing);
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "KADE-1 has testing off", "Review");
    assert!(t.gh_calls().is_empty());
}

#[tokio::test]
async fn refused_when_qa_never_passed_the_card() {
    let t = setup_with(None).await;
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "QA hasn't passed KADE-1: it has no QA verdict", "Review");
    assert!(t.gh_calls().is_empty());
}

#[tokio::test]
async fn refused_when_the_latest_qa_verdict_is_qa_fail_even_after_an_earlier_pass() {
    let t = setup().await;
    let head = t.head.clone();
    t.qa_run("qa_fail", &head);
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "KADE-1's latest QA verdict is qa_fail", "Review");
    // a later pass counts again
    t.qa_run("qa_pass", &head);
    t.merge().await.unwrap();
    assert_eq!(t.task().state_name, "Deploy");
}

#[tokio::test]
async fn refused_when_a_commit_was_pushed_after_qa() {
    let t = setup().await;
    commit(&t.wt, "late.csv");
    git(&t.local, &["push", "-q", "origin", &t.branch]);
    let new_head = git(&t.wt, &["rev-parse", "HEAD"]);
    t.view(|v| v["headRefOid"] = json!(new_head));
    let e = t.merge().await.unwrap_err();
    t.refused(&e, &format!("QA passed commit {}, but pull request #7's latest commit is {}: something was pushed after QA",
                           &t.head[..7], &new_head[..7]), "Review");
    // QA passes the new commit: then it merges, at that commit
    t.qa_run("qa_pass", &new_head);
    std::fs::write(t.gh.join("merged.json"), list_json("MERGED", &new_head)).unwrap();
    t.merge().await.unwrap();
    assert!(t.gh_calls().contains(&format!("pr merge 7 --repo acme/shop --merge --match-head-commit {new_head}")), "{:?}", t.gh_calls());
}

#[tokio::test]
async fn refused_when_qa_recorded_no_commit() {
    let t = setup_with(None).await;
    let wt = t.wt.display().to_string();
    let run = core_runs::create(&t.st.db, &t.qa, &t.task, "qa", "S-qa", &wt, &wt, &t.branch, "/tmp/qa.jsonl").unwrap();
    core_runs::finish(&t.st.db, &run, "succeeded", Some(&Outcome { outcome: "qa_pass".into(), summary: "ok".into(), issues: vec![] }), 1, 0, 0, None).unwrap();
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "didn't record which commit", "Review");
}

#[tokio::test]
async fn refused_when_github_says_the_pull_request_cannot_be_merged() {
    let t = setup().await;
    t.view(|v| { v["mergeable"] = json!("CONFLICTING"); v["mergeStateStatus"] = json!("DIRTY"); });
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "pull request #7 has merge conflicts with main", "Review");
    // GitHub hasn't worked it out yet: wait
    t.view(|v| { v["mergeable"] = json!("UNKNOWN"); v["mergeStateStatus"] = json!("UNKNOWN"); });
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "GitHub is still working out whether pull request #7 can be merged", "Review");
    // a rule of the branch (a required review) blocks it
    t.view(|v| v["mergeStateStatus"] = json!("BLOCKED"));
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "GitHub blocks merging pull request #7", "Review");
    // behind main where the branch must be up to date
    t.view(|v| v["mergeStateStatus"] = json!("BEHIND"));
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "pull request #7 is behind main", "Review");
}

#[tokio::test]
async fn refused_when_the_pull_request_is_not_open_a_draft_or_not_into_the_main_branch() {
    let t = setup().await;
    t.view(|v| v["isDraft"] = json!(true));
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "pull request #7 is a draft", "Review");
    t.view(|v| v["baseRefName"] = json!("release/1.0"));
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "pull request #7 goes into release/1.0, not the project's main branch main", "Review");
    t.view(|v| v["isCrossRepository"] = json!(true));
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "isn't from the card's own branch", "Review");
    // no open pull request from the branch
    std::fs::write(t.gh.join("list.json"), list_json("CLOSED", &t.head)).unwrap();
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "there is no open pull request from", "Review");
}

#[tokio::test]
async fn refused_while_a_check_is_still_running_and_says_to_wait() {
    let t = setup().await;
    t.view(|v| {
        v["statusCheckRollup"][1] = job("macos-14", "IN_PROGRESS", Value::Null);
        v["mergeStateStatus"] = json!("UNSTABLE");
    });
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "1 of 4 checks on pull request #7 is still running (CI / macos-14): wait until it has finished", "Review");
    // a queued job and a pending commit status
    t.view(|v| {
        v["statusCheckRollup"][0] = job("ubuntu-24.04", "QUEUED", Value::Null);
        v["statusCheckRollup"][3]["state"] = json!("PENDING");
    });
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "2 of 4 checks on pull request #7 are still running (CI / ubuntu-24.04, ci/legacy)", "Review");
    // no checks at all yet
    t.view(|v| v["statusCheckRollup"] = json!([]));
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "no checks have run on pull request #7 yet", "Review");
}

#[tokio::test]
async fn refused_when_a_check_failed() {
    let t = setup().await;
    t.view(|v| v["statusCheckRollup"][2] = job("windows-latest", "COMPLETED", json!("FAILURE")));
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "1 check on pull request #7 failed: CI / windows-latest", "Review");
    // a failed one wins over one still running; a cancelled job and a failed commit status are failures too
    t.view(|v| {
        v["statusCheckRollup"][0] = job("ubuntu-24.04", "IN_PROGRESS", Value::Null);
        v["statusCheckRollup"][1] = job("macos-14", "COMPLETED", json!("CANCELLED"));
        v["statusCheckRollup"][2] = job("windows-latest", "COMPLETED", json!("TIMED_OUT"));
        v["statusCheckRollup"][3]["state"] = json!("ERROR");
    });
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "3 checks on pull request #7 failed: CI / macos-14, CI / windows-latest, ci/legacy", "Review");
}

#[tokio::test]
async fn refused_while_a_release_card_of_the_project_is_in_deploy() {
    let t = setup().await;
    let team_id = team::list(&t.st.db).unwrap()[0].id.clone();
    let ops = team::add_agent(&t.st.db, &t.st.you_id, &team_id, AgentInput { name: "DevOps Agent".into(), role_key: "devops".into(),
        ..Default::default() }).unwrap();
    let release = tasks::create(&t.st.db, &t.st.you_id, TaskInput { project_id: t.project.clone(), title: "Release Kade v1.2.0".into(),
        state_id: Some(t.col("deploy")), ..Default::default() }).unwrap();
    // a card in Deploy that no devops agent has is no release
    t.merge_ready_check_only().await;
    tasks::update(&t.st.db, &t.st.you_id, &release, TaskPatch { assignee_id: Some(ops.clone()), ..Default::default() }).unwrap();
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "A release is under way: KADE-2 is in Deploy with DevOps Agent", "Review");
    assert!(!t.gh_calls().iter().any(|c| c.starts_with("pr ")), "gh never asked: {:?}", t.gh_calls());
    assert!(t.may_merge_findings().is_empty(), "the board check doesn't offer it during a release");
    // the release is done: it merges
    tasks::move_to(&t.st.db, &t.st.you_id, &release, &t.col("done"), "").unwrap();
    t.merge().await.unwrap();
    assert_eq!(t.task().state_name, "Deploy");
}

impl T {
    /// No release under way: the board's may-merge finding is there for KADE-1 (the tool's own rule, read the same way).
    async fn merge_ready_check_only(&self) {
        assert!(gizai_core::pulls::release_under_way(&self.st.db, &self.project).unwrap().is_none());
        assert!(self.may_merge_findings().contains(&"KADE-1".to_string()));
    }
    fn may_merge_findings(&self) -> Vec<String> {
        gizai_lib::board::findings(&self.st, gizai_core::ids::now_ms()).unwrap().into_iter()
            .filter(|f| f.kind == "review" && f.code == "may_merge").map(|f| f.task).collect()
    }
}

#[tokio::test]
async fn refused_for_a_bitbucket_project() {
    let t = setup().await;
    let p = projects::get(&t.st.db, &t.project).unwrap();
    projects::update(&t.st.db, &t.st.you_id, &p.id, ProjectInput { name: p.name.clone(), key: p.key.clone(), repo_path: p.repo_path.clone(),
        repo_url: Some("https://bitbucket.org/acme/shop".into()), default_branch: Some("main".into()), ..Default::default() }).unwrap();
    assert!(projects::get(&t.st.db, &t.project).unwrap().lead_may_merge, "the switch is still on");
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "KADE is on Bitbucket: merge_pull_request merges pull requests on GitHub only", "Review");
    assert!(t.gh_calls().is_empty());
    // and the board check doesn't offer it
    assert!(t.may_merge_findings().is_empty());
}

#[tokio::test]
async fn refused_when_the_card_is_on_hold() {
    let t = setup().await;
    tasks::update(&t.st.db, &t.st.you_id, &t.task, TaskPatch { hold: Some("needs_decision".into()), hold_reason: Some("Which date format?".into()),
        ..Default::default() }).unwrap();
    let e = t.merge().await.unwrap_err();
    t.refused(&e, "KADE-1 is on hold (needs_decision: Which date format?)", "Review");
    assert!(t.gh_calls().is_empty());
}

// ---- chat answers and board checks ----

#[tokio::test]
async fn refused_after_an_outside_tool_in_the_same_chat_answer_and_allowed_in_the_next_message() {
    let t = setup().await;
    assert!(tools::NOT_AFTER_OUTSIDE.contains(&"merge_pull_request"));
    let thread = gizai_core::chat::create_thread(&t.st.db, &t.st.you_id, &t.lead, "Merge KADE-1").unwrap();
    gizai_core::chat::add_message(&t.st.db, gizai_core::chat::NewMessage { thread_id: thread.clone(), role: "user".into(),
        author_id: Some(t.st.you_id.clone()), body_md: Some("Look it up in Otus, then merge KADE-1".into()), ..Default::default() }).unwrap();
    gizai_lib::chat::mark_outside(&t.st, &thread, "mcp__otus__search");
    let e = tools::call_in(&t.st, &t.lead, Some(&thread), "merge_pull_request", json!({"task": "KADE-1"})).await.unwrap_err();
    assert!(e.starts_with("merge_pull_request is refused for the rest of this answer") && e.contains("mcp__otus__search"), "{e}");
    assert!(t.gh_calls().is_empty(), "gh never asked: {:?}", t.gh_calls());
    assert_eq!(t.task().state_name, "Review");
    assert!(t.lead_comments().is_empty());
    // another chat's answer, without an outside tool, may merge
    let other = gizai_core::chat::create_thread(&t.st.db, &t.st.you_id, &t.lead, "Merge KADE-1 please").unwrap();
    tools::call_in(&t.st, &t.lead, Some(&other), "merge_pull_request", json!({"task": "KADE-1"})).await.unwrap();
    assert_eq!(t.task().state_name, "Deploy");
}

#[tokio::test]
async fn a_board_check_offers_the_card_and_may_merge_it() {
    let t = setup().await;
    // the finding: a card in Review that QA passed, in a project that lets the Team Lead merge
    let all = gizai_lib::board::findings(&t.st, gizai_core::ids::now_ms()).unwrap();
    let f = all.iter().find(|f| f.kind == "review").expect("a review finding");
    assert_eq!((f.code.as_str(), f.task.as_str(), f.column.as_str(), f.agent.as_deref()), ("may_merge", "KADE-1", "Review", Some("QA Agent")));
    assert!(f.reason.contains("merge_pull_request"), "{}", f.reason);
    let v = tools::call(&t.st, &t.lead, "check_board", json!({})).await.unwrap();
    assert!(v["findings"].as_array().unwrap().iter().any(|f| f["kind"] == "review" && f["why"] == "may_merge" && f["task"] == "KADE-1"), "{v}");
    // a check may merge it
    let run = core_board::create_run(&t.st.db, &t.lead, "S", "/tmp", "/tmp/c.jsonl", &core_board::seen_of(&all)).unwrap();
    let r = tools::call_check(&t.st, &t.lead, &run, "merge_pull_request", json!({"task": "KADE-1"})).await.unwrap();
    assert_eq!((r["done"].as_str(), r["card"].as_str()), (Some("merged"), Some("moved to Deploy")), "{r}");
    assert_eq!(t.task().state_name, "Deploy");
    assert_eq!(t.lead_comments().len(), 1);
    // merged: no finding any more
    assert!(t.may_merge_findings().is_empty());
}

#[tokio::test]
async fn in_a_board_check_a_merge_that_has_to_wait_is_offered_again_and_a_failed_one_is_not() {
    let t = setup().await;
    let seen_keys = |run: &str| -> Vec<String> {
        let json: Option<String> = t.st.db.read(|c| Ok(c.query_row("SELECT findings_json FROM runs WHERE id=?1", [run], |r| r.get(0))?)).unwrap();
        serde_json::from_str::<Vec<core_board::Seen>>(&json.unwrap_or_default()).unwrap_or_default().into_iter().map(|s| s.key).collect()
    };
    let key = core_board::may_merge_key(&t.task);
    let all = gizai_lib::board::findings(&t.st, gizai_core::ids::now_ms()).unwrap();
    // checks still running: refused, and the next check offers the card again
    t.view(|v| v["statusCheckRollup"][0] = job("ubuntu-24.04", "IN_PROGRESS", Value::Null));
    let run = core_board::create_run(&t.st.db, &t.lead, "S", "/tmp", "/tmp/c.jsonl", &core_board::seen_of(&all)).unwrap();
    assert!(seen_keys(&run).contains(&key));
    let e = tools::call_check(&t.st, &t.lead, &run, "merge_pull_request", json!({"task": "KADE-1"})).await.unwrap_err();
    assert!(e.contains("still running") && e.contains("Your next board check offers it again"), "{e}");
    assert!(!seen_keys(&run).contains(&key), "unseen: {:?}", seen_keys(&run));
    assert_eq!(t.task().state_name, "Review");
    // a failed check: refused, and the finding stays seen (it goes to you in a chat)
    t.view(|v| v["statusCheckRollup"][0] = job("ubuntu-24.04", "COMPLETED", json!("FAILURE")));
    let run2 = core_board::create_run(&t.st.db, &t.lead, "S2", "/tmp", "/tmp/c2.jsonl", &core_board::seen_of(&all)).unwrap();
    let e = tools::call_check(&t.st, &t.lead, &run2, "merge_pull_request", json!({"task": "KADE-1"})).await.unwrap_err();
    assert!(e.contains("failed") && !e.contains("offers it again"), "{e}");
    assert!(seen_keys(&run2).contains(&key));
    // outside a check a wait unsees nothing
    t.view(|v| v["statusCheckRollup"][0] = job("ubuntu-24.04", "IN_PROGRESS", Value::Null));
    let e = t.merge().await.unwrap_err();
    assert!(!e.contains("offers it again"), "{e}");
    assert!(!t.gh_calls().iter().any(|c| c.starts_with("pr merge")));
}

#[tokio::test]
async fn the_board_offers_no_merge_when_a_rule_of_its_own_fails() {
    let t = setup().await;
    assert_eq!(t.may_merge_findings(), ["KADE-1"]);
    t.set_switch(false);
    assert!(t.may_merge_findings().is_empty(), "switch off");
    t.set_switch(true);
    tasks::update(&t.st.db, &t.st.you_id, &t.task, TaskPatch { testing: Some(false), ..Default::default() }).unwrap();
    assert!(t.may_merge_findings().is_empty(), "testing off");
    tasks::update(&t.st.db, &t.st.you_id, &t.task, TaskPatch { testing: Some(true), ..Default::default() }).unwrap();
    let head = t.head.clone();
    t.qa_run("qa_fail", &head);
    assert!(t.may_merge_findings().is_empty(), "latest QA verdict qa_fail");
    t.qa_run("qa_pass", &head);
    assert_eq!(t.may_merge_findings(), ["KADE-1"]);
}

// ---- the switch: off by default, only a person sets it ----

#[tokio::test]
async fn the_switch_is_off_by_default_and_only_a_person_sets_it_update_project_cannot() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    let lead = team::add_agent(&st.db, &st.you_id, &team_id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(),
        chat_enabled: Some(true), ..Default::default() }).unwrap();
    let id = projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    assert!(!projects::get(&st.db, &id).unwrap().lead_may_merge, "off by default");
    let get = tools::call(&st, &lead, "get_project", json!({"project": "KADE"})).await.unwrap();
    assert_eq!(get["project"]["team_lead_may_merge"], false, "{get}");
    // the Team Lead's tools can't switch it, under any name, on or off
    for (k, v) in [("lead_may_merge", json!(true)), ("team_lead_may_merge", json!(true)), ("leadMayMerge", json!(true)), ("may_merge", json!(true)),
                   ("lead_may_merge", json!(false))] {
        let e = tools::call(&st, &lead, "update_project", json!({"project": "KADE", k: v})).await.unwrap_err();
        assert!(e.contains("Team Lead may merge can't be switched from chat") && e.contains("Nothing changed"), "{k}: {e}");
        let e = tools::call(&st, &lead, "create_project", json!({"name": "Shop", "key": "SHOP", k: v})).await.unwrap_err();
        assert!(e.contains("Team Lead may merge can't be switched from chat"), "{k}: {e}");
    }
    assert!(!projects::get(&st.db, &id).unwrap().lead_may_merge);
    assert!(projects::list(&st.db).unwrap().iter().all(|p| p.key != "SHOP"), "no project made");
    // nor can an agent through Gizai's core
    let e = projects::update(&st.db, &lead, &id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), lead_may_merge: Some(true),
        ..Default::default() }).unwrap_err().to_string();
    assert!(e.contains("switched only by a person"), "{e}");
    let e = projects::create(&st.db, &lead, ProjectInput { name: "Shop".into(), key: "SHOP".into(), lead_may_merge: Some(true), ..Default::default() })
        .unwrap_err().to_string();
    assert!(e.contains("switched only by a person"), "{e}");
    assert!(!projects::get(&st.db, &id).unwrap().lead_may_merge);
    // you switch it on in the app
    projects::update(&st.db, &st.you_id, &id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), lead_may_merge: Some(true),
        ..Default::default() }).unwrap();
    assert!(projects::get(&st.db, &id).unwrap().lead_may_merge);
    // the Team Lead's other changes keep it as it is, and can't switch it off either
    tools::call(&st, &lead, "update_project", json!({"project": "KADE", "goal_md": "Ship the portal"})).await.unwrap();
    let p = projects::get(&st.db, &id).unwrap();
    assert_eq!((p.lead_may_merge, p.goal_md.as_deref()), (true, Some("Ship the portal")));
    let e = tools::call(&st, &lead, "update_project", json!({"project": "KADE", "lead_may_merge": false})).await.unwrap_err();
    assert!(e.contains("can't be switched from chat"), "{e}");
    assert!(projects::get(&st.db, &id).unwrap().lead_may_merge);
    let get = tools::call(&st, &lead, "get_project", json!({"project": "KADE"})).await.unwrap();
    assert_eq!(get["project"]["team_lead_may_merge"], true, "{get}");
    // and you switch it off again
    projects::update(&st.db, &st.you_id, &id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), lead_may_merge: Some(false),
        ..Default::default() }).unwrap();
    assert!(!projects::get(&st.db, &id).unwrap().lead_may_merge);
}

#[test]
fn the_lead_role_template_and_the_tool_describe_the_rules() {
    let lead = gizai_core::seed::role_template("lead");
    for part in ["merge_pull_request", "Team Lead may merge", "In a board check, merge each card in Review that it allows",
                 "Never work around a refusal", "Releases and deploys stay the user's"] {
        assert!(lead.contains(part), "{part:?} in the lead's template");
    }
    let tool = tools::catalog().into_iter().find(|d| d.name == "merge_pull_request").expect("the tool is in the catalog");
    assert!(!tool.read_only);
    for part in ["merge commit", "Review with testing on", "qa_pass on exactly the pull request's latest commit", "every check on it succeeded",
                 "no release card of the project is in Deploy", "GitHub only", "never work around a refusal"] {
        assert!(tool.description.contains(part), "{part:?} in: {}", tool.description);
    }
    let docs = include_str!("../../docs/agent-tools.md");
    for part in ["## The Team Lead merges pull requests", "merge_pull_request", "Team Lead may merge", "--merge --match-head-commit",
                 "A Bitbucket project is refused", "No release is under way", "Not after an outside tool"] {
        assert!(docs.contains(part), "{part:?} in docs/agent-tools.md");
    }
}

/// A temp folder by its real path, the way git and Gizai report it: on macOS /var/folders is /private/var/folders.
fn real_tempdir() -> tempfile::TempDir {
    let base = std::env::temp_dir().canonicalize().unwrap();
    tempfile::tempdir_in(base).unwrap()
}
