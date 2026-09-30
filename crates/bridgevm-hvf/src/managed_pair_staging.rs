//! Finish and own the staged inodes before they can become current media.

use super::*;
use std::path::PathBuf;

impl LockedPair {
    pub(super) fn stage_restore(
        &mut self,
        snapshot: &Path,
        manifest: &SnapshotManifest,
        copy: impl FnMut(&Path, &Path) -> io::Result<u64>,
    ) -> Result<(PathBuf, SnapshotManifest), SnapshotError> {
        let staged = self.root.join("staging");
        debris::clear(&staged)?;
        super::super::private_directory(&staged, true)?;
        let copied = fill::fill(snapshot, manifest, &staged, copy)
            .inspect_err(|_| drop(debris::clear(&staged)))?;
        self._lease.extend([
            staged.join("disk.raw").as_path(),
            staged.join("vars.fd").as_path(),
        ])?;
        Ok((staged, copied))
    }
}

#[path = "managed_pair_staging_debris.rs"]
mod debris;
#[path = "managed_pair_staging_fill.rs"]
mod fill;
