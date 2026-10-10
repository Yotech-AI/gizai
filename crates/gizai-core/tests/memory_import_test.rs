//! GA-85 QA: Claude Code's own memory notes into Gizai's memory (`memory_import`). Scratch account folders only (a
//! scratch home with `.claude` and `.claude-2`), never your own `~/.claude*`: the account folders of the Claude Code CLIs
//! in Settings, each file one note in `Team Lead/Imported/` with its source in its properties, `MEMORY.md` and the rest
//! left out, a second start that adds nothing, a file with a secret skipped and listed by its path only, the open thread
//! in `Team Lead/Notes`, the files left as they were, and the imported notes kept out of every other agent's prompt.
//! Round 3: only the folders Claude Code keeps for Gizai's own places come in (`places`, `project_folder`); the others are
//! counted, never read. The paths here run on every system: `place` gives `C:\home\me\shop` on Windows, and Windows lists a
//! file as `~\.claude\…` (`shown`).
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gizai_core::clis::{self, Cli};
use gizai_core::db::Db;
use gizai_core::memory::{self, Context, Who};
use gizai_core::memory_import::{self, LeftOut, Report, Skipped};
use gizai_core::model::*;
use gizai_core::{folders, projects, seed, settings, tasks, team};

const DAY: &str = "2026-10-10";
/// A GitHub token as the secret check sees one (made up).
const TOKEN: &str = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";

/// The places of Gizai's (projects' linked folders) whose Claude Code folders most tests write in.
const SHOP: &str = "/home/me/shop";
const BLOG: &str = "/home/me/blog";
const GIZAI: &str = "/home/jefsev/Herd/gizai";
const OTUS: &str = "/home/jefsev/Herd/otus";
const LONG: &str = "/home/me/a-project-with-a-rather-long-folder-name";
const P: &str = "/p";
const Q: &str = "/q";
const PLACES: [&str; 7] = [SHOP, BLOG, GIZAI, OTUS, LONG, P, Q];

/// A path of these tests as this system writes it: `/home/me/shop`, on Windows `C:\home\me\shop`.
fn place(unix: &str) -> PathBuf {
    if cfg!(windows) { PathBuf::from(format!("C:{}", unix.replace('/', "\\"))) } else { PathBuf::from(unix) }
}

/// The folder Claude Code keeps that path in: `-home-me-shop`, on Windows `C--home-me-shop`.
fn folder(unix: &str) -> String {
    memory_import::project_folder(&place(unix).to_string_lossy())
}

/// A file as the import lists it on this system: `~/.claude/projects/…`, on Windows `~\.claude\projects\…`.
fn shown(path: &str) -> String {
    path.replace('/', std::path::MAIN_SEPARATOR_STR)
}

struct F {
    db: Db,
    you: String,
    team: String,
    lead: String,
    home: PathBuf,
    tmp: tempfile::TempDir,
}

/// A scratch home folder, a Team Lead (Chat on) and Jeffrey.
fn setup() -> F {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let lead = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput {
        name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap();
    F { db, you: s.you_id, team: s.team_id, lead, home, tmp }
}

impl F {
    fn home(&self) -> &str { self.home.to_str().unwrap() }
    /// Writes `text` at `rel` under the scratch home (folders made as needed).
    fn file(&self, rel: &str, text: &[u8]) -> PathBuf {
        let p = self.home.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
        p
    }
    /// The account folders as Gizai finds them, without CLAUDE_CONFIG_DIR of its own.
    fn dirs(&self) -> Vec<PathBuf> {
        memory_import::claude_dirs(&self.db, self.home(), &|_| None).unwrap()
    }
    /// The import with `PLACES` as Gizai's places.
    fn import(&self) -> Report {
        self.import_with(&PLACES.map(place))
    }
    fn import_with(&self, places: &[PathBuf]) -> Report {
        memory_import::import(&self.db, &self.dirs(), places, self.home(), &self.you, "Jeffrey", DAY).unwrap()
    }
    fn lead(&self) -> Who { Who::Lead(self.lead.clone()) }
    fn note(&self, path: &str) -> memory::Note {
        memory::find(&self.db, path).unwrap().unwrap_or_else(|| panic!("no note {path}: {:?}", self.paths()))
    }
    fn paths(&self) -> Vec<String> {
        memory::list(&self.db, &self.lead()).unwrap().into_iter().map(|n| n.path).collect()
    }
    fn imported(&self) -> Vec<String> {
        self.paths().into_iter().filter(|p| p.starts_with("Team Lead/Imported/")).collect()
    }
    /// The scratch folder by its real path as Gizai writes paths (on macOS /private/var/…, on Windows without `\\?\`).
    fn root(&self) -> PathBuf {
        folders::normalize(&std::fs::canonicalize(self.tmp.path()).unwrap().to_string_lossy(), Path::new("")).unwrap()
    }
    /// A project with this key, linked to folder `linked` (None: no linked folder).
    fn project(&self, key: &str, linked: Option<&Path>) -> String {
        projects::create(&self.db, &self.you, ProjectInput { name: format!("Project {key}"), key: key.into(),
            repo_path: linked.map(|p| p.display().to_string()), ..Default::default() }).unwrap()
    }
    /// A card on project `project`: its identifier (`SHOP-1`).
    fn card(&self, project: &str, title: &str) -> String {
        let id = tasks::create(&self.db, &self.you, TaskInput { project_id: project.into(), title: title.into(), ..Default::default() }).unwrap();
        tasks::get(&self.db, &id).unwrap().identifier
    }
}

/// Every file under `dir` with its bytes and modified time: to show nothing in an account folder changed.
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, (Vec<u8>, std::time::SystemTime)> {
    let mut out = BTreeMap::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                todo.push(p);
            } else {
                out.insert(p.clone(), (std::fs::read(&p).unwrap(), std::fs::metadata(&p).unwrap().modified().unwrap()));
            }
        }
    }
    out
}

/// The properties (front matter) of a note and the text after them.
fn split(body: &str) -> (&str, &str) {
    let rest = body.strip_prefix("---\n").expect("properties first");
    let end = rest.find("\n---\n").expect("properties end");
    (&rest[..end + 1], &rest[end + 5..])
}

const DEPLOY_GA: &str = "---\nname: deploy-GA\ndescription: \"How Gizai is released\"\nmetadata:\n  type: reference\n---\n\nDeploy mode: agent.\n\
Release: main -> production PR, then a GitHub Release vX.Y.Z.\n";
const PREFS: &str = "# Jeffrey's preferences\n\nShort answers, plain words.\n";

