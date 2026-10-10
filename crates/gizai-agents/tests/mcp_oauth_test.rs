//! Signing in to an MCP server with OAuth (GA-39), against a fake authorization server on 127.0.0.1 modelled on Otus OS
//! (tests/support/fake_otus.rs): discovery, registration as a public client, the sign-in address with PKCE S256, state
//! and resource, the loopback answer (the test plays the browser with plain HTTP GETs: no browser opens), refresh with a
//! replaced refresh token, revocation, and the TokenStore's one-refresh-at-a-time per server. The keychain is in memory.
#[path = "support/fake_otus.rs"]
mod fake_otus;

use std::net::TcpStream;
use std::sync::{Arc, Barrier, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fake_otus::{Config, FakeOtus, b64url, http, query_of, sha256};
use gizai_agents::oauth::{self, RefreshError, Saved, TokenProblem, TokenStore};
use gizai_agents::secrets::{Keychain, MemoryKeychain};

const WAIT: Duration = Duration::from_secs(20);
const FIVE_MINUTES: Duration = Duration::from_secs(5 * 60);

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
}

/// The 401 header the fake's MCP gives without a token.
fn challenge(fake: &FakeOtus) -> String {
    let (status, headers, _) = http("POST", &fake.mcp_url(), &[("Content-Type", "application/json")], "{}");
    assert_eq!(status, 401);
    fake_otus::header(&headers, "www-authenticate").expect("a WWW-Authenticate header").to_string()
}

/// A sign-in as the keychain keeps it, with tokens the fake gave `client`, the access token running out in `ms`.
fn saved(fake: &FakeOtus, client: &str, ms: i64) -> Saved {
    let (access, refresh) = fake.mint(client);
    Saved {
        access_token: access,
        refresh_token: Some(refresh),
        expires_at: Some(now_ms() + ms),
        client_id: client.into(),
        token_endpoint: format!("{}/oauth/token", fake.base),
        revocation_endpoint: Some(format!("{}/oauth/revoke", fake.base)),
        resource: fake.mcp_url(),
        issuer: fake.base.clone(),
        scope: None,
    }
}

fn store() -> TokenStore {
    TokenStore::new(Arc::new(MemoryKeychain::default()))
}

/// Whether something still listens on the redirect's port.
fn listening(redirect_uri: &str) -> bool {
    let authority = redirect_uri.trim_start_matches("http://").split('/').next().unwrap();
    TcpStream::connect(authority).is_ok()
}

