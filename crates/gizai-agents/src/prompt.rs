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
    /// The project's main branch fetched for this run (from GitHub), when it has a link.
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


pub fn build(task: &TaskContext, role_instructions: &str) -> String {
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
        p.push_str(&format!("\n\n## Your branch\n\nThe main branch is {}, just fetched from GitHub.", b.from));
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
pub fn continue_prompt(reason: &str, limits: Option<RunLimits>) -> String {
    let mut p = format!("Your last run on this task was stopped: {}.\n\nContinue where you left off. First check `git status` and \
`git diff`: edits that were cut off may not have been saved. Then finish the task, commit, and end with your GIZAI_RESULT line.",
        reason.trim().trim_end_matches('.'));
    if let Some(l) = limits {
        p.push_str(&limits_section(l));
    }
    p.trim_end().to_string() + "\n"
}
