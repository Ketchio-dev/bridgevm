//! Clear only known restore copies; unfamiliar content is recovery evidence.

use crate::snapshot_pair::managed::private_directory;
use std::io;
use std::path::Path;

/// Runs before capacity admission and each copy, and after a failed one.
/// A crashed copy must not consume the space needed for its own retry.
pub(super) fn clear(staged: &Path) -> io::Result<()> {
    if private_directory(staged, false)? {
        contents::clear(staged)?;
    }
    Ok(())
}
#[path = "managed_pair_staging_contents.rs"]
mod contents;
