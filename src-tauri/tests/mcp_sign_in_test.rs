//! Settings → MCP servers' sign-in (GA-39) the way the app runs it, against a fake authorization server and MCP server on
//! 127.0.0.1 modelled on Otus OS (crates/gizai-agents/tests/support/fake_otus.rs): an address server that asks for OAuth
//! shows needs sign-in, signs in (the test reads the address Gizai would open and plays the browser with plain HTTP GETs),
//! keeps its tokens in the keychain and not in Gizai's database, lists its tools, refreshes before List tools and before
//! a run, one refresh at a time, and is left out of a run with a plain note when it can't be used. The keychain is in
//! memory (test_state).
#[path = "../../crates/gizai-agents/tests/support/fake_otus.rs"]
mod fake_otus;

use std::path::Path;
use std::sync::Barrier;
use std::time::Duration;

use fake_otus::{Config, FakeOtus, http, query_of};
use gizai_core::mcp_servers::{AgentServer, AgentTools, McpServer};
use gizai_core::model::AgentInput;
use gizai_core::team::{self, Member};
use gizai_lib::AppState;
use gizai_lib::mcp_servers::{self, ServerInput, ServerView};
use serde_json::Value;

const WAIT: Duration = Duration::from_secs(20);
const THIRTY_MINUTES: Duration = Duration::from_secs(30 * 60);

struct T {
    st: AppState,
    id: String,
    fake: FakeOtus,
    _dir: tempfile::TempDir,
}

fn setup(cfg: Config) -> T {
    let dir = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(dir.path());
    let fake = FakeOtus::start(cfg);
    let v = mcp_servers::save(&st, ServerInput {
        server: McpServer { name: "otus".into(), transport: "http".into(), url: fake.mcp_url(), ..Default::default() },
        ..Default::default()
    }).unwrap();
    T { st, id: v.server.id, fake, _dir: dir }
}

impl T {
    /// Sign in, the test playing the browser on the address Gizai would open.
    fn sign_in(&self) -> Result<ServerView, String> {
        let mut browser = None;
        let v = mcp_servers::sign_in(&self.st, &self.id, |url| {
            let url = url.to_string();
            browser = Some(std::thread::spawn(move || fake_otus::play_browser(&url)));
            Ok(())
        }, WAIT);
        if let Some(b) = browser {
            let (status, page) = b.join().unwrap();
            if v.is_ok() {
                assert_eq!(status, 200);
                assert!(page.contains("Gizai is signed in"), "{page}");
            }
        }
        v
    }

    fn view(&self) -> ServerView {
        mcp_servers::get(&self.st, &self.id).unwrap()
    }

    /// What the keychain keeps for the server.
    fn saved(&self) -> Option<gizai_agents::oauth::Saved> {
        self.st.tokens.load(&self.id).unwrap()
    }

    /// A new agent with the server switched on, as a run gets it.
    fn agent_with_otus_on(&self) -> Member {
        let team_id = team::list(&self.st.db).unwrap()[0].id.clone();
        let agent = team::add_agent(&self.st.db, &self.st.you_id, &team_id, AgentInput {
            name: "Dev Agent".into(), role_key: "dev".into(), ..Default::default() }).unwrap();
        mcp_servers::save_agent(&self.st, &agent, AgentTools {
            mcp: vec![AgentServer { server_id: self.id.clone(), on: true, tools_off: vec![] }],
        }).unwrap();
        team::agent(&self.st.db, &agent).unwrap()
    }
}

/// The token in a run's entry for the server.
fn bearer(entry: &Value) -> String {
    let h = entry["headers"]["Authorization"].as_str().unwrap_or_else(|| panic!("no Authorization header in the entry"));
    h.strip_prefix("Bearer ").expect("a Bearer token").to_string()
}

/// Whether any file under `dir` holds `needle`.
fn any_file_holds(dir: &Path, needle: &str) -> Vec<String> {
    let mut found = vec![];
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            found.extend(any_file_holds(&p, needle));
        } else if let Ok(bytes) = std::fs::read(&p) {
            if bytes.windows(needle.len()).any(|w| w == needle.as_bytes()) {
                found.push(p.display().to_string());
            }
        }
    }
    found
}