#[test]
fn the_fakes_pkce_check_is_real_sha256() {
    // RFC 7636, appendix B.
    assert_eq!(b64url(&sha256(b"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk")), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    use sha2::Digest;
    for s in ["", "abc", "a longer verifier of more than one block of sixty-four bytes, to cover the padding"] {
        assert_eq!(sha256(s.as_bytes()).to_vec(), sha2::Sha256::digest(s.as_bytes()).to_vec(), "{s:?}");
    }
}

#[test]
fn discovery_follows_the_401s_resource_metadata_to_the_authorization_server() {
    let fake = FakeOtus::start(Config {
        prm_paths: vec!["/meta/otus-prm".into()], header_prm: Some("/meta/otus-prm".into()), ..Config::otus()
    });
    let header = challenge(&fake);
    assert!(header.contains("resource_metadata="), "{header}");
    let d = oauth::discover(&fake.mcp_url(), Some(&header)).unwrap();
    assert_eq!(d.resource, fake.mcp_url());
    assert_eq!(d.issuer, fake.base);
    assert_eq!(d.authorization_endpoint, format!("{}/oauth/authorize", fake.base));
    assert_eq!(d.token_endpoint, format!("{}/oauth/token", fake.base));
    assert_eq!(d.registration_endpoint.as_deref(), Some(format!("{}/oauth/register", fake.base).as_str()));
    assert_eq!(d.revocation_endpoint.as_deref(), Some(format!("{}/oauth/revoke", fake.base).as_str()));
    assert_eq!(d.service_name.as_deref(), Some("OTUS"));
    assert_eq!(d.scope, None);
    let asked = fake.log().requests;
    assert!(asked.contains(&"GET /meta/otus-prm".to_string()), "{asked:?}");
    assert!(asked.contains(&"GET /.well-known/oauth-authorization-server".to_string()), "{asked:?}");
    assert!(!asked.iter().any(|r| r.starts_with("GET /.well-known/oauth-protected-resource")), "the header's address was enough: {asked:?}");
}

#[test]
fn discovery_without_resource_metadata_in_the_header_tries_the_well_known_address_under_the_mcp_path_then_at_the_root() {
    // Only under the MCP path.
    let fake = FakeOtus::start(Config {
        prm_paths: vec!["/.well-known/oauth-protected-resource/api/mcp".into()], header_prm: None, ..Config::otus()
    });
    let d = oauth::discover(&fake.mcp_url(), Some(&challenge(&fake))).unwrap();
    assert_eq!(d.resource, fake.mcp_url());
    assert_eq!(d.service_name.as_deref(), Some("OTUS"), "the metadata was found");
    assert!(fake.log().requests.contains(&"GET /.well-known/oauth-protected-resource/api/mcp".to_string()));

    // Only at the root: the path's address is tried first, then the root's.
    let fake = FakeOtus::start(Config { header_prm: None, ..Config::otus() });
    let d = oauth::discover(&fake.mcp_url(), None).unwrap();
    assert_eq!(d.service_name.as_deref(), Some("OTUS"), "the metadata was found");
    assert_eq!(d.token_endpoint, format!("{}/oauth/token", fake.base));
    let asked = fake.log().requests;
    let under = asked.iter().position(|r| r == "GET /.well-known/oauth-protected-resource/api/mcp");
    let root = asked.iter().position(|r| r == "GET /.well-known/oauth-protected-resource");
    assert!(under.is_some() && root.is_some() && under < root, "{asked:?}");
}

#[test]
fn discovery_falls_back_to_openid_configuration() {
    let fake = FakeOtus::start(Config { as_paths: vec!["/.well-known/openid-configuration".into()], ..Config::otus() });
    let d = oauth::discover(&fake.mcp_url(), Some(&challenge(&fake))).unwrap();
    assert_eq!(d.authorization_endpoint, format!("{}/oauth/authorize", fake.base));
    assert_eq!(d.token_endpoint, format!("{}/oauth/token", fake.base));
    let asked = fake.log().requests;
    let rfc8414 = asked.iter().position(|r| r == "GET /.well-known/oauth-authorization-server");
    let openid = asked.iter().position(|r| r == "GET /.well-known/openid-configuration");
    assert!(rfc8414.is_some() && openid.is_some() && rfc8414 < openid, "RFC 8414 first, then OpenID: {asked:?}");
}

#[test]
fn discovery_refuses_a_server_without_pkce_s256() {
    let fake = FakeOtus::start(Config { s256: false, ..Config::otus() });
    let e = oauth::discover(&fake.mcp_url(), Some(&challenge(&fake))).unwrap_err();
    assert!(e.contains("S256"), "{e}");
}

#[test]
fn begin_registers_gizai_as_a_public_client_with_a_loopback_redirect() {
    let fake = FakeOtus::start(Config::otus());
    let d = oauth::discover(&fake.mcp_url(), Some(&challenge(&fake))).unwrap();
    let p = oauth::begin(&d, None).unwrap();
    let regs = fake.log().registrations;
    assert_eq!(regs.len(), 1, "registered once");
    let r = &regs[0];
    assert_eq!(r["client_name"], "Gizai");
    assert_eq!(r["token_endpoint_auth_method"], "none", "a public client, without a secret");
    assert_eq!(r["redirect_uris"], serde_json::json!([p.redirect_uri.clone()]));
    let grants = r["grant_types"].as_array().unwrap();
    assert!(grants.contains(&"authorization_code".into()) && grants.contains(&"refresh_token".into()), "{r}");
    assert_eq!(r["response_types"], serde_json::json!(["code"]));
    assert!(r.get("client_secret").is_none());

    let port: u16 = p.redirect_uri.strip_prefix("http://127.0.0.1:").and_then(|rest| rest.strip_suffix("/callback"))
        .and_then(|port| port.parse().ok()).unwrap_or_else(|| panic!("not a loopback redirect: {}", p.redirect_uri));
    assert_ne!(port, 0);
    assert!(p.client_id.starts_with("otus-client-"), "the client id the server gave: {}", p.client_id);
}

#[test]
fn the_sign_in_address_carries_pkce_s256_state_the_resource_and_the_redirect() {
    let fake = FakeOtus::start(Config::otus());
    let d = oauth::discover(&fake.mcp_url(), Some(&challenge(&fake))).unwrap();
    let p = oauth::begin(&d, None).unwrap();
    assert!(p.authorize_url.starts_with(&format!("{}/oauth/authorize?", fake.base)), "{}", p.authorize_url);
    let q = query_of(&p.authorize_url);
    assert_eq!(q["response_type"], "code");
    assert_eq!(q["client_id"], p.client_id);
    assert_eq!(q["redirect_uri"], p.redirect_uri);
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(q["code_challenge"].len(), 43, "base64url of a SHA-256: {}", q["code_challenge"]);
    assert!(q["code_challenge"].chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    assert!(q["state"].len() >= 32, "a random state: {}", q["state"]);
    assert_eq!(q["resource"], fake.mcp_url(), "RFC 8707");
    assert!(!q.contains_key("scope"), "the server offers no scopes");
    // The Debug output hides the query (state, challenge).
    assert!(!format!("{p:?}").contains(&q["state"]));

    let other = oauth::begin(&d, None).unwrap();
    let q2 = query_of(&other.authorize_url);
    assert_ne!(q2["state"], q["state"]);
    assert_ne!(q2["code_challenge"], q["code_challenge"]);
    assert_ne!(other.redirect_uri, p.redirect_uri, "each sign-in on its own free port");
}

#[test]
fn each_sign_in_has_its_own_state_and_verifier_of_32_random_bytes_as_43_base64url_characters() {
    // GA-51: the bytes come from the system's random source (getrandom), not /dev/urandom, which Windows lacks.
    let fake = FakeOtus::start(Config::otus());
    let d = oauth::discover(&fake.mcp_url(), Some(&challenge(&fake))).unwrap();
    let b64url_43 = |s: &str| s.len() == 43 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    let states: Vec<String> = (0..8).map(|_| query_of(&oauth::begin(&d, None).unwrap().authorize_url)["state"].clone()).collect();
    for s in &states {
        assert!(b64url_43(s), "a state of 43 base64url characters: {s}");
    }
    let distinct: std::collections::HashSet<&String> = states.iter().collect();
    assert_eq!(distinct.len(), states.len(), "every state differs: {states:?}");
    // No fixed or zeroed bytes: each of the 43 places takes more than one value across the sign-ins.
    for i in 0..43 {
        let seen: std::collections::HashSet<u8> = states.iter().map(|s| s.as_bytes()[i]).collect();
        assert!(seen.len() > 1, "place {i} is the same in every state: {states:?}");
    }

    // A whole sign-in: the PKCE verifier in the token request is 43 base64url characters too, and its S256 the challenge.
    let p = oauth::begin(&d, None).unwrap();
    let url = p.authorize_url.clone();
    let (state, challenge_sent) = (query_of(&url)["state"].clone(), query_of(&url)["code_challenge"].clone());
    let browser = std::thread::spawn(move || http("GET", &fake_otus::authorize(&url), &[], "").0);
    oauth::finish(p, WAIT).unwrap();
    assert_eq!(browser.join().unwrap(), 200);
    let verifier = fake.log().token_requests[0]["code_verifier"].clone();
    assert!(b64url_43(&verifier), "a verifier of 43 base64url characters: {verifier}");
    assert_ne!(verifier, state, "the verifier and the state are drawn apart");
    assert_eq!(b64url(&sha256(verifier.as_bytes())), challenge_sent);
}

#[test]
fn finish_refuses_a_wrong_state_and_saves_the_tokens_of_the_right_answer() {
    let fake = FakeOtus::start(Config::otus());
    let d = oauth::discover(&fake.mcp_url(), Some(&challenge(&fake))).unwrap();
    let p = oauth::begin(&d, None).unwrap();
    let (url, redirect, client_id) = (p.authorize_url.clone(), p.redirect_uri.clone(), p.client_id.clone());
    let challenge_sent = query_of(&url)["code_challenge"].clone();

    let (f, redirect_b) = (fake.clone(), redirect.clone());
    let browser = std::thread::spawn(move || {
        let redirect = redirect_b;
        let back = fake_otus::authorize(&url);
        let code = query_of(&back)["code"].clone();
        // A browser asks for a favicon too: not the answer, so the wait goes on.
        let favicon = http("GET", &redirect.replace("/callback", "/favicon.ico"), &[], "");
        // The right code with a wrong state: refused, and nothing exchanged.
        let wrong = http("GET", &format!("{redirect}?code={code}&state=not-the-state-gizai-sent"), &[], "");
        let exchanged_after_wrong = f.log().token_requests.len();
        let right = http("GET", &back, &[], "");
        (code, favicon.0, wrong, exchanged_after_wrong, right)
    });
    let s = oauth::finish(p, WAIT).unwrap();
    let (code, favicon, wrong, exchanged_after_wrong, right) = browser.join().unwrap();

    assert_eq!(favicon, 400);
    assert_eq!(wrong.0, 400, "a wrong state is refused: {}", wrong.2);
    assert!(wrong.2.contains("isn't for the sign-in Gizai is waiting for"), "{}", wrong.2);
    assert_eq!(exchanged_after_wrong, 0, "no token request for the wrong state's code");
    assert_eq!(right.0, 200);
    assert!(right.2.contains("Gizai is signed in. You can close this tab."), "{}", right.2);

    // The tokens, and they work at the MCP.
    assert!(fake.accepts(&s.access_token));
    assert_eq!(fake_otus::mcp_status(&fake.mcp_url(), &s.access_token), 200);
    let refresh_token = s.refresh_token.clone().expect("a refresh token");
    assert!(fake.refresh_valid(&refresh_token));
    assert_eq!(s.client_id, client_id);
    assert_eq!(s.resource, fake.mcp_url());
    assert_eq!(s.token_endpoint, format!("{}/oauth/token", fake.base));
    assert_eq!(s.revocation_endpoint.as_deref(), Some(format!("{}/oauth/revoke", fake.base).as_str()));
    let left = s.expires_at.unwrap() - now_ms();
    assert!((3_500_000..=3_600_000).contains(&left), "expires_in 3600 s: {left} ms left");
    let shown = format!("{s:?}");
    assert!(!shown.contains(&s.access_token) && !shown.contains(&refresh_token), "no tokens in Debug: {shown}");

    // The token request: the code with its PKCE verifier, redirect, client id and resource.
    let reqs = fake.log().token_requests;
    assert_eq!(reqs.len(), 1);
    let t = &reqs[0];
    assert_eq!(t["grant_type"], "authorization_code");
    assert_eq!(t["code"], code);
    assert_eq!(t["redirect_uri"], redirect);
    assert_eq!(t["client_id"], client_id);
    assert_eq!(t["resource"], fake.mcp_url());
    let verifier = &t["code_verifier"];
    assert!((43..=128).contains(&verifier.len()), "{verifier}");
    assert_eq!(b64url(&sha256(verifier.as_bytes())), challenge_sent, "the verifier's S256 is the challenge sent");

    // One answer: the port is closed after it.
    assert!(!listening(&redirect), "the listener closed after the answer");
}

#[test]
fn finish_gives_up_after_its_timeout_and_closes_the_port() {
    let fake = FakeOtus::start(Config::otus());
    let d = oauth::discover(&fake.mcp_url(), Some(&challenge(&fake))).unwrap();
    let p = oauth::begin(&d, None).unwrap();
    let redirect = p.redirect_uri.clone();
    let e = oauth::finish(p, Duration::from_millis(300)).unwrap_err();
    assert!(e.contains("wasn't finished within"), "{e}");
    assert!(!listening(&redirect));
    assert!(fake.log().token_requests.is_empty());
}

#[test]
fn a_cancelled_sign_in_stops_waiting() {
    let fake = FakeOtus::start(Config::otus());
    let d = oauth::discover(&fake.mcp_url(), Some(&challenge(&fake))).unwrap();
    let p = oauth::begin(&d, None).unwrap();
    let cancel = p.cancel_handle();
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        cancel.cancel();
    });
    let e = oauth::finish(p, WAIT).unwrap_err();
    t.join().unwrap();
    assert_eq!(e, "The sign-in was cancelled");
}

