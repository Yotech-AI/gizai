//! Settings → Coding CLIs: the programs agents run on (Claude Code, Codex, Gemini, others, more accounts of one), whether
//! each is found, finding the ones installed, and what a run of an agent starts (`spec`).
use std::path::{Path, PathBuf};

use gizai_agents::cli::{CliSpec, Kind};
use gizai_core::clis::{self as core_clis, Cli};
use serde::Serialize;

use crate::AppState;

/// A CLI and the program that would run (None: not found, with the reason).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliStatus {
    #[serde(flatten)]
    pub cli: Cli,
    pub path: Option<String>,
    pub problem: Option<String>,
}

fn home() -> String {
    core_clis::home()
}

/// The program a command names: a path (`~` expanded) that is executable, or a name found in `path`. On Windows a path
/// has `\` or `/` in it, and a name or path without its extension is tried with PATHEXT's: `codex` is found as npm's
/// `codex.cmd` or as `codex.exe`.
pub fn resolve_program(command: &str, path: &std::ffi::OsStr) -> Option<PathBuf> {
    let c = core_clis::expand_home(command.trim(), &home());
    if c.is_empty() {
        return None;
    }
    if cfg!(windows) {
        return gizai_agents::os::find_in(&c, path);
    }
    if c.contains('/') {
        let p = PathBuf::from(&c);
        return crate::runs::executable(&p).then_some(p);
    }
    std::env::split_paths(path).map(|d| d.join(&c)).find(|p| crate::runs::executable(p))
}

fn status(st: &AppState, cli: Cli, path: &std::ffi::OsStr) -> CliStatus {
    let found = if cli.id == core_clis::CLAUDE_CODE {
        crate::runs::claude_bin(st, None).ok()
    } else {
        resolve_program(&cli.command, path)
    };
    let problem = match &found {
        Some(_) => git_bash_problem(&cli, path),
        None if cli.command.trim().is_empty() => Some(format!("{} not found: set its program", cli.name)),
        None => Some(format!("{} not found", cli.command.trim())),
    };
    CliStatus { path: found.map(|p| p.display().to_string()), problem, cli }
}

/// Windows: Claude Code runs its Bash tool in Git for Windows' bash and doesn't start without it, so a Claude Code CLI
/// whose bash isn't there says why: its own CLAUDE_CODE_GIT_BASH_PATH line, else Gizai's, the bash next to git, or
/// Git's usual places (`gizai_agents::os::git_bash`). The agent form shows it as a warning and the chat doesn't offer
/// that CLI. None on Linux and macOS.
fn git_bash_problem(cli: &Cli, path: &std::ffi::OsStr) -> Option<String> {
    if !cfg!(windows) || cli.kind != "claude_code" {
        return None;
    }
    let own = core_clis::env_pairs(cli, &home()).into_iter().find(|(k, _)| k.eq_ignore_ascii_case("CLAUDE_CODE_GIT_BASH_PATH"));
    match own {
        Some((_, bash)) if !Path::new(&bash).is_file() =>
            Some(format!("CLAUDE_CODE_GIT_BASH_PATH is {bash}, which isn't there: point it to Git Bash's bash.exe")),
        Some(_) => None,
        None => gizai_agents::os::git_bash(path).is_none()
            .then(|| "Claude Code needs Git for Windows (Git Bash): install it from git-scm.com, or set CLAUDE_CODE_GIT_BASH_PATH".to_string()),
    }
}

/// Every CLI, Claude Code first, with the program each would run.
pub fn list(st: &AppState) -> Result<Vec<CliStatus>, String> {
    let path = crate::runs::command_path();
    let all = core_clis::list(&st.db).map_err(|e| e.to_string())?;
    Ok(all.into_iter().map(|c| status(st, c, &path)).collect())
}

/// Saves the CLIs added in Settings; returns the full list as `list` does.
pub fn save(st: &AppState, clis: Vec<Cli>) -> Result<Vec<CliStatus>, String> {
    core_clis::save(&st.db, clis).map_err(|e| e.to_string())?;
    st.runs.forget_models();
    list(st)
}

