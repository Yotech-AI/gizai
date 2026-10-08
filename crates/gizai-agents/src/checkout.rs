//! A project's own checkout, its linked folder (the main checkout you work in, where a new card's worktree copies
//! vendor/ and node_modules/ from): how its dependencies stand against main (`status`, local only: no fetch), and its
//! update to main when you said yes in chat (`plan`, then `update`). The update switches to the default branch only when
//! asked (the other branch stays as it is), moves with a fast-forward (never a merge commit, a reset or force), and
//! installs the dependency folders that are behind as a card's preparation would. It never runs the project's setup
//! command, which could touch your local database.
use std::ffi::OsStr;
use std::path::Path;

use crate::AgentError;
use crate::copies::is_checkout_of;
use crate::prepare;
use crate::worktree::git;

/// The dependency folders a card's preparation installs, with the lock file that says what belongs in them.
const DEPS: [(&str, &str); 2] = [("vendor", "composer.lock"), ("node_modules", "package-lock.json")];

/// A dependency folder that is behind main.
#[derive(Debug, Clone, PartialEq)]
pub struct DepBehind {
    /// "vendor" or "node_modules".
    pub folder: String,
    /// Its lock file: "composer.lock" or "package-lock.json".
    pub lock: String,
    /// The folder isn't there; else its lock file differs from main's.
    pub missing: bool,
}

impl DepBehind {
    /// "vendor/ is missing", "package-lock.json differs from main's".
    pub fn said(&self) -> String {
        if self.missing { format!("{}/ is missing", self.folder) } else { format!("{} differs from main's", self.lock) }
    }
}

/// How a checkout stands against main.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    /// The branch it is on; None without one (a detached HEAD).
    pub branch: Option<String>,
    /// How many commits of main it doesn't have.
    pub behind: u32,
    /// How many commits an update would move its default branch: None when that branch has commits main doesn't (only
    /// a merge could bring main in), or isn't there.
    pub moves: Option<u32>,
    /// Its dependency folders that are behind main. Only these make it outdated: missing commits alone don't.
    pub deps: Vec<DepBehind>,
}

impl Status {
    pub fn outdated(&self) -> bool {
        !self.deps.is_empty()
    }
}

/// How the checkout `dir` stands against `main` (a ref of its repository), read on this disk only: nothing is fetched
/// or changed. A dependency folder counts when main has its lock file; it is behind when it is missing, or when the
/// checkout's lock file (as it is on disk) differs from main's.
pub fn status(dir: &Path, default_branch: &str, main: &str) -> Result<Status, AgentError> {
    let branch = git(dir, &["branch", "--show-current"])?;
    let count = |range: String| -> Option<u32> { git(dir, &["rev-list", "--count", &range]).ok()?.trim().parse().ok() };
    let behind = count(format!("HEAD..{main}")).ok_or_else(|| AgentError::Git(format!("{main} isn't there")))?;
    let local = format!("refs/heads/{default_branch}");
    let moves = match count(format!("{main}..{local}")) {
        Some(0) => count(format!("{local}..{main}")),
        _ => None,
    };
    let deps = DEPS.iter().filter_map(|(folder, lock)| {
        let theirs = git(dir, &["rev-parse", "--verify", "--quiet", &format!("{main}:{lock}")]).ok()?;
        let missing = !dir.join(folder).is_dir();
        let ours = if dir.join(lock).is_file() { git(dir, &["hash-object", "--", lock]).ok() } else { None };
        (missing || ours.as_deref() != Some(theirs.as_str()))
            .then(|| DepBehind { folder: folder.to_string(), lock: lock.to_string(), missing })
    }).collect();
    Ok(Status { branch: Some(branch).filter(|b| !b.is_empty()), behind, moves, deps })
}

/// What `update` will do (`plan` found nothing in the way).
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The branch the checkout is on now; None without one.
    pub on: Option<String>,
    /// Switch to `default_branch` first: it is on another branch, or none.
    pub switch: bool,
    pub default_branch: String,
    /// The commit its default branch is at now, and main's: the fast-forward goes from one to the other.
    pub from: String,
    pub to: String,
    /// How many commits the fast-forward moves.
    pub commits: u32,
    /// The dependency folders it installs afterwards ("vendor", "node_modules"): those that are behind main now.
    pub install: Vec<String>,
}