/// Two accounts: ~/.claude (the built-in Claude Code) and ~/.claude-2 (a second Claude Code in Settings), a Codex in
/// Settings, and a third Claude Code entry on ~/.claude again.
fn two_accounts(f: &F) {
    clis::save(&f.db, vec![
        Cli { name: "Claude Code (2nd account)".into(), kind: "claude_code".into(), command: "claude".into(),
              env: vec!["CLAUDE_CONFIG_DIR=~/.claude-2".into()], ..Default::default() },
        Cli { name: "Claude Code again".into(), kind: "claude_code".into(), command: "claude".into(),
              env: vec!["CLAUDE_CONFIG_DIR=$HOME/.claude/".into()], ..Default::default() },
        Cli { name: "Codex".into(), kind: "codex".into(), command: "codex".into(), env: vec!["CODEX_HOME=~/.codex".into()], ..Default::default() },
    ]).unwrap();
    let (shop, blog, gizai) = (folder(SHOP), folder(BLOG), folder(GIZAI));
    // ~/.claude: two projects
    f.file(&format!(".claude/projects/{shop}/memory/MEMORY.md"), b"- [deploy-SHOP](deploy-SHOP.md) - how SHOP deploys\n- [prefs](prefs.md)\n");
    f.file(&format!(".claude/projects/{shop}/memory/deploy-SHOP.md"), b"---\nname: deploy-SHOP\nsource: hand-written\n---\nDeploy mode: manual.\n");
    f.file(&format!(".claude/projects/{shop}/memory/prefs.md"), PREFS.as_bytes());
    f.file(&format!(".claude/projects/{blog}/memory/lessons.MD"), b"Lesson: the blog builds with hugo.\n");
    // not notes: a file that isn't Markdown, a folder below memory/, a memory folder's index, a file outside memory/
    f.file(&format!(".claude/projects/{blog}/memory/todo.txt"), b"not Markdown\n");
    f.file(&format!(".claude/projects/{blog}/memory/old/archived.md"), b"in a folder below memory/\n");
    f.file(&format!(".claude/projects/{blog}/memory/memory.md"), b"the index, in lower case\n");
    f.file(&format!(".claude/projects/{blog}/CLAUDE.md"), b"not in memory/\n");
    // ~/.claude-2: Gizai's own project, with a token in one file
    f.file(&format!(".claude-2/projects/{gizai}/memory/MEMORY.md"), b"- [deploy-GA](deploy-GA.md)\n");
    f.file(&format!(".claude-2/projects/{gizai}/memory/deploy-GA.md"), DEPLOY_GA.as_bytes());
    f.file(&format!(".claude-2/projects/{gizai}/memory/push-token.md"), format!("Push with {TOKEN}.\n").as_bytes());
    // Codex keeps no such folder: a look-alike there is not read
    f.file(".codex/projects/x/memory/codex.md", b"not Claude Code\n");
}

#[test]
fn the_account_folders_are_those_of_the_claude_code_clis_in_settings_each_once() {
    let f = setup();
    // Only the built-in Claude Code: Gizai's own CLAUDE_CONFIG_DIR, else ~/.claude.
    assert_eq!(f.dirs(), [f.home.join(".claude")]);
    let other = f.home.join("elsewhere");
    let inherited = |name: &str| (name == "CLAUDE_CONFIG_DIR").then(|| other.display().to_string());
    assert_eq!(memory_import::claude_dirs(&f.db, f.home(), &inherited).unwrap(), [other.clone()]);
    two_accounts(&f);
    std::fs::create_dir_all(f.home.join(".claude")).unwrap();
    // The third entry is ~/.claude again (written another way): once. Codex is left out.
    assert_eq!(f.dirs(), [f.home.join(".claude"), f.home.join(".claude-2")]);
}

