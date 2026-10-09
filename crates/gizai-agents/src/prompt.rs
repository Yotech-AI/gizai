//! The prompt for one agent run: the role's instructions, then the task.

#[derive(Debug, Clone, Default)]
pub struct TaskContext {
    pub identifier: String,
    pub title: String,
    pub description_md: String,
    pub acceptance_md: String,
    /// (author, body), oldest first.
    pub recent_comments: Vec<(String, String)>,
    pub role: String,
    /// Issues from the last failed QA run, to fix first.
    pub qa_issues: Vec<String>,
    /// Where Gizai will stop the run; told to the agent so it saves its work in time.
    pub limits: Option<RunLimits>,
    /// The project's main branch fetched for this run (from GitHub, Bitbucket or another git URL: see `build_from`), when
    /// it has a link.
    pub base: Option<BaseInfo>,
    /// The project's goal (Markdown), read with every task.
    pub project_goal_md: String,
}

#[derive(Debug, Clone)]
pub struct BaseInfo {
    /// The ref the card's branch starts from, like "acme-labs/main".
    pub from: String,
    /// Commits it has that the card's branch doesn't (0 for a new branch).
    pub behind: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct RunLimits {
    pub minutes: u64,
    pub tool_calls: u32,
}

const MAX_COMMENT_CHARS: usize = 2000;

/// "1. Pin overlaps" / "2) x" / "3.Bad" → the text without its list marker; "404 on /login" and
/// "3.5 s slow" keep their numbers.
fn strip_list_marker(s: &str) -> &str {
    let t = s.trim();
    let digits = t.len() - t.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 { return t; }
    let rest = &t[digits..];
    match rest.chars().next() {
        Some('.') | Some(')') if !rest[1..].starts_with(|c: char| c.is_ascii_digit()) => rest[1..].trim_start(),
        _ => t,
    }
}


/// The prompt for a run of a project on GitHub (`build_from` names where its main branch came from).
pub fn build(task: &TaskContext, role_instructions: &str) -> String {
    build_from(task, role_instructions, "GitHub")
}

/// The prompt for a run, its main branch (`TaskContext::base`) just fetched from `fetched_from`: "GitHub", "Bitbucket",
/// or "the project's repository" for another git URL.
pub fn build_from(task: &TaskContext, role_instructions: &str, fetched_from: &str) -> String {
    let mut p = String::new();
    p.push_str(role_instructions.trim_end());
    p.push_str(&format!("\n\n# Task {}: {}\n\n", task.identifier, task.title));
    if !task.project_goal_md.trim().is_empty() {
        p.push_str(&format!("## Project goal\n\n{}\n\n", task.project_goal_md.trim()));
    }
    p.push_str("## Description\n\n");
    p.push_str(if task.description_md.trim().is_empty() { "No description was written; work from the title." } else { task.description_md.trim() });
    p.push_str("\n\n## Acceptance criteria\n\n");
    p.push_str(if task.acceptance_md.trim().is_empty() { "No acceptance criteria were written; use the description." } else { task.acceptance_md.trim() });
    if !task.qa_issues.is_empty() {
        p.push_str("\n\n## QA found these issues last time; fix them first\n\n");
        for (i, issue) in task.qa_issues.iter().enumerate() {
            // QA often numbers its own issues; don't number them twice
            p.push_str(&format!("{}. {}\n", i + 1, strip_list_marker(issue)));
        }
    }
    if !task.recent_comments.is_empty() {
        p.push_str("\n\n## Recent comments (oldest first)\n\n");
        for (author, body) in &task.recent_comments {
            let body: String = body.trim().chars().take(MAX_COMMENT_CHARS).collect();
            p.push_str(&format!("{author}: {body}\n\n"));
        }
    }
    if let Some(b) = &task.base {
        p.push_str(&format!("\n\n## Your branch\n\nThe main branch is {}, just fetched from {fetched_from}.", b.from));
        if b.behind > 0 {
            p.push_str(&format!(" It has {} commits your branch doesn't have yet. Merge {} into your branch before you finish, and resolve \
any conflicts.", b.behind, b.from));
        }
        p.push('\n');
    }
    if let Some(l) = task.limits {
        p.push_str(&limits_section(l));
    }
    p.trim_end().to_string() + "\n"
}

fn limits_section(l: RunLimits) -> String {
    format!("\n\n## Limits of this run\n\nGizai stops this run after {} tool calls or {} minutes, whichever comes first. \
Commit your work on this branch as you go. If the task won't fit, stop before the limit: commit what you have and end with \
your GIZAI_RESULT line, saying in the summary what is done and what is left.\n", l.tool_calls, l.minutes)
}

/// The message that resumes a session Gizai (or a person) stopped: the task and instructions are already in it.
/// Continue after an answer: the last run ended asking for a decision, and `answer` is what was written on the card since
/// (its comments, oldest first).
pub fn answered_prompt(answer: &str, limits: Option<RunLimits>) -> String {
    let quoted: Vec<String> = answer.trim().lines().map(|l| format!("> {l}")).collect();
    let mut p = format!("Your last run on this task ended asking for a decision. It was answered on the card since:\n\n{}\n\n\
Continue where you left off, with this answer. First check `git status` and `git diff`. Then finish the task, commit, and end with your \
GIZAI_RESULT line.", quoted.join("\n"));
    if let Some(l) = limits {
        p.push_str(&limits_section(l));
    }
    p.trim_end().to_string() + "\n"
}

pub fn continue_prompt(reason: &str, limits: Option<RunLimits>) -> String {
    let mut p = format!("Your last run on this task was stopped: {}.\n\nContinue where you left off. First check `git status` and \
`git diff`: edits that were cut off may not have been saved. Then finish the task, commit, and end with your GIZAI_RESULT line.",
        reason.trim().trim_end_matches('.'));
    if let Some(l) = limits {
        p.push_str(&limits_section(l));
    }
    p.trim_end().to_string() + "\n"
}

/// Gizai's nudge (GA-54): the message that continues, once and by itself, a run that ended normally without its
/// GIZAI_RESULT line, in the same session. Often the agent ended its message to wait for something outside the run.
pub fn nudge_prompt(limits: Option<RunLimits>) -> String {
    let mut p = String::from("Your run ended without your GIZAI_RESULT line, and nothing wakes you up later. If you were waiting for \
something, check it in the foreground now and finish the task. End with your GIZAI_RESULT line.");
    if let Some(l) = limits {
        p.push_str(&limits_section(l));
    }
    p.trim_end().to_string() + "\n"
}

/// How a headless task run works, told at the end of every task prompt, new and continued (`with_rules`): nobody can
/// approve anything, the commands the agent may run, its CLI's shell rules, the run's temp folder, how to wait, and that
/// Gizai pushes the branch when the run ends (`PUSHED_BY_GIZAI`).
/// Only what holds for its CLI and permission mode goes in. Claude Code's shell rules were checked against Claude Code
/// 2.1.289 in acceptEdits mode with Gizai's task-run flags (GA-48): `$(…)`, backticks, variables, a heredoc with an
/// unquoted delimiter, and reading or writing outside the working folders (an allowed `ls /tmp`, a redirect to `/tmp`,
/// the Write tool on `/tmp`) were refused; pipes, `2>&1`, `&&`, `;`, a quoted heredoc and a redirect to a file in the
/// worktree ran.
#[derive(Debug, Clone)]
pub struct RunRules {
    pub kind: crate::cli::Kind,
    /// The agent's permission mode in its CLI's terms; empty is Gizai's default for that CLI (acceptEdits for Claude
    /// Code, workspace-write for Codex, auto_edit for Gemini).
    pub mode: String,
    /// Claude Code style, e.g. `Bash(git status:*)`: the agent's own list, or Gizai's default one.
    pub allowed_tools: Vec<String>,
    /// The agent's folders besides the worktree that this run gets (absolute paths).
    pub folders: Vec<String>,
    /// The run's temp folder (`<worktree>/.gizai-tmp`); None when Gizai couldn't make it.
    pub temp_dir: Option<String>,
}

/// `prompt` (a new run's, a continued run's or an answered one's) with "How this run works" at its end.
pub fn with_rules(prompt: &str, rules: &RunRules) -> String {
    format!("{}\n{}", prompt.trim_end(), rules_section(rules)).trim_end().to_string() + "\n"
}

/// The allowed list as the prompt shows it: `Bash(git status:*)` → `git status`, `Bash(npm test)` → `npm test` (exact),
/// `Bash(./vendor/bin/*)` as it is, and other tools by their rule, like `WebFetch(domain:docs.rs)`.
fn commands_line(tools: &[String], other_tools: bool) -> String {
    let (mut cmds, mut other) = (vec![], vec![]);
    for t in tools.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
        match t.strip_prefix("Bash(").and_then(|r| r.strip_suffix(')')).map(str::trim) {
            Some(inner) => match inner.strip_suffix(":*") {
                Some(prefix) => cmds.push(format!("`{}`", prefix.trim())),
                None if inner.ends_with('*') => cmds.push(format!("`{inner}`")),
                None => cmds.push(format!("`{inner}` (exact)")),
            },
            None if other_tools => other.push(format!("`{t}`")),
            None => {}
        }
    }
    let mut line = if cmds.is_empty() {
        "No commands are allowed for you.".to_string()
    } else {
        format!("The commands you may run, with any arguments unless marked exact: {}.", cmds.join(", "))
    };
    if !other.is_empty() {
        line.push_str(&format!(" Also allowed: {}.", other.join(", ")));
    }
    line
}

