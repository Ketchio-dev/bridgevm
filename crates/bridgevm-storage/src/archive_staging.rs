//! Exclusive private staging keeps concurrent archive operations and their cleanup separate.

use crate::{unique_temp_path, TempDirGuard};
use std::fs;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;

pub(crate) fn create(prefix: &str) -> io::Result<TempDirGuard> {
    reserve((0..64).map(|_| unique_temp_path(prefix)))
}

fn reserve(candidates: impl IntoIterator<Item = PathBuf>) -> io::Result<TempDirGuard> {
    for path in candidates {
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        builder.mode(0o700);
        match builder.create(&path) {
            Ok(()) => return Ok(TempDirGuard::new(path)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not reserve a private archive staging directory",
    ))
}

#[cfg(test)]
#[path = "tests_split/archive_staging.rs"]
mod tests;
