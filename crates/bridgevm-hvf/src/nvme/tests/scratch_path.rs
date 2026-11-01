//! Per-process NVMe test scratch paths, independent of the host clock's resolution.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

// The clock can repeat on concurrent calls; PID and time still separate crash leftovers across runs.
static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) fn temp_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    path_at(&std::env::temp_dir(), name, nanos)
}

fn path_at(directory: &Path, name: &str, nanos: u128) -> PathBuf {
    let sequence = NEXT
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            next.checked_add(1)
        })
        .expect("NVMe test scratch path sequence exhausted");
    directory.join(format!(
        "bridgevm-hvf-nvme-{name}-{}-{nanos}-{sequence}",
        std::process::id()
    ))
}

#[cfg(test)]
#[path = "scratch_path_tests.rs"]
mod tests;
