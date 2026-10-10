//! merge_pull_request (GA-86): the Team Lead merges a card's own pull request on GitHub, in a project that lets it (Team
//! Lead may merge, a person's switch), once QA passed exactly its latest commit and every check on it succeeded. Every
//! rule is checked here, in Gizai's code, before gh merges anything: a rule that fails refuses with its reason and
//! changes nothing. The merge is a merge commit (`gh pr merge --merge`), as the team merges, and only while the pull
//! request's latest commit is still the one QA passed. Then the card gets the Team Lead's comment, its activity and a
//! desktop notification say the Team Lead merged it, and Gizai's PR check moves it on (Deploy) and cleans up its
//! worktree, as when you merge. Releases and deploys stay yours.
use std::path::{Path, PathBuf};

use gizai_agents::github::{self, CheckState, PullDetails};
use gizai_core::model::{Project, Run, Task};
use gizai_core::{comments, projects, pulls, runs, tasks};
use serde_json::{Value, json};

use super::{Cx, err, resolve, short};
use crate::board::when;

/// How long after QA passed a card a board check offers it again when its merge had to wait (checks still running,
/// GitHub still working out whether it can be merged): each check after a wait costs a model run, so not for ever.
const OFFER_AGAIN_FOR_MS: i64 = 3 * 60 * 60_000;

/// Why a merge was refused, and whether it is a wait (it may work in a few minutes, with nothing changed by anyone).
struct Refused {
    why: String,
    wait: bool,
}

fn no(why: String) -> Refused {
    Refused { why, wait: false }
}

fn wait(why: String) -> Refused {
    Refused { why, wait: true }
}

/// What the merge works with once the card's own rules hold.
struct Ready {
    task: Task,
    project: Project,
    /// The QA run that passed it last.
    qa: Run,
    /// The commit that QA run ended at.
    qa_head: String,
    repo: String,
    branch: String,
    repo_path: PathBuf,
    default_branch: String,
}

pub(crate) async fn merge_pull_request(cx: &Cx<'_>, a: &super::Args) -> Result<Value, String> {
    let t = resolve::task(cx, &a.req("task")?)?;
    let you = you_name(cx);
    let ready = card_rules(cx, t, &you)?;
    let gh = {
        let st = cx.st.clone();
        tokio::task::spawn_blocking(move || crate::pulls::gh_bin(&st)).await.map_err(|e| e.to_string())??
    };
    let (gh2, repo, branch, dir) = (gh.clone(), ready.repo.clone(), ready.branch.clone(), ready.repo_path.clone());
    let found = tokio::task::spawn_blocking(move || open_pull(&gh2, &dir, &repo, &branch)).await.map_err(|e| e.to_string())?;
    let pr = match found {
        Ok(pr) => pr,
        Err(r) => return Err(refuse(cx, &ready, r)),
    };
    if let Err(r) = pull_rules(&ready, &pr, &you) {
        return Err(refuse(cx, &ready, r));
    }
    // The merge: a merge commit, and only while the pull request's latest commit is the one QA passed.
    let (gh2, repo, dir, number, head) = (gh.clone(), ready.repo.clone(), ready.repo_path.clone(), pr.number, pr.head_ref_oid.clone());
    let merged = tokio::task::spawn_blocking(move || github::merge_pull(&gh2, &dir, &repo, number, &head)).await.map_err(|e| e.to_string())?;
    if let Err(e) = merged {
        return Err(format!("GitHub didn't merge pull request #{number} of {}: {e}. Nothing changed in Gizai: tell {you} what GitHub said.",
                           ready.task.identifier));
    }
    after_merge(cx, &ready, &pr, &you).await
}

/// The user's name, for the reasons ("Jefsev merges it").
fn you_name(cx: &Cx) -> String {
    gizai_core::users::list(cx.db()).ok().and_then(|l| l.into_iter().find(|p| p.id == cx.st.you_id)).map(|p| p.name)
        .unwrap_or_else(|| "the user".into())
}