#[test]
fn a_server_without_registration_needs_an_entered_client_id() {
    let fake = FakeOtus::start(Config { registration: false, known_client: Some("otus-entered-id".into()), ..Config::otus() });
    let d = oauth::discover(&fake.mcp_url(), Some(&challenge(&fake))).unwrap();
    assert_eq!(d.registration_endpoint, None);
    assert_eq!(oauth::begin(&d, None).unwrap_err(), oauth::NO_REGISTRATION);
    assert_eq!(oauth::begin(&d, Some("   ")).unwrap_err(), oauth::NO_REGISTRATION, "blank is no client id");

    let p = oauth::begin(&d, Some("  otus-entered-id ")).unwrap();
    assert_eq!(p.client_id, "otus-entered-id");
    assert_eq!(query_of(&p.authorize_url)["client_id"], "otus-entered-id");
    let url = p.authorize_url.clone();
    let browser = std::thread::spawn(move || fake_otus::play_browser(&url));
    let s = oauth::finish(p, WAIT).unwrap();
    assert_eq!(browser.join().unwrap().0, 200);
    assert_eq!(s.client_id, "otus-entered-id");
    assert!(fake.accepts(&s.access_token));
    let log = fake.log();
    assert!(log.registrations.is_empty(), "no registration with an entered client id");
    assert!(!log.requests.iter().any(|r| r.contains("/oauth/register")));
    assert_eq!(log.token_requests[0]["client_id"], "otus-entered-id");
}

