//! Disk-only import: reuse selected pair capture, or own an unmanaged RAW copy.
use super::{copy, managed, runtime_exclusion, SnapshotError};
use crate::media_lease::MediaLease;
use std::{fs, path::Path};

/// A managed disk brings its selected vars; only an unmanaged disk needs a seed.
/// Returns true when vars.fd was captured with the disk.
pub fn import_disk(disk: &Path, destination: &Path) -> Result<bool, SnapshotError> {
    let disk = fs::canonicalize(disk)?;
    if let Some(vars) = managed::vars_for_disk(&disk)? {
        super::create_snapshot(&disk, &vars, destination, "import", false, u64::MAX)?;
        return Ok(true);
    }
    let _writer = runtime_exclusion::writer(&disk, "powered-off snapshot create")?;
    let _lease = MediaLease::acquire([disk.as_path()])?;
    // Check again while owning the disk; a restore may have published during discovery.
    if managed::vars_for_disk(&disk)?.is_some() {
        return Err(std::io::Error::other("media binding changed; retry import").into());
    }
    fs::create_dir(destination)?;
    let result = copy::copy_and_sync(&disk, &destination.join("disk.raw"));
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result?;
    Ok(false)
}

#[cfg(test)]
#[path = "snapshot_import_disk_tests.rs"]
mod tests;
