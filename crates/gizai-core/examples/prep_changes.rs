//! Prepares a copy of the demo data for the UI Changes test (GA-112): links project KADE to a new git repository at <repo>
//! (main only, no remote: the changes count from "main") and adds a card in each state, with worktrees in <repo>.wt:
//!  - Worktree: a card in its worktree, two runs of a paused Backend Agent (each one commit) and files not committed yet.
//!    Run 1 changes 40 scattered lines of the 1,500-line src/big.ts (two of them one empty line apart) and adds
//!    src/added.rs; run 2 deletes src/old.rs (60 lines), renames src/folders.rs as it is, renames src/rename_me.ts with
//!    an edit, changes package-lock.json and the binary assets/logo.png. Not committed: a change in README.md and the
//!    untracked src/untracked.ts.
//!  - No branch: a card before its first run.
//!  - Branch only: its branch with one commit, no worktree.
//!  - Merged: its range saved at the merge (refs/gizai/merged/<id>/base and /head), its branch gone, pull request #81.
//!  - No changes: a worktree at main's tip.
//!  - 2,000 lines: a worktree changing 2,112 lines in 32 files.
//!  - 15,000 lines: a worktree changing 15,190 lines in 245 files.
//! Prints the card ids in that order, then run 1's and run 2's, on one line.
//! usage: cargo run -p gizai-core --example prep_changes -- <data dir> <repo (an empty folder)>
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use gizai_core::{db::Db, ids, model::*, projects, runs, seed::ensure_seed, settings, tasks, team};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").current_dir(dir)
        .args(["-c", "user.email=demo@gizai.test", "-c", "user.name=Demo", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null"])
        .args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn write(dir: &Path, rel: &str, body: &str) {
    let p = dir.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, body).unwrap();
}

fn commit(dir: &Path, message: &str) -> String {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", message]);
    git(dir, &["rev-parse", "HEAD"])
}

/// src/big.ts: 1,500 lines, every 25th empty.
fn big_ts(changed: &[usize]) -> String {
    (1..=1500).map(|i| {
        if i % 25 == 0 { String::new() }
        else if changed.contains(&i) { format!("export const value{i} = computeAll({i}, \"item {i}\"); // changed") }
        else { format!("export const value{i} = compute({i}, \"item {i}\");") }
    }).collect::<Vec<_>>().join("\n") + "\n"
}

fn lines(prefix: &str, n: usize) -> String {
    (1..=n).map(|i| format!("{prefix} line {i}: let x{i} = {i};")).collect::<Vec<_>>().join("\n") + "\n"
}

/// A file of `n` lines whose every `every`-th line (from the 2nd) reads differently on the card's branch.
fn many(name: &str, n: usize, every: usize, changed: bool) -> String {
    (1..=n).map(|i| {
        if changed && i % every == 2 { format!("export function {name}_{i}(a: number) {{ return a * {i} + 1; }} // changed") }
        else { format!("export function {name}_{i}(a: number) {{ return a * {i}; }}") }
    }).collect::<Vec<_>>().join("\n") + "\n"
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (dir, repo) = (&a[1], PathBuf::from(&a[2]));
    let db = Db::open(&Path::new(dir).join("gizai.db")).unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    settings::set(&db, "agents_paused", &true).unwrap();
    let wt = PathBuf::from(format!("{}.wt", repo.display()));
    fs::create_dir_all(&repo).unwrap();
    fs::create_dir_all(&wt).unwrap();

    // main
    git(&repo, &["init", "-q", "-b", "main"]);
    write(&repo, "README.md", "# Kade portal\n\nInvoices and exports.\n");
    write(&repo, "src/big.ts", &big_ts(&[]));
    write(&repo, "src/old.rs", &lines("// old", 60));
    write(&repo, "src/folders.rs", &lines("// folders", 30));
    write(&repo, "src/rename_me.ts", &lines("// rename me", 50));
    write(&repo, "package-lock.json", &format!("{{\n{}  \"name\": \"kade\"\n}}\n", (1..=100).map(|i| format!("  \"dep{i}\": \"1.0.{i}\",\n")).collect::<String>()));
    fs::create_dir_all(repo.join("assets")).unwrap();
    fs::write(repo.join("assets/logo.png"), [0x89u8, b'P', b'N', b'G', 0, 1, 2, 3, 0, 0, 0, 0].repeat(1000)).unwrap();
    for i in 1..=32 { write(&repo, &format!("e/file{i:02}.ts"), &many(&format!("e{i}"), 132, 4, false)); }
    for i in 1..=245 { write(&repo, &format!("f/file{i:03}.ts"), &many(&format!("f{i}"), 93, 3, false)); }
    let base = commit(&repo, "The Kade portal as it is");

    let project = projects::list(&db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    projects::update(&db, &s.you_id, &project.id, ProjectInput { name: project.name.clone(), key: project.key.clone(), client_id: project.client_id.clone(),
        repo_path: Some(repo.display().to_string()), default_branch: Some("main".into()), color: project.color.clone(), goal_md: project.goal_md.clone(),
        ..Default::default() }).unwrap();
    let card = |title: &str, branch: Option<&str>| {
        let id = tasks::create(&db, &s.you_id, TaskInput { project_id: project.id.clone(), title: title.into(), description_md: String::new(),
            acceptance_md: None, state_id: None, priority: 2, assignee_id: None, label_ids: vec![], testing: None }).unwrap();
        if let Some(b) = branch {
            db.write(None, |w| { w.conn().execute("UPDATE tasks SET branch=?2 WHERE id=?1", rusqlite::params![id, b])?; Ok(()) }).unwrap();
        }
        id
    };
    let agent = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "Backend Agent".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
    team::set_agent_status(&db, &s.you_id, &agent, "paused").unwrap();

    // Worktree: two runs, each one commit, and files not committed yet
    let b1 = "gizai/kade-changes-worktree";
    let w1 = wt.join("worktree");
    git(&repo, &["worktree", "add", "-q", "-b", b1, &w1.display().to_string(), &base]);
    let mut changed: Vec<usize> = vec![49, 51];
    let mut i = 10;
    while changed.len() < 40 { if i % 25 != 0 && !changed.contains(&i) { changed.push(i); } i += 37; }
    write(&w1, "src/big.ts", &big_ts(&changed));
    write(&w1, "src/added.rs", &lines("// added", 120));
    let c1 = commit(&w1, "Exports compute every item");
    fs::remove_file(w1.join("src/old.rs")).unwrap();
    fs::rename(w1.join("src/folders.rs"), w1.join("src/folder_rules.rs")).unwrap();
    let renamed = lines("// rename me", 50).replace("line 20: let x20 = 20;", "line 20: let x20 = 2000;");
    fs::remove_file(w1.join("src/rename_me.ts")).unwrap();
    write(&w1, "src/renamed.ts", &renamed);
    let lock = fs::read_to_string(w1.join("package-lock.json")).unwrap().replace("\"dep7\": \"1.0.7\"", "\"dep7\": \"2.0.0\"");
    write(&w1, "package-lock.json", &lock);
    fs::write(w1.join("assets/logo.png"), [0x89u8, b'P', b'N', b'G', 9, 9, 9, 9, 0, 0, 0, 0].repeat(1200)).unwrap();
    let c2 = commit(&w1, "Old folders go, renames, a new dependency and logo");
    write(&w1, "README.md", "# Kade portal\n\nInvoices, exports and the changes view.\n");
    write(&w1, "src/untracked.ts", "export const untracked = true;\n");
    let worktree_card = card("Changes: a card in its worktree", Some(b1));
    let run = |from: &str, to: &str| {
        let r = runs::create(&db, &agent, &worktree_card, "backend", &ids::new_id(), &w1.display().to_string(), &w1.display().to_string(), b1, "/dev/null").unwrap();
        runs::set_base_sha(&db, &r, from).unwrap();
        runs::set_head_sha(&db, &r, to).unwrap();
        runs::finish(&db, &r, "succeeded", None, 0, 0, 0, None).unwrap();
        r
    };
    let run1 = run(&base, &c1);
    let run2 = run(&c1, &c2);

    // No branch
    let no_branch = card("Changes: no branch yet", None);

    // Branch only
    let b3 = "gizai/kade-changes-branch";
    git(&repo, &["branch", b3, &base]);
    let w3 = wt.join("branch");
    git(&repo, &["worktree", "add", "-q", &w3.display().to_string(), b3]);
    write(&w3, "README.md", "# Kade portal\n\nInvoices, exports and a branch.\n");
    commit(&w3, "README: the branch");
    git(&repo, &["worktree", "remove", "--force", &w3.display().to_string()]);
    let branch_only = card("Changes: its branch only", Some(b3));

    // Merged: the range saved at its merge, its branch gone
    let b4 = "gizai/kade-changes-merged";
    let merged = card("Changes: merged", Some(b4));
    let ident = tasks::get(&db, &merged).unwrap().identifier;
    git(&repo, &["branch", b4, &base]);
    let w4 = wt.join("merged");
    git(&repo, &["worktree", "add", "-q", &w4.display().to_string(), b4]);
    write(&w4, "src/merged.ts", &lines("// merged", 40));
    write(&w4, "README.md", "# Kade portal\n\nInvoices, exports, merged.\n");
    let head4 = commit(&w4, "What the merged card did");
    git(&repo, &["worktree", "remove", "--force", &w4.display().to_string()]);
    git(&repo, &["update-ref", &format!("refs/gizai/merged/{ident}/base"), &base]);
    git(&repo, &["update-ref", &format!("refs/gizai/merged/{ident}/head"), &head4]);
    git(&repo, &["branch", "-D", b4]);
    db.write(None, |w| {
        w.conn().execute("UPDATE tasks SET pr_url='https://github.com/demo/kade/pull/81', pr_state='merged' WHERE id=?1", rusqlite::params![merged])?;
        Ok(())
    }).unwrap();

    // No changes
    let b5 = "gizai/kade-changes-none";
    git(&repo, &["worktree", "add", "-q", "-b", b5, &wt.join("none").display().to_string(), &base]);
    let none = card("Changes: nothing yet", Some(b5));

    // 2,000 lines in 32 files, and 15,000 lines in 245 files
    let b6 = "gizai/kade-changes-2000";
    let w6 = wt.join("e2000");
    git(&repo, &["worktree", "add", "-q", "-b", b6, &w6.display().to_string(), &base]);
    for i in 1..=32 { write(&w6, &format!("e/file{i:02}.ts"), &many(&format!("e{i}"), 132, 4, true)); }
    commit(&w6, "2,000 changed lines");
    let e2000 = card("Changes: 2,000 lines", Some(b6));
    let b7 = "gizai/kade-changes-15000";
    let w7 = wt.join("f15000");
    git(&repo, &["worktree", "add", "-q", "-b", b7, &w7.display().to_string(), &base]);
    for i in 1..=245 { write(&w7, &format!("f/file{i:03}.ts"), &many(&format!("f{i}"), 93, 3, true)); }
    commit(&w7, "15,000 changed lines");
    let f15000 = card("Changes: 15,000 lines", Some(b7));

    println!("{worktree_card} {no_branch} {branch_only} {merged} {none} {e2000} {f15000} {run1} {run2}");
}
