//! GA-59 QA: Bitbucket Cloud repositories in gizai-core. Covers `repo_url::normalize` and `same_repo` for every usual
//! Bitbucket form (and that GitHub and plain git links behave as before), a project linked to Bitbucket and its cards'
//! pull requests (`pulls::card`, `pulls::to_check`), and migration 0013 on a genuine schema 12 database (built by running
//! migrations 0001..0012 only, filled with raw SQL, then opened with `Db::open` like the app does).
use gizai_core::db::{self, Db};
use gizai_core::{model::*, projects, pulls, repo_url, seed::ensure_seed, tasks, team};
use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};
use std::path::{Path, PathBuf};

const TIDY: &str = "https://bitbucket.org/snxo2021/sts_dias";

/// The forms people paste or git shows for one Bitbucket Cloud repository.
const FORMS: [&str; 10] = [
    "https://bitbucket.org/snxo2021/sts_dias",
    "https://bitbucket.org/snxo2021/sts_dias/",
    "https://bitbucket.org/snxo2021/sts_dias/src/master/",
    "https://bitbucket.org/snxo2021/sts_dias/pull-requests/12",
    "https://bitbucket.org/snxo2021/sts_dias.git",
    "https://jefsev@bitbucket.org/snxo2021/sts_dias.git",
    "git@bitbucket.org:snxo2021/sts_dias.git",
    "ssh://git@bitbucket.org/snxo2021/sts_dias.git",
    "  https://bitbucket.org/snxo2021/sts_dias/src/master/  ",
    "https://bitbucket.org/snxo2021/sts_dias/src/master/?at=master",
];

// --- repo_url ---

#[test]
fn every_usual_bitbucket_form_becomes_one_link() {
    for form in FORMS {
        let r = repo_url::normalize(form).unwrap().unwrap_or_else(|| panic!("{form}"));
        assert_eq!((r.url.as_str(), r.provider.as_str(), r.owner.as_deref(), r.name.as_deref()),
                   (TIDY, "bitbucket", Some("snxo2021"), Some("sts_dias")), "{form}");
        assert_eq!(r.full_name().as_deref(), Some("snxo2021/sts_dias"), "{form}");
    }
    assert_eq!(repo_url::provider_name("bitbucket"), Some("Bitbucket"));
}

#[test]
fn a_bitbucket_address_without_a_repository_is_refused() {
    for bad in ["https://bitbucket.org/snxo2021", "https://bitbucket.org/snxo2021/", "git@bitbucket.org:snxo2021"] {
        let e = repo_url::normalize(bad).expect_err(bad).to_string();
        assert!(e.contains("Bitbucket"), "{bad}: {e}");
    }
}

#[test]
fn every_form_of_a_bitbucket_link_is_the_same_repository_whatever_its_case() {
    let mut forms = FORMS.to_vec();
    forms.extend(["https://bitbucket.org/SNXO2021/STS_DIAS", "git@bitbucket.org:Snxo2021/Sts_Dias.git"]);
    for a in &forms {
        for b in &forms {
            assert!(repo_url::same_repo(a, b), "{a} vs {b}");
        }
    }
}

#[test]
fn another_repository_or_workspace_or_the_github_twin_is_not_the_same() {
    for form in FORMS {
        for other in ["https://bitbucket.org/snxo2021/sts_web", "git@bitbucket.org:snxo2021/sts_dias2.git",
                      "https://bitbucket.org/other/sts_dias", "ssh://git@bitbucket.org/other/sts_dias.git",
                      // the same owner/name on GitHub is another repository
                      "https://github.com/snxo2021/sts_dias", "git@github.com:snxo2021/sts_dias.git", "snxo2021/sts_dias"] {
            assert!(!repo_url::same_repo(form, other), "{form} vs {other}");
            assert!(!repo_url::same_repo(other, form), "{other} vs {form}");
        }
    }
}

