//! Stage a verified pair, then publish its complete managed generation.

use super::{layout, LockedPair};
use crate::snapshot_pair::{verify_snapshot, SnapshotError, SnapshotManifest};
use std::{fs, io, path::Path};

impl LockedPair {
    pub(super) fn restore_using(
        &mut self,
        snapshot: &Path,
        publish: impl FnOnce(&Path, &Path) -> io::Result<()>,
    ) -> Result<SnapshotManifest, SnapshotError> {
        self.restore_with_capacity(snapshot, super::super::free_space::available_bytes, publish)
    }

    pub(in crate::snapshot_pair) fn restore_with_capacity(
        &mut self,
        snapshot: &Path,
        available_bytes: impl FnOnce(&Path) -> Option<u64>,
        publish: impl FnOnce(&Path, &Path) -> io::Result<()>,
    ) -> Result<SnapshotManifest, SnapshotError> {
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
        layout::initialize(&self.root)?;
        let (staged, copied) = self.stage_restore(&snapshot, &manifest)?;
        publish(&staged, &self.root.join("current"))?;
        self.own_selected()?;
        layout::acknowledge(&self.root)?;
        Ok(copied)
    }
}

#[path = "managed_pair_staging.rs"]
mod staging;
