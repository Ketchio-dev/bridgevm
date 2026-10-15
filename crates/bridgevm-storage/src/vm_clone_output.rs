//! Reserve a clone destination before any copy or failure cleanup can touch it.

use crate::{copy_dir_all, StorageError};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::DirBuilderExt;
use std::path::Path;

pub(super) fn copy_new(source: &Path, output: &Path, name: &str) -> Result<(), StorageError> {
    // Nonrecursive creation claims an absent root. In particular, a dangling
    // symlink is occupied even though Path::exists() would report false.
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(output).map_err(|error| match error.kind() {
        std::io::ErrorKind::AlreadyExists => StorageError::AlreadyExists(name.to_owned()),
        _ => error.into(),
    })?;

    // Only this attempt's reserved directory reaches failure cleanup. The copy
    // still acquires source media ownership and preserves bundle permissions.
    if let Err(error) = copy_dir_all(source, output) {
        let _ = fs::remove_dir_all(output);
        return Err(error);
    }
    Ok(())
}