#[test]
fn an_address_server_that_asks_for_oauth_shows_needs_sign_in_then_signs_in_and_lists_its_tools() {
    let t = setup(Config::otus());
    assert_eq!(mcp_servers::SIGN_IN_TIMEOUT, Duration::from_secs(10 * 60), "the loopback listener waits 10 minutes");

    let v = mcp_servers::list_tools(&t.st, &t.id).unwrap();
    assert_eq!(v.sign_in, "needs_sign_in");
    assert!(v.problem.as_deref().is_some_and(|p| p.contains("needs sign-in")), "{:?}", v.problem);
    assert!(v.listed.is_none());

    let v = t.sign_in().unwrap();
    assert_eq!(v.sign_in, "signed_in");
    assert_eq!(v.problem, None);
    let log = t.fake.log();
    assert_eq!(log.registrations.len(), 1, "Gizai registered itself");
    assert_eq!(log.registrations[0]["client_name"], "Gizai");
    let auth = &log.authorizations[0];
    assert_eq!(auth["resource"], t.fake.mcp_url());
    assert_eq!(auth["code_challenge_method"], "S256");

    // The tokens are in the keychain, and nowhere in Gizai's data folder (database, WAL, settings).
    let s = t.saved().expect("the sign-in in the keychain");
    let raw = t.st.keychain.get(&format!("mcp/{}/oauth", t.id)).unwrap().unwrap();
    assert!(raw.contains(&s.access_token));
    let refresh_token = s.refresh_token.clone().unwrap();
    for secret in [&s.access_token, &refresh_token] {
        let found = any_file_holds(&t.st.data_dir, secret);
        assert!(found.is_empty(), "a token is in {found:?}");
    }

    // List tools works now, with the token.
    let v = mcp_servers::list_tools(&t.st, &t.id).unwrap();
    assert_eq!(v.sign_in, "signed_in");
    assert_eq!(v.problem, None);
    let listed = v.listed.expect("its tools");
    assert_eq!(listed.server_name, "otus-os");
    let names: Vec<&str> = listed.tools.iter().map(|tool| tool.name.as_str()).collect();
    assert_eq!(names, ["list_tasks", "create_task"]);
    assert!(t.fake.log().mcp_tokens.iter().all(|tok| *tok == s.access_token));
    assert_eq!(t.fake.refreshes(), 0, "a token good for an hour isn't refreshed");
}

#[test]
fn list_tools_refreshes_a_token_with_less_than_five_minutes_left_first() {
    let t = setup(Config { access_ttl: 60, ..Config::otus() });
    mcp_servers::list_tools(&t.st, &t.id).unwrap();
    t.sign_in().unwrap();
    let first = t.saved().unwrap();
    t.fake.set_access_ttl(3600);
    let refused_before = t.fake.log().mcp_refused;

    let v = mcp_servers::list_tools(&t.st, &t.id).unwrap();
    assert!(v.listed.is_some(), "{:?}", v.problem);
    assert_eq!(t.fake.refreshes(), 1);
    let now = t.saved().unwrap();
    assert_ne!(now.access_token, first.access_token);
    let log = t.fake.log();
    assert_eq!(log.mcp_refused, refused_before, "refreshed before asking, not after a 401");
    assert!(!log.mcp_tokens.is_empty() && log.mcp_tokens.iter().all(|tok| *tok == now.access_token), "only the new token was used");
    assert!(t.fake.refresh_valid(now.refresh_token.as_deref().unwrap()), "the replaced refresh token is the one kept");
}

#[test]
fn a_run_gets_a_token_refreshed_when_less_than_its_time_cap_is_left() {
    // 20 minutes left after the sign-in.
    let t = setup(Config { access_ttl: 20 * 60, ..Config::otus() });
    t.sign_in().unwrap();
    let first = t.saved().unwrap();
    t.fake.set_access_ttl(3600);
    let agent = t.agent_with_otus_on();

    // A run capped at 10 minutes: 20 are enough.
    let (servers, notes) = mcp_servers::for_run(&t.st, &agent, Duration::from_secs(10 * 60));
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(servers.len(), 1);
    assert_eq!(bearer(&servers[0].entry), first.access_token);
    assert_eq!(t.fake.refreshes(), 0);

    // A run capped at 30 minutes: refreshed first.
    let (servers, notes) = mcp_servers::for_run(&t.st, &agent, THIRTY_MINUTES);
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(servers.len(), 1);
    let s = &servers[0];
    assert_eq!(s.name, "otus");
    assert_eq!(s.entry["type"], "http");
    assert_eq!(s.entry["url"], t.fake.mcp_url());
    let token = bearer(&s.entry);
    assert_ne!(token, first.access_token);
    assert_eq!(t.fake.refreshes(), 1);
    assert_eq!(fake_otus::mcp_status(&t.fake.mcp_url(), &token), 200);
    let kept = t.saved().unwrap();
    assert_eq!(kept.access_token, token, "saved in the keychain before the run gets it");
    assert!(t.fake.refresh_valid(kept.refresh_token.as_deref().unwrap()));
}

