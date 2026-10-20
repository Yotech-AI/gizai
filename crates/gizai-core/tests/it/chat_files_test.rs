// GA-41: files on chat messages. A message keeps the files you added (also after reopening the chat); a queued message
// keeps its files and takes them along when it goes; removing it from the queue removes them. A folder or a file over
// 1 GB can't be added. And a gizai: link's task is found by its identifier.
use std::path::Path;

use gizai_core::chat::{self, NewMessage};
use gizai_core::model::*;
use gizai_core::{db::Db, files, projects, seed, tasks, team};

struct S {
    db: Db,
    you: String,
    thread: String,
    dir: tempfile::TempDir,
}

fn setup() -> S {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("gizai.db")).unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let lead = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true),
        ..Default::default() }).unwrap();
    let thread = chat::create_thread(&db, &s.you_id, &lead, "Files").unwrap();
    S { db, you: s.you_id, thread, dir }
}

impl S {
    fn data(&self) -> &Path {
        self.dir.path()
    }
    /// A file on this computer, stored in Gizai's file store.
    fn blob(&self, name: &str, body: &str) -> files::Blob {
        let src = self.dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        let p = src.join(name);
        std::fs::write(&p, body).unwrap();
        files::store(&self.data().join("data"), &p).unwrap()
    }
    fn say(&self, text: &str, files: Vec<files::Blob>) -> chat::ChatMessage {
        chat::add_message(&self.db, NewMessage { thread_id: self.thread.clone(), role: "user".into(), author_id: Some(self.you.clone()),
            body_md: Some(text.into()), files, ..Default::default() }).unwrap()
    }
}

fn names(fs: &[FileRow]) -> Vec<&str> {
    fs.iter().map(|f| f.name.as_str()).collect()
}

#[test]
fn a_message_keeps_its_files_in_the_order_added_also_when_the_chat_is_read_again() {
    let s = setup();
    let (a, b) = (s.blob("spec.pdf", "spec"), s.blob("shot.png", "png"));
    let m = s.say("Have a look", vec![a, b]);
    assert_eq!(names(&m.files), ["spec.pdf", "shot.png"]);
    assert_eq!(m.files[0].size_bytes, 4);
    assert_eq!(m.files[0].mime.as_deref(), Some("application/pdf"));
    s.say("No files here", vec![]);

    let msgs = chat::messages(&s.db, &s.thread).unwrap();
    assert_eq!(names(&msgs[0].files), ["spec.pdf", "shot.png"]);
    assert!(msgs[1].files.is_empty());
    assert_eq!(names(&chat::thread_files(&s.db, &s.thread).unwrap()), ["spec.pdf", "shot.png"]);
    // the stored copy is the file's content
    let blob = files::blob_path(&s.data().join("data"), &msgs[0].files[0].sha256);
    assert_eq!(std::fs::read_to_string(blob).unwrap(), "spec");
    // the Team Lead's answers carry none: thread_files is only yours
    chat::add_message(&s.db, NewMessage { thread_id: s.thread.clone(), role: "agent".into(), body_md: Some("Seen.".into()), ..Default::default() }).unwrap();
    assert_eq!(chat::thread_files(&s.db, &s.thread).unwrap().len(), 2);
}

#[test]
fn a_message_of_files_only_can_be_queued_and_its_text_changed_to_empty_but_not_one_without_either() {
    let s = setup();
    let q = chat::enqueue_with_files(&s.db, &s.you, &s.thread, "  ", &[s.blob("notes.md", "n")]).unwrap();
    assert_eq!(q.body_md, "");
    assert_eq!(names(&q.files), ["notes.md"]);
    assert!(chat::enqueue(&s.db, &s.you, &s.thread, " ").is_err(), "no text and no files");
    let plain = chat::enqueue(&s.db, &s.you, &s.thread, "text only").unwrap();
    assert!(chat::edit_queued(&s.db, &s.you, &plain.id, "").is_err(), "a queued message without files needs its text");
    let q2 = chat::edit_queued(&s.db, &s.you, &q.id, "now with words").unwrap();
    assert_eq!((q2.body_md.as_str(), names(&q2.files)), ("now with words", vec!["notes.md"]));
    assert_eq!(chat::edit_queued(&s.db, &s.you, &q.id, "").unwrap().files.len(), 1);
}