#[test]
fn each_memory_file_of_two_accounts_becomes_one_note_in_the_team_leads_folder_and_a_second_start_adds_nothing() {
    let f = setup();
    two_accounts(&f);
    let (shop, blog, gizai) = (folder(SHOP), folder(BLOG), folder(GIZAI));
    let before = snapshot(&f.home);
    let r = f.import();

    // what came in: four files, by account and project; MEMORY.md, memory.md, todo.txt, old/archived.md, CLAUDE.md and
    // Codex's file did not
    let mut came: Vec<(String, String)> = r.imported.iter().map(|i| (i.file.clone(), i.note.clone())).collect();
    came.sort();
    assert_eq!(came, [
        (shown(&format!("~/.claude-2/projects/{gizai}/memory/deploy-GA.md")), format!("Team Lead/Imported/{gizai}/deploy-GA")),
        (shown(&format!("~/.claude/projects/{blog}/memory/lessons.MD")), format!("Team Lead/Imported/{blog}/lessons")),
        (shown(&format!("~/.claude/projects/{shop}/memory/deploy-SHOP.md")), format!("Team Lead/Imported/{shop}/deploy-SHOP")),
        (shown(&format!("~/.claude/projects/{shop}/memory/prefs.md")), format!("Team Lead/Imported/{shop}/prefs")),
    ]);
    let mut imported = f.imported();
    imported.sort();
    assert_eq!(imported, [format!("Team Lead/Imported/{gizai}/deploy-GA"), format!("Team Lead/Imported/{blog}/lessons"),
                          format!("Team Lead/Imported/{shop}/deploy-SHOP"), format!("Team Lead/Imported/{shop}/prefs")]);
    assert!(f.paths().iter().all(|p| !p.to_lowercase().contains("memory") && !p.contains("archived") && !p.contains("todo") && !p.contains("codex")),
            "{:?}", f.paths());

    // each note: in the Team Lead's own folder, written by it, its source first in its properties, its text as it was
    let ga = f.note(&format!("Team Lead/Imported/{gizai}/deploy-GA"));
    assert_eq!((ga.scope.as_str(), ga.owner_id.as_deref(), ga.current_version, ga.updated_by.as_deref()),
               ("agent", Some(f.lead.as_str()), 1, Some("Team Lead")));
    let (props, text) = split(&ga.body_md);
    assert_eq!(props, format!("source: claude-code\nclaude_config_dir: {}\nclaude_project: {gizai}\nclaude_file: deploy-GA.md\n\
name: deploy-GA\ndescription: \"How Gizai is released\"\nmetadata:\n  type: reference\n", shown("~/.claude-2")), "the file's own properties stay after ours");
    assert_eq!(text, &DEPLOY_GA[DEPLOY_GA.find("\n---\n").unwrap() + 5..], "its text as it was");
    let p = memory::properties(&ga.body_md);
    assert_eq!(p.get("source").map(Vec::as_slice), Some(&["claude-code".to_string()][..]));
    assert_eq!(p.get("claude_config_dir").map(Vec::as_slice), Some(&[shown("~/.claude-2")][..]));
    assert_eq!(p.get("claude_project").map(Vec::as_slice), Some(&[gizai.clone()][..]));
    assert_eq!(p.get("claude_file").map(Vec::as_slice), Some(&["deploy-GA.md".to_string()][..]));
    // a file without properties gets ours, then its text unchanged
    let prefs = f.note(&format!("Team Lead/Imported/{shop}/prefs"));
    assert_eq!(prefs.body_md, format!("---\nsource: claude-code\nclaude_config_dir: {}\nclaude_project: {shop}\nclaude_file: prefs.md\n---\n\n{PREFS}",
                                      shown("~/.claude")));
    // a property of the file's own by one of our names gives way to ours
    let shop_note = f.note(&format!("Team Lead/Imported/{shop}/deploy-SHOP")).body_md;
    assert!(shop_note.starts_with("---\nsource: claude-code\n") && !shop_note.contains("hand-written") && shop_note.contains("name: deploy-SHOP\n"),
            "{shop_note}");
    assert!(shop_note.ends_with("---\nDeploy mode: manual.\n"), "{shop_note}");
    assert!(f.note(&format!("Team Lead/Imported/{blog}/lessons")).body_md.contains("claude_file: lessons.MD\n"));

    // the token's file: skipped, listed by its path, its text nowhere
    assert_eq!(r.skipped.len(), 1, "{:?}", r.skipped);
    assert_eq!(r.skipped[0].file, shown(&format!("~/.claude-2/projects/{gizai}/memory/push-token.md")));
    assert!(r.skipped[0].why.contains("GitHub token") && !r.skipped[0].why.contains("ghp_abc"), "{:?}", r.skipped[0]);
    assert_eq!(r.list, None, "a short list goes in the thread itself");
    assert_eq!(r.left_out, LeftOut::default(), "every folder here is one of Gizai's places");
    for n in memory::list(&f.db, &f.lead()).unwrap() {
        let body = memory::get(&f.db, &f.lead(), &n.id).unwrap().body_md;
        assert!(!body.contains(TOKEN) && !body.contains("Push with"), "{}: {body}", n.path);
    }

    // Team Lead/Notes: made from its template, with an open thread listing what came and what didn't
    let notes = f.note("Team Lead/Notes");
    assert_eq!(notes.owner_id.as_deref(), Some(f.lead.as_str()));
    let open = &notes.body_md[notes.body_md.find("## Open threads").expect("its template")..notes.body_md.find("## How to use memory").unwrap()];
    assert!(open.contains(&format!("- {DAY}: Imported 4 notes from Claude Code's own memory into Team Lead/Imported/")), "{open}");
    assert!(open.contains("1 file could not be imported") && open.contains("Sort them with Jeffrey") && open.contains("Deployments/<KEY>"), "{open}");
    assert!(open.contains("memory_move") && open.contains("memory_list with folder \"Team Lead/Imported\""), "{open}");
    assert!(open.contains("(like deploy-") && (open.contains("to Deployments/GA)") || open.contains("to Deployments/SHOP)")), "{open}");
    assert!(open.contains(&format!("- From {}: [[{gizai}/deploy-GA]]", shown(&format!("~/.claude-2/projects/{gizai}/memory")))), "{open}");
    assert!(open.contains(&format!("- From {}: [[{shop}/deploy-SHOP]], [[{shop}/prefs]]", shown(&format!("~/.claude/projects/{shop}/memory")))),
            "{open}");
    assert!(open.contains(&format!("- From {}: [[{blog}/lessons]]", shown(&format!("~/.claude/projects/{blog}/memory")))), "{open}");
    assert!(open.contains(&format!("- Not imported: {} (it holds a GitHub token (ghp_…); nothing of its text was kept)",
                                   shown(&format!("~/.claude-2/projects/{gizai}/memory/push-token.md")))), "{open}");
    assert!(!open.contains("Left out"), "nothing was left out: {open}");
    // the links in the thread find the notes
    let linked: Vec<String> = memory::links_from(&f.db, &notes.id).unwrap().into_iter().map(|l| l.target_id).collect();
    for n in [format!("Team Lead/Imported/{shop}/prefs"), format!("Team Lead/Imported/{gizai}/deploy-GA")] {
        assert!(linked.contains(&f.note(&n).id), "{n}: {linked:?}");
    }
    assert_eq!(settings::get::<serde_json::Value>(&f.db, "claude_memory_imports").unwrap().unwrap().as_object().unwrap().len(), 5,
               "four imported, one skipped");

    // a second start: nothing new, nothing written
    let (count, version) = (f.paths().len(), f.note("Team Lead/Notes").current_version);
    assert_eq!(f.import(), Report::default());
    assert_eq!((f.paths().len(), f.note("Team Lead/Notes").current_version), (count, version));
    // also not once an imported file changed: it came in once
    let changed = f.home.join(format!(".claude/projects/{shop}/memory/prefs.md"));
    std::fs::write(&changed, "Changed after the import.\n").unwrap();
    assert_eq!(f.import(), Report::default());
    assert!(!f.note(&format!("Team Lead/Imported/{shop}/prefs")).body_md.contains("Changed after"));

    // nothing in the account folders changed, apart from the one file this test changed itself
    let mut after = snapshot(&f.home);
    let mut before = before;
    before.remove(&changed);
    after.remove(&changed);
    assert_eq!(before, after, "Gizai only reads Claude Code's folders");
}

