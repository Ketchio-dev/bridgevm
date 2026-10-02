//! Read bounded snapshot metadata without following a link or blocking on a FIFO.

use super::{SnapshotError, SnapshotManifest};
use std::fs::OpenOptions;
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// Shared by restore verification and destructive replacement admission.
pub(super) const MANIFEST_LIMIT: u64 = 64 * 1024;

pub(super) fn read(path: &Path) -> Result<SnapshotManifest, SnapshotError> {
    let text = read_text(path)
        .map_err(|error| SnapshotError::BadManifest(format!("cannot read manifest: {error}")))?;
    SnapshotManifest::from_json(&text)
}

fn read_text(path: &Path) -> io::Result<String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::other("manifest must be a regular file"));
    }
    if metadata.len() > MANIFEST_LIMIT {
        return Err(io::Error::other("manifest exceeds the 64 KiB size limit"));
    }
    let mut text = String::new();
    file.take(MANIFEST_LIMIT + 1).read_to_string(&mut text)?;
    if text.len() as u64 > MANIFEST_LIMIT {
        return Err(io::Error::other("manifest exceeds the 64 KiB size limit"));
    }
    Ok(text)
}

#[cfg(test)]
#[path = "snapshot_manifest_read_tests.rs"]
mod tests;