/// The rules Gizai checks without GitHub: the project's switch and link, the card's column, testing, hold and runs, a
/// release under way and the card's latest QA verdict.
fn card_rules(cx: &Cx, t: Task, you: &str) -> Result<Ready, String> {
    let id = &t.identifier;
    let project_id = t.project_id.clone().ok_or_else(|| format!("{id} has no project, so it has no pull request to merge. Nothing changed."))?;
    let p = projects::get(cx.db(), &project_id).map_err(err)?;
    if !p.lead_may_merge {
        return Err(format!("{} doesn't let you merge: its Team Lead may merge switch is off, and only {you} switches it on (the project page → \
                            Edit). {id} waits in Review for {you} to merge. Nothing changed.", p.key));
    }
    match super::provider(p.repo_url.as_deref()).as_deref() {
        Some("github") => {}
        Some("bitbucket") => return Err(format!("{} is on Bitbucket: merge_pull_request merges pull requests on GitHub only, for now. {id} waits \
                                                 in Review for {you} to merge. Nothing changed.", p.key)),
        _ => return Err(format!("{} isn't linked to a GitHub repository, so {id} has no pull request there to merge. Nothing changed.", p.key)),
    }
    if t.state_category != "review" {
        return Err(format!("{id} is in {}, not Review: only a card in Review that QA passed can be merged. Nothing changed.", t.state_name));
    }
    if !t.testing {
        return Err(format!("{id} has testing off: it went to Review without QA, so it is {you}'s to open and merge. Nothing changed."));
    }
    if let Some(hold) = &t.hold {
        let why = t.hold_reason.as_deref().map(str::trim).filter(|r| !r.is_empty()).unwrap_or("no reason given");
        return Err(format!("{id} is on hold ({hold}: {why}): it waits for a person, so it isn't merged. Nothing changed."));
    }
    if crate::runs::live(cx.st).iter().any(|r| r.task_id == t.id) {
        return Err(format!("An agent is working on {id} right now and may push to its branch: merge only after its run has ended. Nothing changed."));
    }
    if let Some((card, agent)) = pulls::release_under_way(cx.db(), &p.id).map_err(err)? {
        return Err(format!("A release is under way: {card} is in Deploy with {agent}. Main mustn't move under the commit it waits on, so {id} \
                            is merged after that release. Nothing changed."));
    }
    let qa = runs::list_for_task(cx.db(), &t.id).map_err(err)?.into_iter()
        .find(|r| matches!(r.outcome.as_deref(), Some("qa_pass" | "qa_fail")))
        .ok_or_else(|| format!("QA hasn't passed {id}: it has no QA verdict. Nothing changed."))?;
    if qa.outcome.as_deref() != Some("qa_pass") {
        return Err(format!("{id}'s latest QA verdict is qa_fail ({} on {}): QA has to pass it first. Nothing changed.", qa.agent_name,
                           qa.ended_at.map(when).unwrap_or_default()));
    }
    let qa_head = qa.head_sha.clone().filter(|h| !h.trim().is_empty())
        .ok_or_else(|| format!("Gizai didn't record which commit {}'s QA run on {id} ended at, so it can't tell that QA passed the pull \
                                request's commit. {id} waits in Review for {you} to merge. Nothing changed.", qa.agent_name))?;
    let card = pulls::card(cx.db(), &t.id).map_err(err)?;
    Ok(Ready { repo: card.repo, branch: card.branch, repo_path: PathBuf::from(card.repo_path), default_branch: card.default_branch,
               task: t, project: p, qa, qa_head })
}

/// The card's open pull request on GitHub (from its branch), as GitHub has it now.
fn open_pull(gh: &Path, dir: &Path, repo: &str, branch: &str) -> Result<PullDetails, Refused> {
    let prs = github::pulls_for_branch(gh, dir, repo, branch).map_err(|e| no(format!("gh couldn't list the pull requests of {branch}: {e}")))?;
    let open = prs.iter().find(|p| p.state == "OPEN")
        .ok_or_else(|| no(format!("there is no open pull request from {branch} on GitHub")))?;
    github::pull_details(gh, dir, repo, open.number).map_err(|e| no(format!("gh couldn't read pull request #{}: {e}", open.number)))
}

/// The rules on the pull request: the card's own, open, into the project's main branch, QA passed its latest commit,
/// GitHub can merge it, and every check on it succeeded.
fn pull_rules(r: &Ready, pr: &PullDetails, you: &str) -> Result<(), Refused> {
    let n = pr.number;
    if pr.state != "OPEN" {
        return Err(no(format!("pull request #{n} is {}", pr.state.to_lowercase())));
    }
    if pr.is_cross_repository || pr.head_ref_name != r.branch {
        return Err(no(format!("pull request #{n} isn't from the card's own branch {} in this repository", r.branch)));
    }
    if pr.is_draft {
        return Err(no(format!("pull request #{n} is a draft")));
    }
    if pr.base_ref_name != r.default_branch {
        return Err(no(format!("pull request #{n} goes into {}, not the project's main branch {}", pr.base_ref_name, r.default_branch)));
    }
    if pr.head_ref_oid != r.qa_head {
        return Err(no(format!("QA passed commit {}, but pull request #{n}'s latest commit is {}: something was pushed after QA, so QA has to \
                               pass the new commit first", sha7(&r.qa_head), sha7(&pr.head_ref_oid))));
    }
    if pr.mergeable == "CONFLICTING" || pr.merge_state_status == "DIRTY" {
        return Err(no(format!("pull request #{n} has merge conflicts with {}", pr.base_ref_name)));
    }
    if pr.mergeable != "MERGEABLE" || pr.merge_state_status == "UNKNOWN" {
        return Err(wait(format!("GitHub is still working out whether pull request #{n} can be merged: try again in a minute")));
    }
    let checks = &pr.status_check_rollup;
    if checks.is_empty() {
        return Err(wait(format!("no checks have run on pull request #{n} yet: CI is green only when its checks ran and passed. Wait for CI; a \
                                 repository without CI is {you}'s to merge")));
    }
    let names = |s: CheckState| checks.iter().filter(|c| c.standing() == s).map(|c| c.label()).collect::<Vec<_>>();
    let failed = names(CheckState::Failed);
    if !failed.is_empty() {
        return Err(no(format!("{} on pull request #{n} failed: {}. Re-running failed CI jobs is for the DevOps Agent or {you}",
                              plural(failed.len(), "check"), failed.join(", "))));
    }
    let running = names(CheckState::Running);
    if !running.is_empty() {
        return Err(wait(format!("{} of {} on pull request #{n} {} still running ({}): wait until {} finished", running.len(),
                                plural(checks.len(), "check"), if running.len() == 1 { "is" } else { "are" }, running.join(", "),
                                if running.len() == 1 { "it has" } else { "they have" })));
    }
    match pr.merge_state_status.as_str() {
        "BEHIND" => Err(no(format!("pull request #{n} is behind {} and GitHub wants it up to date first", pr.base_ref_name))),
        "BLOCKED" => Err(no(format!("GitHub blocks merging pull request #{n}: a rule of {} (like a required review) isn't met", pr.base_ref_name))),
        "UNSTABLE" => Err(wait(format!("GitHub doesn't count every check on pull request #{n} as passed yet: try again in a minute"))),
        _ => Ok(()),
    }
}

