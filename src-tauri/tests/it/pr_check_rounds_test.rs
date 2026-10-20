//! GA-214: the PR check asks git first, and GitHub only when something moved there, with one light gh call per
//! repository. "GitHub" is local bare repositories under tmp/github/acme (each project's clone sends
//! https://github.com/acme/ there and allows no transport but local files) and gh a fake that answers per repository,
//! so nothing here reaches the real GitHub. A round's time is set (`check_all_at`), so "ten minutes later" takes no time.
// Linux and macOS only: these tests run shell scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use gizai_lib::pulls::{self, PullInfo};
use serde_json::{Value, json};

const ID: &[&str] = &["-c", "user.email=t@t", "-c", "user.name=t", "-c", "commit.gpgsign=false"];
/// The light fields (no `commits`).
const FIELDS: &str = "number,url,state,isDraft,headRefName,headRefOid,mergeCommit";
const MIN: Duration = Duration::from_secs(60);

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(ID).args(args).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

fn has_branch(repo: &Path, branch: &str) -> bool {
    Command::new("git").args(["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).current_dir(repo).output().unwrap().status.success()
}

/// Writes an executable script through a child `sh`, so this process never holds it open for writing (ETXTBSY).
fn write_script(path: &Path, script: &str) {
    use std::io::Write;
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path)
        .stdin(std::process::Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

/// A fake gh: each call's arguments go to dir/gh-args. `pr list --repo <owner/name> --head <branch>` answers with
/// dir/head-<branch>.json, `pr list --repo <owner/name>` (a repository's newest) with dir/list-<owner_name>.json (no file:
/// no pull requests), `pr view <n>` with dir/view-<n>.json ("/" is "_" in the names).
fn fake_gh(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let gh = dir.join("gh");
    write_script(&gh, &format!(r#"#!/bin/sh
d='{d}'
echo "$*" >> "$d/gh-args"
case "$1 $2" in
  "pr list")
    case " $* " in
      *" --head "*) f="$d/head-$(printf '%s' "$6" | tr / _).json" ;;
      *) f="$d/list-$(printf '%s' "$4" | tr / _).json" ;;
    esac
    if [ -f "$f" ]; then cat "$f"; fi; exit 0 ;;
  "pr view") cat "$d/view-$3.json"; exit 0 ;;
esac
echo "unknown command: $*" >&2; exit 1
"#, d = dir.display()));
    gh
}

/// A project's repository: "GitHub"'s bare copy and the project's local clone.
struct Repo { name: String, bare: PathBuf, local: PathBuf, project: String }

impl Repo {
    fn full(&self) -> String { format!("acme/{}", self.name) }
    fn link(&self) -> String { format!("https://github.com/acme/{}", self.name) }
    fn url(&self, number: u64) -> String { format!("{}/pull/{number}", self.link()) }
    /// gh's repository-wide call for it.
    fn listed(&self) -> String { format!("pr list --repo {} --state all --limit 100 --json {FIELDS}", self.full()) }
    /// A pull request as gh lists it with the light fields.
    fn pr(&self, number: u64, state: &str, draft: bool, branch: &str, head: &str) -> Value {
        json!({"number": number, "url": self.url(number), "state": state, "isDraft": draft, "headRefName": branch, "headRefOid": head,
               "mergeCommit": null})
    }
    /// GitHub out of reach: the clone no longer sends https://github.com/acme/ to the bare repositories, and allows no
    /// transport but local files, so git fails at once without the network.
    fn offline(&self, tmp: &Path) {
        git(&self.local, &["config", "--remove-section", &format!("url.{}/", tmp.join("github/acme").display())]);
    }
    fn online(&self, tmp: &Path) {
        git(&self.local, &["config", &format!("url.{}/.insteadOf", tmp.join("github/acme").display()), "https://github.com/acme/"]);
    }
    /// A commit on GitHub's main, as a merge there makes it.
    fn main_moves_on(&self, tmp: &Path, message: &str) -> String {
        let other = tmp.join(format!("other-{}-{}", self.name, message.len()));
        git(tmp, &["clone", "-q", self.bare.to_str().unwrap(), other.to_str().unwrap()]);
        git(&other, &["commit", "-q", "--allow-empty", "-m", message]);
        git(&other, &["push", "-q", "origin", "main"]);
        git(&other, &["rev-parse", "HEAD"])
    }
}

/// A new commit on `branch` of `repo` (a local clone or a bare repository), on top of `from`; returns it.
fn commit_on(repo: &Path, branch: &str, from: &str, message: &str) -> String {
    let tree = git(repo, &["rev-parse", &format!("{from}^{{tree}}")]);
    let sha = git(repo, &["commit-tree", &tree, "-p", from, "-m", message]);
    git(repo, &["update-ref", &format!("refs/heads/{branch}"), &sha]);
    sha
}

struct Card { task: String, branch: String }

struct Hub { st: gizai_lib::AppState, gh: PathBuf, tmp: tempfile::TempDir }

impl Hub {
    fn new() -> Hub {
        let tmp = real_tempdir();
        let st = gizai_lib::test_state(tmp.path());
        let gh = tmp.path().join("gh");
        gizai_core::settings::set(&st.db, "gh_bin", &fake_gh(&gh).to_string_lossy().to_string()).unwrap();
        // nothing starts on a card that moves
        gizai_core::settings::set(&st.db, "agents_paused", &true).unwrap();
        Hub { st, gh, tmp }
    }
    fn path(&self) -> &Path { self.tmp.path() }

    /// Project `key`, linked to https://github.com/acme/<name>, with "GitHub"'s bare copy and its local clone.
    fn repo(&self, name: &str, key: &str) -> Repo {
        let tmp = self.path();
        let src = tmp.join(format!("src-{name}"));
        std::fs::create_dir(&src).unwrap();
        git(&src, &["init", "-q", "-b", "main"]);
        git(&src, &["commit", "-q", "--allow-empty", "-m", &format!("init {name}")]);
        let bare = tmp.join(format!("github/acme/{name}"));
        git(tmp, &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
        let local = tmp.join(format!("local-{name}"));
        let rewrite = format!("url.{}/.insteadOf=https://github.com/acme/", tmp.join("github/acme").display());
        git(tmp, &["clone", "-q", "-c", &rewrite, "-c", "protocol.allow=never", "-c", "protocol.file.allow=always",
                   &format!("https://github.com/acme/{name}"), local.to_str().unwrap()]);
        let project = gizai_core::projects::create(&self.st.db, &self.st.you_id, gizai_core::model::ProjectInput { name: name.into(), key: key.into(),
            repo_path: Some(local.to_string_lossy().into()), repo_url: Some(format!("https://github.com/acme/{name}")),
            default_branch: Some("main".into()), ..Default::default() }).unwrap();
        Repo { name: name.into(), bare, local, project }
    }

    fn state_id(&self, category: &str) -> String {
        let team = gizai_core::team::get(&self.st.db, &gizai_core::team::list(&self.st.db).unwrap()[0].id).unwrap();
        team.states.iter().find(|s| s.category == category).unwrap().id.clone()
    }

    /// A card of `repo` in Review with branch `branch` (an agent's run made it). With `pushed`, the branch has a commit
    /// of its own, pushed to GitHub.
    fn card(&self, repo: &Repo, branch: &str, pushed: bool) -> Card {
        let task = gizai_core::tasks::create(&self.st.db, &self.st.you_id, gizai_core::model::TaskInput { project_id: repo.project.clone(),
            title: format!("Work on {branch}"), state_id: Some(self.state_id("review")), ..Default::default() }).unwrap();
        self.st.db.write(None, |w| { w.conn().execute("UPDATE tasks SET branch=?2 WHERE id=?1", [task.as_str(), branch])?; Ok(()) }).unwrap();
        if pushed {
            commit_on(&repo.local, branch, "main", &format!("first commit of {branch}"));
            git(&repo.local, &["push", "-q", "origin", branch]);
        }
        Card { task, branch: branch.into() }
    }

    /// The card follows pull request `url` (as Open pull request or an earlier check left it).
    fn follows(&self, card: &Card, url: &str, state: &str) {
        gizai_core::pulls::record(&self.st.db, None, &card.task, url, state, false).unwrap();
    }

    /// gh's answer for the newest pull requests of `repo`.
    fn list(&self, repo: &Repo, prs: Vec<Value>) {
        std::fs::write(self.gh.join(format!("list-acme_{}.json", repo.name)), Value::Array(prs).to_string()).unwrap();
    }
    /// gh's answer for the pull requests of one branch.
    fn head(&self, branch: &str, prs: Vec<Value>) {
        std::fs::write(self.gh.join(format!("head-{}.json", branch.replace('/', "_"))), Value::Array(prs).to_string()).unwrap();
    }
    /// gh's answer for the commits of pull request `number`.
    fn view(&self, number: u64, commits: &[&str]) {
        let commits: Vec<Value> = commits.iter().map(|c| json!({"oid": c, "authors": [{"login": "x"}]})).collect();
        std::fs::write(self.gh.join(format!("view-{number}.json")), json!({"commits": commits}).to_string()).unwrap();
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.gh.join("gh-args")).unwrap_or_default().lines().map(String::from).collect()
    }
    /// gh's calls in the round at `at`.
    async fn round(&self, at: Instant) -> (Vec<pulls::Checked>, Vec<String>) {
        let before = self.calls().len();
        let checked = pulls::check_all_at(&self.st, at).await;
        (checked, self.calls()[before..].to_vec())
    }
    fn task(&self, card: &Card) -> gizai_core::model::Task { gizai_core::tasks::get(&self.st.db, &card.task).unwrap() }
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

/// No gh call asks for commits, but one `pr view <n> --json commits`.
fn light(calls: &[String]) {
    for c in calls {
        let view = c.starts_with("pr view ") && c.ends_with(" --json commits");
        assert!(view || !c.contains("commits"), "a call that asks for commits: {c}");
    }
}

#[tokio::test]
async fn six_cards_in_two_repositories_cost_one_light_call_per_repository_and_nothing_when_git_shows_nothing_moved() {
    let h = Hub::new();
    let (alpha, beta) = (h.repo("alpha", "ALP"), h.repo("beta", "BET"));
    let a: Vec<Card> = (1..=3).map(|n| h.card(&alpha, &format!("gizai/alp-{n}-x"), true)).collect();
    let b: Vec<Card> = (1..=3).map(|n| h.card(&beta, &format!("gizai/bet-{n}-y"), true)).collect();
    // alpha: an open pull request from ALP-1, a draft from ALP-2, none from ALP-3, and one from a branch no card has
    let tip = |r: &Repo, c: &Card| git(&r.local, &["rev-parse", &c.branch]);
    h.list(&alpha, vec![alpha.pr(14, "OPEN", false, "someone/else", "abc1234"), alpha.pr(12, "OPEN", true, &a[1].branch, &tip(&alpha, &a[1])),
                        alpha.pr(11, "OPEN", false, &a[0].branch, &tip(&alpha, &a[0]))]);
    h.list(&beta, vec![beta.pr(3, "OPEN", false, &b[2].branch, &tip(&beta, &b[2]))]);

    // the first round since Gizai started asks GitHub about both, once each
    let t0 = Instant::now();
    let (checked, calls) = h.round(t0).await;
    assert_eq!(sorted(calls.clone()), [alpha.listed(), beta.listed()], "{calls:?}");
    assert_eq!(checked.len(), 6);
    // each card takes the pull requests from its own branch
    let shown = |c: &Card| { let t = h.task(c); (t.pr_url, t.pr_state) };
    assert_eq!(shown(&a[0]), (Some(alpha.url(11)), Some("open".into())));
    assert_eq!(shown(&a[1]), (Some(alpha.url(12)), Some("draft".into())));
    assert_eq!(shown(&a[2]), (None, None), "no pull request from its branch; #14 isn't its");
    assert_eq!(shown(&b[0]), (None, None));
    assert_eq!(shown(&b[2]), (Some(beta.url(3)), Some("open".into())));

    // both change in git (a push to ALP-1's branch, a pull request opened in beta): two light calls, one each
    commit_on(&alpha.local, &a[0].branch, &a[0].branch, "a fix");
    git(&alpha.local, &["push", "-q", "origin", &a[0].branch]);
    git(&beta.bare, &["update-ref", "refs/pull/4/head", &tip(&beta, &b[0])]);
    h.list(&beta, vec![beta.pr(4, "OPEN", false, &b[0].branch, &tip(&beta, &b[0])), beta.pr(3, "OPEN", false, &b[2].branch, &tip(&beta, &b[2]))]);
    let (checked, calls) = h.round(t0 + 2 * MIN).await;
    assert_eq!(sorted(calls.clone()), [alpha.listed(), beta.listed()], "one `pr list --repo <r> --state all --limit 100` each: {calls:?}");
    assert_eq!(checked.len(), 6);
    assert_eq!(shown(&b[0]), (Some(beta.url(4)), Some("open".into())), "the pull request an agent opened shows within the round");

    // nothing changed in git: no gh at all, round after round
    for minutes in [4, 6, 8, 11] {
        let (checked, calls) = h.round(t0 + minutes * MIN).await;
        assert!(calls.is_empty() && checked.is_empty(), "{minutes} min: {calls:?}");
    }
    // closing a pull request moves no ref: it shows once the repository's last call is 10 minutes old
    h.list(&alpha, vec![alpha.pr(12, "CLOSED", false, &a[1].branch, &tip(&alpha, &a[1])), alpha.pr(11, "OPEN", false, &a[0].branch, &tip(&alpha, &a[0]))]);
    assert_eq!(shown(&a[1]).1.as_deref(), Some("draft"), "not seen yet");
    let (_, calls) = h.round(t0 + 12 * MIN).await;
    assert_eq!(sorted(calls), [alpha.listed(), beta.listed()], "10 minutes after each repository's last call");
    assert_eq!(shown(&a[1]).1.as_deref(), Some("closed"));
    let (_, calls) = h.round(t0 + 14 * MIN).await;
    assert!(calls.is_empty(), "{calls:?}");
    light(&h.calls());
    assert!(!h.calls().iter().any(|c| c.contains("--head")), "every card was in its repository's list: {:?}", h.calls());
}

#[tokio::test]
async fn a_push_a_new_pull_request_ref_a_merge_or_a_new_card_makes_the_next_round_ask_about_that_repository_only() {
    let h = Hub::new();
    let (alpha, beta) = (h.repo("alpha", "ALP"), h.repo("beta", "BET"));
    let a1 = h.card(&alpha, "gizai/alp-1-x", true);
    let b1 = h.card(&beta, "gizai/bet-1-y", true);
    let t0 = Instant::now();
    let (_, calls) = h.round(t0).await;
    assert_eq!(calls.len(), 2);
    let (_, calls) = h.round(t0 + 2 * MIN).await;
    assert!(calls.is_empty(), "{calls:?}");

    // an agent pushed to ALP-1's branch
    commit_on(&alpha.local, &a1.branch, &a1.branch, "pushed by an agent");
    git(&alpha.local, &["push", "-q", "origin", &a1.branch]);
    let (_, calls) = h.round(t0 + 4 * MIN).await;
    assert_eq!(calls, [alpha.listed()], "a push");
    assert!(h.round(t0 + 6 * MIN).await.1.is_empty());

    // a pull request opened in beta (GitHub's refs/pull/<n>/head), and it shows on its card in that round
    let tip = git(&beta.local, &["rev-parse", &b1.branch]);
    git(&beta.bare, &["update-ref", "refs/pull/9/head", &tip]);
    h.list(&beta, vec![beta.pr(9, "OPEN", false, &b1.branch, &tip)]);
    let (_, calls) = h.round(t0 + 8 * MIN).await;
    assert_eq!(calls, [beta.listed()], "a new pull request ref");
    assert_eq!(h.task(&b1).pr_url, Some(beta.url(9)));
    assert!(h.round(t0 + 9 * MIN).await.1.is_empty());

    // a new card in Review in alpha, whose branch isn't on GitHub yet: git shows nothing new, the set of cards does
    let a2 = h.card(&alpha, "gizai/alp-2-z", false);
    let (checked, calls) = h.round(t0 + 10 * MIN).await;
    assert_eq!(calls, [alpha.listed()], "a new followed card");
    assert_eq!(checked.iter().map(|c| c.task_id.as_str()).collect::<Vec<_>>(), [a1.task.as_str(), a2.task.as_str()]);
    // beta's last call was at 8 minutes: 10 minutes later it is asked again, though nothing moved
    assert!(h.round(t0 + 13 * MIN).await.1.is_empty());
    assert_eq!(h.round(t0 + 18 * MIN).await.1, [beta.listed()]);

    // a merge on GitHub moves alpha's main
    alpha.main_moves_on(h.path(), "Merge pull request #5");
    let (_, calls) = h.round(t0 + 19 * MIN).await;
    assert_eq!(calls, [alpha.listed()], "main moved");
    assert!(h.round(t0 + 20 * MIN).await.1.is_empty());
    light(&h.calls());
}

#[tokio::test]
async fn a_merge_on_main_makes_the_next_round_ask_and_the_merged_card_moves_to_deploy_without_its_branch() {
    let h = Hub::new();
    let alpha = h.repo("alpha", "ALP");
    let a1 = h.card(&alpha, "gizai/alp-1-x", true);
    let a2 = h.card(&alpha, "gizai/alp-2-x", true);
    let tip = git(&alpha.local, &["rev-parse", &a1.branch]);
    h.follows(&a1, &alpha.url(7), "open");
    h.list(&alpha, vec![alpha.pr(7, "OPEN", false, &a1.branch, &tip)]);
    let t0 = Instant::now();
    h.round(t0).await;
    // merged on GitHub: gh says so, but git shows nothing moved yet, so the round doesn't ask
    h.list(&alpha, vec![alpha.pr(7, "MERGED", false, &a1.branch, &tip)]);
    let (checked, calls) = h.round(t0 + 2 * MIN).await;
    assert!(calls.is_empty() && checked.is_empty(), "{calls:?}");
    // the merge commit on GitHub's main: the next round asks, and the card moves on and loses its branch
    let other = h.path().join("merger");
    git(h.path(), &["clone", "-q", alpha.bare.to_str().unwrap(), other.to_str().unwrap()]);
    git(&other, &["merge", "-q", "--no-ff", &format!("origin/{}", a1.branch), "-m", "Merge pull request #7"]);
    git(&other, &["push", "-q", "origin", "main"]);
    let (checked, calls) = h.round(t0 + 4 * MIN).await;
    assert_eq!(calls, [alpha.listed()]);
    let merged = checked.iter().find(|c| c.task_id == a1.task).unwrap();
    let said = format!("deleted branch {} after the merge", a1.branch);
    assert_eq!((merged.moved_to.as_deref(), merged.pull.clone()),
               (Some("Deploy"), Some(PullInfo { url: alpha.url(7), number: Some(7), state: "merged".into(), note: Some(said) })));
    let t = h.task(&a1);
    assert_eq!((t.state_name.as_str(), t.pr_state.as_deref()), ("Deploy", Some("merged")));
    assert!(!has_branch(&alpha.local, &a1.branch), "its local branch is deleted");
    assert!(has_branch(&alpha.bare, &a1.branch), "GitHub's copy is GitHub's");
    assert_eq!(h.task(&a2).state_name, "Review", "the other card stays");
    // Deploy: no longer followed. The set of cards in alpha changed (git no longer asks about its branch), so the next
    // round asks once more, for the card left; then nothing
    let (checked, calls) = h.round(t0 + 6 * MIN).await;
    assert_eq!(calls, [alpha.listed()]);
    assert_eq!(checked.iter().map(|c| c.task_id.as_str()).collect::<Vec<_>>(), [a2.task.as_str()]);
    assert!(h.round(t0 + 8 * MIN).await.1.is_empty());
    light(&h.calls());
}

#[tokio::test]
async fn when_git_cant_ask_the_round_asks_github_with_one_light_call_and_the_log_says_why_once() {
    let h = Hub::new();
    let alpha = h.repo("alpha", "ALP");
    let a1 = h.card(&alpha, "gizai/alp-1-x", true);
    h.card(&alpha, "gizai/alp-2-x", true);
    let tip = git(&alpha.local, &["rev-parse", &a1.branch]);
    h.list(&alpha, vec![alpha.pr(2, "OPEN", false, &a1.branch, &tip)]);
    alpha.offline(h.path());
    let t0 = Instant::now();
    for (round, minutes) in [0, 2, 4].into_iter().enumerate() {
        let (checked, calls) = h.round(t0 + minutes * MIN).await;
        assert_eq!(calls, [alpha.listed()], "round {round}: one light call for the repository, every round");
        assert_eq!(checked.len(), 2);
        let said: Vec<String> = pulls::problems(&h.st).into_iter().filter(|p| p.contains("ask git")).collect();
        assert_eq!(said.len(), 1, "said once: {said:?}");
        assert!(said[0].starts_with("the pull request check can't ask git about acme/alpha, so it asks GitHub every round: "), "{}", said[0]);
    }
    assert_eq!(h.task(&a1).pr_url, Some(alpha.url(2)));
    // git can answer again: the problem goes, the next round asks (it has no git answer to compare with), then none
    alpha.online(h.path());
    let (_, calls) = h.round(t0 + 6 * MIN).await;
    assert_eq!(calls, [alpha.listed()]);
    assert!(!pulls::problems(&h.st).iter().any(|p| p.contains("ask git")), "{:?}", pulls::problems(&h.st));
    assert!(h.round(t0 + 8 * MIN).await.1.is_empty());
    light(&h.calls());
}

#[tokio::test]
async fn a_card_whose_pull_request_isnt_among_the_newest_gets_its_own_light_call() {
    let h = Hub::new();
    let alpha = h.repo("alpha", "ALP");
    let old = h.card(&alpha, "gizai/alp-1-old", true);
    let new = h.card(&alpha, "gizai/alp-2-new", true);
    let fresh = h.card(&alpha, "gizai/alp-3-fresh", true);
    let tip = |c: &Card| git(&alpha.local, &["rev-parse", &c.branch]);
    // ALP-1 follows #3, older than the newest 100; ALP-2 follows #150, which is among them; ALP-3 follows nothing
    h.follows(&old, &alpha.url(3), "open");
    h.follows(&new, &alpha.url(150), "open");
    let mut newest: Vec<Value> = (51..=150).rev().map(|n| alpha.pr(n, "CLOSED", false, &format!("someone/b{n}"), "abc1234")).collect();
    newest[0] = alpha.pr(150, "OPEN", false, &new.branch, &tip(&new));
    h.list(&alpha, newest);
    h.head(&old.branch, vec![alpha.pr(3, "CLOSED", false, &old.branch, &tip(&old))]);
    let (checked, calls) = h.round(Instant::now()).await;
    assert_eq!(calls, [alpha.listed(), format!("pr list --repo acme/alpha --head {} --state all --limit 20 --json {FIELDS}", old.branch)],
               "one own call, for ALP-1 only, with the light fields");
    assert_eq!(checked.len(), 3);
    assert_eq!((h.task(&old).pr_url, h.task(&old).pr_state), (Some(alpha.url(3)), Some("closed".into())), "found by its own call");
    assert_eq!(h.task(&new).pr_state.as_deref(), Some("open"));
    assert_eq!(h.task(&fresh).pr_url, None);
    light(&h.calls());
}

#[tokio::test]
async fn a_merged_pull_request_has_the_branchs_tip_by_its_head_by_git_or_by_one_pr_view_of_its_commits() {
    let h = Hub::new();
    let alpha = h.repo("alpha", "ALP");
    // (1) its head is the branch's tip
    let same = h.card(&alpha, "gizai/alp-1-same", true);
    let same_tip = git(&alpha.local, &["rev-parse", &same.branch]);
    // (2) its head came after the tip (pushed on GitHub, and this repository fetched it): git sees the tip in it
    let later = h.card(&alpha, "gizai/alp-2-later", true);
    let later_tip = git(&alpha.local, &["rev-parse", &later.branch]);
    let tree = git(&alpha.local, &["rev-parse", &format!("{later_tip}^{{tree}}")]);
    let later_head = git(&alpha.local, &["commit-tree", &tree, "-p", &later_tip, "-m", "pushed on GitHub"]);
    git(&alpha.local, &["push", "-q", "origin", &format!("{later_head}:refs/heads/{}", later.branch)]);
    git(&alpha.local, &["fetch", "-q", "origin"]);
    // (3) the tip came after the pull request's head (a commit made here after it was merged): the card follows #7
    let ahead = h.card(&alpha, "gizai/alp-3-ahead", true);
    let ahead_head = git(&alpha.local, &["rev-parse", &ahead.branch]);
    let ahead_tip = commit_on(&alpha.local, &ahead.branch, &ahead.branch, "not in the pull request");
    h.follows(&ahead, &alpha.url(7), "open");
    // (4) its head is on GitHub only (pushed there, not fetched): one pr view of its commits, which have the tip
    let remote = h.card(&alpha, "gizai/alp-4-remote", true);
    let remote_tip = git(&alpha.local, &["rev-parse", &remote.branch]);
    let remote_head = commit_on(&alpha.bare, &remote.branch, &remote_tip, "pushed on GitHub, not fetched");
    h.view(8, &[&remote_tip, &remote_head]);
    // (5) an older merged pull request from the branch, its head on GitHub only, without the tip: the card stays
    let older = h.card(&alpha, "gizai/alp-5-older", true);
    let older_tip = git(&alpha.local, &["rev-parse", &older.branch]);
    let older_head = commit_on(&alpha.bare, "elsewhere", "main", "an older line of work");
    h.view(9, &[&older_head]);
    assert!(lacks(&alpha.local, &remote_head) && lacks(&alpha.local, &older_head));
    h.list(&alpha, vec![
        alpha.pr(9, "MERGED", false, &older.branch, &older_head),
        alpha.pr(8, "MERGED", false, &remote.branch, &remote_head),
        alpha.pr(7, "MERGED", false, &ahead.branch, &ahead_head),
        alpha.pr(6, "MERGED", false, &later.branch, &later_head),
        alpha.pr(5, "MERGED", false, &same.branch, &same_tip),
    ]);

    let (checked, calls) = h.round(Instant::now()).await;
    let note = |c: &Card| checked.iter().find(|x| x.task_id == c.task).and_then(|x| x.pull.as_ref()).and_then(|p| p.note.clone());
    let moved = |c: &Card| checked.iter().find(|x| x.task_id == c.task).and_then(|x| x.moved_to.clone());
    for (c, tip) in [(&same, &same_tip), (&later, &later_tip), (&remote, &remote_tip)] {
        assert_eq!(moved(c).as_deref(), Some("Deploy"), "{}", c.branch);
        assert_eq!(note(c), Some(format!("deleted branch {} after the merge", c.branch)));
        assert!(!has_branch(&alpha.local, &c.branch), "{}: the pull request has its tip {tip}", c.branch);
    }
    assert_eq!(moved(&ahead).as_deref(), Some("Deploy"), "the card's own pull request");
    assert_eq!(note(&ahead), Some(format!("kept branch {}, which has commits the pull request doesn't after the merge", ahead.branch)));
    assert_eq!(git(&alpha.local, &["rev-parse", &ahead.branch]), ahead_tip, "nothing lost");
    assert_eq!(moved(&older), None);
    let t = h.task(&older);
    assert_eq!((t.state_name.as_str(), t.pr_state.as_deref()), ("Review", Some("merged")), "an older merge: it stays for your review");
    assert_eq!(git(&alpha.local, &["rev-parse", &older.branch]), older_tip);
    // gh: the repository's list, and one pr view for each merged pull request whose head this repository doesn't have
    assert_eq!(calls, [alpha.listed(), "pr view 8 --repo acme/alpha --json commits".to_string(), "pr view 9 --repo acme/alpha --json commits".to_string()]);
    // the card that stayed doesn't get its commits asked for again
    let (_, calls) = h.round(Instant::now() + 12 * MIN).await;
    assert_eq!(calls, [alpha.listed()]);
    light(&h.calls());
}

/// Whether `repo` doesn't have commit `sha`.
fn lacks(repo: &Path, sha: &str) -> bool {
    !Command::new("git").args(["cat-file", "-e", &format!("{sha}^{{commit}}")]).current_dir(repo).output().unwrap().status.success()
}

#[tokio::test]
async fn check_now_asks_github_at_once_with_one_light_call_even_when_git_shows_nothing_moved() {
    let h = Hub::new();
    let alpha = h.repo("alpha", "ALP");
    let a1 = h.card(&alpha, "gizai/alp-1-x", true);
    let tip = git(&alpha.local, &["rev-parse", &a1.branch]);
    let t0 = Instant::now();
    h.round(t0).await;
    assert!(h.round(t0 + 2 * MIN).await.1.is_empty(), "nothing moved");
    // an agent opened a pull request (Check now runs when you open the card, and when a run moves it to Review)
    h.head(&a1.branch, vec![alpha.pr(21, "OPEN", true, &a1.branch, &tip)]);
    let before = h.calls().len();
    let got = pulls::check(&h.st, &a1.task).await.unwrap();
    assert_eq!(got, Some(PullInfo { url: alpha.url(21), number: Some(21), state: "draft".into(), note: None }));
    assert_eq!(h.calls()[before..], [format!("pr list --repo acme/alpha --head {} --state all --limit 20 --json {FIELDS}", a1.branch)]);
    // also with git out of reach: Check now doesn't ask git
    alpha.offline(h.path());
    let before = h.calls().len();
    pulls::check(&h.st, &a1.task).await.unwrap();
    assert_eq!(h.calls().len(), before + 1);
    assert!(!pulls::problems(&h.st).iter().any(|p| p.contains("ask git")), "{:?}", pulls::problems(&h.st));
    light(&h.calls());
}

/// A temp folder by its real path, the way git and Gizai report it (on macOS /var/folders is /private/var/folders).
fn real_tempdir() -> tempfile::TempDir {
    let base = std::env::temp_dir().canonicalize().unwrap();
    tempfile::tempdir_in(base).unwrap()
}
