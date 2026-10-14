//! Finish and own the staged inodes before they can become current media.

use super::*;

impl LockedPair {
    pub(super) fn stage_restore(
        &mut self,
        snapshot: &Path,
        manifest: &SnapshotManifest,
        copy: impl FnMut(&Path, &Path) -> io::Result<u64>,
    ) -> Result<(ownership::StageOwner, SnapshotManifest), SnapshotError> {
        let staged = self.root.join("staging");
        self.reclaim_staging()?;
        super::super::private_directory(&staged, true)?;
        let mut owned = ownership::StageOwner::capture(&staged)?;
        let copied = owned.fill(snapshot, manifest, copy)?;
        owned.claim_media(&mut self._lease)?;
        Ok((owned, copied))
    }
}

#[path = "managed_pair_staging_debris.rs"]
mod debris;
#[path = "managed_pair_staging_fill.rs"]
mod fill;
#[path = "managed_pair_staging_ownership.rs"]
mod ownership;
#[path = "managed_pair_staging_reclaim.rs"]
mod reclaim;

#[path = "managed_pair_staging_record.rs"]
mod record;
#[path = "managed_pair_staging_recovery.rs"]
mod recovery;
#[path = "managed_pair_staging_seal.rs"]
mod seal;
