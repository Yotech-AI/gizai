// GA-45: the agent's folders on each CLI's command line, and Gizai's notes in a run log.
use gizai_agents::cli::{self, CliSpec, Kind, RunFolder, TaskRun};
use gizai_agents::stream::{self, RunEvent};

fn spec(kind: Kind) -> CliSpec {
    CliSpec { kind, bin: "/bin/fake".into(), env: vec![], args: "{prompt}".into() }
}

fn folder(path: &str, change: bool) -> RunFolder {
    RunFolder { path: path.into(), change }
}

/// A read folder, a read and change folder, and a read folder whose path has a space.
fn run() -> TaskRun {
    TaskRun { prompt: "Do the card".into(), session_id: "S-1".into(), allowed_tools: vec!["Bash(npm test:*)".into()],
              folders: vec![folder("/srv/shared", false), folder("/srv/out", true), folder("/home/u/My docs", false)], ..Default::default() }
}

/// The values after the variadic option `flag`, up to the next option.
fn values(args: &[String], flag: &str) -> Vec<String> {
    let at = args.iter().position(|a| a == flag).unwrap_or_else(|| panic!("{flag} missing in {args:?}"));
    args[at + 1..].iter().take_while(|a| !a.starts_with("--")).cloned().collect()
}

#[test]
fn claude_code_gets_add_dir_for_each_folder_and_deny_rules_for_edit_and_write_in_each_read_folder() {
    let e = cli::task_exec(&spec(Kind::ClaudeCode), &run());
    assert_eq!(values(&e.args, "--add-dir"), ["/srv/shared", "/srv/out", "/home/u/My docs"]);
    assert_eq!(values(&e.args, "--disallowedTools"),
               ["Edit(//srv/shared/**)", "Write(//srv/shared/**)", "Edit(//home/u/My docs/**)", "Write(//home/u/My docs/**)"],
               "// starts an absolute path in a Claude Code rule; the read and change folder gets none");
    assert_eq!(values(&e.args, "--allowedTools"), ["Bash(npm test:*)"], "the allowed commands stay as they were");
    assert_eq!(e.args.iter().filter(|a| *a == "--add-dir").count(), 1);
    assert_eq!(e.stdin, "Do the card", "the prompt still goes in on stdin, not after a variadic option");
}

#[test]
fn claude_codes_deny_rules_take_no_slash_at_the_end_twice() {
    assert_eq!(cli::claude_read_only(&[folder("/srv/a/", false), folder("/srv/b", true)]), ["Edit(//srv/a/**)", "Write(//srv/a/**)"]);
    assert!(cli::claude_read_only(&[folder("/srv/b", true)]).is_empty());
}

#[test]
fn without_folders_claude_code_runs_as_before() {
    let e = cli::task_exec(&spec(Kind::ClaudeCode), &TaskRun { folders: vec![], ..run() });
    assert!(!e.args.iter().any(|a| a == "--add-dir" || a == "--disallowedTools"), "{:?}", e.args);
    let only_change = cli::task_exec(&spec(Kind::ClaudeCode), &TaskRun { folders: vec![folder("/srv/out", true)], ..run() });
    assert_eq!(values(&only_change.args, "--add-dir"), ["/srv/out"]);
    assert!(!only_change.args.iter().any(|a| a == "--disallowedTools"), "no read folder, no deny rule");
}