#[test]
fn github_and_plain_git_links_behave_as_before() {
    let gh = repo_url::normalize("git@github.com:acme/shop.git").unwrap().unwrap();
    assert_eq!((gh.url.as_str(), gh.provider.as_str(), gh.owner.as_deref(), gh.name.as_deref()),
               ("https://github.com/acme/shop", "github", Some("acme"), Some("shop")));
    let lab = repo_url::normalize("https://gitlab.com/acme/shop.git").unwrap().unwrap();
    assert_eq!((lab.url.as_str(), lab.provider.as_str(), lab.owner, lab.name), ("https://gitlab.com/acme/shop.git", "git", None, None));
    let lab_user = repo_url::normalize("https://jefsev@gitlab.com/acme/shop.git").unwrap().unwrap();
    assert_eq!((lab_user.url.as_str(), lab_user.provider.as_str()), ("https://jefsev@gitlab.com/acme/shop.git", "git"), "a user name is kept off Bitbucket");
    assert_eq!(repo_url::provider_name("git"), None);
    assert!(repo_url::same_repo("/srv/git/shop.git", "file:///srv/git/shop.git"));
    assert!(repo_url::same_repo("https://github.com/acme/shop", "git@github.com:ACME/shop.git"));
    assert!(!repo_url::same_repo("https://gitlab.com/acme/shop.git", "https://github.com/acme/shop"));
    assert!(!repo_url::same_repo("https://gitlab.com/acme/shop.git", "https://bitbucket.org/acme/shop"));
    assert!(repo_url::normalize("https://github.com/acme").is_err());
}

// --- a project on Bitbucket and its cards' pull requests ---

struct Board { db: Db, you: String, states: Vec<(String, String)> }

fn board() -> Board {
    let db = Db::open_in_memory().unwrap();
    let you = ensure_seed(&db, "Jeffrey").unwrap().you_id;
    let t = team::get(&db, &team::list(&db).unwrap()[0].id).unwrap();
    let states = t.states.iter().map(|s| (s.category.clone(), s.id.clone())).collect();
    Board { db, you, states }
}

