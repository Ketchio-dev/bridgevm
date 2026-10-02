//! Fill claimed staging, and clear it again when filling fails.

use super::stage::CreateStage;
use super::staging_debris::clear_staging;
use super::{SnapshotError, SnapshotManifest};
use std::io;
use std::path::Path;

/// Staging was created empty under the destination lease, so a failure leaves
/// only this attempt's partial copy there. Clearing keeps anything else.
pub(super) fn fill_staging(
    sources: [&Path; 2],
    staging: &Path,
    vm_id: &str,
    quota_bytes: u64,
    copy: impl FnMut(&Path, &Path) -> io::Result<u64>,
    observe: &mut impl FnMut(CreateStage),
) -> Result<SnapshotManifest, SnapshotError> {
    let filled = contents::fill(sources, staging, vm_id, quota_bytes, copy, observe);
    if filled.is_err() {
        let _ = clear_staging(staging);
    }
    filled
}

#[path = "snapshot_create_contents.rs"]
mod contents;