/// Where throwaway files go, after `make` (how to make a file, when the CLI's file tool may write in this mode).
fn temp_line(temp_dir: Option<&str>, make: Option<&str>, literal_path: bool) -> String {
    let make = make.map(|m| format!("{m} ")).unwrap_or_default();
    match temp_dir {
        Some(d) => format!("{make}Throwaway files go only in `{d}` (TMPDIR, TMP and TEMP point there{}), never in `/tmp` or anywhere else \
outside the worktree, and are never committed. Gizai empties that folder when the run ends.",
                           if literal_path { "; in a command, write this path, not `$TMPDIR`" } else { "" }),
        None => format!("{make}Throwaway files never go in `/tmp` or anywhere else outside the worktree, and are never committed."),
    }
}

/// Whether the agent may run `sleep`, so the waiting rule may name it: its CLI runs commands without a list (Codex, its
/// sandbox decides), its mode lets every command run (bypassPermissions, yolo), or its list allows `sleep` with any
/// arguments (`Bash(sleep:*)`, `Bash(sleep *)`, and for Claude Code a bare `Bash` or `Bash(*)` too). An Other CLI has
/// no list Gizai knows: no.
fn may_sleep(r: &RunRules) -> bool {
    use crate::cli::Kind;
    let listed = |all_bash: bool| r.allowed_tools.iter().map(|t| t.trim()).any(|t| {
        if t == "Bash" { return all_bash; }
        match t.strip_prefix("Bash(").and_then(|i| i.strip_suffix(')')).map(str::trim) {
            Some("*") => all_bash,
            Some(inner) => inner.strip_suffix(":*").or_else(|| inner.strip_suffix('*')).is_some_and(|p| p.trim() == "sleep"),
            None => false,
        }
    });
    match r.kind {
        Kind::ClaudeCode => r.mode.trim() == "bypassPermissions" || listed(true),
        Kind::Codex => true,
        Kind::Gemini => r.mode.trim() == "yolo" || listed(false),
        Kind::Other => false,
    }
}