impl Board {
    fn state(&self, category: &str) -> String { self.states.iter().find(|(c, _)| c == category).unwrap().1.clone() }
    fn project(&self, name: &str, key: &str, url: &str, branch: &str) -> String {
        projects::create(&self.db, &self.you, ProjectInput { name: name.into(), key: key.into(), repo_path: Some(format!("/home/j/Code/{key}")),
            repo_url: Some(url.into()), default_branch: Some(branch.into()), ..Default::default() }).unwrap()
    }
    /// A card in the column `state_id`, with `branch` (an agent's run gives a card its branch).
    fn card_in(&self, project: &str, state_id: &str, branch: Option<&str>) -> String {
        let id = tasks::create(&self.db, &self.you, TaskInput { project_id: project.into(), title: "Export invoices".into(),
            state_id: Some(state_id.into()), ..Default::default() }).unwrap();
        if let Some(b) = branch {
            self.db.read(|c| Ok(c.execute("UPDATE tasks SET branch=?2 WHERE id=?1", [&id, b])?)).unwrap();
        }
        id
    }
    fn card(&self, project: &str, category: &str, branch: Option<&str>) -> String { self.card_in(project, &self.state(category), branch) }
    /// (provider, remote_url, owner, name) of the project's repos row.
    fn repo_row(&self, project: &str) -> (String, String, Option<String>, Option<String>) {
        self.db.read(|c| Ok(c.query_row("SELECT provider, remote_url, owner, name FROM repos WHERE project_id=?1 AND deleted_at IS NULL",
            [project], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?)).unwrap()
    }
}

#[test]
fn a_bitbucket_project_is_saved_tidy_and_its_card_can_have_a_pull_request() {
    let b = board();
    let sts = b.project("STS Dias", "STS", "https://jefsev@bitbucket.org/snxo2021/sts_dias.git", "master");
    let tidy = ("bitbucket".to_string(), TIDY.to_string(), Some("snxo2021".to_string()), Some("sts_dias".to_string()));
    assert_eq!(b.repo_row(&sts), tidy);
    let p = projects::get(&b.db, &sts).unwrap();
    assert_eq!((p.repo_url.as_deref(), p.default_branch.as_str()), (Some(TIDY), "master"));

    let id = b.card(&sts, "review", Some("gizai/sts-1-export"));
    let c = pulls::card(&b.db, &id).unwrap();
    assert_eq!((c.provider.as_str(), c.repo.as_str(), c.repo_url.as_str(), c.default_branch.as_str()),
               ("bitbucket", "snxo2021/sts_dias", TIDY, "master"));
    assert_eq!((c.branch.as_str(), c.repo_path.as_str(), c.category.as_str()), ("gizai/sts-1-export", "/home/j/Code/STS", "review"));
    assert!(c.followed());

    // saved again in another form: the same tidy link
    projects::update(&b.db, &b.you, &sts, ProjectInput { name: "STS Dias".into(), key: "STS".into(), repo_path: Some("/home/j/Code/STS".into()),
        repo_url: Some("git@bitbucket.org:snxo2021/sts_dias.git".into()), default_branch: Some("master".into()), ..Default::default() }).unwrap();
    assert_eq!(b.repo_row(&sts), tidy);
    assert_eq!(pulls::card(&b.db, &id).unwrap().repo_url, TIDY);

    // a plain git link still can't have a pull request, and the message names Bitbucket too
    let lab = b.project("Lab", "LAB", "https://gitlab.com/acme/lab.git", "main");
    assert_eq!(b.repo_row(&lab).0, "git");
    let e = pulls::card(&b.db, &b.card(&lab, "review", Some("gizai/lab-1"))).unwrap_err().to_string();
    assert!(e.contains("Link Lab to its GitHub repository first") && e.contains("Bitbucket"), "{e}");
}

#[test]
fn the_pr_check_follows_bitbucket_and_github_cards_but_not_plain_git_or_finished_ones() {
    let b = board();
    let sts = b.project("STS Dias", "STS", "https://bitbucket.org/snxo2021/sts_dias/src/master/", "master");
    let shop = b.project("Shop", "SHOP", "git@github.com:acme/shop.git", "trunk");
    let lab = b.project("Lab", "LAB", "https://gitlab.com/acme/lab.git", "main");
    let team_id = team::list(&b.db).unwrap()[0].id.clone();
    let cancelled = team::add_state(&b.db, &b.you, &team_id, "Cancelled", &b.state("done"), "cancelled").unwrap();
    let bb_pr = "https://bitbucket.org/snxo2021/sts_dias/pull-requests/12";

    let bb_review = b.card(&sts, "review", Some("gizai/sts-1"));
    let bb_open = b.card(&sts, "ready", Some("gizai/sts-2"));
    pulls::record(&b.db, None, &bb_open, bb_pr, "open", false).unwrap();
    let gh_review = b.card(&shop, "review", Some("gizai/shop-1"));
    let _lab_review = b.card(&lab, "review", Some("gizai/lab-1"));
    let bb_done = b.card(&sts, "done", Some("gizai/sts-3"));
    pulls::record(&b.db, None, &bb_done, bb_pr, "open", false).unwrap();
    let bb_cancelled = b.card_in(&sts, &cancelled, Some("gizai/sts-4"));
    pulls::record(&b.db, None, &bb_cancelled, bb_pr, "open", false).unwrap();
    let bb_merged = b.card(&sts, "in_progress", Some("gizai/sts-5"));
    pulls::record(&b.db, None, &bb_merged, bb_pr, "merged", false).unwrap();
    let _bb_no_branch = b.card(&sts, "review", None);

    let mut followed: Vec<(String, String, String)> = pulls::to_check(&b.db).unwrap().into_iter().map(|c| (c.task_id, c.provider, c.repo)).collect();
    followed.sort();
    let mut want = vec![(bb_review, "bitbucket".to_string(), "snxo2021/sts_dias".to_string()),
                        (bb_open, "bitbucket".to_string(), "snxo2021/sts_dias".to_string()),
                        (gh_review, "github".to_string(), "acme/shop".to_string())];
    want.sort();
    assert_eq!(followed, want, "Bitbucket and GitHub cards in Review or with an open pull request; not the plain git one, Done, Cancelled or merged");
    assert_eq!(tasks::get(&b.db, &bb_cancelled).unwrap().state_category, "cancelled");
}

// --- migration 0013 on a schema 12 database ---

fn v12_migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(include_str!("../migrations/0001_init.sql")),
        M::up(include_str!("../migrations/0002_agents.sql")),
        M::up(include_str!("../migrations/0003_chat.sql")),
        M::up(include_str!("../migrations/0004_effort.sql")),
        M::up(include_str!("../migrations/0005_pull_requests.sql")),
        M::up(include_str!("../migrations/0006_worktree_prepare.sql")),
        M::up(include_str!("../migrations/0007_card_flow.sql")),
        M::up(include_str!("../migrations/0008_board_check.sql")),
        M::up(include_str!("../migrations/0009_agent_folders.sql")),
        M::up(include_str!("../migrations/0010_run_refusals.sql")),
        M::up(include_str!("../migrations/0011_column_agents.sql")),
        M::up(include_str!("../migrations/0012_chat_runs_on.sql")),
    ])
}