#[test]
fn a_new_file_comes_in_on_the_next_start_and_a_skipped_one_once_it_no_longer_holds_the_secret() {
    let f = setup();
    two_accounts(&f);
    let (gizai, otus) = (folder(GIZAI), folder(OTUS));
    f.import();
    let token = f.home.join(format!(".claude-2/projects/{gizai}/memory/push-token.md"));
    // unchanged: tried again but not imported (nor listed again)
    assert_eq!(f.import(), Report::default());
    // a new file in a new project folder, and the secret taken out
    f.file(&format!(".claude-2/projects/{otus}/memory/user_role.md"), b"Jefsev is the user.\n");
    std::fs::write(&token, "The push token is in the keychain as gizai-push.\n").unwrap();
    // a later modified time, also on file systems that keep whole seconds
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    std::fs::File::options().write(true).open(&token).unwrap().set_modified(later).unwrap();
    let r = f.import();
    let mut notes: Vec<&str> = r.imported.iter().map(|i| i.note.as_str()).collect();
    notes.sort();
    assert_eq!(notes, [format!("Team Lead/Imported/{gizai}/push-token"), format!("Team Lead/Imported/{otus}/user_role")]);
    assert!(r.skipped.is_empty());
    assert!(f.note(&format!("Team Lead/Imported/{gizai}/push-token")).body_md.ends_with("The push token is in the keychain as gizai-push.\n"));
    // a second thread, for what came this time
    let notes = f.note("Team Lead/Notes").body_md;
    assert_eq!(notes.matches(&format!("- {DAY}: Imported ")).count(), 2, "{notes}");
    assert!(notes.contains(&format!("- {DAY}: Imported 2 notes")), "{notes}");
    assert_eq!(f.import(), Report::default());
}

#[test]
fn a_file_that_is_too_big_or_not_text_is_skipped_by_its_path_and_a_thread_still_says_so() {
    let f = setup();
    let p = folder(P);
    f.file(&format!(".claude/projects/{p}/memory/big.md"), &vec![b'a'; memory_import::MAX_BYTES as usize + 1]);
    f.file(&format!(".claude/projects/{p}/memory/binary.md"), &[0xff, 0xfe, 0x00, 0x41, 0xc3]);
    let r = f.import();
    assert!(r.imported.is_empty());
    let mut why: Vec<(String, String)> = r.skipped.iter().map(|s| (s.file.clone(), s.why.clone())).collect();
    why.sort();
    assert_eq!(why.iter().map(|(f, _)| f.clone()).collect::<Vec<_>>(),
               [shown(&format!("~/.claude/projects/{p}/memory/big.md")), shown(&format!("~/.claude/projects/{p}/memory/binary.md"))]);
    assert!(why[0].1.contains("too big") && why[1].1.contains("isn't text"), "{why:?}");
    assert!(f.imported().is_empty());
    let notes = f.note("Team Lead/Notes").body_md;
    assert!(notes.contains(&format!("- {DAY}: 2 files in Claude Code's own memory could not be imported into Team Lead/Imported/:")), "{notes}");
    assert!(notes.contains(&format!("- Not imported: {} (it is 201 KB, too big for a note; nothing of its text was kept)",
                                    shown(&format!("~/.claude/projects/{p}/memory/big.md")))), "{notes}");
    assert!(notes.contains(&format!("- Not imported: {} (it isn't text (UTF-8); nothing of its text was kept)",
                                    shown(&format!("~/.claude/projects/{p}/memory/binary.md")))), "{notes}");
    assert!(!notes.contains("aaaaaaaaaa"), "never the text: {notes}");
    assert_eq!(f.import(), Report::default(), "not tried again while unchanged");
}

#[test]
fn a_file_with_a_private_key_is_listed_by_its_path_only() {
    // The reason, "it holds a private key (-----BEGIN … PRIVATE KEY-----)", is what the secret check looks for itself: the
    // thread's line must still name the file by its path, like any other secret, and never hold its text.
    let f = setup();
    let p = folder(P);
    f.file(&format!(".claude/projects/{p}/memory/key.md"), b"-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAA\n-----END OPENSSH PRIVATE KEY-----\n");
    let r = f.import();
    assert!(r.imported.is_empty());
    assert_eq!(r.skipped.len(), 1);
    assert_eq!(r.skipped[0].file, shown(&format!("~/.claude/projects/{p}/memory/key.md")));
    assert!(r.skipped[0].why.contains("private key"), "{:?}", r.skipped);
    let notes = f.note("Team Lead/Notes").body_md;
    assert!(!notes.contains("b3BlbnNzaC1rZXktdjEAAAAA"), "never the text: {notes}");
    assert!(notes.contains(&format!("- Not imported: {} (it holds a private key", shown(&format!("~/.claude/projects/{p}/memory/key.md")))),
            "the file by its path, not as 'a file whose path looks like a secret': {notes}");
}

#[test]
fn a_certificate_comes_in_but_a_bundle_with_a_private_key_after_it_is_skipped_and_listed_by_its_path() {
    // Round 2: the secret check looks at every -----BEGIN line, so a private key after a certificate is found too, and the
    // thread names the file by its path with the reason, without the -----BEGIN … example.
    let f = setup();
    let p = folder(P);
    const CERT: &str = "-----BEGIN CERTIFICATE-----\nMIIBszCCAVmgAwIBAgIUQ\n-----END CERTIFICATE-----\n";
    f.file(&format!(".claude/projects/{p}/memory/cert.md"), format!("The shop's certificate:\n{CERT}").as_bytes());
    f.file(&format!(".claude/projects/{p}/memory/bundle.md"),
           format!("{CERT}-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQEF\n-----END PRIVATE KEY-----\n").as_bytes());
    let r = f.import();
    assert_eq!(r.imported.iter().map(|i| i.note.clone()).collect::<Vec<_>>(), [format!("Team Lead/Imported/{p}/cert")]);
    assert_eq!(r.skipped, [Skipped { file: shown(&format!("~/.claude/projects/{p}/memory/bundle.md")), why: "it holds a private key".into() }]);
    assert!(f.note(&format!("Team Lead/Imported/{p}/cert")).body_md.ends_with(CERT));
    let notes = f.note("Team Lead/Notes").body_md;
    assert!(notes.contains(&format!("- Not imported: {} (it holds a private key; nothing of its text was kept)",
                                    shown(&format!("~/.claude/projects/{p}/memory/bundle.md")))), "{notes}");
    assert!(!notes.contains("MIIEvQ") && !notes.contains("-----BEGIN") && !notes.contains("looks like a secret"), "{notes}");
}

