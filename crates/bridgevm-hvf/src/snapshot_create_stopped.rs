//! Create from the selected stopped pair under both source and destination ownership.

use super::*;

/// `copy` stages one file; tests substitute a streaming copy that fails.
pub(super) fn create_stopped(
    disk: &Path,
    vars: &Path,
    dest: &Path,
    vm_id: &str,
    quota_bytes: u64,
    copy: impl FnMut(&Path, &Path) -> io::Result<u64>,
    mut observe: impl FnMut(CreateStage),
) -> Result<SnapshotManifest, SnapshotError> {
    // Refuse before writing anything, not after filling the disk.
    let owner = managed::LockedPair::open(disk, vars)?;
    let (selected_disk, selected_vars) = owner.paths()?;
    let logical_disk = fs::canonicalize(disk)?;
    let logical_vars = fs::canonicalize(vars)?;
    let (disk, vars) = (selected_disk.as_path(), selected_vars.as_path());
    manifest_admission::admit([disk, vars], vm_id, quota_bytes)?;

    let dest = prepare_destination(dest, [&logical_disk, &logical_vars, disk, vars])?;
    let (_destination_lease, staging) = claim_staging(&dest)?;
    let manifest = fill::fill_staging(
        [disk, vars],
        &staging,
        vm_id,
        quota_bytes,
        copy,
        &mut observe,
    )?;
    // Never remove the previous snapshot before its replacement is published.
    admission::publish(&staging, &dest)?;
    observe(CreateStage::SnapshotPublished);
    Ok(manifest)
}
