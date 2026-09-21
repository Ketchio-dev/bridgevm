//! Create a snapshot while preserving its logical and selected source media.

use super::*;

#[path = "snapshot_create_destination.rs"]
mod destination;
use destination::prepare_destination;
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
    mut observe: impl FnMut(CreateStage),
) -> Result<SnapshotManifest, SnapshotError> {
    if vm_running {
        return Err(SnapshotError::VmRunning);
    }

    // Refuse before writing anything, not after filling the disk.
    let owner = managed::LockedPair::open(disk, vars)?;
    let (selected_disk, selected_vars) = owner.paths()?;
    let logical_disk = fs::canonicalize(disk)?;
    let logical_vars = fs::canonicalize(vars)?;
    let (disk, vars) = (selected_disk.as_path(), selected_vars.as_path());
    let projected = fs::metadata(disk)?.len() + fs::metadata(vars)?.len();
    if projected > quota_bytes {
        return Err(SnapshotError::QuotaExceeded {
            bytes: projected,
            quota: quota_bytes,
        });
    }

    let dest = prepare_destination(dest, [&logical_disk, &logical_vars, disk, vars])?;
    let staging = staging_path(&dest);
    // Refuse source overlap before cleaning debris from an interrupted attempt.
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;

    let disk_bytes = copy_and_sync(disk, &staging.join(DISK_NAME))?;
    observe(CreateStage::DiskSynced);
    let vars_bytes = copy_and_sync(vars, &staging.join(VARS_NAME))?;
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
    sync_dir(&staging)?;
    observe(CreateStage::StagingDirectorySynced);

    // Never remove the previous snapshot before its replacement is published.
    snapshot_publish::publish(&staging, &dest)?;
    observe(CreateStage::SnapshotPublished);
    Ok(manifest)
}

#[cfg(test)]
#[path = "snapshot_create_interruption_tests.rs"]
mod interruption_tests;