#[test]
fn a_server_with_registration_also_takes_an_entered_client_id_without_registering() {
    let fake = FakeOtus::start(Config { known_client: Some("my-own-id".into()), ..Config::otus() });
    let d = oauth::discover(&fake.mcp_url(), Some(&challenge(&fake))).unwrap();
    let p = oauth::begin(&d, Some("my-own-id")).unwrap();
    assert_eq!(p.client_id, "my-own-id");
    assert!(fake.log().registrations.is_empty());
}

#[test]
fn refresh_replaces_the_refresh_token_and_the_old_one_is_refused() {
    let fake = FakeOtus::start(Config::otus());
    let s = saved(&fake, "otus-client-1", 30_000);
    let old_refresh = s.refresh_token.clone().unwrap();

    let s2 = oauth::refresh(&s).unwrap();
    assert_ne!(s2.access_token, s.access_token);
    let new_refresh = s2.refresh_token.clone().unwrap();
    assert_ne!(new_refresh, old_refresh, "the server replaced the refresh token");
    assert!(fake.refresh_valid(&new_refresh) && !fake.refresh_valid(&old_refresh));
    assert!(fake.accepts(&s2.access_token));
    assert!(s2.expires_at.unwrap() - now_ms() > 3_500_000);
    assert_eq!((s2.client_id.as_str(), s2.resource.as_str()), (s.client_id.as_str(), s.resource.as_str()));
    let t = &fake.log().token_requests[0];
    assert_eq!(t["grant_type"], "refresh_token");
    assert_eq!(t["refresh_token"], old_refresh);
    assert_eq!(t["client_id"], "otus-client-1");
    assert_eq!(t["resource"], fake.mcp_url(), "the resource again with the refresh");

    // The old one again: refused, and the message asks to sign in again without the token in it.
    match oauth::refresh(&s) {
        Err(RefreshError::Refused(why)) => {
            assert!(why.contains("sign in again"), "{why}");
            assert!(!why.contains(&old_refresh), "{why}");
        }
        other => panic!("expected a refused refresh, got {other:?}"),
    }
    assert!(oauth::refresh(&s2).is_ok(), "the new one still works");
}

