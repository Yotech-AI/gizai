// The coding CLIs an agent can run on (GA-3): each kind's command line, and how its output becomes RunEvents.
use gizai_agents::cli::{self, CliSpec, Kind, Parser, TaskRun};
use gizai_agents::stream::RunEvent;

fn spec(kind: Kind, args: &str) -> CliSpec {
    CliSpec { kind, bin: "/bin/fake".into(), env: vec![("CODEX_HOME".into(), "/home/u/.codex-2".into())], args: args.into() }
}

fn run() -> TaskRun {
    TaskRun { prompt: "Do the card".into(), session_id: "S-1".into(), ..Default::default() }
}

/// `-c` values, in order.
fn configs(args: &[String]) -> Vec<String> {
    args.windows(2).filter(|w| w[0] == "-c").map(|w| w[1].clone()).collect()
}

fn feed(kind: Kind, lines: &[&str]) -> Vec<RunEvent> {
    let mut p = Parser::new(kind);
    let mut out: Vec<RunEvent> = lines.iter().flat_map(|l| p.line(l)).collect();
    out.extend(p.finish());
    out
}

fn result_of(evs: &[RunEvent]) -> Option<(bool, String, String, i64, i64)> {
    evs.iter().rev().find_map(|e| match e {
        RunEvent::Result { is_error, subtype, text, input_tokens, output_tokens, cost_usd, .. } => {
            assert_eq!(*cost_usd, None, "only Claude Code reports cost");
            Some((*is_error, subtype.clone(), text.clone(), *input_tokens, *output_tokens))
        }
        _ => None,
    })
}

#[test]
fn kinds_round_trip_and_only_other_cannot_resume() {
    for k in [Kind::ClaudeCode, Kind::Codex, Kind::Gemini, Kind::Other] {
        assert_eq!(Kind::parse(k.key()), Some(k));
    }
    assert_eq!(Kind::parse("cursor"), None);
    assert!(Kind::ClaudeCode.can_resume() && Kind::Codex.can_resume() && Kind::Gemini.can_resume());
    assert!(!Kind::Other.can_resume());
}

#[test]
fn claude_code_runs_as_before_with_its_accounts_environment() {
    let mut s = spec(Kind::ClaudeCode, "");
    s.env = vec![("CLAUDE_CONFIG_DIR".into(), "/home/u/.claude-2".into())];
    let e = cli::task_exec(&s, &TaskRun { model: Some("opus".into()), effort: Some("max".into()), ..run() });
    assert_eq!(e.bin, std::path::PathBuf::from("/bin/fake"));
    assert_eq!(e.env, vec![("CLAUDE_CONFIG_DIR".to_string(), "/home/u/.claude-2".to_string())]);
    assert_eq!(e.stdin, "Do the card", "the prompt goes in on stdin");
    let a = e.args.join(" ");
    for want in ["-p", "--session-id S-1", "--permission-mode acceptEdits", "--disable-slash-commands", r#"{"disableAllHooks":true}"#, "--model opus", "--effort max"] {
        assert!(a.contains(want), "{want} missing in {a}");
    }
}

#[test]
fn codex_runs_exec_json_with_the_prompt_on_stdin_in_its_workspace_sandbox() {
    let e = cli::task_exec(&spec(Kind::Codex, ""), &TaskRun { writable_dirs: vec!["/repo/.git".into()], ..run() });
    assert_eq!(&e.args[..2], ["exec", "--json"]);
    assert_eq!(e.args.last().map(String::as_str), Some("-"), "the prompt is read from stdin");
    assert_eq!(e.stdin, "Do the card");
    assert_eq!(e.env, vec![("CODEX_HOME".to_string(), "/home/u/.codex-2".to_string())], "a second Codex account");
    // GA-55: web search is off until the agent's Web switch is on, so a run says so.
    assert_eq!(configs(&e.args), [
        r#"approval_policy="never""#,
        r#"web_search="disabled""#,
        r#"sandbox_mode="workspace-write""#,
        "sandbox_workspace_write.network_access=true",
        r#"sandbox_workspace_write.writable_roots=["/repo/.git"]"#,
    ]);
    assert!(!e.args.iter().any(|a| a == "-m" || a.contains("dangerously")), "{:?}", e.args);
    assert!(!e.args.contains(&"S-1".to_string()), "a new run doesn't name a session: Codex picks its thread id");
}