#[test]
fn a_note_whose_path_turns_into_a_token_comes_in_and_the_thread_leaves_that_path_out() {
    // Round 2: '#' can't be in a note's path (nor ':', which Windows file names can't hold either), so a file "sk#" + 20
    // letters becomes a note "sk-…", which looks like an API key; a file "deploy-sk-…" would give the example
    // "Deployments/sk-…". Team Lead/Notes refuses a text with a secret, so such a line would have failed the whole import at
    // every start: the thread leaves those out, the import goes through.
    let f = setup();
    let (p, q) = (folder(P), folder(Q));
    let key = "abcdefghijklmnopqrstuvwxyz";
    f.file(&format!(".claude/projects/{p}/memory/sk#{key}.md"), b"A note.\n");
    f.file(&format!(".claude/projects/{q}/memory/deploy-sk-{key}.md"), b"Deploy mode: manual.\n");
    f.file(&format!(".claude/projects/{q}/memory/prefs.md"), b"Short answers.\n");
    let r = f.import();
    let mut notes: Vec<String> = r.imported.iter().map(|i| i.note.clone()).collect();
    notes.sort();
    assert_eq!(notes, [format!("Team Lead/Imported/{p}/sk-{key}"), format!("Team Lead/Imported/{q}/deploy-sk-{key}"),
                       format!("Team Lead/Imported/{q}/prefs")]);
    assert!(r.skipped.is_empty(), "{:?}", r.skipped);
    let text = f.note("Team Lead/Notes").body_md;
    assert_eq!(memory::secret_in(&text), None, "{text}");
    assert!(text.contains(&format!("- {DAY}: Imported 3 notes from Claude Code's own memory")), "{text}");
    assert!(text.contains("- From a folder whose path, or a note's, looks like a secret (not shown): 1 note"), "{text}");
    assert!(text.contains(&format!("- From {}: [[{q}/deploy-sk-{key}]], [[{q}/prefs]]", shown(&format!("~/.claude/projects/{q}/memory")))), "{text}");
    assert!(text.contains("a project's deploy note goes to Deployments/<KEY>, with project: <KEY>"), "no example: {text}");
    assert!(!text.contains(&format!("/sk-{key}")) && !text.contains(&format!("[[{p}/")), "{text}");
    // each file was recorded: a second start adds nothing
    assert_eq!(f.import(), Report::default());
    assert_eq!(f.imported().len(), 3);
}

#[test]
fn a_file_whose_name_holds_a_token_is_skipped_and_its_path_not_shown() {
    let f = setup();
    f.file(&format!(".claude/projects/{}/memory/{TOKEN}.md", folder(P)), b"Nothing secret in here.\n");
    let r = f.import();
    assert!(r.imported.is_empty());
    assert_eq!(r.skipped.len(), 1);
    assert!(r.skipped[0].why.contains("GitHub token"), "{:?}", r.skipped);
    let notes = f.note("Team Lead/Notes").body_md;
    assert!(notes.contains("- Not imported: a file whose path looks like a secret (not shown)"), "{notes}");
    assert!(!notes.contains(TOKEN), "{notes}");
    assert!(f.imported().is_empty());
}

#[test]
fn notes_that_would_share_a_path_get_a_number_and_odd_names_are_made_safe() {
    let f = setup();
    let shop = folder(SHOP);
    clis::save(&f.db, vec![Cli { name: "Second".into(), kind: "claude_code".into(), command: "claude".into(),
                                 env: vec!["CLAUDE_CONFIG_DIR=~/.claude-2".into()], ..Default::default() }]).unwrap();
    // one project folder in both accounts, with a file of the same name
    f.file(&format!(".claude/projects/{shop}/memory/deploy-SHOP.md"), b"From the first account.\n");
    f.file(&format!(".claude-2/projects/{shop}/memory/deploy-SHOP.md"), b"From the second account.\n");
    // characters a note's path can't hold (and a file name on every system can)
    f.file(&format!(".claude/projects/{shop}/memory/what^ a [note] #1.md"), b"Odd name.\n");
    let r = f.import();
    assert_eq!(r.imported.len(), 3, "{r:?}");
    let mut imported = f.imported();
    imported.sort();
    assert_eq!(imported, [format!("Team Lead/Imported/{shop}/deploy-SHOP"), format!("Team Lead/Imported/{shop}/deploy-SHOP (2)"),
                          format!("Team Lead/Imported/{shop}/what- a -note- -1")]);
    let bodies: Vec<String> = ["deploy-SHOP", "deploy-SHOP (2)"].iter()
        .map(|t| f.note(&format!("Team Lead/Imported/{shop}/{t}")).body_md).collect();
    let account = |a: &str| format!("claude_config_dir: {}\n", shown(a));
    assert!(bodies.iter().any(|b| b.contains(&account("~/.claude")) && b.ends_with("From the first account.\n")), "{bodies:?}");
    assert!(bodies.iter().any(|b| b.contains(&account("~/.claude-2")) && b.ends_with("From the second account.\n")), "{bodies:?}");
    // the original name stays in its properties, quoted where YAML needs it
    let odd = f.note(&format!("Team Lead/Imported/{shop}/what- a -note- -1")).body_md;
    assert!(odd.contains("claude_file: 'what^ a [note] #1.md'\n"), "{odd}");
    assert_eq!(memory::properties(&odd).get("claude_file").map(Vec::as_slice), Some(&["what^ a [note] #1.md".to_string()][..]));
}

#[cfg(not(windows))]
#[test]
fn names_windows_cant_hold_are_made_safe_too() {
    // ':' and '?' are no part of a note's path either (Windows file names can't hold them, so not there).
    let f = setup();
    let shop = folder(SHOP);
    f.file(&format!(".claude/projects/{shop}/memory/what: a [note] #1?.md"), b"Odd name.\n");
    let r = f.import();
    assert_eq!(r.imported.iter().map(|i| i.note.clone()).collect::<Vec<_>>(), [format!("Team Lead/Imported/{shop}/what- a -note- -1-")]);
    let odd = f.note(&format!("Team Lead/Imported/{shop}/what- a -note- -1-")).body_md;
    assert!(odd.contains("claude_file: 'what: a [note] #1?.md'\n"), "{odd}");
}

#[test]
fn a_long_list_goes_in_a_note_of_its_own_linked_from_the_thread() {
    let f = setup();
    let long = folder(LONG);
    for i in 0..40 {
        f.file(&format!(".claude/projects/{long}/memory/note-number-{i:02}-about-something.md"), format!("Note {i}.\n").as_bytes());
    }
    let r = f.import();
    assert_eq!(r.imported.len(), 40);
    let list = r.list.clone().expect("the list in a note of its own");
    assert_eq!(list, format!("Team Lead/Imported/From Claude Code {DAY}"));
    let body = f.note(&list).body_md;
    assert!(body.contains(&format!("[[{long}/note-number-39-about-something]]")), "{body}");
    let notes = f.note("Team Lead/Notes").body_md;
    assert!(notes.contains(&format!("- The list: [[{list}]]")) && notes.contains("Imported 40 notes"), "{notes}");
    assert!(!notes.contains("note-number-39"), "the thread links the list instead: {notes}");
}