/// Links as v0.3.0 saved them: a GitHub one, a plain git one, two Bitbucket ones it only knew as plain git URLs (a page
/// link and the Clone button's user@ address), and STS's earlier link, removed (soft-deleted).
const DATA: &str = r#"
INSERT INTO orgs (id, created_at, updated_at, name, key) VALUES ('org', 1, 1, 'Yotech', 'YT');
INSERT INTO actors (id, created_at, updated_at, org_id, kind, name, handle, status) VALUES
  ('you', 1, 1, 'org', 'person', 'Tjitske', 'tjitske', 'active');
INSERT INTO projects (id, created_at, updated_at, created_by, updated_by, org_id, number, key, name) VALUES
  ('p-shop', 2, 2, 'you', 'you', 'org', '2026-001', 'SHOP', 'Shop'),
  ('p-lab',  3, 3, 'you', 'you', 'org', '2026-002', 'LAB',  'Lab'),
  ('p-sts',  4, 4, 'you', 'you', 'org', '2026-003', 'STS',  'STS Dias'),
  ('p-web',  5, 5, 'you', 'you', 'org', '2026-004', 'WEB',  'Web');
INSERT INTO repos (id, created_at, updated_at, deleted_at, version, created_by, updated_by, project_id, provider, remote_url, owner, name,
                   default_branch, local_path) VALUES
  ('r-shop', 10, 11, NULL, 2, 'you', 'you', 'p-shop', 'github', 'https://github.com/acme/shop', 'acme', 'shop', 'trunk', '/home/t/Code/shop'),
  ('r-lab',  20, 21, NULL, 1, 'you', 'you', 'p-lab',  'git', 'https://gitlab.com/acme/shop.git', NULL, NULL, 'main', '/home/t/Code/lab'),
  ('r-old',  25, 29, 29,   2, 'you', 'you', 'p-sts',  'git', 'git@gitlab.com:snxo2021/sts_dias.git', NULL, NULL, 'develop', '/home/t/Code/sts_old'),
  ('r-sts',  30, 31, NULL, 3, 'you', 'you', 'p-sts',  'git', 'https://bitbucket.org/snxo2021/sts_dias/src/master/', NULL, NULL, 'master', '/home/t/Code/sts_dias'),
  ('r-web',  40, 41, NULL, 1, 'you', NULL,  'p-web',  'git', 'https://jefsev@bitbucket.org/acme/web.git', NULL, NULL, 'production', '/home/t/Code/web');
"#;

struct Fixture {
    _dir: tempfile::TempDir,
    path: PathBuf,
    backups: PathBuf,
}