#[test]
fn revoke_withdraws_the_refresh_token_at_the_revocation_endpoint() {
    let fake = FakeOtus::start(Config::otus());
    let s = saved(&fake, "otus-client-1", 3_600_000);
    let refresh_token = s.refresh_token.clone().unwrap();
    assert_eq!(oauth::revoke(&s), Ok(true));
    assert_eq!(fake.log().revoked, vec![(refresh_token.clone(), "refresh_token".to_string())]);
    assert!(!fake.refresh_valid(&refresh_token));

    // No revocation endpoint: nothing to call.
    let quiet = Saved { revocation_endpoint: None, ..saved(&fake, "otus-client-1", 3_600_000) };
    assert_eq!(oauth::revoke(&quiet), Ok(false));
    assert_eq!(fake.log().revoked.len(), 1);
}

#[test]
fn the_token_store_refreshes_when_less_than_min_valid_is_left_and_saves_the_new_refresh_token() {
    let fake = FakeOtus::start(Config::otus());
    let tokens = store();
    let s = saved(&fake, "otus-client-1", 60_000);
    tokens.save("otus", &s).unwrap();

    // A minute left is enough for 30 s.
    assert_eq!(tokens.access_token("otus", Duration::from_secs(30)).unwrap(), s.access_token);
    assert_eq!(fake.refreshes(), 0);

    // Not for 5 minutes: refreshed, and the replaced refresh token is in the keychain when the new access token comes back.
    let new = tokens.access_token("otus", FIVE_MINUTES).unwrap();
    assert_ne!(new, s.access_token);
    assert_eq!(fake.refreshes(), 1);
    let kept = tokens.load("otus").unwrap().unwrap();
    assert_eq!(kept.access_token, new);
    let kept_refresh = kept.refresh_token.clone().unwrap();
    assert_ne!(Some(kept_refresh.clone()), s.refresh_token);
    assert!(fake.refresh_valid(&kept_refresh), "the stored refresh token is the server's current one");
    assert_eq!(fake_otus::mcp_status(&fake.mcp_url(), &new), 200);

    // Now it lasts: no second refresh.
    assert_eq!(tokens.access_token("otus", FIVE_MINUTES).unwrap(), new);
    assert_eq!(fake.refreshes(), 1);
}