#[test]
fn the_team_leads_memory_block_counts_the_imported_notes_not_the_list_of_them() {
    // Round 2: the list note (From Claude Code <day>) is in Team Lead/Imported/ too, but it came from no file: 40 files are
    // 40 notes in the block, and one sorted out of the folder makes 39.
    let f = setup();
    let project = folder(LONG);
    for i in 0..40 {
        f.file(&format!(".claude/projects/{project}/memory/note-number-{i:02}-about-something.md"), format!("Note {i}.\n").as_bytes());
    }
    let r = f.import();
    assert!(r.list.is_some(), "the list in a note of its own");
    assert_eq!(f.imported().len(), 41, "40 notes and the list");
    let block = |f: &F| memory::prompt_block(&f.db, &f.lead(), &Context::default()).unwrap().text;
    let text = block(&f);
    assert!(text.contains("- Team Lead/Imported/: 40 notes from Claude Code's own memory, to sort with the user"), "{text}");
    // neither the notes nor the list in full (Team Lead/Notes, shown in full, links the list)
    assert!(!text.contains("note-number-") && !text.contains("### Team Lead/Imported/"), "{text}");
    memory::move_note(&f.db, &f.lead(), &format!("Team Lead/Imported/{project}/note-number-00-about-something"), "Lessons/Note zero", false)
        .unwrap();
    assert!(block(&f).contains("- Team Lead/Imported/: 39 notes from Claude Code's own memory"), "{}", block(&f));
}

#[test]
fn the_thread_names_the_agents_whose_instructions_still_say_memory_md() {
    let f = setup();
    let old = "Keep one memory per project, named deploy-<KEY>, in your memory directory, with a line for it in MEMORY.md.";
    team::add_agent(&f.db, &f.you, &f.team, AgentInput { name: "DevOps Agent 2".into(), role_key: "devops".into(),
        instructions_md: Some(old.into()), ..Default::default() }).unwrap();
    // a DevOps agent made now gets the role's new text, which no longer says so
    team::add_agent(&f.db, &f.you, &f.team, AgentInput { name: "DevOps Agent 3".into(), role_key: "devops".into(), ..Default::default() }).unwrap();
    f.file(&format!(".claude/projects/{}/memory/deploy-SHOP.md", folder(SHOP)), b"Deploy mode: manual.\n");
    f.import();
    let notes = f.note("Team Lead/Notes").body_md;
    assert!(notes.contains("- The instructions of DevOps Agent 2 still say to keep notes in Claude Code's memory folder (MEMORY.md)"), "{notes}");
    assert!(!notes.contains("DevOps Agent 3"), "{notes}");
}

#[test]
fn without_a_team_lead_the_notes_still_come_in() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().to_str().unwrap();
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let p = tmp.path().join(".claude/projects").join(folder(P)).join("memory/a.md");
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, "A note.\n").unwrap();
    let dirs = memory_import::claude_dirs(&db, home, &|_| None).unwrap();
    let r = memory_import::import(&db, &dirs, &[place(P)], home, &s.you_id, "Jeffrey", DAY).unwrap();
    assert_eq!(r.imported.len(), 1, "{r:?}");
    let n = memory::find(&db, &format!("Team Lead/Imported/{}/a", folder(P))).unwrap().expect("the note");
    assert!(n.body_md.ends_with("A note.\n"));
    assert!(memory::find(&db, "Team Lead/Notes").unwrap().unwrap().body_md.contains("Imported 1 note from"));
}

#[test]
fn imported_notes_are_one_line_in_the_team_leads_memory_block_and_reach_no_other_agent() {
    let f = setup();
    two_accounts(&f);
    let devops = team::add_agent(&f.db, &f.you, &f.team, AgentInput { name: "DevOps Agent".into(), role_key: "devops".into(), ..Default::default() }).unwrap();
    f.import();
    let lead = memory::prompt_block(&f.db, &f.lead(), &Context::default()).unwrap();
    assert!(lead.text.contains("- Team Lead/Imported/: 4 notes from Claude Code's own memory, to sort with the user (memory_list with folder \
\"Team Lead/Imported\" lists them)"), "{}", lead.text);
    assert!(!lead.text.contains("Release: main -> production PR"), "not in full: {}", lead.text);
    assert!(!lead.given.iter().any(|g| g.path.starts_with("Team Lead/Imported/")), "{:?}", lead.given);
    assert!(lead.text.contains("### Team Lead/Notes"), "its own notes still come in full: {}", lead.text);
    // the DevOps Agent's run on GA: not one of them, though deploy-GA is about that project
    let cx = Context { role: "devops".into(), project: Some(("GA".into(), "Gizai".into())), client: None };
    let run = memory::prompt_block(&f.db, &Who::Agent(devops), &cx).unwrap();
    assert!(!run.text.contains("Imported") && !run.text.contains("deploy-GA") && !run.text.contains("Release: main"), "{}", run.text);
    // once the Team Lead moves deploy-GA to Deployments/GA with the project and role, it reaches the DevOps Agent
    memory::move_note(&f.db, &f.lead(), &format!("Team Lead/Imported/{}/deploy-GA", folder(GIZAI)), "Deployments/GA", false).unwrap();
    let body = f.note("Deployments/GA").body_md.replacen("---\n", "---\ntype: deployment\nproject: GA\napplies_to: devops\n", 1);
    memory::write(&f.db, &f.lead(), "Deployments/GA", &body, Some(1), None).unwrap();
    let run = memory::prompt_block(&f.db, &Who::Agent(team::all_agents(&f.db).unwrap().into_iter()
        .find(|(_, a)| a.name == "DevOps Agent").unwrap().1.actor_id), &cx).unwrap();
    assert!(run.text.contains("Deployments/GA") && run.text.contains("Release: main -> production PR"), "{}", run.text);
    let lead = memory::prompt_block(&f.db, &f.lead(), &Context::default()).unwrap();
    assert!(lead.text.contains("- Team Lead/Imported/: 3 notes from Claude Code's own memory"), "{}", lead.text);
}

