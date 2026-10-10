// Backup names use local time (GA-25). One test in its own binary: it sets TZ, which the whole process shares.
// Linux and macOS only: Windows takes its time zone from its own settings, not from TZ.
#![cfg(unix)]
use gizai_core::db::{self, Db};
use gizai_core::{ids, seed};

fn tz(zone: &str) {
    // SAFETY: the only test in this binary, so no other thread reads the environment meanwhile.
    unsafe { std::env::set_var("TZ", zone) };
}

#[test]
fn backup_names_use_local_time_summer_time_included() {
    // Amsterdam's rules, written out so the test needs no time zone files
    tz("CET-1CEST,M3.5.0,M10.5.0/3");
    assert_eq!(db::stamp(1_791_549_302_123), "20261009-143502-123", "12:35:02.123 UTC on 9 October is 14:35 summer time");
    assert_eq!(db::stamp(1_768_519_800_000), "20260116-003000-000", "23:30 UTC on 15 January is past midnight in winter");
    assert_eq!(db::stamp(1_774_745_999_999), "20260329-015959-999", "the last moment of winter time");
    assert_eq!(db::stamp(1_774_746_000_000), "20260329-030000-000", "the clock jumps to 3:00");
    // behind UTC: New York's rules
    tz("EST5EDT,M3.2.0,M11.1.0");
    assert_eq!(db::stamp(1_791_511_200_000), "20261008-220000-000", "02:00 UTC is still the evening before");
    tz("UTC0");
    assert_eq!(db::stamp(1_791_549_302_123), "20261009-123502-123");

    // a snapshot is named on the local clock: here 14 hours ahead of UTC
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    seed::ensure_seed(&Db::open(&path).unwrap(), "Jeffrey").unwrap();
    tz("LINT-14");
    let before = ids::now_ms();
    let snap = db::snapshot(&path, &dir.path().join("backups"), "manual").unwrap();
    let after = ids::now_ms();
    let name = snap.file_name().unwrap().to_string_lossy().into_owned();
    let when = name.strip_prefix("gizai-manual-").and_then(|n| n.strip_suffix(".db")).unwrap_or_else(|| panic!("{name}")).to_string();
    // UTC moved on by 14 hours, worked out without the C library's time zones
    tz("UTC0");
    let (from, to) = (db::stamp(before + 14 * 3_600_000), db::stamp(after + 14 * 3_600_000));
    assert!(from <= when && when <= to, "{name} is not between {from} and {to}");
    assert!(!(db::stamp(before) <= when && when <= db::stamp(after)), "{name} is in UTC");
}