/// How to wait for something outside the run (GA-54), for every CLI. Claude Code 2.1.289 was checked in a headless run
/// with Gizai's task-run flags, acceptEdits and the default list plus `Bash(sleep:*)`:
/// - A command that starts with a sleep longer than 20 seconds is blocked before it runs ("Blocked: sleep 45 followed
///   by: git status --short. To wait for a condition, use Monitor with an until-loop … To wait for a command you
///   started, use run_in_background: true. Do not chain shorter sleeps to work around this block."), and so is a lone
///   `sleep 30`. `sleep 20 && pwd` ran, `sleep 25 && pwd` was blocked. A sleep after the first command runs:
///   `git status --short && sleep 40 && git status --short` took its 40 seconds.
/// - A foreground command gets 2 minutes: `pwd && sleep 150 && pwd` was moved to the background after 120 s ("Command
///   did not complete within its 120s timeout and was moved to the background"). The Bash tool's `timeout` allows up
///   to 10 minutes and keeps it in the foreground (`pwd && sleep 130 && pwd` with timeout 200000 ran to its end).
/// - A command started with run_in_background is killed when the agent ends its message: the run ended at once (8 s,
///   the command needed 20), the stream says `task_updated` killed, and what the command was to write never appeared.
///   Gizai ends the run's process group after that anyway (`process::spawn_exec`), for every CLI.
/// - A Monitor is the exception: `claude -p` stays alive while one watches, wakes the agent at each event and when its
///   command ends (each wake-up is a turn with its own result line; Gizai reads the last), and a Monitor expires after
///   5 minutes. The rule doesn't offer it: one foreground check after another keeps the run one turn, with the
///   GIZAI_RESULT line in its last message.
/// - Loops are a gamble: `until git status --short; do sleep 2; done` and `until test -f ci-done.txt; do sleep 30; done`
///   ran in the foreground, but `for i in {1..20}; …` ("A brace pattern in this command can't be checked before it
///   runs"), `while true; do test -f ci-done.txt && break; sleep 30; done` ("requires approval: break") and an `until`
///   with a pipe in its check were refused. So the rule asks for one check and then the sleep in one command, no loop.
/// - A Haiku run told to wait for a file a fake CI wrote after 100 s, with an earlier wording of this rule (without "no
///   loop"), tried the two refused loops, then waited with the `until` loop. With this wording it tried a subshell
///   (`… || ( sleep 30 && … )`, refused: "shell operators that require approval"), then waited with one check per
///   command (`test -f ci-done.txt && cat ci-done.txt || sleep 30`, four times). Both ended with their GIZAI_RESULT line.
///
/// Codex, Gemini and Other CLIs were not checked; they hear the same rule without Claude Code's limits, and `sleep` only
/// when they may run it (`may_sleep`).
pub fn rules_section(r: &RunRules) -> String {
    use crate::cli::Kind;
    let mode = r.mode.trim();
    let temp = r.temp_dir.as_deref();
    let refused = "Nobody can approve anything during this run: a command or tool that needs approval is refused.".to_string();
    let mut lines: Vec<String> = vec![];
    match r.kind {
        Kind::ClaudeCode => {
            let mode = if mode.is_empty() { "acceptEdits" } else { mode };
            lines.push(refused);
            if mode != "bypassPermissions" {
                lines.push(commands_line(&r.allowed_tools, true));
            }
            // Checked in acceptEdits only; stricter modes refuse more (a redirect too), looser ones less.
            if mode == "acceptEdits" {
                let reach = if r.folders.is_empty() { "this worktree".to_string() } else {
                    format!("this worktree and your folders ({})", r.folders.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>().join(", "))
                };
                lines.push(format!("Commands and file tools reach only {reach}: reading or writing anywhere else (`/tmp`, `..`) is refused, \
also with an allowed command."));
                lines.push("In a command, `$(…)`, backticks, variables such as `$TMPDIR` and heredocs with an unquoted delimiter (`<<EOF`) are \
refused. Pipes, `2>&1`, `&&` and `;` between allowed commands are fine, and so is a redirect (`>`, `>>`, `2>`) to a file in this worktree.".into());
            }
            let writes = matches!(mode, "acceptEdits" | "auto" | "bypassPermissions")
                || r.allowed_tools.iter().any(|t| t.trim().starts_with("Write") || t.trim().starts_with("Edit"));
            lines.push(temp_line(temp, writes.then_some("Make files with the Write tool."), mode == "acceptEdits"));
        }
        Kind::Codex => {
            // Codex gets no allowed list: approval_policy "never" and its sandbox decide.
            lines.push("Nobody can approve anything during this run: nothing is asked, and a command the sandbox blocks fails.".into());
            lines.push(temp_line(temp, None, false));
        }
        Kind::Gemini => {
            let mode = if mode.is_empty() { "auto_edit" } else { mode };
            lines.push(refused);
            if mode != "yolo" {
                lines.push(commands_line(&r.allowed_tools, false));
            }
            let writes = matches!(mode, "auto_edit" | "yolo");
            lines.push(temp_line(temp, writes.then_some("Make files with the write_file tool."), false));
        }
        Kind::Other => {
            lines.push("Nobody can approve anything during this run.".into());
            lines.push(temp_line(temp, None, false));
        }
    }
    lines.extend(waiting_lines(r));
    lines.push(PUSHED_BY_GIZAI.into());
    lines.push("When something is refused, don't try other spellings of it: go on without it, and name the exact command in your summary \
under what you could not check.".into());
    let mut s = String::from("\n## How this run works\n\n");
    for l in lines {
        s.push_str(&format!("- {l}\n"));
    }
    s
}

