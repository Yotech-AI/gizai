//! Opening the same data again (GA-82). A test that drops its Gizai and opens the same data folder again first waits
//! until the data-folder lock (`<data>/gizai.lock`, see `lock_data_dir` in src/lib.rs) is free: something the old state
//! started (the queue's pull after a run ends) can still hold a clone of it, and with it the lock, for a moment.
#![allow(dead_code)]
use std::path::Path;
use std::time::{Duration, Instant};

/// At most this long; then the test opens the data anyway, and `test_state` says Gizai is already running.
const MAX_WAIT: Duration = Duration::from_secs(10);
const STEP: Duration = Duration::from_millis(20);

/// Whether no Gizai holds the data in `data`: takes the lock the way Gizai does, and lets it go at once.
fn free(data: &Path) -> bool {
    let Ok(f) = std::fs::OpenOptions::new().write(true).open(data.join("gizai.lock")) else { return true };
    f.try_lock().is_ok()
}

/// Waits until the data in `data` is free. Async, so the test's runtime finishes what still holds the old state.
pub async fn released(data: &Path) {
    let start = Instant::now();
    while !free(data) && start.elapsed() < MAX_WAIT {
        tokio::time::sleep(STEP).await;
    }
}

/// `released`, for a test without a Tokio runtime.
pub fn released_blocking(data: &Path) {
    let start = Instant::now();
    while !free(data) && start.elapsed() < MAX_WAIT {
        std::thread::sleep(STEP);
    }
}
