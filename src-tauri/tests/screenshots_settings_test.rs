//! GA-182: Settings → Agents and runs → Screenshots, as `get_settings` gives them (contract CP) and `save_settings` keeps
//! them: each with its default until set, a value outside its range refused with why and nothing saved, a value left
//! out kept, and the preview ports holding ten ports for each run that may run at once.
use gizai_lib::runs::{self, Settings};

fn shots(s: &Settings) -> [Option<u32>; 6] {
    [s.screenshots_per_run, s.screenshot_max_mb, s.screenshots_keep_sets, s.screenshots_keep_days, s.preview_port_from, s.preview_port_to]
}

#[test]
fn the_screenshot_settings_have_their_defaults_and_the_ui_gets_them() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let s = runs::get_settings(&st);
    assert_eq!(shots(&s), [Some(12), Some(10), Some(3), Some(30), Some(47100), Some(47999)]);
    let json = serde_json::to_value(&s).unwrap();
    for (key, want) in [("screenshotsPerRun", 12), ("screenshotMaxMb", 10), ("screenshotsKeepSets", 3), ("screenshotsKeepDays", 30),
                        ("previewPortFrom", 47100), ("previewPortTo", 47999)] {
        assert_eq!(json[key], serde_json::json!(want), "{key}");
    }
}

#[test]
fn a_screenshot_setting_outside_its_range_is_refused_with_why_and_nothing_is_saved() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let s = runs::get_settings(&st);
    let cases: Vec<(Settings, &str)> = vec![
        (Settings { screenshots_per_run: Some(0), ..s.clone() }, "Screenshots per run must be from 1 to 30."),
        (Settings { screenshots_per_run: Some(31), ..s.clone() }, "Screenshots per run must be from 1 to 30."),
        (Settings { screenshot_max_mb: Some(0), ..s.clone() }, "Largest screenshot must be from 1 to 50 MB."),
        (Settings { screenshot_max_mb: Some(51), ..s.clone() }, "Largest screenshot must be from 1 to 50 MB."),
        (Settings { screenshots_keep_sets: Some(0), ..s.clone() }, "Keep per card must be from 1 to 20 sets."),
        (Settings { screenshots_keep_sets: Some(21), ..s.clone() }, "Keep per card must be from 1 to 20 sets."),
        (Settings { screenshots_keep_days: Some(366), ..s.clone() }, "Delete a finished card's screenshots after must be from 0 to 365 days."),
        (Settings { preview_port_from: Some(1023), ..s.clone() }, "Preview ports must be from 1024 to 65535."),
        (Settings { preview_port_to: Some(65536), ..s.clone() }, "Preview ports must be from 1024 to 65535."),
    ];
    for (bad, why) in cases {
        let err = runs::save_settings(&st, &Settings { max_run_minutes: 99, ..bad }).unwrap_err();
        assert_eq!(err, why);
        let now = runs::get_settings(&st);
        assert_eq!(shots(&now), shots(&s), "nothing was saved for: {why}");
        assert_ne!(now.max_run_minutes, 99, "nor the rest of the form: {why}");
    }
    // the ports the wrong way round, and too few for the runs at once
    let err = runs::save_settings(&st, &Settings { preview_port_from: Some(48000), preview_port_to: Some(47000), ..s.clone() }).unwrap_err();
    assert!(err.starts_with("Preview ports must go from the lower port to the higher one"), "{err}");
    let err = runs::save_settings(&st, &Settings { max_concurrent_runs: 3, preview_port_from: Some(47100), preview_port_to: Some(47119), ..s.clone() })
        .unwrap_err();
    assert!(err.starts_with("Preview ports must hold at least ten ports for each run at once: 47100 to 47119 has room for 2 runs"), "{err}");
    assert_eq!(shots(&runs::get_settings(&st)), shots(&s));
}

#[test]
fn the_edges_of_each_range_are_saved_and_a_value_left_out_keeps_what_was_saved() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let s = runs::get_settings(&st);
    let low = Settings { max_concurrent_runs: 1, screenshots_per_run: Some(1), screenshot_max_mb: Some(1), screenshots_keep_sets: Some(1),
                         screenshots_keep_days: Some(0), preview_port_from: Some(1024), preview_port_to: Some(1033), ..s.clone() };
    runs::save_settings(&st, &low).unwrap();
    assert_eq!(shots(&runs::get_settings(&st)), [Some(1), Some(1), Some(1), Some(0), Some(1024), Some(1033)]);
    let high = Settings { screenshots_per_run: Some(30), screenshot_max_mb: Some(50), screenshots_keep_sets: Some(20),
                          screenshots_keep_days: Some(365), preview_port_from: Some(65526), preview_port_to: Some(65535), ..low.clone() };
    runs::save_settings(&st, &high).unwrap();
    assert_eq!(shots(&runs::get_settings(&st)), [Some(30), Some(50), Some(20), Some(365), Some(65526), Some(65535)]);
    // an older UI that sends none of them keeps what was saved
    let none = Settings { screenshots_per_run: None, screenshot_max_mb: None, screenshots_keep_sets: None, screenshots_keep_days: None,
                          preview_port_from: None, preview_port_to: None, ..runs::get_settings(&st) };
    runs::save_settings(&st, &none).unwrap();
    assert_eq!(shots(&runs::get_settings(&st)), [Some(30), Some(50), Some(20), Some(365), Some(65526), Some(65535)]);
    // and it parses from JSON without them
    let mut json = serde_json::to_value(runs::get_settings(&st)).unwrap();
    for k in ["screenshotsPerRun", "screenshotMaxMb", "screenshotsKeepSets", "screenshotsKeepDays", "previewPortFrom", "previewPortTo"] {
        json.as_object_mut().unwrap().remove(k);
    }
    let parsed: Settings = serde_json::from_value(json).unwrap();
    assert_eq!(shots(&parsed), [None; 6]);
}
