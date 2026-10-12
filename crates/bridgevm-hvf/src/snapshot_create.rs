//! Create a snapshot while preserving its logical and selected source media.

use super::*;

#[path = "snapshot_create_destination.rs"]
mod destination;
use destination::prepare_destination;
#[path = "snapshot_create_admission.rs"]
mod admission;
#[path = "snapshot_create_destination_lease.rs"]
mod destination_lease;
#[path = "snapshot_create_member_lease.rs"]
mod member_lease;
#[path = "snapshot_create_staging_debris.rs"]
mod staging_debris;
#[path = "snapshot_create_staging_reclaim.rs"]
mod staging_reclaim;
use destination_lease::claim_staging;
#[path = "snapshot_create_fill.rs"]
mod fill;
#[path = "snapshot_create_manifest_admission.rs"]
mod manifest_admission;
#[path = "snapshot_create_stage.rs"]
mod stage;
use stage::CreateStage;
#[path = "snapshot_create_stopped.rs"]
mod stopped;
use stopped::create_stopped;
#[path = "snapshot_create_quota.rs"]
mod quota;

/// Capture `disk` and `vars` into `dest` as one atomic pair.
///
/// `vm_running` is the caller's knowledge; an owned runtime's writer leases
/// are checked too, because between guest generations nothing else shows that
/// it owns the media. A snapshot of a running VM is what this module prevents.
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

#[cfg(test)]
#[path = "snapshot_create_concurrency_tests.rs"]
mod concurrency_tests;
#[cfg(test)]
#[path = "snapshot_create_failure_tests.rs"]
mod failure_tests;
#[cfg(test)]
#[path = "snapshot_create_interruption_tests.rs"]
mod interruption_tests;
