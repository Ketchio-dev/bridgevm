//! Verify an external source before reclaiming restore debris or admitting space.

use super::LockedPair;
use crate::snapshot_pair::{verify_snapshot, SnapshotError, SnapshotManifest};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

impl LockedPair {
    /// The source must verify and its staging copy must fit.
    pub(super) fn admit_restore(
        &self,
        snapshot: &Path,
        available_bytes: impl FnOnce(&Path) -> Option<u64>,
    ) -> Result<(PathBuf, SnapshotManifest), SnapshotError> {
        let snapshot = fs::canonicalize(snapshot)?;
        if snapshot.starts_with(&self.root) {
            return Err(io::Error::other("restore source must be outside managed storage").into());
        }
        let manifest = verify_snapshot(&snapshot)?;
        self.admit_restore_capacity(&manifest, available_bytes)?;
        Ok((snapshot, manifest))
    }
}

#[path = "managed_pair_restore_capacity.rs"]
mod capacity;
