//! Copy a verified snapshot into private staging and prove the copy.

use crate::snapshot_pair::{verify_snapshot, SnapshotError, SnapshotManifest};
use std::fs::File;
use std::io;
use std::path::Path;

pub(super) fn fill(
    snapshot: &Path,
    manifest: &SnapshotManifest,
    staged: &Path,
    mut copy: impl FnMut(&Path, &Path) -> io::Result<u64>,
) -> Result<SnapshotManifest, SnapshotError> {
    for name in ["disk.raw", "vars.fd", "manifest.json"] {
        copy(&snapshot.join(name), &staged.join(name))?;
    }
    let copied = verify_snapshot(staged)?;
    if &copied != manifest {
        return Err(io::Error::other("snapshot changed while staging restore").into());
    }
    File::open(staged)?.sync_all()?;
    Ok(copied)
}