#[test]
fn codex_makes_only_the_read_and_change_folders_writable_roots() {
    let e = cli::task_exec(&spec(Kind::Codex), &TaskRun { writable_dirs: vec!["/repo/.git".into()], ..run() });
    let roots: Vec<&String> = e.args.iter().filter(|a| a.starts_with("sandbox_workspace_write.writable_roots=")).collect();
    assert_eq!(roots, [r#"sandbox_workspace_write.writable_roots=["/repo/.git","/srv/out"]"#]);
    assert!(!e.args.iter().any(|a| a.contains("/srv/shared")), "a read folder: Codex reads every folder anyway");
    // without a git folder the read and change folder is still a writable root
    let e = cli::task_exec(&spec(Kind::Codex), &run());
    assert!(e.args.contains(&r#"sandbox_workspace_write.writable_roots=["/srv/out"]"#.to_string()), "{:?}", e.args);
    // its read-only sandbox writes nothing
    let e = cli::task_exec(&spec(Kind::Codex), &TaskRun { permission_mode: "read-only".into(), ..run() });
    assert!(!e.args.iter().any(|a| a.contains("writable_roots")), "{:?}", e.args);
}

#[test]
fn gemini_gets_include_directories_for_the_read_and_change_folders_only() {
    let e = cli::task_exec(&spec(Kind::Gemini), &run());
    let dirs: Vec<&String> = e.args.iter().filter(|a| a.starts_with("--include-directories")).collect();
    assert_eq!(dirs, ["--include-directories=/srv/out"]);
    let p = e.args.iter().position(|a| a == "-p").unwrap();
    assert!(e.args.iter().position(|a| a == "--include-directories=/srv/out").unwrap() < p, "before -p");
}

#[test]
fn an_other_cli_gets_no_folders() {
    let e = cli::task_exec(&spec(Kind::Other), &run());
    assert!(!e.args.iter().any(|a| a.contains("/srv/")), "{:?}", e.args);
}

#[test]
fn the_run_log_says_which_folders_a_cli_cant_be_given() {
    let f = run().folders;
    assert_eq!(cli::folders_left_out(Kind::ClaudeCode, &f), None);
    assert_eq!(cli::folders_left_out(Kind::Codex, &f), None);
    assert_eq!(cli::folders_left_out(Kind::Gemini, &f).as_deref(),
               Some("Left out the folders /srv/shared, /home/u/My docs: Gemini can't keep a folder read only."));
    assert_eq!(cli::folders_left_out(Kind::Gemini, &[folder("/srv/a", false)]).as_deref(),
               Some("Left out the folder /srv/a: Gemini can't keep a folder read only."));
    assert_eq!(cli::folders_left_out(Kind::Gemini, &[folder("/srv/out", true)]), None);
    assert_eq!(cli::folders_left_out(Kind::Other, &[folder("/srv/out", true)]).as_deref(),
               Some("Left out the folder /srv/out: this CLI can't be given folders."));
    assert_eq!(cli::folders_left_out(Kind::Other, &[]), None);
}

#[test]
fn gizais_notes_in_a_log_become_note_events_first() {
    let note = cli::note_line("Skipped the folder /srv/gone: it's missing, so this run goes without it.");
    let want = RunEvent::Note { text: "Skipped the folder /srv/gone: it's missing, so this run goes without it.".into() };
    // Claude Code's log: no header, the notes, then its own output
    let claude = format!("{note}\n{}\n", r#"{"type":"system","subtype":"init","session_id":"S","model":"opus"}"#);
    assert_eq!(cli::parse_log(&claude), [want.clone(), RunEvent::Init { session_id: "S".into(), model: "opus".into() }]);
    // another CLI's log: the header, the notes, then its output
    let codex = format!("{}\n{note}\n{}\n", cli::log_header(Kind::Codex), r#"{"type":"thread.started","thread_id":"T"}"#);
    let evs = cli::parse_log(&codex);
    assert_eq!(evs[..2], [want.clone(), RunEvent::Init { session_id: "T".into(), model: String::new() }]);
    let other = format!("{}\n{note}\nhello\n", cli::log_header(Kind::Other));
    assert_eq!(cli::parse_log(&other)[..2], [want.clone(), RunEvent::Text { text: "hello".into() }]);
    // the stream parser knows the line too, and the UI gets kind "note"
    assert_eq!(stream::parse_line(&note), [want.clone()]);
    assert_eq!(serde_json::to_value(&want).unwrap(), serde_json::json!({"kind": "note", "text": want_text(&want)}));
}

fn want_text(e: &RunEvent) -> String {
    match e { RunEvent::Note { text } => text.clone(), _ => unreachable!() }
}
