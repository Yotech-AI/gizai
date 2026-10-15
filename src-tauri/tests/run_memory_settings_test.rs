//! GA-202: Settings → Runs → Memory for agent runs, on every system: half of the computer's memory until it is set, with
//! the computer's memory next to it for the hint; 2 GB up to the computer's memory is saved and kept after a restart, and
//! anything else is refused with a message. A test's Gizai hasn't started its slice, so the limit isn't enforced there.
#[path = "support/data_lock.rs"]
mod data_lock;

use gizai_agents::run_memory::{self as mem, GB};
use gizai_lib::runs::{self, Settings};

fn with(s: &Settings, gb: Option<u32>) -> Settings {
    Settings { runs_memory_gb: gb, ..s.clone() }
}

#[test]
fn memory_for_agent_runs_is_half_the_computer_until_set_2_gb_up_to_the_computer_is_saved_and_kept_after_a_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let total = mem::total().expect("this system says how much memory it has");
    let computer = ((total + GB / 2) / GB) as u32;
    let half = ((total / 2 + GB / 2) / GB) as u32;
    {
        let st = gizai_lib::test_state(tmp.path());
        let s = runs::get_settings(&st);
        assert_eq!(s.computer_memory_gb, computer, "the hint's 'This computer has … GB'");
        assert_eq!(s.runs_memory_gb, Some(half.max(2)), "half of the computer's memory until it is set");
        assert!(!s.runs_memory_enforced, "a test's Gizai has no slice until it starts one");
        // what the UI gets
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!((&json["runsMemoryGb"], &json["computerMemoryGb"], &json["runsMemoryEnforced"]),
                   (&serde_json::json!(half.max(2)), &serde_json::json!(computer), &serde_json::json!(false)));

        // anything outside 2 GB to the computer's memory is refused with a message, and nothing is saved
        for gb in [0, 1, computer + 1, 10_000] {
            let err = runs::save_settings(&st, &with(&s, Some(gb))).unwrap_err();
            assert_eq!(err, format!("memory for agent runs may be between 2 and {computer} GB: this computer has {computer} GB"), "{gb} GB");
            assert_eq!(runs::get_settings(&st).runs_memory_gb, Some(half.max(2)), "{gb} GB wasn't saved");
        }
        // a refused value doesn't save the rest of the form either
        let err = runs::save_settings(&st, &Settings { max_run_minutes: 99, ..with(&s, Some(1)) }).unwrap_err();
        assert!(err.contains("memory for agent runs"), "{err}");
        assert_ne!(runs::get_settings(&st).max_run_minutes, 99);

        // 2 GB and the computer's memory are the ends of the range
        for gb in [2, computer, 3] {
            runs::save_settings(&st, &with(&s, Some(gb))).unwrap();
            assert_eq!(runs::get_settings(&st).runs_memory_gb, Some(gb));
        }
        // a form without the field (an older page) keeps what was saved
        runs::save_settings(&st, &with(&s, None)).unwrap();
        assert_eq!(runs::get_settings(&st).runs_memory_gb, Some(3));
        let from_ui: Settings = serde_json::from_value({
            let mut v = serde_json::to_value(runs::get_settings(&st)).unwrap();
            v.as_object_mut().unwrap().remove("runsMemoryGb");
            v
        }).unwrap();
        assert_eq!(from_ui.runs_memory_gb, None);
        runs::save_settings(&st, &from_ui).unwrap();
        assert_eq!(runs::get_settings(&st).runs_memory_gb, Some(3));
        // the start check and the activity line use the setting while Gizai hasn't started its slice
        assert_eq!(gizai_lib::run_memory::limit(&st), 3 * GB);
    }
    // a restart keeps it
    data_lock::released_blocking(&data);
    let st = gizai_lib::test_state(tmp.path());
    assert_eq!(runs::get_settings(&st).runs_memory_gb, Some(3));
    assert_eq!(gizai_lib::run_memory::setting_gb(&st), 3);
}

#[test]
fn the_default_follows_the_computer_until_someone_sets_another_value() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let default = gizai_lib::run_memory::default_gb();
    let s = runs::get_settings(&st);
    // saving the form as it is (the default) keeps the default unset, so it keeps following the computer's memory
    runs::save_settings(&st, &s).unwrap();
    assert_eq!(gizai_core::settings::get::<u32>(&st.db, gizai_lib::run_memory::KEY).unwrap(), None);
    assert_eq!(runs::get_settings(&st).runs_memory_gb, Some(default));
    // another value is saved
    let other = if default > 2 { default - 1 } else { default + 1 };
    runs::save_settings(&st, &with(&s, Some(other))).unwrap();
    assert_eq!(gizai_core::settings::get::<u32>(&st.db, gizai_lib::run_memory::KEY).unwrap(), Some(other));
}

#[test]
fn the_check_allows_2_gb_up_to_the_computers_memory() {
    let computer = gizai_lib::run_memory::computer_gb().unwrap();
    assert!(gizai_lib::run_memory::check(2).is_ok());
    assert!(gizai_lib::run_memory::check(computer).is_ok());
    assert!(gizai_lib::run_memory::check(1).is_err());
    assert!(gizai_lib::run_memory::check(0).is_err());
    assert!(gizai_lib::run_memory::check(computer + 1).is_err());
}