/// A genuine schema 12 database file (what v0.3.0 left on disk), filled with DATA.
fn v12_db() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    let mut c = Connection::open(&path).unwrap();
    c.pragma_update(None, "journal_mode", "WAL").unwrap();
    c.pragma_update(None, "foreign_keys", "OFF").unwrap();
    v12_migrations().to_latest(&mut c).unwrap();
    c.pragma_update(None, "foreign_keys", "ON").unwrap();
    c.execute_batch(DATA).unwrap();
    c.execute("INSERT INTO devices(id, created_at, name, is_self) VALUES ('dev', 1, 'test', 1)", []).unwrap();
    assert_eq!(version(&c), 12, "the fixture is schema 12");
    assert_eq!(broken_keys(&c), 0, "the fixture's rows are consistent");
    assert!(c.execute("INSERT INTO repos (id, created_at, updated_at, project_id, provider, remote_url) VALUES ('x', 1, 1, 'p-web', 'bitbucket', 'u')", []).is_err(),
            "schema 12 doesn't know Bitbucket yet");
    drop(c);
    let backups = dir.path().join("backups");
    Fixture { _dir: dir, path, backups }
}

fn version(c: &Connection) -> i64 {
    c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap()
}

fn broken_keys(c: &Connection) -> i64 {
    c.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r.get(0)).unwrap()
}

fn backup_names(dir: &Path) -> Vec<String> {
    match std::fs::read_dir(dir) {
        Ok(rd) => rd.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect(),
        Err(_) => vec![],
    }
}

#[derive(Debug, Clone, PartialEq)]
struct RepoRow {
    id: String, created_at: i64, updated_at: i64, deleted_at: Option<i64>, version: i64, created_by: Option<String>, updated_by: Option<String>,
    project_id: String, provider: String, remote_url: String, owner: Option<String>, name: Option<String>,
    default_branch: Option<String>, local_path: Option<String>,
}