/// Gizai pushes the card's branch itself when a run ends (GA-56), so a CLI that refuses the agent's `git push` no longer
/// holds a card up: told to every CLI in every mode. Agents keep `git push` in their list.
pub const PUSHED_BY_GIZAI: &str = "When the run ends, Gizai itself pushes this branch's commits (not uncommitted changes) to the project's \
remote, when it has one. So a refused `git push` of this branch is no reason for `needs_decision`: mention it in your summary and end \
with the outcome your work deserves.";

/// The waiting rule (see `rules_section`): ending the message ends the run, how to wait in the foreground, and what to do
/// when it won't be done in time. `sleep` is named only when the agent may run it.
fn waiting_lines(r: &RunRules) -> Vec<String> {
    let claude = r.kind == crate::cli::Kind::ClaudeCode;
    let sleep = may_sleep(r);
    let what = "To wait for something outside this run (a CI run, a release or deploy workflow, a pull request's checks)";
    let wait = if sleep {
        format!("{what}, check it in the foreground about once a minute, within this run's limits: one check and then the sleep in one \
command, like `<check>; sleep 45`, and again until it is done.")
    } else {
        format!("{what}, check it again in the foreground until it is done, within this run's limits.")
    };
    let mut lines = vec![
        format!("Ending your message ends the run: nothing wakes you up later, and {} is stopped.",
                if claude { "a command still running in the background (run_in_background)" } else { "anything still running in the background" }),
        wait,
    ];
    if claude {
        lines.push(format!("Don't write a loop (`for`, `while`, `until`): most are refused. {}A command that runs longer than 2 minutes, \
or than the timeout you give it (at most 10 minutes), is moved to the background.",
                           if sleep { "A command that starts with a sleep longer than 20 seconds is blocked. " } else { "" }));
    }
    lines.push("If it won't be done before the limit, don't wait for it: end with your GIZAI_RESULT line, and say in your summary what to \
check and what is left.".into());
    lines
}