#[test]
fn many_callers_at_once_get_one_refresh_and_the_same_working_token() {
    let fake = FakeOtus::start(Config { refresh_delay: Duration::from_millis(400), ..Config::otus() });
    let tokens = store();
    let s = saved(&fake, "otus-client-1", 10_000);
    tokens.save("otus", &s).unwrap();

    let n = 8;
    let gate = Barrier::new(n);
    let got: Mutex<Vec<Result<String, TokenProblem>>> = Mutex::new(vec![]);
    std::thread::scope(|scope| {
        for _ in 0..n {
            scope.spawn(|| {
                gate.wait();
                let t = tokens.access_token("otus", FIVE_MINUTES);
                got.lock().unwrap().push(t);
            });
        }
    });
    let got = got.into_inner().unwrap();
    assert_eq!(fake.refreshes(), 1, "one refresh request reached the server");
    assert_eq!(fake.log().max_refreshes_at_once, 1);
    let first = got[0].clone().unwrap_or_else(|p| panic!("{p}"));
    for t in &got {
        assert_eq!(t.as_ref().unwrap(), &first, "every caller got the same token");
    }
    assert_ne!(first, s.access_token);
    assert_eq!(fake_otus::mcp_status(&fake.mcp_url(), &first), 200);
    let kept = tokens.load("otus").unwrap().unwrap();
    assert!(fake.refresh_valid(kept.refresh_token.as_deref().unwrap()), "the stored refresh token is the new one");
}

#[test]
fn different_servers_refresh_independently() {
    let fake = FakeOtus::start(Config::otus());
    let tokens = store();
    tokens.save("one", &saved(&fake, "otus-client-1", 10_000)).unwrap();
    tokens.save("two", &saved(&fake, "otus-client-2", 10_000)).unwrap();
    let a = tokens.access_token("one", FIVE_MINUTES).unwrap();
    let b = tokens.access_token("two", FIVE_MINUTES).unwrap();
    assert_ne!(a, b);
    assert_eq!(fake.refreshes(), 2);
}

/// A keychain that gives `stale` for the next `n` reads: what a caller saw before another process refreshed.
struct StaleKeychain {
    inner: MemoryKeychain,
    stale: Mutex<Option<(usize, String)>>,
}

impl Keychain for StaleKeychain {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        let mut stale = self.stale.lock().unwrap();
        if let Some((n, v)) = stale.as_mut().filter(|(n, _)| *n > 0) {
            *n -= 1;
            return Ok(Some(v.clone()));
        }
        drop(stale);
        self.inner.get(key)
    }
    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        self.inner.set(key, value)
    }
    fn delete(&self, key: &str) -> Result<(), String> {
        self.inner.delete(key)
    }
}