impl Plan {
    /// What it will do, in one sentence ("switch from feature/x to main, move main 12 commits to 319bf0b and run npm ci").
    pub fn said(&self) -> String {
        let mut parts = vec![];
        if self.switch {
            parts.push(match &self.on {
                Some(b) => format!("switch from {b} to {} ({b} stays as it is)", self.default_branch),
                None => format!("switch to {}", self.default_branch),
            });
        }
        parts.push(if self.commits == 0 {
            format!("leave {} where it is ({}, already main's commit)", self.default_branch, short(&self.to))
        } else {
            format!("move {} {} commit{} to {}", self.default_branch, self.commits, if self.commits == 1 { "" } else { "s" }, short(&self.to))
        });
        let installs: Vec<&str> = self.install.iter().map(|f| install_name(f)).collect();
        if !installs.is_empty() {
            parts.push(format!("run {}", installs.join(" and ")));
        }
        sentence(&parts)
    }
}

/// What an install of the dependency folder is called (as `prepare` runs it with a lock file).
pub fn install_name(folder: &str) -> &'static str {
    if folder == "vendor" { "composer install" } else { "npm ci" }
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

/// "a, b and c".
fn sentence(parts: &[String]) -> String {
    match parts.len() {
        0 => String::new(),
        1 => parts[0].clone(),
        n => format!("{} and {}", parts[..n - 1].join(", "), parts[n - 1]),
    }
}