fn repo_rows(c: &Connection) -> Vec<RepoRow> {
    let mut st = c.prepare("SELECT id, created_at, updated_at, deleted_at, version, created_by, updated_by, project_id, provider, remote_url,
                                   owner, name, default_branch, local_path FROM repos ORDER BY id").unwrap();
    st.query_map([], |r| Ok(RepoRow {
        id: r.get(0)?, created_at: r.get(1)?, updated_at: r.get(2)?, deleted_at: r.get(3)?, version: r.get(4)?, created_by: r.get(5)?,
        updated_by: r.get(6)?, project_id: r.get(7)?, provider: r.get(8)?, remote_url: r.get(9)?, owner: r.get(10)?, name: r.get(11)?,
        default_branch: r.get(12)?, local_path: r.get(13)?,
    })).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
}

#[test]
fn a_schema_12_database_upgrades_to_13_after_a_backup() {
    let f = v12_db();
    let db = Db::open(&f.path).unwrap();
    let (v, broken, leftover) = db.read(|c| Ok((version(c), broken_keys(c),
        c.query_row("SELECT count(*) FROM sqlite_master WHERE name = 'repos_new'", [], |r| r.get::<_, i64>(0))?))).unwrap();
    assert_eq!((v, broken, leftover), (db::SCHEMA_VERSION, 0, 0), "the current schema, foreign keys intact, no half-done rebuild");
    assert_eq!(db::SCHEMA_VERSION, 13);

    let names = backup_names(&f.backups);
    assert_eq!(names.len(), 1, "{names:?}");
    assert!(names[0].starts_with("gizai-before-v13-") && names[0].ends_with(".db"), "{names:?}");
    let old = Connection::open(f.backups.join(&names[0])).unwrap();
    let old_rows = repo_rows(&old);
    assert_eq!(version(&old), 12, "the backup is the schema 12 database as it was");
    assert_eq!(old_rows.iter().map(|r| (r.id.as_str(), r.provider.as_str())).collect::<Vec<_>>(),
               [("r-lab", "git"), ("r-old", "git"), ("r-shop", "github"), ("r-sts", "git"), ("r-web", "git")]);
}

#[test]
fn every_repos_row_is_kept_and_old_bitbucket_links_get_their_tidy_form() {
    let f = v12_db();
    let before = repo_rows(&Connection::open(&f.path).unwrap());
    let db = Db::open(&f.path).unwrap();
    let after = db.read(|c| Ok(repo_rows(c))).unwrap();
    let ids = |rows: &[RepoRow]| rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&after), ids(&before), "every row is still there");
    for (b, a) in before.iter().zip(&after) {
        assert_eq!((&a.project_id, a.created_at, &a.created_by, &a.default_branch, &a.local_path, a.deleted_at),
                   (&b.project_id, b.created_at, &b.created_by, &b.default_branch, &b.local_path, b.deleted_at), "{}", b.id);
    }
    let row = |rows: &[RepoRow], id: &str| rows.iter().find(|r| r.id == id).unwrap().clone();
    for id in ["r-shop", "r-lab", "r-old"] {
        assert_eq!(row(&after, id), row(&before, id), "{id}: GitHub, plain git and removed links stay exactly as they were");
    }
    let link = |id: &str| { let r = row(&after, id); (r.provider, r.remote_url, r.owner, r.name) };
    assert_eq!(link("r-sts"), ("bitbucket".into(), TIDY.into(), Some("snxo2021".into()), Some("sts_dias".into())));
    assert_eq!(link("r-web"), ("bitbucket".into(), "https://bitbucket.org/acme/web".into(), Some("acme".into()), Some("web".into())));

    let sts = projects::get(&db, "p-sts").unwrap();
    assert_eq!((sts.repo_url.as_deref(), sts.repo_path.as_deref(), sts.default_branch.as_str()),
               (Some(TIDY), Some("/home/t/Code/sts_dias"), "master"), "the live link, not the removed one");
    assert_eq!(projects::get(&db, "p-web").unwrap().repo_url.as_deref(), Some("https://bitbucket.org/acme/web"));
    assert_eq!(projects::get(&db, "p-shop").unwrap().repo_url.as_deref(), Some("https://github.com/acme/shop"));
    assert_eq!(projects::get(&db, "p-lab").unwrap().repo_url.as_deref(), Some("https://gitlab.com/acme/shop.git"));
}

#[test]
fn the_upgraded_repos_table_takes_bitbucket_and_still_refuses_an_unknown_provider() {
    let f = v12_db();
    let db = Db::open(&f.path).unwrap();
    let insert = |id: &str, provider: &str| db.read(|c| Ok(c.execute(
        "INSERT INTO repos (id, created_at, updated_at, project_id, provider, remote_url, owner, name, local_path)
         VALUES (?1, 50, 50, 'p-lab', ?2, 'https://bitbucket.org/acme/lab', 'acme', 'lab', '/home/t/Code/lab2')", [id, provider])?));
    assert_eq!(insert("r-bb", "bitbucket").unwrap(), 1);
    let e = insert("r-gl", "gitlab").unwrap_err().to_string();
    assert!(e.contains("CHECK constraint failed"), "{e}");
    let e = db.read(|c| Ok(c.execute("INSERT INTO repos (id, created_at, updated_at, project_id, remote_url) VALUES ('r-x', 1, 1, 'nope', 'u')", [])?))
        .unwrap_err().to_string();
    assert!(e.contains("FOREIGN KEY constraint failed"), "the project must exist: {e}");
}

#[test]
fn opening_the_upgraded_database_again_changes_nothing() {
    let f = v12_db();
    let first = db_rows(&Db::open(&f.path).unwrap());
    let second = db_rows(&Db::open(&f.path).unwrap());
    assert_eq!(first, second);
    assert_eq!(first.len(), 5);
    assert_eq!(backup_names(&f.backups).len(), 1, "no second backup: the database is current");
}

fn db_rows(db: &Db) -> Vec<RepoRow> {
    db.read(|c| Ok(repo_rows(c))).unwrap()
}