#[test]
fn a_refused_refresh_reads_the_store_again_before_asking_to_sign_in_again() {
    let fake = FakeOtus::start(Config::otus());
    let k = Arc::new(StaleKeychain { inner: MemoryKeychain::default(), stale: Mutex::new(None) });
    let tokens = TokenStore::new(k.clone());
    let a = saved(&fake, "otus-client-1", 30_000);
    // Another run (another Gizai) refreshed meanwhile, using up a's refresh token, and stored the new sign-in.
    let b = oauth::refresh(&a).unwrap();
    tokens.save("otus", &b).unwrap();
    // This caller still saw `a` on its first two reads (before and under the lock).
    *k.stale.lock().unwrap() = Some((2, serde_json::to_string(&a).unwrap()));

    let got = tokens.access_token("otus", FIVE_MINUTES).unwrap_or_else(|p| panic!("should have used the stored token: {p}"));
    assert_eq!(fake.refreshes(), 2, "the other run's refresh, and this caller's refused one");
    assert_eq!(got, b.access_token, "the token the other run stored");
    assert_eq!(fake_otus::mcp_status(&fake.mcp_url(), &got), 200);
}

#[test]
fn a_refused_refresh_with_nothing_newer_stored_means_sign_in_again() {
    let fake = FakeOtus::start(Config::otus());
    let tokens = store();
    assert_eq!(tokens.access_token("otus", FIVE_MINUTES), Err(TokenProblem::SignedOut));
    tokens.save("otus", &saved(&fake, "otus-client-1", 30_000)).unwrap();
    fake.withdraw_refresh_tokens();
    match tokens.access_token("otus", FIVE_MINUTES) {
        Err(TokenProblem::SignInAgain(why)) => assert!(why.contains("sign in again"), "{why}"),
        other => panic!("expected SignInAgain, got {other:?}"),
    }
}

#[test]
fn renewed_token_takes_the_token_another_caller_just_got_or_refreshes_the_rejected_one() {
    let fake = FakeOtus::start(Config::otus());
    let tokens = store();
    let a = saved(&fake, "otus-client-1", 3_600_000);
    // Another caller already replaced the token the MCP refused: that one is used, no refresh.
    let b = oauth::refresh(&a).unwrap();
    tokens.save("otus", &b).unwrap();
    let before = fake.refreshes();
    assert_eq!(tokens.renewed_token("otus", &a.access_token, FIVE_MINUTES).unwrap(), b.access_token);
    assert_eq!(fake.refreshes(), before, "no refresh: the stored token isn't the rejected one");

    // The stored token itself was rejected, though it looks valid for an hour: refreshed.
    let c = tokens.renewed_token("otus", &b.access_token, FIVE_MINUTES).unwrap();
    assert_ne!(c, b.access_token);
    assert_eq!(fake.refreshes(), before + 1);
    assert_eq!(tokens.load("otus").unwrap().unwrap().access_token, c);
}

#[test]
fn sign_out_revokes_and_forgets_the_tokens() {
    let fake = FakeOtus::start(Config::otus());
    let tokens = store();
    let s = saved(&fake, "otus-client-1", 3_600_000);
    tokens.save("otus", &s).unwrap();
    tokens.sign_out("otus").unwrap();
    assert_eq!(fake.log().revoked, vec![(s.refresh_token.clone().unwrap(), "refresh_token".to_string())]);
    assert!(!fake.refresh_valid(s.refresh_token.as_deref().unwrap()));
    assert_eq!(tokens.load("otus").unwrap(), None);
    assert_eq!(tokens.access_token("otus", FIVE_MINUTES), Err(TokenProblem::SignedOut));

    // A revocation that fails doesn't stop the sign-out.
    let gone = Saved { revocation_endpoint: Some("http://127.0.0.1:1/oauth/revoke".into()), ..saved(&fake, "otus-client-1", 3_600_000) };
    tokens.save("otus", &gone).unwrap();
    tokens.sign_out("otus").unwrap();
    assert_eq!(tokens.load("otus").unwrap(), None);
}
