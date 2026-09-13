//! Pure candidate names and atomic reservation of fresh test fixture directories.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_RESERVATION_ATTEMPTS: usize = 1024;

/// Unique within this process, but deliberately does not create a directory.
/// PID reuse can revisit an old path; writers must reserve before using it.
pub(crate) fn unique_temp_dir(prefix: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("{prefix}-{}-{sequence}", std::process::id()))
}

pub(crate) fn fresh_temp_dir(prefix: &str) -> PathBuf {
    reserve_with(|| unique_temp_dir(prefix), MAX_RESERVATION_ATTEMPTS)
        .expect("could not reserve a fresh test fixture directory")
}

fn reserve_with(mut candidate: impl FnMut() -> PathBuf, attempts: usize) -> io::Result<PathBuf> {
    for _ in 0..attempts {
        let path = candidate();
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "fresh test fixture reservation exhausted its collision bound",
    ))
}

#[cfg(test)]
#[path = "temp_fixtures_tests.rs"]
mod tests;