#[test]
fn codex_takes_model_effort_and_the_other_sandboxes() {
    let e = cli::task_exec(&spec(Kind::Codex, ""), &TaskRun { model: Some("gpt-5-codex".into()), effort: Some("high".into()),
        permission_mode: "read-only".into(), ..run() });
    let i = e.args.iter().position(|a| a == "-m").expect("-m");
    assert_eq!(e.args[i + 1], "gpt-5-codex");
    assert_eq!(configs(&e.args), [r#"model_reasoning_effort="high""#, r#"approval_policy="never""#, r#"web_search="disabled""#, r#"sandbox_mode="read-only""#]);

    let e = cli::task_exec(&spec(Kind::Codex, ""), &TaskRun { permission_mode: "danger-full-access".into(), writable_dirs: vec!["/r/.git".into()], ..run() });
    assert!(e.args.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()), "{:?}", e.args);
    assert!(!configs(&e.args).iter().any(|c| c.starts_with("sandbox")), "no sandbox settings without a sandbox: {:?}", e.args);
}

#[test]
fn codex_writable_roots_are_toml_strings() {
    let e = cli::task_exec(&spec(Kind::Codex, ""), &TaskRun { writable_dirs: vec![r#"/a "b"\c"#.into(), "/d".into()], ..run() });
    assert!(configs(&e.args).contains(&r#"sandbox_workspace_write.writable_roots=["/a \"b\"\\c","/d"]"#.to_string()), "{:?}", e.args);
}

#[test]
fn codex_continue_resumes_its_thread() {
    let e = cli::task_exec(&spec(Kind::Codex, ""), &TaskRun { resume: true, session_id: "019a-thread".into(), ..run() });
    assert_eq!(&e.args[..3], ["exec", "resume", "--json"]);
    let n = e.args.len();
    assert_eq!(&e.args[n - 2..], ["019a-thread", "-"], "the thread, then the prompt from stdin");
    assert_eq!(e.stdin, "Do the card");
}

#[test]
fn gemini_runs_headless_stream_json_with_gizais_session() {
    let e = cli::task_exec(&spec(Kind::Gemini, ""), &TaskRun { allowed_tools: vec!["Bash(git status:*)".into(), "Bash(npm test:*)".into(), "Read".into()], ..run() });
    let a = e.args.join(" ");
    for want in ["--output-format stream-json", "--skip-trust", "--approval-mode auto_edit", "--session-id S-1",
                 "--allowed-tools=run_shell_command(git status)", "--allowed-tools=run_shell_command(npm test)"] {
        assert!(a.contains(want), "{want} missing in {a}");
    }
    assert!(!a.contains("--model"), "no model: Gemini's default");
    assert!(!a.contains("Read"), "tools without a Gemini name are left out: {a}");
    let p = e.args.iter().position(|x| x == "-p").expect("-p makes it headless");
    assert!(!e.args[p + 1].contains("Do the card"), "the prompt itself goes on stdin");
    assert_eq!(e.stdin, "Do the card");

    let e = cli::task_exec(&spec(Kind::Gemini, ""), &TaskRun { resume: true, model: Some("gemini-2.5-pro".into()), permission_mode: "plan".into(), ..run() });
    let a = e.args.join(" ");
    assert!(a.contains("--resume S-1") && !a.contains("--session-id"), "{a}");
    assert!(a.contains("--model gemini-2.5-pro") && a.contains("--approval-mode plan"), "{a}");
}

#[test]
fn claude_allowed_commands_become_gemini_shell_tools() {
    assert_eq!(cli::gemini_tool("Bash(git status:*)").as_deref(), Some("run_shell_command(git status)"));
    assert_eq!(cli::gemini_tool("Bash(cargo test*)").as_deref(), Some("run_shell_command(cargo test)"));
    assert_eq!(cli::gemini_tool(" Bash(ls) ").as_deref(), Some("run_shell_command(ls)"));
    assert_eq!(cli::gemini_tool("Bash(:*)"), None);
    assert_eq!(cli::gemini_tool("Bash"), None);
    assert_eq!(cli::gemini_tool("Edit"), None);
}

#[test]
fn other_cli_arguments_follow_the_template() {
    // {prompt} as an argument: nothing on stdin
    let e = cli::task_exec(&spec(Kind::Other, "run -m {model} {prompt}"), &TaskRun { model: Some("anthropic/x".into()), ..run() });
    assert_eq!(e.args, ["run", "-m", "anthropic/x", "Do the card"]);
    assert_eq!(e.stdin, "");
    // no model: {model} and the option before it are left out
    let e = cli::task_exec(&spec(Kind::Other, "run -m {model} {prompt}"), &run());
    assert_eq!(e.args, ["run", "Do the card"]);
    // --model={model} is one argument: only it goes
    assert_eq!(cli::other_args("-p --model={model} {prompt}", "P", None).0, ["-p", "P"]);
    // without {prompt} the prompt goes in on stdin
    let e = cli::task_exec(&spec(Kind::Other, "run --quiet"), &run());
    assert_eq!((e.args.clone(), e.stdin.as_str()), (vec!["run".to_string(), "--quiet".to_string()], "Do the card"));
    // a prompt with spaces and quotes stays one argument
    let (a, stdin) = cli::other_args("ask --text='{prompt}'", "say \"hi\" now", None);
    assert_eq!((a, stdin), (vec!["ask".to_string(), "--text=say \"hi\" now".to_string()], false));
}

#[test]
fn other_cli_without_a_model_loses_only_the_model_argument_and_its_own_option() {
    let args = |t: &str, m: Option<&str>| cli::other_args(t, "P", m).0;
    // --model={model}: with a model it is one argument; without, the -p before it stays
    assert_eq!(args("-p --model={model} {prompt}", Some("x")), ["-p", "--model=x", "P"]);
    assert_eq!(args("-p --model={model} {prompt}", None), ["-p", "P"]);
    // Cursor Agent: only --model goes, the options before it and their values stay
    assert_eq!(args("-p --force --output-format text --model {model} {prompt}", None), ["-p", "--force", "--output-format", "text", "P"]);
    assert_eq!(args("-p --force --output-format text --model {model} {prompt}", Some("gpt-5")),
               ["-p", "--force", "--output-format", "text", "--model", "gpt-5", "P"]);
    // -m{model} holds it itself: it goes alone
    assert_eq!(args("run -m{model} {prompt}", None), ["run", "P"]);
    assert_eq!(args("-q -m{model} {prompt}", None), ["-q", "P"]);
    assert_eq!(args("run -m{model} {prompt}", Some("x")), ["run", "-mx", "P"]);
    // a value around {model} still belongs to the option before it
    assert_eq!(args("run --model openai/{model} {prompt}", None), ["run", "P"]);
    assert_eq!(args("run --model openai/{model} {prompt}", Some("x")), ["run", "--model", "openai/x", "P"]);
    // {model} first, or after a value: only itself
    assert_eq!(args("{model} {prompt}", None), ["P"]);
    assert_eq!(args("--format text {model} {prompt}", None), ["--format", "text", "P"]);
    // an option with its own value (--x=y) is never taken along
    assert_eq!(args("--quiet=yes {model} {prompt}", None), ["--quiet=yes", "P"]);
    // a prompt with spaces or quotes stays one argument, and no stdin
    let (a, stdin) = cli::other_args("-p --model={model} {prompt}", "fix 'a' and \"b\" now", None);
    assert_eq!((a, stdin), (vec!["-p".to_string(), "fix 'a' and \"b\" now".to_string()], false));
}

#[test]
fn arguments_split_like_a_shell_for_words_and_quotes() {
    assert_eq!(cli::split_args(r#"a  'b c' "d \"e\"" f\ g ''"#), ["a", "b c", "d \"e\"", "f g", ""]);
    assert!(cli::split_args("   ").is_empty());
}

#[test]
fn codex_json_becomes_run_events() {
    let evs = feed(Kind::Codex, &[
        r#"{"type":"thread.started","thread_id":"019a-thread"}"#,
        r#"{"type":"turn.started"}"#,
        r#"{"type":"item.completed","item":{"id":"item_0","type":"reasoning","text":"thinking"}}"#,
        r#"{"type":"item.started","item":{"id":"item_1","type":"command_execution","command":"bash -lc ls","aggregated_output":"","status":"in_progress"}}"#,
        r#"{"type":"item.completed","item":{"id":"item_1","type":"command_execution","command":"bash -lc ls","aggregated_output":"a\nb\n","exit_code":0,"status":"completed"}}"#,
        r#"{"type":"item.completed","item":{"id":"item_2","type":"command_execution","command":"false","aggregated_output":"","exit_code":1,"status":"failed"}}"#,
        r#"{"type":"item.completed","item":{"id":"item_3","type":"file_change","changes":[{"path":"src/a.rs","kind":"update"},{"path":"b.rs","kind":"add"}],"status":"completed"}}"#,
        r#"{"type":"item.completed","item":{"id":"item_4","type":"agent_message","text":"Done.\nGIZAI_RESULT: {\"outcome\":\"ready_for_testing\",\"summary\":\"ok\",\"issues\":[]}"}}"#,
        r#"{"type":"turn.completed","usage":{"input_tokens":1200,"cached_input_tokens":100,"output_tokens":340}}"#,
        "not json",
    ]);
    assert_eq!(evs[0], RunEvent::Init { session_id: "019a-thread".into(), model: String::new() });
    let tools: Vec<(String, String)> = evs.iter().filter_map(|e| match e { RunEvent::ToolUse { name, summary } => Some((name.clone(), summary.clone())), _ => None }).collect();
    assert_eq!(tools, [("Shell".into(), "bash -lc ls".into()), ("Shell".into(), "false".into()), ("Edit".into(), "src/a.rs, b.rs".into())],
               "a started command counts once when it completes");
    let results: Vec<(bool, String)> = evs.iter().filter_map(|e| match e { RunEvent::ToolResult { is_error, preview } => Some((*is_error, preview.clone())), _ => None }).collect();
    assert_eq!(results, [(false, "a\nb\n".to_string()), (true, String::new())]);
    let (is_error, subtype, text, input, output) = result_of(&evs).expect("turn.completed is the result");
    assert_eq!((is_error, subtype.as_str(), input, output), (false, "success", 1200, 340));
    assert!(text.contains("GIZAI_RESULT"), "the last message is the result: {text}");
    assert!(matches!(evs.last(), Some(RunEvent::Other { raw_type }) if raw_type == "invalid"), "{evs:?}");
}

#[test]
fn codex_failures_become_an_error_result() {
    let evs = feed(Kind::Codex, &[
        r#"{"type":"thread.started","thread_id":"t"}"#,
        r#"{"type":"error","message":"stream disconnected"}"#,
        r#"{"type":"turn.failed","error":{"message":"You've hit your usage limit."}}"#,
    ]);
    let (is_error, subtype, text, ..) = result_of(&evs).unwrap();
    assert_eq!((is_error, subtype.as_str(), text.as_str()), (true, "error", "You've hit your usage limit."));
    assert_eq!(evs.iter().filter(|e| matches!(e, RunEvent::Result { .. })).count(), 1, "finish adds no second result");

    // it ended after an error without a turn result: the error is the result
    let evs = feed(Kind::Codex, &[r#"{"type":"error","message":"Not logged in: run codex login"}"#]);
    let (is_error, _, text, ..) = result_of(&evs).unwrap();
    assert!(is_error && text.contains("codex login"), "{text}");
    // an error item inside the turn
    let evs = feed(Kind::Codex, &[r#"{"type":"item.completed","item":{"id":"e","type":"error","message":"sandbox denied"}}"#]);
    assert_eq!(result_of(&evs).map(|r| r.2), Some("sandbox denied".into()));
    // nothing at all: no result (the exit code tells)
    assert_eq!(result_of(&feed(Kind::Codex, &[])), None);
}

#[test]
fn gemini_stream_json_becomes_run_events() {
    let evs = feed(Kind::Gemini, &[
        r#"{"type":"init","timestamp":"t","session_id":"S-1","model":"gemini-2.5-pro"}"#,
        r#"{"type":"message","role":"user","content":"Do the card"}"#,
        r#"{"type":"message","role":"assistant","content":"Looking ","delta":true}"#,
        r#"{"type":"message","role":"assistant","content":"around.","delta":true}"#,
        r#"{"type":"tool_use","tool_name":"run_shell_command","tool_id":"1","parameters":{"command":"git status"}}"#,
        r#"{"type":"tool_result","tool_id":"1","status":"success","output":"clean"}"#,
        r#"{"type":"tool_use","tool_name":"write_file","tool_id":"2","parameters":{"file_path":"a.txt","content":"x"}}"#,
        r#"{"type":"tool_result","tool_id":"2","status":"error","error":{"type":"x","message":"denied"}}"#,
        r#"{"type":"message","role":"assistant","content":"All done.\nGIZAI_RESULT: {\"outcome\":\"ready_for_testing\"}","delta":true}"#,
        r#"{"type":"result","status":"success","stats":{"total_tokens":900,"input_tokens":700,"output_tokens":200,"duration_ms":1,"tool_calls":2}}"#,
    ]);
    assert_eq!(evs[0], RunEvent::Init { session_id: "S-1".into(), model: "gemini-2.5-pro".into() });
    assert!(evs.contains(&RunEvent::Text { text: "Looking around.".into() }), "deltas are joined: {evs:?}");
    assert!(!evs.iter().any(|e| matches!(e, RunEvent::Text { text } if text.contains("Do the card"))), "the user's prompt isn't shown");
    assert!(evs.contains(&RunEvent::ToolUse { name: "run_shell_command".into(), summary: "git status".into() }));
    assert!(evs.contains(&RunEvent::ToolUse { name: "write_file".into(), summary: "a.txt".into() }));
    assert!(evs.contains(&RunEvent::ToolResult { is_error: false, preview: "clean".into() }));
    assert!(evs.contains(&RunEvent::ToolResult { is_error: true, preview: "denied".into() }));
    let (is_error, _, text, input, output) = result_of(&evs).unwrap();
    assert_eq!((is_error, input, output), (false, 700, 200));
    assert!(text.starts_with("All done.") && text.contains("GIZAI_RESULT") && !text.contains("Looking"), "the text after the last tool: {text}");
    assert!(evs.iter().position(|e| matches!(e, RunEvent::Text { text } if text.starts_with("All done"))).unwrap()
        < evs.iter().position(|e| matches!(e, RunEvent::Result { .. })).unwrap(), "the answer shows before the result");
}

#[test]
fn gemini_errors_become_an_error_result() {
    let evs = feed(Kind::Gemini, &[r#"{"type":"result","status":"error","error":{"type":"FatalAuthenticationError","message":"Please set an Auth method"}}"#]);
    let (is_error, subtype, text, ..) = result_of(&evs).unwrap();
    assert_eq!((is_error, subtype.as_str(), text.as_str()), (true, "error", "Please set an Auth method"));
    let evs = feed(Kind::Gemini, &[r#"{"type":"error","severity":"error","message":"Quota exceeded"}"#]);
    assert_eq!(result_of(&evs).map(|r| (r.0, r.2)), Some((true, "Quota exceeded".into())), "an error without a result line is the result");
    let evs = feed(Kind::Gemini, &[r#"{"type":"message","role":"assistant","content":"half an answer"}"#]);
    assert_eq!(evs, [RunEvent::Text { text: "half an answer".into() }], "text still due is shown when the output ends");
}

#[test]
fn plain_text_output_is_shown_and_its_end_is_the_result() {
    let evs = feed(Kind::Other, &[
        "\u{1b}[32mStarting\u{1b}[0m",
        "",
        "working 10%\rworking 100%",
        "GIZAI_RESULT: {\"outcome\":\"ready_for_testing\",\"summary\":\"s\",\"issues\":[]}",
    ]);
    assert_eq!(evs[..3], [RunEvent::Text { text: "Starting".into() }, RunEvent::Text { text: "working 100%".into() },
        RunEvent::Text { text: "GIZAI_RESULT: {\"outcome\":\"ready_for_testing\",\"summary\":\"s\",\"issues\":[]}".into() }]);
    let (is_error, _, text, ..) = result_of(&evs).unwrap();
    assert!(!is_error && text.starts_with("Starting") && text.ends_with("\"issues\":[]}"), "{text}");
    assert_eq!(cli::plain("\u{1b}[1;31mred\u{1b}[0m\ttab\u{7}"), "red\ttab");
}

#[test]
fn a_long_plain_text_run_keeps_the_end_where_the_result_line_is() {
    let mut p = Parser::new(Kind::Other);
    let line = "é".repeat(500);
    for _ in 0..400 {
        p.line(&line);
    }
    p.line("GIZAI_RESULT: {\"outcome\":\"ready_for_testing\"}");
    let evs = p.finish();
    let (_, _, text, ..) = result_of(&evs).unwrap();
    assert!(text.len() <= 32 * 1024 && text.ends_with("{\"outcome\":\"ready_for_testing\"}"), "{}", text.len());
}

#[test]
fn a_finished_runs_log_is_read_with_its_clis_parser() {
    // Claude Code's logs have no header: read as before
    let claude = r#"{"type":"system","subtype":"init","session_id":"S","model":"opus"}"#;
    assert_eq!(cli::parse_log(claude), [RunEvent::Init { session_id: "S".into(), model: "opus".into() }]);
    // other CLIs: Gizai's header line names the kind
    let header = cli::log_header(Kind::Codex);
    assert!(header.contains("gizai_cli") && header.contains("codex"), "{header}");
    let log = format!("{header}\n{}\n{}\n", r#"{"type":"thread.started","thread_id":"T"}"#, r#"{"type":"error","message":"boom"}"#);
    let evs = cli::parse_log(&log);
    assert_eq!(evs[0], RunEvent::Init { session_id: "T".into(), model: String::new() });
    assert_eq!(result_of(&evs).map(|r| r.2), Some("boom".into()), "finish runs on a log too");
    let evs = cli::parse_log(&format!("{}\nhello\n", cli::log_header(Kind::Other)));
    assert_eq!(evs[0], RunEvent::Text { text: "hello".into() });
    assert!(cli::parse_log("").is_empty());
}

#[test]
fn every_cli_gets_the_runs_temp_folder_as_tmpdir_tmp_and_temp_after_its_own_environment() {
    // GA-48: the CLI's own lines first, then the temp folder, so a CLI's TMPDIR line can't send the run back to /tmp
    let temp = "/w/KADE-1/.gizai-tmp";
    let want: Vec<(String, String)> = ["TMPDIR", "TMP", "TEMP"].iter().map(|k| (k.to_string(), temp.to_string())).collect();
    for kind in [Kind::ClaudeCode, Kind::Codex, Kind::Gemini, Kind::Other] {
        let mut s = spec(kind, "");
        s.env.push(("TMPDIR".into(), "/tmp".into()));
        let e = cli::task_exec(&s, &TaskRun { temp_dir: Some(temp.into()), ..run() });
        assert_eq!(&e.env[..2], &s.env[..], "{kind:?}: the CLI's own lines stay");
        assert_eq!(&e.env[2..], &want[..], "{kind:?}");
        // without a temp folder (Gizai couldn't make it), the environment is the CLI's own
        assert_eq!(cli::task_exec(&s, &run()).env, s.env, "{kind:?}");
    }
}

#[test]
fn claude_codes_refusals_show_as_they_happen_with_their_reason_and_the_result_line_adds_only_new_ones() {
    let lines: Vec<&str> = include_str!("fixtures/run-refused.jsonl").lines().collect();
    let refused = |evs: &[RunEvent]| -> Vec<(String, String, String)> {
        evs.iter().filter_map(|e| match e {
            RunEvent::Refused { tool, input, reason } => Some((tool.clone(), input.clone(), reason.clone())),
            _ => None,
        }).collect()
    };
    let evs = feed(Kind::ClaudeCode, &lines);
    let got = refused(&evs);
    assert_eq!(got.iter().map(|(t, i, _)| (t.as_str(), i.as_str())).collect::<Vec<_>>(), [
        ("Bash", "cat <<EOF\nhello\nEOF"), ("Bash", "ls /tmp"), ("Bash", "git status --short > /tmp/ga48-check-b.txt"),
        ("Write", "/tmp/ga48-check-write.txt")]);
    assert_eq!(got[0].2, "Heredoc with unquoted delimiter undergoes shell expansion");
    assert_eq!(got[3].2, "Path is outside allowed working directories", "decision_reason before the message");
    assert!(got[1].2.starts_with("ls in '/tmp' was blocked"), "only a message: {}", got[1].2);
    // each one where it happened: right after the call that asked for it
    let at = evs.iter().position(|e| matches!(e, RunEvent::Refused { .. })).unwrap();
    assert!(matches!(&evs[at - 1], RunEvent::ToolUse { name, summary } if name == "Bash" && summary.contains("cat <<EOF")), "{:?}", &evs[at - 1]);
    assert!(matches!(evs.last(), Some(RunEvent::Result { is_error: false, .. })), "{:?}", evs.last());

    // stopped before its result line (Stop, a limit): the refusals so far stay
    assert_eq!(refused(&feed(Kind::ClaudeCode, &lines[..lines.len() - 1])), got);
    // only the result line (no permission_denied lines): every denial, without a reason
    let only = refused(&feed(Kind::ClaudeCode, &[lines[lines.len() - 1]]));
    assert_eq!(only.iter().map(|(t, i, r)| (t.as_str(), i.as_str(), r.as_str())).collect::<Vec<_>>(), [
        ("Bash", "cat <<EOF\nhello\nEOF", ""), ("Bash", "ls /tmp", ""), ("Bash", "git status --short > /tmp/ga48-check-b.txt", ""),
        ("Write", "/tmp/ga48-check-write.txt", "")]);
    // a log read back gives the same as the live run
    assert_eq!(refused(&cli::parse_log(include_str!("fixtures/run-refused.jsonl"))), got);
}