#[test]
fn with_source_keeps_the_text_and_puts_where_it_came_from_first() {
    // no properties: ours, a blank line, the text
    assert_eq!(memory_import::with_source("Hello.\n", "~/.claude", "-p", "a.md"),
               "---\nsource: claude-code\nclaude_config_dir: ~/.claude\nclaude_project: -p\nclaude_file: a.md\n---\n\nHello.\n");
    // a byte order mark goes; properties ended by `...` stay properties; a list under one of our names goes with it
    let t = "\u{feff}---\nname: x\nsource:\n  - somewhere\n  - else\ntags: [a]\n...\nBody.\n";
    assert_eq!(memory_import::with_source(t, "~/.claude", "-p", "x.md"),
               "---\nsource: claude-code\nclaude_config_dir: ~/.claude\nclaude_project: -p\nclaude_file: x.md\nname: x\ntags: [a]\n---\nBody.\n");
    // values YAML would read otherwise are quoted
    let b = memory_import::with_source("x", "~/my: claude", "yes", "123.md");
    assert!(b.contains("claude_config_dir: '~/my: claude'\nclaude_project: 'yes'\nclaude_file: 123.md\n"), "{b}");
    // a first line `---` without an end is text, not properties
    let b = memory_import::with_source("---\nno end\n", "~/.claude", "-p", "y.md");
    assert!(b.ends_with("---\n\n---\nno end\n"), "{b}");
}

// Round 3: only the folders of Gizai's own places.