/// Coding CLIs Gizai knows how to start, by program name: kind, name and (Other) arguments.
const KNOWN: [(&str, &str, &str, &str); 5] = [
    ("codex", "codex", "Codex", ""),
    ("gemini", "gemini", "Gemini", ""),
    ("opencode", "other", "OpenCode", "run -m {model} {prompt}"),
    ("cursor-agent", "other", "Cursor Agent", "-p --force --output-format text --model {model} {prompt}"),
    ("crush", "other", "Crush", "run --quiet"),
];

/// The known CLIs installed here (your login shell's PATH) that aren't in the list yet, ready to add.
pub fn find(st: &AppState) -> Result<Vec<Cli>, String> {
    let have = core_clis::list(&st.db).map_err(|e| e.to_string())?;
    Ok(find_in(&have, &crate::runs::command_path()))
}

/// The known CLIs found in `path` that aren't in `have` yet.
fn find_in(have: &[Cli], path: &std::ffi::OsStr) -> Vec<Cli> {
    let listed = |program: &str| have.iter().any(|c| {
        let cmd = core_clis::expand_home(c.command.trim(), &home());
        // Windows: `codex` is codex.cmd or codex.exe there, in any case
        let named = if cfg!(windows) {
            Path::new(&cmd).file_stem().is_some_and(|n| n.eq_ignore_ascii_case(program))
        } else {
            Path::new(&cmd).file_name().is_some_and(|n| n == program)
        };
        named && c.env.is_empty()
    });
    KNOWN.iter()
        .filter(|(program, _, name, _)| !listed(program) && !have.iter().any(|c| c.name.eq_ignore_ascii_case(name)))
        .filter_map(|(program, kind, name, args)| {
            let found = resolve_program(program, path)?;
            Some(Cli { id: String::new(), name: name.to_string(), kind: kind.to_string(), command: found.display().to_string(),
                       env: vec![], args: args.to_string() })
        })
        .collect()
}

/// A new install's first start (`gizai_core::seed::ensure_seed_with_agents`): Codex, ready to add, when Claude Code isn't
/// installed but Codex is (your login shell's PATH, as `find` looks). None when Claude Code is found, or Codex isn't.
pub fn codex_at_first_start() -> Option<Cli> {
    if crate::runs::find_claude().is_some() {
        return None;
    }
    find_in(&[], &crate::runs::command_path()).into_iter().find(|c| c.kind == "codex")
}

/// What a run on `cli` starts: its kind, program, environment and arguments. `bin_override` replaces the program (tests).
/// A CLI other than Claude Code also gets your login shell's PATH, since many are Node programs that need `node` on it.
pub fn spec(st: &AppState, cli: &Cli, bin_override: Option<String>) -> Result<CliSpec, String> {
    let kind = Kind::parse(&cli.kind).ok_or_else(|| format!("{} is of an unknown kind {}", cli.name, cli.kind))?;
    let mut env = core_clis::env_pairs(cli, &home());
    let bin = if cli.id == core_clis::CLAUDE_CODE {
        crate::runs::claude_bin(st, bin_override)?
    } else {
        let path = crate::runs::command_path();
        let bin = match bin_override {
            Some(b) => Some(PathBuf::from(b)).filter(|p| crate::runs::executable(p)),
            None => resolve_program(&cli.command, &path),
        };
        if kind != Kind::ClaudeCode && !env.iter().any(|(k, _)| k == "PATH") {
            env.push(("PATH".into(), path.to_string_lossy().into_owned()));
        }
        bin.ok_or_else(|| format!("{} not found ({}): set its program in Settings → Coding CLIs", cli.name, cli.command))?
    };
    Ok(CliSpec { kind, bin, env, args: cli.args.clone() })
}

/// The CLI an agent runs on (its `adapter`; none = Claude Code).
pub fn of_agent(st: &AppState, adapter: Option<&str>) -> Result<Cli, String> {
    core_clis::get(&st.db, adapter.unwrap_or_default()).map_err(|e| e.to_string())
}
