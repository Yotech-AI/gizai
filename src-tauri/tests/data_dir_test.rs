//! GA-51: Gizai's data lives in each system's own place, and GIZAI_DATA_DIR still overrides it. One test in its own
//! binary, as it changes the environment.
use std::path::PathBuf;

use gizai_lib::{data_dir, default_data_dir};

fn set(name: &str, value: &str) {
    // SAFETY: the only test in this binary, so no other thread reads the environment meanwhile.
    unsafe { std::env::set_var(name, value) };
}

fn unset(name: &str) {
    // SAFETY: as in `set`.
    unsafe { std::env::remove_var(name) };
}

#[test]
fn the_data_folder_is_each_systems_own_and_gizai_data_dir_overrides_it() {
    #[cfg(target_os = "linux")]
    {
        set("XDG_DATA_HOME", "/srv/me/data");
        assert_eq!(default_data_dir(), PathBuf::from("/srv/me/data/gizai"));
        unset("XDG_DATA_HOME");
        set("HOME", "/home/me");
        assert_eq!(default_data_dir(), PathBuf::from("/home/me/.local/share/gizai"), "unchanged on Linux");
    }
    #[cfg(target_os = "macos")]
    {
        set("HOME", "/Users/me");
        set("XDG_DATA_HOME", "/Users/me/.xdg");
        assert_eq!(default_data_dir(), PathBuf::from("/Users/me/Library/Application Support/Gizai"));
    }
    #[cfg(windows)]
    {
        set("APPDATA", r"C:\Users\me\AppData\Roaming");
        assert_eq!(default_data_dir(), PathBuf::from(r"C:\Users\me\AppData\Roaming\Gizai"));
        // without APPDATA: the same place in your profile folder (Windows has no HOME)
        unset("APPDATA");
        set("USERPROFILE", r"C:\Users\other");
        assert_eq!(default_data_dir(), PathBuf::from(r"C:\Users\other\AppData\Roaming\Gizai"));
    }
    unset("GIZAI_DATA_DIR");
    assert_eq!(data_dir(), default_data_dir());
    let own = std::env::temp_dir().join("gizai-own-data");
    set("GIZAI_DATA_DIR", &own.display().to_string());
    assert_eq!(data_dir(), own, "GIZAI_DATA_DIR overrides it on every system");
}