#[test]
fn two_runs_starting_at_once_both_get_a_working_token_from_one_refresh() {
    let t = setup(Config { access_ttl: 60, ..Config::otus() });
    t.sign_in().unwrap();
    t.fake.set_access_ttl(3600);
    t.fake.set_refresh_delay(Duration::from_millis(400));
    let agent = t.agent_with_otus_on();

    let gate = Barrier::new(2);
    let (a, b) = std::thread::scope(|scope| {
        let run = || {
            gate.wait();
            mcp_servers::for_run(&t.st, &agent, THIRTY_MINUTES)
        };
        let a = scope.spawn(run);
        let b = scope.spawn(run);
        (a.join().unwrap(), b.join().unwrap())
    });
    for (servers, notes) in [&a, &b] {
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(servers.len(), 1);
        assert_eq!(fake_otus::mcp_status(&t.fake.mcp_url(), &bearer(&servers[0].entry)), 200, "a working token");
    }
    assert_eq!(bearer(&a.0[0].entry), bearer(&b.0[0].entry));
    assert_eq!(t.fake.refreshes(), 1, "one refresh for both runs");
    assert_eq!(t.fake.log().max_refreshes_at_once, 1);
}

#[test]
fn two_runs_at_once_with_the_default_90_minute_cap_and_otus_hour_long_tokens_refresh_one_at_a_time() {
    // Otus's access tokens last an hour, less than the default run cap: no token ever lasts the cap.
    let t = setup(Config::otus());
    t.sign_in().unwrap();
    t.fake.set_refresh_delay(Duration::from_millis(400));
    let agent = t.agent_with_otus_on();
    let cap = Duration::from_secs(gizai_lib::runs::DEFAULT_MAX_RUN_MINUTES * 60);

    let gate = Barrier::new(2);
    let (a, b) = std::thread::scope(|scope| {
        let run = || {
            gate.wait();
            mcp_servers::for_run(&t.st, &agent, cap)
        };
        let a = scope.spawn(run);
        let b = scope.spawn(run);
        (a.join().unwrap(), b.join().unwrap())
    });
    for (servers, notes) in [&a, &b] {
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(servers.len(), 1);
        assert_eq!(fake_otus::mcp_status(&t.fake.mcp_url(), &bearer(&servers[0].entry)), 200, "a working token");
    }
    assert_eq!(t.fake.log().max_refreshes_at_once, 1, "one refresh at a time");
    let kept = t.saved().unwrap();
    assert!(t.fake.refresh_valid(kept.refresh_token.as_deref().unwrap()), "the last replaced refresh token is the one kept");
    eprintln!("refreshes for two runs at once with a {} min cap and 60 min tokens: {}", cap.as_secs() / 60, t.fake.refreshes());
}

#[test]
fn list_tools_renews_a_token_the_server_dropped_and_asks_again() {
    let t = setup(Config::otus());
    t.sign_in().unwrap();
    let first = t.saved().unwrap();
    t.fake.drop_access_tokens();
    let v = mcp_servers::list_tools(&t.st, &t.id).unwrap();
    assert!(v.listed.is_some(), "{:?}", v.problem);
    assert_eq!(v.sign_in, "signed_in");
    assert_eq!(t.fake.refreshes(), 1);
    assert_ne!(t.saved().unwrap().access_token, first.access_token);
}

#[test]
fn a_refresh_that_cant_reach_the_server_leaves_it_out_of_the_run_and_keeps_the_sign_in() {
    let t = setup(Config { access_ttl: 60, ..Config::otus() });
    t.sign_in().unwrap();
    let agent = t.agent_with_otus_on();
    let mut s = t.saved().unwrap();
    // Nothing listens on port 1.
    s.token_endpoint = "http://127.0.0.1:1/oauth/token".into();
    t.st.tokens.save(&t.id, &s).unwrap();
    let (servers, notes) = mcp_servers::for_run(&t.st, &agent, THIRTY_MINUTES);
    assert!(servers.is_empty());
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].starts_with("Left out otus: couldn't refresh its sign-in"), "{}", notes[0]);
    assert!(t.saved().is_some(), "not signed out for a network problem");
}

#[test]
fn removing_a_signed_in_server_revokes_and_forgets_its_tokens() {
    let t = setup(Config::otus());
    t.sign_in().unwrap();
    let s = t.saved().unwrap();
    mcp_servers::remove(&t.st, &t.id).unwrap();
    assert_eq!(t.fake.log().revoked.len(), 1);
    assert!(!t.fake.refresh_valid(s.refresh_token.as_deref().unwrap()));
    assert!(t.saved().is_none());
}

#[test]
fn signing_in_without_list_tools_first_finds_the_metadata_at_the_well_known_address() {
    let t = setup(Config::otus());
    let v = t.sign_in().unwrap();
    assert_eq!(v.sign_in, "signed_in");
    assert!(t.fake.log().requests.contains(&"GET /.well-known/oauth-protected-resource/api/mcp".to_string()));
}

