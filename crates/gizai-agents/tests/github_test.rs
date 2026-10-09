// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use gizai_agents::github::{self, PullRequest};
use std::path::{Path, PathBuf};

/// Writes an executable script through a child `sh`, so this process never holds it open for writing: such a
/// handle leaks into any process another test thread starts at that moment, and running the script would then
/// fail with "Text file busy" (ETXTBSY).
fn write_script(path: &Path, script: &str) {
    use std::io::Write;
    let mut sh = std::process::Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path)
        .stdin(std::process::Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

/// A fake gh in `dir`, never the real one. It writes each call's arguments to dir/gh-args (one line per call) and
/// GH_PROMPT_DISABLED to dir/gh-env; `pr list` answers with dir/list.json (nothing when it is missing); `pr create`
/// keeps its stdin in dir/gh-stdin and fails with dir/create.err, else answers with dir/create.out.
fn fake_gh(dir: &Path) -> PathBuf {
    let gh = dir.join("gh");
    write_script(&gh, &format!(r#"#!/bin/sh
d='{d}'
echo "$*" >> "$d/gh-args"
echo "prompt=$GH_PROMPT_DISABLED" >> "$d/gh-env"
case "$1 $2" in
  "pr list") [ -f "$d/list.json" ] && cat "$d/list.json"; exit 0 ;;
  "pr create")
    cat > "$d/gh-stdin"
    if [ -f "$d/create.err" ]; then cat "$d/create.err" >&2; exit 1; fi
    cat "$d/create.out"; exit 0 ;;
esac
echo "unknown command: $*" >&2; exit 1
"#, d = dir.display()));
    gh
}

fn read(p: PathBuf) -> String { std::fs::read_to_string(p).unwrap_or_default() }

#[test]
fn pull_request_links_and_numbers_are_read_from_ghs_answers() {
    let created = "Creating pull request for gizai/sh-1-x into main in acme/shop\n\nhttps://github.com/acme/shop/pull/12\n";
    assert_eq!(github::pull_url(created).as_deref(), Some("https://github.com/acme/shop/pull/12"));
    let exists = "a pull request for branch \"gizai/sh-1-x\" into branch \"main\" already exists:\nhttps://github.com/acme/shop/pull/3.";
    assert_eq!(github::pull_url(exists).as_deref(), Some("https://github.com/acme/shop/pull/3"), "trailing dot dropped");
    assert_eq!(github::pull_url("see (https://github.com/acme/shop/pull/1) and https://github.com/acme/shop/pull/2").as_deref(),
               Some("https://github.com/acme/shop/pull/2"), "the last link");
    assert_eq!(github::pull_url("https://github.com/acme/shop/issues/4"), None, "an issue isn't a pull request");
    assert_eq!(github::pull_url("https://github.com/acme/shop/pull/new"), None);
    assert_eq!(github::pull_url("https://gitlab.com/acme/shop/pull/4"), None);
    assert_eq!(github::pull_url(""), None);
    assert_eq!(github::pull_number("https://github.com/acme/shop/pull/12"), Some(12));
    assert_eq!(github::pull_number("https://github.com/acme/shop/pull/12/"), Some(12));
    assert_eq!(github::pull_number("https://github.com/acme/shop"), None);
}

#[test]
fn a_pull_requests_state_and_commits_as_gizai_names_them() {
    let prs: Vec<PullRequest> = serde_json::from_str(r#"[
        {"number":1,"url":"https://github.com/acme/shop/pull/1","state":"OPEN","isDraft":false,"headRefOid":"aaa","commits":[{"oid":"zzz"},{"oid":"aaa"}]},
        {"number":2,"url":"https://github.com/acme/shop/pull/2","state":"OPEN","isDraft":true,"headRefOid":"bbb","commits":[]},
        {"number":3,"url":"https://github.com/acme/shop/pull/3","state":"MERGED","isDraft":false,"headRefOid":"ccc","commits":[]},
        {"number":4,"url":"https://github.com/acme/shop/pull/4","state":"CLOSED","isDraft":true,"headRefOid":"ddd","commits":[]},
        {"number":5,"url":"https://github.com/acme/shop/pull/5","state":"OPEN"}
    ]"#).unwrap();
    let states: Vec<&str> = prs.iter().map(PullRequest::state).collect();
    assert_eq!(states, ["open", "draft", "merged", "closed", "open"]);
    assert!(prs[0].contains("aaa") && prs[0].contains("zzz"), "its latest commit and an earlier one");
    assert!(!prs[0].contains("ccc") && !prs[0].contains(""), "another commit, or none");
    assert!(!prs[4].contains(""), "a missing headRefOid is no match for an empty sha");
}

#[test]
fn the_pull_requests_of_a_branch_are_asked_in_every_state_without_prompts() {
    let tmp = tempfile::tempdir().unwrap();
    let gh = fake_gh(tmp.path());
    assert_eq!(github::pulls_for_branch(&gh, tmp.path(), "acme/shop", "gizai/sh-1-x").unwrap(), vec![], "no answer: no pull requests");
    assert_eq!(read(tmp.path().join("gh-args")).trim(),
               "pr list --repo acme/shop --head gizai/sh-1-x --state all --limit 20 --json number,url,state,isDraft,headRefOid,commits");
    assert_eq!(read(tmp.path().join("gh-env")).trim(), "prompt=1");
    std::fs::write(tmp.path().join("list.json"),
        r#"[{"number":9,"url":"https://github.com/acme/shop/pull/9","state":"MERGED","isDraft":false,"headRefOid":"abc","commits":[{"oid":"abc"}]}]"#).unwrap();
    let prs = github::pulls_for_branch(&gh, tmp.path(), "acme/shop", "gizai/sh-1-x").unwrap();
    assert_eq!((prs.len(), prs[0].number, prs[0].state()), (1, 9, "merged"));
    std::fs::write(tmp.path().join("list.json"), "not json").unwrap();
    let e = github::pulls_for_branch(&gh, tmp.path(), "acme/shop", "gizai/sh-1-x").unwrap_err();
    assert!(e.contains("gh gave an answer Gizai can't read"), "{e}");
}

#[test]
fn opening_a_pull_request_sends_the_body_on_stdin_and_returns_its_link() {
    let tmp = tempfile::tempdir().unwrap();
    let gh = fake_gh(tmp.path());
    std::fs::write(tmp.path().join("create.out"), "https://github.com/acme/shop/pull/8\n").unwrap();
    let body = "He said \"ship it\" and it's $HOME\n\n- `one`\n- two\n".repeat(200);
    let url = github::create_pull(&gh, tmp.path(), "acme/shop", "gizai/sh-1-x", "main", "SH-1: Export \"all\" invoices", &body).unwrap();
    assert_eq!(url, "https://github.com/acme/shop/pull/8");
    assert_eq!(read(tmp.path().join("gh-stdin")), body, "the body arrives untouched, quotes and all");
    assert_eq!(read(tmp.path().join("gh-args")).trim(),
               "pr create --repo acme/shop --head gizai/sh-1-x --base main --title SH-1: Export \"all\" invoices --body-file -");
}

#[test]
fn a_branch_that_already_has_a_pull_request_gets_its_link() {
    let tmp = tempfile::tempdir().unwrap();
    let gh = fake_gh(tmp.path());
    std::fs::write(tmp.path().join("create.err"),
        "a pull request for branch \"gizai/sh-1-x\" into branch \"main\" already exists:\nhttps://github.com/acme/shop/pull/5\n").unwrap();
    let url = github::create_pull(&gh, tmp.path(), "acme/shop", "gizai/sh-1-x", "main", "t", "b").unwrap();
    assert_eq!(url, "https://github.com/acme/shop/pull/5");
}

#[test]
fn gh_problems_are_plain_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let gh = fake_gh(tmp.path());
    // not logged in
    std::fs::write(tmp.path().join("create.err"), "To get started with GitHub CLI, please run:  gh auth login\n").unwrap();
    let e = github::create_pull(&gh, tmp.path(), "acme/shop", "b", "main", "t", "b").unwrap_err();
    assert_eq!(e, "The GitHub CLI isn't logged in: run gh auth login in a terminal");
    // gh's own message
    std::fs::write(tmp.path().join("create.err"), "pull request create failed: GraphQL: No commits between main and b\n").unwrap();
    let e = github::create_pull(&gh, tmp.path(), "acme/shop", "b", "main", "t", "b").unwrap_err();
    assert!(e.contains("No commits between main and b"), "{e}");
    // an answer without a link
    std::fs::remove_file(tmp.path().join("create.err")).unwrap();
    std::fs::write(tmp.path().join("create.out"), "done\n").unwrap();
    let e = github::create_pull(&gh, tmp.path(), "acme/shop", "b", "main", "t", "b").unwrap_err();
    assert!(e.contains("gh didn't say which pull request it opened"), "{e}");
    // no gh at all
    let e = github::pulls_for_branch(Path::new("/nonexistent/gh"), tmp.path(), "acme/shop", "b").unwrap_err();
    assert!(e.starts_with("GitHub CLI not found at /nonexistent/gh"), "{e}");
}
