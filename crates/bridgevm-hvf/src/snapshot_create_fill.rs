//! Fill claimed staging, and clear it again when filling fails.

use super::stage::CreateStage;
use super::staging_debris::clear_staging;
use super::{sha256_file, sync_dir, write_file_atomically, SnapshotError, SnapshotManifest};
use super::{DISK_NAME, MANIFEST_NAME, SNAPSHOT_FORMAT_VERSION, VARS_NAME};
use std::io;
use std::path::Path;

/// Staging was created empty under the destination lease, so a failure leaves
/// only this attempt's partial copy there. Clearing keeps anything else.
pub(super) fn fill_staging(
    sources: [&Path; 2],
    staging: &Path,
    vm_id: &str,
    copy: impl FnMut(&Path, &Path) -> io::Result<u64>,
    observe: &mut impl FnMut(CreateStage),
) -> Result<SnapshotManifest, SnapshotError> {
    let filled = fill(sources, staging, vm_id, copy, observe);
    if filled.is_err() {
        let _ = clear_staging(staging);
    }
    filled
}

fn fill(
    [disk, vars]: [&Path; 2],
    staging: &Path,
    vm_id: &str,
    mut copy: impl FnMut(&Path, &Path) -> io::Result<u64>,
    observe: &mut impl FnMut(CreateStage),
) -> Result<SnapshotManifest, SnapshotError> {
    let disk_bytes = copy(disk, &staging.join(DISK_NAME))?;
    observe(CreateStage::DiskSynced);
    let vars_bytes = copy(vars, &staging.join(VARS_NAME))?;
    observe(CreateStage::VarsSynced);

    let manifest = SnapshotManifest {
        format_version: SNAPSHOT_FORMAT_VERSION,
        vm_id: vm_id.to_string(),
        disk_bytes,
        disk_sha256: sha256_file(&staging.join(DISK_NAME))?,
        vars_bytes,
        vars_sha256: sha256_file(&staging.join(VARS_NAME))?,
    };
    // The manifest is written last and is what makes the directory valid.
    write_file_atomically(&staging.join(MANIFEST_NAME), manifest.to_json().as_bytes())?;
    observe(CreateStage::ManifestPublished);
    sync_dir(staging)?;
    observe(CreateStage::StagingDirectorySynced);
    Ok(manifest)
}
