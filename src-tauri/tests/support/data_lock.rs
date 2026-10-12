//! Opening the same data again (GA-82, GA-125). A test that drops its Gizai and opens the same data folder again first
//! waits until the data-folder lock (`<data>/gizai.lock`, see `lock_data_dir` in src/lib.rs) is free. Dropping the state
//! closes the lock, but two things can keep it for a moment:
//! - something the old state started (the queue's pull after a run ends) can still hold a clone of it;
//! - a program another test in the same test binary is starting: the new process gets a copy of every open file of the
//!   test process, the lock included, and only lets go of it once the program runs (close-on-exec closes it then). The
//!   lock stays taken until the last copy closes. The tests run side by side and start git, shells and fake CLIs all the
//!   time, and macOS takes long enough over it that a test opening its data again at once now and then found it taken
//!   ("Gizai is already running with the data in …").
#![allow(dead_code)]
use std::path::Path;
use std::time::{Duration, Instant};

/// At most this long; a lock still taken by then is a real failure (the old state was never let go of).
const MAX_WAIT: Duration = Duration::from_secs(10);
const STEP: Duration = Duration::from_millis(20);

/// Whether no Gizai holds the data in `data`: takes the lock the way Gizai does, and lets it go at once.
fn free(data: &Path) -> bool {
    let Ok(f) = std::fs::OpenOptions::new().write(true).open(data.join("gizai.lock")) else { return true };
    f.try_lock().is_ok()
}

fn still_taken(data: &Path) -> ! {
    panic!("the data in {} is still locked {MAX_WAIT:?} after the test dropped its Gizai: something still holds the old \
            state (a clone of it in a task or thread), or a process keeps its gizai.lock open", data.display())
}

/// Waits until the data in `data` is free, and fails the test when it isn't within `MAX_WAIT`. Async, so the test's
/// runtime finishes what still holds the old state.
pub async fn released(data: &Path) {
    let start = Instant::now();
    while !free(data) {
        if start.elapsed() >= MAX_WAIT {
            still_taken(data);
        }
        tokio::time::sleep(STEP).await;
    }
}

/// `released`, for a test without a Tokio runtime.
pub fn released_blocking(data: &Path) {
    let start = Instant::now();
    while !free(data) {
        if start.elapsed() >= MAX_WAIT {
            still_taken(data);
        }
        std::thread::sleep(STEP);
    }
}
