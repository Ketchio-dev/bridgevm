//! Fill claimed staging, and clear it again when filling fails.

use super::owned_staging::OwnedStaging;
use super::stage::CreateStage;
use super::{SnapshotError, SnapshotManifest};
use std::io;
use std::path::Path;

impl OwnedStaging {
    /// Staging was created empty under the destination lease, so a failure leaves
    /// only this attempt's partial copy there. Clearing keeps anything else.
    pub(super) fn fill(
        &self,
        sources: [&Path; 2],
        vm_id: &str,
        quota_bytes: u64,
        copy: impl FnMut(&Path, &Path) -> io::Result<u64>,
        observe: &mut impl FnMut(CreateStage),
    ) -> Result<SnapshotManifest, SnapshotError> {
        contents::fill(sources, &self.path, vm_id, quota_bytes, copy, observe)
            .inspect_err(|_| self.discard())
    }
}

#[path = "snapshot_create_contents.rs"]
mod contents;