#[test]
fn a_queued_message_keeps_its_files_and_takes_them_along_when_it_goes() {
    let s = setup();
    let q = chat::enqueue_with_files(&s.db, &s.you, &s.thread, "and this one", &[s.blob("a.txt", "a"), s.blob("b.txt", "b")]).unwrap();
    let listed = chat::queue(&s.db, &s.thread).unwrap();
    assert_eq!(names(&listed[0].files), ["a.txt", "b.txt"]);
    // while queued it isn't in the chat yet, so the Team Lead can't attach its files
    assert!(chat::thread_files(&s.db, &s.thread).unwrap().is_empty());

    let sent = chat::send_queued(&s.db, &s.thread, true).unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].id, q.id, "it keeps its id, and with it its files");
    assert_eq!(names(&sent[0].files), ["a.txt", "b.txt"]);
    assert!(chat::queue(&s.db, &s.thread).unwrap().is_empty());
    let msgs = chat::messages(&s.db, &s.thread).unwrap();
    assert_eq!((msgs[0].body_md.as_deref(), names(&msgs[0].files)), (Some("and this one"), vec!["a.txt", "b.txt"]));
    assert_eq!(chat::thread_files(&s.db, &s.thread).unwrap().len(), 2);
}

#[test]
fn removing_a_queued_message_removes_its_files() {
    let s = setup();
    let q = chat::enqueue_with_files(&s.db, &s.you, &s.thread, "never mind", &[s.blob("gone.txt", "g")]).unwrap();
    let fid = q.files[0].id.clone();
    chat::remove_queued(&s.db, &s.you, &q.id).unwrap();
    assert!(chat::queue(&s.db, &s.thread).unwrap().is_empty());
    assert!(files::list(&s.db, "chat_message", &q.id).unwrap().is_empty());
    assert!(files::get(&s.db, &fid).is_err(), "the file row is gone too");
}

#[test]
fn a_folder_or_a_file_over_1_gb_cant_be_added_and_says_which() {
    let s = setup();
    let folder = s.dir.path().join("a folder");
    std::fs::create_dir_all(&folder).unwrap();
    let e = files::check_path(&folder).unwrap_err().to_string();
    assert!(e.contains("a folder is a folder, not a file"), "{e}");
    assert!(files::store(&s.data().join("data"), &folder).is_err());

    let big = s.dir.path().join("huge.iso");
    std::fs::File::create(&big).unwrap().set_len(files::MAX_BYTES + 1).unwrap();
    let e = files::check_path(&big).unwrap_err().to_string();
    assert!(e.contains("huge.iso is larger than 1 GB"), "{e}");
    let ok = s.dir.path().join("fits.bin");
    std::fs::File::create(&ok).unwrap().set_len(10).unwrap();
    assert_eq!(files::check_path(&ok).unwrap(), 10);

    let e = files::check_path(&s.dir.path().join("missing.txt")).unwrap_err().to_string();
    assert!(e.contains("missing.txt"), "{e}");
}

#[test]
fn a_task_is_found_by_its_identifier_case_ignored() {
    let s = setup();
    let p = projects::create(&s.db, &s.you, ProjectInput { name: "Giz AI".into(), key: "GA".into(), ..Default::default() }).unwrap();
    let t = tasks::create(&s.db, &s.you, TaskInput { project_id: p.clone(), title: "Fix the login".into(), ..Default::default() }).unwrap();
    let ident = tasks::get(&s.db, &t).unwrap().identifier;
    assert_eq!(tasks::id_of(&s.db, &ident).unwrap(), t);
    assert_eq!(tasks::id_of(&s.db, &format!(" {} ", ident.to_lowercase())).unwrap(), t);
    assert!(tasks::id_of(&s.db, "GA-999").is_err());
}