/// Whether `update` may run in the checkout `dir` of the repository whose main checkout is `repo`, and what it would do:
/// bring its `default_branch` to `main` (a ref) with a fast-forward, after switching to it when it is on another branch
/// and `switch` allows that. Nothing changes here. Says why not, in a sentence, when:
/// - it isn't a checkout of `repo`;
/// - a merge, rebase, cherry-pick or revert is in progress, or tracked files have uncommitted changes (it names them);
/// - it is on another branch, or none, and `switch` is false;
/// - its default branch has commits main doesn't, so only a merge could bring main in;
/// - main hasn't been fetched yet, or the default branch isn't there.
pub fn plan(dir: &Path, repo: &Path, default_branch: &str, main: &str, switch: bool) -> Result<Plan, String> {
    let name = dir.display();
    if !dir.is_dir() || !is_checkout_of(dir, repo) {
        return Err(format!("{name} isn't a checkout of the project's repository: nothing changed"));
    }
    let said = |e: AgentError| match e {
        AgentError::Git(m) => m,
        other => other.to_string(),
    };
    for (marker, what) in [("MERGE_HEAD", "a merge"), ("rebase-merge", "a rebase"), ("rebase-apply", "a rebase"),
                           ("CHERRY_PICK_HEAD", "a cherry-pick"), ("REVERT_HEAD", "a revert")] {
        let path = git(dir, &["rev-parse", "--path-format=absolute", "--git-path", marker]).map_err(said)?;
        if Path::new(&path).exists() {
            return Err(format!("{what} is in progress in {name}: nothing changed. Finish or abort it first"));
        }
    }
    let changed: Vec<String> = git(dir, &["status", "--porcelain", "--untracked-files=no"]).map_err(said)?
        .lines().filter(|l| l.len() > 3).map(|l| l[3..].to_string()).collect();
    if !changed.is_empty() {
        let shown = if changed.len() > 10 {
            format!("{} and {} more", changed[..10].join(", "), changed.len() - 10)
        } else {
            changed.join(", ")
        };
        return Err(format!("{name} has uncommitted changes in {shown}: nothing changed. Commit or stash them first"));
    }
    let on = Some(git(dir, &["branch", "--show-current"]).map_err(said)?).filter(|b| !b.is_empty());
    let switching = on.as_deref() != Some(default_branch);
    if switching && !switch {
        return Err(match &on {
            Some(b) => format!("{name} is on branch {b}, not {default_branch}: nothing changed. Updating it means switching \
                                to {default_branch} first ({b} stays as it is): call again with switch only when that is agreed"),
            None => format!("{name} has no branch checked out: nothing changed. Updating it means switching to \
                             {default_branch} first: call again with switch only when that is agreed"),
        });
    }
    let to = git(dir, &["rev-parse", "--verify", "--quiet", &format!("{main}^{{commit}}")])
        .map_err(|_| format!("main hasn't been fetched into {name} yet ({main}): nothing changed"))?;
    let from = git(dir, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{default_branch}^{{commit}}")])
        .map_err(|_| format!("{name} has no {default_branch} branch: nothing changed"))?;
    let count = |range: String| -> Result<u32, String> {
        Ok(git(dir, &["rev-list", "--count", &range]).map_err(said)?.trim().parse().unwrap_or(0))
    };
    let ahead = count(format!("{to}..{from}"))?;
    if ahead > 0 {
        return Err(format!("{default_branch} in {name} has {ahead} commit{} that main doesn't, so main can't be reached \
                            with a fast-forward: nothing changed", if ahead == 1 { "" } else { "s" }));
    }
    let commits = count(format!("{from}..{to}"))?;
    let install = status(dir, default_branch, main).map_err(said)?.deps.into_iter().map(|d| d.folder).collect();
    Ok(Plan { on, switch: switching, default_branch: default_branch.to_string(), from, to, commits, install })
}

/// What `update` did.
#[derive(Debug, Clone, PartialEq)]
pub struct Updated {
    /// The branch it switched from, when it switched ("" for none).
    pub switched_from: Option<String>,
    pub from: String,
    pub to: String,
    /// The installs it ran ("composer install", "npm ci").
    pub installed: Vec<String>,
}

/// Why `update` stopped part-way.
#[derive(Debug, Clone, PartialEq)]
pub struct Stopped {
    /// What it had done by then ("switched from feature/x to main").
    pub done: Vec<String>,
    /// What failed ("npm ci failed (exit code 1)").
    pub why: String,
    /// The end of the output.
    pub output: String,
}

/// Carries out `plan` in the checkout `dir`: switches to the default branch when the plan says so, fast-forwards it to
/// main (`git merge --ff-only`), then installs the dependency folders that were behind (`prepare::install`: no prompts,
/// within its time limit). git's hooks don't run, so nothing happens beyond what the plan says. `path` is the PATH the
/// installs get (None: Gizai's own). The first step that fails stops it.
pub fn update(dir: &Path, plan: &Plan, path: Option<&OsStr>) -> Result<Updated, Stopped> {
    let mut done = vec![];
    let stop = |done: &Vec<String>, why: String, e: AgentError| Stopped {
        done: done.clone(), why,
        output: match e { AgentError::Git(m) => m, other => other.to_string() },
    };
    const NO_HOOKS: [&str; 2] = ["-c", "core.hooksPath=/dev/null"];
    if plan.switch {
        let args = [NO_HOOKS[0], NO_HOOKS[1], "switch", "--quiet", plan.default_branch.as_str()];
        git(dir, &args).map_err(|e| stop(&done, format!("switching to {} failed", plan.default_branch), e))?;
        done.push(match &plan.on {
            Some(b) => format!("switched from {b} to {}", plan.default_branch),
            None => format!("switched to {}", plan.default_branch),
        });
    }
    let args = [NO_HOOKS[0], NO_HOOKS[1], "merge", "--ff-only", "--quiet", plan.to.as_str()];
    git(dir, &args).map_err(|e| stop(&done, "the fast-forward to main failed".into(), e))?;
    if plan.commits > 0 {
        done.push(format!("moved {} from {} to {}", plan.default_branch, short(&plan.from), short(&plan.to)));
    }
    let mut installed = vec![];
    for folder in &plan.install {
        match prepare::install(dir, folder, path) {
            Ok(shown) => {
                done.push(format!("ran {shown}"));
                installed.push(shown);
            }
            Err(f) => return Err(Stopped { done, why: format!("{} {}", f.command, f.why), output: f.output }),
        }
    }
    Ok(Updated {
        switched_from: plan.switch.then(|| plan.on.clone().unwrap_or_default()),
        from: plan.from.clone(), to: plan.to.clone(), installed,
    })
}
