//! GA-179: Tauri runs a command that isn't async on the main thread, so the window waits for it. Looking up a program can
//! start your login shell (Linux: `runs::command_path`, `runs::find_claude`, gh's lookup), which may take seconds. A test
//! can't start the window, so this reads the commands' source: the chat's CLI list (`chat_clis`) and Settings → Coding
//! CLIs look their programs up in spawn_blocking, and no command that isn't async calls a lookup itself.
use std::path::Path;

/// Calls that look up a program, or reach a lookup without a thread of their own.
const LOOKUPS: [&str; 22] = [
    "command_path(", "claude_bin(", "detect_claude(", "find_claude(", "codex_at_first_start(",
    "clis::list(", "clis::find(", "clis::save(", "clis::spec(", "chat::clis(", "detect_gh(", "gh_bin(", "models_for(",
    "tools_view(", "browser_view(", "list_browser_tools(", "ask_cli_tools(", "browser_target(",
    // the Editor's apps and its update, and opening a terminal (editor.rs and terminal.rs call their own functions)
    " apps(", "open_in(", "update_now(", "open_with(",
];

/// A Tauri command as written: its name, whether it is async, and its body.
struct Command {
    name: String,
    is_async: bool,
    body: String,
}

/// The `#[tauri::command]` functions in `src`.
fn commands(src: &str) -> Vec<Command> {
    src.match_indices("#[tauri::command]").map(|(at, _)| {
        let rest = &src[at..];
        let fn_at = rest.find("fn ").unwrap_or_else(|| panic!("a command without fn: {}", rest.chars().take(80).collect::<String>()));
        let is_async = rest[..fn_at].contains("async");
        let name: String = rest[fn_at + 3..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        // the body opens at the first `{` outside the parameters' parentheses
        let mut parens = 0;
        let open = fn_at + rest[fn_at..].char_indices().find(|&(_, c)| {
            match c {
                '(' => parens += 1,
                ')' => parens -= 1,
                _ => {}
            }
            c == '{' && parens == 0
        }).unwrap_or_else(|| panic!("{name} has no body")).0;
        let mut depth = 0;
        let close = open + rest[open..].char_indices().find(|&(_, c)| {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
            depth == 0
        }).unwrap_or_else(|| panic!("{name} never closes")).0;
        Command { name, is_async, body: rest[open..=close].to_string() }
    }).collect()
}

/// The commands that aren't async in `src` with the lookups they call: (name, lookup).
fn sync_lookups(src: &str) -> Vec<(String, &'static str)> {
    commands(src).into_iter().filter(|c| !c.is_async)
        .flat_map(|c| LOOKUPS.iter().filter(|l| c.body.contains(**l)).map(|l| (c.name.clone(), *l)).collect::<Vec<_>>())
        .collect()
}

/// Every .rs file under `dir`, with its text.
fn sources(dir: &Path, out: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            sources(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push((p.display().to_string(), std::fs::read_to_string(&p).unwrap()));
        }
    }
}

fn all_commands() -> Vec<(String, Command)> {
    let mut files = vec![];
    sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    files.iter().flat_map(|(file, src)| commands(src).into_iter().map(|c| (file.clone(), c)).collect::<Vec<_>>()).collect()
}

#[test]
fn the_chats_cli_list_and_coding_clis_look_their_programs_up_off_the_main_thread() {
    let all = all_commands();
    for (name, call) in [("chat_clis", "chat::clis("), ("list_clis", "clis::list("), ("find_clis", "clis::find("),
                         ("save_clis", "clis::save(")] {
        let (file, c) = all.iter().find(|(_, c)| c.name == name).unwrap_or_else(|| panic!("no command {name}"));
        assert!(c.is_async, "{name} ({file}) isn't async, so it runs on the main thread");
        let spawned = c.body.find("spawn_blocking(").unwrap_or_else(|| panic!("{name} has no spawn_blocking:\n{}", c.body));
        let called = c.body.find(call).unwrap_or_else(|| panic!("{name} doesn't call {call}:\n{}", c.body));
        assert!(spawned < called, "{name} calls {call} outside spawn_blocking:\n{}", c.body);
    }
}

#[test]
fn no_command_on_the_main_thread_looks_up_a_program() {
    // The check itself: chat_clis as it was before GA-179 is caught, and an async command is not.
    assert_eq!(sync_lookups("#[tauri::command]\npub fn chat_clis(st: State<AppState>) -> R<Vec<chat::ChatCli>> { chat::clis(&st) }"),
               [("chat_clis".to_string(), "chat::clis(")]);
    assert!(sync_lookups("#[tauri::command]\npub async fn list_clis(st: State<'_, AppState>) -> R<()> {\n    clis::list(&st)\n}").is_empty());

    let all = all_commands();
    // GA-191: nearly every command is async now; the six that aren't are named in ga191_perf_test.rs
    assert!(all.len() > 100, "too few commands were read");
    let mut found = vec![];
    for (file, c) in &all {
        if !c.is_async {
            found.extend(LOOKUPS.iter().filter(|l| c.body.contains(**l)).map(|l| format!("{} ({file}) calls {l}", c.name)));
        }
    }
    assert!(found.is_empty(), "commands that aren't async look up a program on the main thread:\n{}", found.join("\n"));
}