#[test]
fn folder_names_follow_claude_codes_own_rule() {
    // Expected names from Claude Code 2.1.289's own functions (cP, k, Le and Hee, copied out of its bundle), run under node:
    // every UTF-16 unit but an ASCII letter or digit as '-', a name over 200 cut there with '-' and the path's djb2 hash
    // (without its sign) in base 36.
    let name = memory_import::project_folder;
    assert_eq!(name("/home/jefsev/Herd/gizai"), "-home-jefsev-Herd-gizai");
    // a card's worktree (Claude Code's own folder for this very run's sessions has this name)
    assert_eq!(name("/home/jefsev/.local/share/gizai/worktrees/GA/GA-85"), "-home-jefsev--local-share-gizai-worktrees-GA-GA-85");
    assert_eq!(name(r"C:\Users\me\shop"), "C--Users-me-shop");
    // é is one UTF-16 unit, the emoji two
    assert_eq!(name("/home/me/café 😀"), "-home-me-caf----");
    // 200 is kept whole; 201 is cut, with the hash
    let a199 = format!("/{}", "a".repeat(199));
    assert_eq!(name(&a199), format!("-{}", "a".repeat(199)));
    assert_eq!(name(&format!("/{}", "a".repeat(200))), format!("-{}-b6ymvl", "a".repeat(199)));
    // an emoji at the end: 200 characters, 201 UTF-16 units (a negative hash)
    assert_eq!(name(&format!("/{}😀", "a".repeat(198))), format!("-{}--b5wpam", "a".repeat(198)));
    assert_eq!(name(&format!("/{}😀", "a".repeat(197))), format!("-{}--", "a".repeat(197)));
    let long = format!("/home/me/{}shop", "a-very-long-folder-name/".repeat(9));
    // the 200th character is the 8th name's last: the dash after it is the one before the hash
    assert_eq!(name(&long), format!("-home-me-{}iws0lr", "a-very-long-folder-name-".repeat(8)));
    let accents = format!("/home/me/{}end", "dossier-très-long-😀/".repeat(10));
    assert_eq!(name(&accents), format!("-home-me-{}do-qqdacb", "dossier-tr-s-long----".repeat(9)));
    let windows = format!(r"C:\Users\me\{}shop", r"Projects\Client Work\".repeat(10));
    assert_eq!(name(&windows), format!("C--Users-me-{}-ve8sht", "Projects-Client-Work-".repeat(9).trim_end_matches('-')));
}

#[test]
fn gizais_places_are_the_linked_folders_the_cards_worktrees_the_code_copies_and_the_leads_folder() {
    let f = setup();
    let root = f.root();
    let data = root.join("data");
    let linked = root.join("work").join("shop");
    std::fs::create_dir_all(&linked).unwrap();
    let shop = f.project("SHOP", Some(&linked));
    f.project("BLOG", None);
    let (one, two) = (f.card(&shop, "Checkout"), f.card(&shop, "Invoices"));
    // on disk: a worktree and a code copy whose card or project is no longer in the database, and a file that is no folder
    std::fs::create_dir_all(data.join("worktrees").join("OLD").join("OLD-7")).unwrap();
    std::fs::create_dir_all(data.join("code").join("OLD")).unwrap();
    std::fs::write(data.join("worktrees").join("OLD").join("notes.txt"), "not a worktree").unwrap();
    let got = memory_import::places(&f.db, &data).unwrap();
    let wt = data.join("worktrees");
    for want in [linked.clone(), wt.join("SHOP").join(&one), wt.join("SHOP").join(&two), wt.join("OLD").join("OLD-7"),
                 data.join("code").join("SHOP"), data.join("code").join("BLOG"), data.join("code").join("OLD"), data.join("lead")] {
        assert!(got.contains(&want), "{} missing in {got:?}", want.display());
    }
    assert!(!got.contains(&wt.join("OLD").join("notes.txt")), "{got:?}");
    // every place is the linked folder or in Gizai's data folder, each once
    assert!(got.iter().all(|p| *p == linked || p.starts_with(&data)), "{got:?}");
    let mut once = got.clone();
    once.dedup();
    assert_eq!(once, got);
}

#[test]
fn only_the_folders_of_gizais_places_come_in_and_the_others_are_only_counted() {
    let f = setup();
    let root = f.root();
    let data = root.join("data");
    let linked = root.join("work").join("shop");
    std::fs::create_dir_all(&linked).unwrap();
    let shop = f.project("SHOP", Some(&linked));
    let card = f.card(&shop, "Checkout");
    let worktree = data.join("worktrees").join("SHOP").join(&card);
    let (copy, lead) = (data.join("code").join("SHOP"), data.join("lead"));
    let named = |p: &Path| memory_import::project_folder(&p.to_string_lossy());
    let write = |p: &Path, file: &str, text: &[u8]| f.file(&format!(".claude/projects/{}/memory/{file}", named(p)), text);
    // Gizai's places: the project's linked folder, the card's worktree (not on disk any more), the code copy, the lead's folder
    write(&linked, "MEMORY.md", b"- [deploy-SHOP](deploy-SHOP.md)\n");
    write(&linked, "deploy-SHOP.md", b"Deploy mode: manual.\n");
    write(&worktree, "checkout.md", b"The checkout keeps its cart in Redis.\n");
    write(&copy, "copy.md", b"The copy is refreshed before each chat answer.\n");
    write(&lead, "prefs.md", b"Short answers.\n");
    // other paths: the user's own site (two notes and an index); a folder named like the linked one plus more; one with a
    // token and a file that isn't text (not read, so not listed either); one with only an index, one with only sessions
    let site = root.join("work").join("my-own-site");
    write(&site, "MEMORY.md", b"- [hugo](hugo.md)\n");
    write(&site, "hugo.md", b"The site runs on Hugo.\n");
    write(&site, "style.md", b"Plain CSS, no framework.\n");
    f.file(&format!(".claude/projects/{}-old/memory/old.md", named(&linked)), b"An old copy of the shop.\n");
    let secret = root.join("work").join("secret");
    write(&secret, "push.md", format!("Push with {TOKEN}.\n").as_bytes());
    write(&secret, "binary.md", &[0xff, 0xfe, 0x00]);
    write(&root.join("work").join("only-an-index"), "MEMORY.md", b"- nothing yet\n");
    f.file(&format!(".claude/projects/{}/session.jsonl", named(&root.join("work").join("sessions"))), b"{}\n");
    let before = snapshot(&f.home);
    let import = |f: &F| f.import_with(&memory_import::places(&f.db, &data).unwrap());

    let r = import(&f);
    let at = |p: &Path, file: &str| shown(&format!("~/.claude/projects/{}/memory/{file}", named(p)));
    let mut came: Vec<String> = r.imported.iter().map(|i| i.file.clone()).collect();
    came.sort();
    let mut want = vec![at(&linked, "deploy-SHOP.md"), at(&worktree, "checkout.md"), at(&copy, "copy.md"), at(&lead, "prefs.md")];
    want.sort();
    assert_eq!(came, want);
    assert!(r.skipped.is_empty(), "the other folders aren't read: {:?}", r.skipped);
    assert_eq!(r.left_out, LeftOut { folders: 3, notes: 5 }, "the site (2), the look-alike (1) and the secret one (2)");
    // each note: its project folder in its properties
    for i in &r.imported {
        let p = memory::properties(&f.note(&i.note).body_md);
        let project = &p.get("claude_project").unwrap()[0];
        assert!(i.file.contains(&format!("{project}{}memory", std::path::MAIN_SEPARATOR)), "{i:?}: {project}");
    }
    // Team Lead/Notes: how many were left out, never which
    let notes = f.note("Team Lead/Notes").body_md;
    assert!(notes.contains(&format!("- {DAY}: Imported 4 notes from Claude Code's own memory")), "{notes}");
    assert!(notes.contains("- Left out, not read: 3 folders of other paths, with 5 notes. Claude Code keeps a folder per path it works in, \
and only those of Gizai's places come in"), "{notes}");
    for gone in [named(&site), named(&secret), format!("{}-old", named(&linked)), "hugo".into(), "style.md".into(), "push.md".into(),
                 "binary.md".into(), "old.md".into(), TOKEN.into()] {
        assert!(!notes.contains(&gone), "{gone}: {notes}");
    }
    for n in memory::list(&f.db, &f.lead()).unwrap() {
        let body = memory::get(&f.db, &f.lead(), &n.id).unwrap().body_md;
        assert!(!body.contains("Hugo") && !body.contains("An old copy") && !body.contains("Push with"), "{}: {body}", n.path);
    }
    // a second start: nothing new, though the other folders are still there; the files as they were
    assert_eq!(import(&f), Report::default());
    assert_eq!(snapshot(&f.home), before, "Gizai only reads Claude Code's folders");

    // once one of those paths is a project's linked folder, its notes come in at the next start: they weren't kept as done
    f.project("SITE", Some(&site));
    let r = import(&f);
    let mut came: Vec<String> = r.imported.iter().map(|i| i.file.clone()).collect();
    came.sort();
    assert_eq!(came, [at(&site, "hugo.md"), at(&site, "style.md")]);
    assert_eq!(r.left_out, LeftOut { folders: 2, notes: 3 });
    assert!(f.note("Team Lead/Notes").body_md.contains("- Left out, not read: 2 folders of other paths, with 3 notes."));
}

#[test]
fn with_only_other_folders_nothing_comes_in_and_no_thread_is_written() {
    let f = setup();
    f.file(&format!(".claude/projects/{}/memory/notes.md", folder("/home/me/elsewhere")), b"Not one of Gizai's.\n");
    assert_eq!(f.import(), Report::default());
    assert!(memory::find(&f.db, "Team Lead/Notes").unwrap().is_none_or(|n| !n.body_md.contains("Left out")));
    assert!(f.imported().is_empty());
}

#[test]
fn a_folder_name_in_another_case_counts_only_where_paths_ignore_case() {
    // On Windows and macOS a path is the same in any case, so Claude Code's folder for /home/me/Shop is the shop's.
    let f = setup();
    f.file(&format!(".claude/projects/{}/memory/a.md", folder("/home/me/SHOP")), b"A note.\n");
    let r = f.import();
    if cfg!(any(windows, target_os = "macos")) {
        assert_eq!(r.imported.len(), 1, "{r:?}");
    } else {
        assert_eq!(r, Report::default());
    }
}

#[cfg(unix)]
#[test]
fn a_linked_folder_through_a_link_counts_by_its_real_path_too() {
    // Claude Code keeps a run's memory in the folder of the repository's real path: a project linked through a symbolic
    // link still gets its notes.
    let f = setup();
    let root = f.root();
    let real = root.join("work").join("shop");
    std::fs::create_dir_all(&real).unwrap();
    let link = root.join("shop-link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    f.project("SHOP", Some(&link));
    let named = |p: &Path| memory_import::project_folder(&p.to_string_lossy());
    f.file(&format!(".claude/projects/{}/memory/real.md", named(&real)), b"By the real path.\n");
    f.file(&format!(".claude/projects/{}/memory/link.md", named(&link)), b"By the link.\n");
    let r = f.import_with(&memory_import::places(&f.db, &root.join("data")).unwrap());
    let mut came: Vec<String> = r.imported.iter().map(|i| i.note.rsplit('/').next().unwrap().to_string()).collect();
    came.sort();
    assert_eq!(came, ["link", "real"]);
}
