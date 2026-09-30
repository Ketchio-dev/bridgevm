//! Refuse a restore before managed storage is touched.

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
        let needed = manifest
            .disk_bytes
            .checked_add(manifest.vars_bytes)
            .ok_or_else(|| io::Error::other("snapshot size overflow"))?;
        if let Some(available) = available_bytes(self.root.parent().unwrap()) {
            if needed > available {
                return Err(SnapshotError::InsufficientSpace { needed, available });
            }
        }
        Ok((snapshot, manifest))
    }
}