#[test]
fn a_refused_refresh_leaves_the_server_out_of_the_run_with_a_plain_note() {
    let t = setup(Config { access_ttl: 60, ..Config::otus() });
    mcp_servers::list_tools(&t.st, &t.id).unwrap();
    t.sign_in().unwrap();
    let s = t.saved().unwrap();
    let agent = t.agent_with_otus_on();
    t.fake.withdraw_refresh_tokens();

    let (servers, notes) = mcp_servers::for_run(&t.st, &agent, THIRTY_MINUTES);
    assert!(servers.is_empty(), "left out");
    assert_eq!(notes.len(), 1, "{notes:?}");
    let note = &notes[0];
    assert!(note.starts_with("Left out otus: "), "{note}");
    assert!(note.contains("refused") && note.contains("Sign in again"), "{note}");
    assert!(!note.contains(&s.access_token) && !note.contains(s.refresh_token.as_deref().unwrap()), "{note}");

    let v = t.view();
    assert_eq!(v.sign_in, "needs_sign_in");
    assert!(v.problem.as_deref().is_some_and(|p| p.contains("Sign in again")), "{:?}", v.problem);

    // The next run: still left out, as signed out.
    let (servers, notes) = mcp_servers::for_run(&t.st, &agent, THIRTY_MINUTES);
    assert!(servers.is_empty());
    assert_eq!(notes, ["Left out otus: signed out. Sign in again in Settings → MCP servers."]);
}

#[test]
fn sign_out_revokes_at_the_server_forgets_the_tokens_and_runs_leave_the_server_out() {
    let t = setup(Config::otus());
    t.sign_in().unwrap();
    let s = t.saved().unwrap();
    let refresh_token = s.refresh_token.clone().unwrap();
    let agent = t.agent_with_otus_on();
    assert_eq!(mcp_servers::for_run(&t.st, &agent, THIRTY_MINUTES).0.len(), 1, "on while signed in");

    let v = mcp_servers::sign_out(&t.st, &t.id).unwrap();
    assert_eq!(v.sign_in, "needs_sign_in");
    assert_eq!(t.fake.log().revoked, vec![(refresh_token.clone(), "refresh_token".to_string())]);
    assert!(!t.fake.refresh_valid(&refresh_token));
    assert!(t.saved().is_none());
    assert_eq!(t.st.keychain.get(&format!("mcp/{}/oauth", t.id)).unwrap(), None);

    let (servers, notes) = mcp_servers::for_run(&t.st, &agent, THIRTY_MINUTES);
    assert!(servers.is_empty());
    assert_eq!(notes, ["Left out otus: signed out. Sign in again in Settings → MCP servers."]);
}

#[test]
fn a_declined_sign_in_shows_why_and_the_server_still_needs_sign_in() {
    let t = setup(Config::otus());
    mcp_servers::list_tools(&t.st, &t.id).unwrap();
    let mut browser = None;
    let e = mcp_servers::sign_in(&t.st, &t.id, |url| {
        let q = query_of(url);
        let back = format!("{}?error=access_denied&state={}", q["redirect_uri"], fake_otus::pct(&q["state"]));
        browser = Some(std::thread::spawn(move || http("GET", &back, &[], "")));
        Ok(())
    }, WAIT).unwrap_err();
    let (status, _, page) = browser.unwrap().join().unwrap();
    assert_eq!(status, 200);
    assert!(page.contains("couldn't sign in"), "{page}");
    assert!(e.contains("declined"), "{e}");
    let v = t.view();
    assert_eq!(v.sign_in, "needs_sign_in");
    assert!(v.problem.as_deref().is_some_and(|p| p.contains("declined")), "{:?}", v.problem);
    assert!(t.saved().is_none());
    assert!(t.fake.log().token_requests.is_empty());
}

#[test]
fn a_server_without_registration_signs_in_with_the_client_id_you_entered() {
    let t = setup(Config { registration: false, known_client: Some("otus-entered-id".into()), ..Config::otus() });
    mcp_servers::list_tools(&t.st, &t.id).unwrap();

    let e = t.sign_in().unwrap_err();
    assert_eq!(e, gizai_agents::oauth::NO_REGISTRATION);
    assert_eq!(t.view().problem.as_deref(), Some(gizai_agents::oauth::NO_REGISTRATION));

    let mut server = t.view().server;
    server.client_id = "otus-entered-id".into();
    mcp_servers::save(&t.st, ServerInput { server, ..Default::default() }).unwrap();
    let v = t.sign_in().unwrap();
    assert_eq!(v.sign_in, "signed_in");
    assert_eq!(t.saved().unwrap().client_id, "otus-entered-id");
    assert!(t.fake.log().registrations.is_empty());
    let v = mcp_servers::list_tools(&t.st, &t.id).unwrap();
    assert!(v.listed.is_some(), "{:?}", v.problem);
}
