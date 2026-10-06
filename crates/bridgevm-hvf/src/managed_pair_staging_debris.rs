//! Managed staging holds only restore's own copies, so it is removed whole.

use crate::snapshot_pair::managed::private_directory;
use std::fs;
use std::io;
use std::path::Path;

/// Runs before capacity admission and each copy, and after a failed one.
/// A crashed copy must not consume the space needed for its own retry.
pub(super) fn clear(staged: &Path) -> io::Result<()> {
    if private_directory(staged, false)? {
        fs::remove_dir_all(staged)?;
    }
    Ok(())
}
