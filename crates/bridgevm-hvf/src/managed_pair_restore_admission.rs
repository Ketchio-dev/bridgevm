//! Verify an external source before reclaiming restore debris.

use super::LockedPair;
use crate::snapshot_pair::{verify_snapshot, SnapshotError, SnapshotManifest};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

impl LockedPair {
    /// Verify the source before touching any previous restore's staging.
    pub(super) fn admit_restore(
        &self,
        snapshot: &Path,
    ) -> Result<(PathBuf, SnapshotManifest), SnapshotError> {
        let snapshot = fs::canonicalize(snapshot)?;
        if snapshot.starts_with(&self.root) {
            return Err(io::Error::other("restore source must be outside managed storage").into());
        }
        let manifest = verify_snapshot(&snapshot)?;
        self.reclaim_staging()?;
        Ok((snapshot, manifest))
    }
}
