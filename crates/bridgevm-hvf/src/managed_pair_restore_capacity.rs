//! Capacity is sampled after reclaiming stale copies under the pair's lease.

use super::*;

impl LockedPair {
    pub(super) fn admit_restore_capacity(
        &self,
        manifest: &SnapshotManifest,
        available_bytes: impl FnOnce(&Path) -> Option<u64>,
    ) -> Result<(), SnapshotError> {
        let needed = manifest
            .disk_bytes
            .checked_add(manifest.vars_bytes)
            .ok_or_else(|| io::Error::other("snapshot size overflow"))?;
        self.reclaim_staging()?;
        if let Some(available) = available_bytes(self.root.parent().unwrap()) {
            if needed > available {
                return Err(SnapshotError::InsufficientSpace { needed, available });
            }
        }
        Ok(())
    }
}