/// The refusal as the tool's error. In a board check, a wait leaves the card's finding unseen, so the next check offers
/// it again (for a few hours after QA passed it).
fn refuse(cx: &Cx, r: &Ready, why: Refused) -> String {
    let id = &r.task.identifier;
    let fresh = r.qa.ended_at.is_some_and(|e| gizai_core::ids::now_ms() - e < OFFER_AGAIN_FOR_MS);
    let again = match cx.check {
        Some(run) if why.wait && fresh => {
            let _ = gizai_core::board::unsee(cx.db(), run, &gizai_core::board::may_merge_key(&r.task.id));
            " Your next board check offers it again."
        }
        _ => "",
    };
    format!("{id} isn't merged: {}. Nothing changed: don't work around this (no other tool merges or moves it); say what waits and why.{again}",
            why.why)
}

/// After GitHub merged it: the Team Lead's comment and activity entry, the desktop notification, then Gizai's PR check,
/// which moves the card on and cleans up its worktree.
async fn after_merge(cx: &Cx<'_>, r: &Ready, pr: &PullDetails, you: &str) -> Result<Value, String> {
    let t = &r.task;
    let passed: Vec<String> = pr.status_check_rollup.iter().map(|c| c.label()).collect();
    let body = format!(
        "Merged pull request [#{n}]({url}) into {base} with a merge commit.\n\n\
         - Head commit: `{head}`\n\
         - QA passed exactly this commit: {qa} (run {run}, ended {ended})\n\
         - {count} passed: {list}\n\n\
         Gizai's pull request check moves the card on and cleans up its worktree. Releases and deploys stay {you}'s.",
        n = pr.number, url = pr.url, base = pr.base_ref_name, head = pr.head_ref_oid, qa = r.qa.agent_name, run = r.qa.id,
        ended = r.qa.ended_at.map(when).unwrap_or_default(), count = plural(passed.len(), "check"), list = passed.join(", "),
    );
    // GitHub merged it: what Gizai notes now can fail without undoing that, so it never turns into an error.
    let _ = comments::add(cx.db(), cx.actor, &t.id, &body, None);
    let _ = pulls::note_merged_by(cx.db(), cx.actor, &t.id, &pr.url, &pr.head_ref_oid);
    cx.changed("comments");
    cx.changed("tasks");
    crate::notifications::lead_merged(cx.st, t, pr.number);
    // As when you merge: the PR check sees the merge, moves the card on and cleans up; now rather than in two minutes.
    let checked = crate::pulls::check(cx.st, &t.id).await;
    let now = tasks::get(cx.db(), &t.id).map_err(err)?;
    let moved = now.state_category != "review";
    if moved {
        let st = cx.st.clone();
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::spawn(async move { crate::runs::pull(&st).await; });
        }
    }
    let column = if moved {
        format!("moved to {}", now.state_name)
    } else {
        let why = checked.err().map(|e| format!(" ({e})")).unwrap_or_default();
        format!("still in {}: Gizai's pull request check moves it on within two minutes{why}", now.state_name)
    };
    Ok(json!({"ok": true, "done": "merged",
              "pull_request": {"number": pr.number, "url": pr.url, "into": pr.base_ref_name, "head": pr.head_ref_oid},
              "qa_run": {"id": r.qa.id, "agent": r.qa.agent_name, "ended": r.qa.ended_at.map(when)},
              "checks_passed": passed, "card": column, "project": r.project.key,
              "link": {"page": "task", "id": t.id, "label": format!("{} {}", t.identifier, short(&t.title, 60))}}))
}

fn sha7(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 { format!("1 {word}") } else { format!("{n} {word}s") }
}
