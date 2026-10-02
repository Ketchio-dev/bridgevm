//! Create a snapshot while preserving its logical and selected source media.

use super::*;

#[path = "snapshot_create_destination.rs"]
mod destination;
use destination::prepare_destination;
#[path = "snapshot_create_admission.rs"]
mod admission;
#[path = "snapshot_create_destination_lease.rs"]
mod destination_lease;
#[path = "snapshot_create_staging_debris.rs"]
mod staging_debris;
use destination_lease::claim_staging;
#[path = "snapshot_create_fill.rs"]
mod fill;
#[path = "snapshot_create_manifest_admission.rs"]
mod manifest_admission;
#[path = "snapshot_create_stage.rs"]
mod stage;
use stage::CreateStage;

/// Capture `disk` and `vars` into `dest` as one atomic pair.
///
/// `vm_running` is passed in rather than probed here: only the caller knows
/// whether a helper still holds the media, and a snapshot of a running VM is
/// the failure this whole module exists to prevent.
pub fn create_snapshot(
    disk: &Path,
    vars: &Path,
    dest: &Path,
    vm_id: &str,
    vm_running: bool,
    quota_bytes: u64,
) -> Result<SnapshotManifest, SnapshotError> {
    create_snapshot_using(disk, vars, dest, vm_id, vm_running, quota_bytes, |_| {})
}

fn create_snapshot_using(
    disk: &Path,
    vars: &Path,
    dest: &Path,
    vm_id: &str,
    vm_running: bool,
    quota_bytes: u64,
    observe: impl FnMut(CreateStage),
) -> Result<SnapshotManifest, SnapshotError> {
    if vm_running {
        return Err(SnapshotError::VmRunning);
    }
    create_stopped(disk, vars, dest, vm_id, quota_bytes, copy_and_sync, observe)
}

/// `copy` stages one file; tests substitute a streaming copy that fails.
fn create_stopped(
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

    let manifest = fill::fill_staging([disk, vars], &staging, vm_id, copy, &mut observe)?;

    // Never remove the previous snapshot before its replacement is published.
    admission::publish(&staging, &dest)?;
    observe(CreateStage::SnapshotPublished);
    Ok(manifest)
}

#[cfg(test)]
#[path = "snapshot_create_concurrency_tests.rs"]
mod concurrency_tests;
#[cfg(test)]
#[path = "snapshot_create_failure_tests.rs"]
mod failure_tests;
#[cfg(test)]
#[path = "snapshot_create_interruption_tests.rs"]
mod interruption_tests;
