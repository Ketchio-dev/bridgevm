//! Finish and own the staged inodes before they can become current media.

use super::*;
use crate::snapshot_pair::copy_and_sync;
use std::{fs::File, path::PathBuf};

impl LockedPair {
    pub(super) fn stage_restore(
        &mut self,
        snapshot: &Path,
        manifest: &SnapshotManifest,
    ) -> Result<(PathBuf, SnapshotManifest), SnapshotError> {
        let staged = self.root.join("staging");
        if super::super::private_directory(&staged, false)? {
            fs::remove_dir_all(&staged)?;
        }
        super::super::private_directory(&staged, true)?;
        for name in ["disk.raw", "vars.fd", "manifest.json"] {
            copy_and_sync(&snapshot.join(name), &staged.join(name))?;
        }
        let copied = verify_snapshot(&staged)?;
        if &copied != manifest {
            return Err(io::Error::other("snapshot changed while staging restore").into());
        }
        File::open(&staged)?.sync_all()?;
        self._lease.extend([
            staged.join("disk.raw").as_path(),
            staged.join("vars.fd").as_path(),
        ])?;
        Ok((staged, copied))
    }
}
