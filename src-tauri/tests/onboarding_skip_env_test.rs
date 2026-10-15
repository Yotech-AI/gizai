// GA-162: GIZAI_SKIP_ONBOARDING=1 starts Gizai as if the first start's onboarding was skipped, for the scripts that
// start it on a new, empty data folder (scripts/smoke-cage.sh, run.sh and the others); 0 or unset shows it. The
// environment belongs to the whole process, so this file has a single test.
#[path = "support/data_lock.rs"]
mod data_lock;
use std::sync::Arc;

use gizai_core::{team, users};
use gizai_lib::onboarding;

#[test]
fn gizai_skip_onboarding_1_starts_as_if_the_onboarding_was_skipped() {
    assert_eq!(onboarding::SKIP_ENV, "GIZAI_SKIP_ONBOARDING");
    let tmp = tempfile::tempdir().unwrap();
    let pending_with = |value: Option<&str>, name: &str| -> bool {
        unsafe {
            match value {
                Some(v) => std::env::set_var(onboarding::SKIP_ENV, v),
                None => std::env::remove_var(onboarding::SKIP_ENV),
            }
        }
        let st = gizai_lib::open_state(tmp.path().join(name), Arc::new(|_| {})).unwrap();
        onboarding::pending(&st)
    };
    assert!(pending_with(None, "unset"), "unset: a new install waits");
    assert!(pending_with(Some("0"), "zero"), "0 shows the onboarding");
    assert!(pending_with(Some(""), "empty"));
    assert!(pending_with(Some("yes"), "yes"), "only 1 skips");
    assert!(!pending_with(Some("1"), "one"), "1 skips it");
    assert!(!pending_with(Some(" 1\n"), "one-spaced"), "trimmed");

    // Skipped at start, a new install is the seed's as it was: you named after your login, My team, five agents.
    data_lock::released_blocking(&tmp.path().join("one-spaced"));
    let st = gizai_lib::open_state(tmp.path().join("one-spaced"), Arc::new(|_| {})).unwrap();
    assert_eq!(users::list(&st.db).unwrap().len(), 1);
    let t = team::get(&st.db, &team::list(&st.db).unwrap()[0].id).unwrap();
    assert_eq!(t.name, "My team");
    assert_eq!(t.members.iter().filter(|m| m.kind == "agent").count(), 5);
    drop(st);

    // A new install that waits, started again with GIZAI_SKIP_ONBOARDING=1: it no longer waits.
    data_lock::released_blocking(&tmp.path().join("unset"));
    assert!(!pending_with(Some("1"), "unset"));
    // And started again without it: it stays skipped.
    data_lock::released_blocking(&tmp.path().join("unset"));
    assert!(!pending_with(None, "unset"));
    unsafe { std::env::remove_var(onboarding::SKIP_ENV) };
}
