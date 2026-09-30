//! Stage a verified pair, then publish its complete managed generation.

use super::{init, layout, LockedPair};
use crate::snapshot_pair::{copy_and_sync, SnapshotError, SnapshotManifest};
use std::{io, path::Path};

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
        self.restore_copying(snapshot, available_bytes, copy_and_sync, publish)
    }

    /// `copy` stages one file; tests substitute a streaming copy that stops.
    pub(in crate::snapshot_pair) fn restore_copying(
        &mut self,
        snapshot: &Path,
        available_bytes: impl FnOnce(&Path) -> Option<u64>,
        copy: impl FnMut(&Path, &Path) -> io::Result<u64>,
        publish: impl FnOnce(&Path, &Path) -> io::Result<()>,
    ) -> Result<SnapshotManifest, SnapshotError> {
        let (snapshot, manifest) = self.admit_restore(snapshot, available_bytes)?;
        init::initialize(&self.root, |_| {})?;
        let (staged, copied) = self.stage_restore(&snapshot, &manifest, copy)?;
        publish(&staged, &self.root.join("current"))?;
        self.own_selected()?;
        layout::acknowledge(&self.root)?;
        Ok(copied)
    }
}

#[path = "managed_pair_restore_admission.rs"]
mod admission;
#[path = "managed_pair_staging.rs"]
mod staging;
