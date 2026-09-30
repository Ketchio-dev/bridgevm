//! Managed staging holds only restore's own copies, so it is removed whole.

use crate::snapshot_pair::managed::private_directory;
use std::fs;
use std::io;
use std::path::Path;

/// Runs before each copy and after a failed one: the next restore measures
/// free space before it clears staging, so a partial copy must not wait.
pub(super) fn clear(staged: &Path) -> io::Result<()> {
    if private_directory(staged, false)? {
        fs::remove_dir_all(staged)?;
    }
    Ok(())
}
