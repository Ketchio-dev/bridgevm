//! The declared quota applies to source estimates and actual copied pair sizes.

use super::SnapshotError;
use std::io;

pub(super) fn admit(disk_bytes: u64, vars_bytes: u64, quota: u64) -> Result<(), SnapshotError> {
    let bytes = disk_bytes.checked_add(vars_bytes).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "snapshot byte count overflow")
    })?;
    if bytes > quota {
        return Err(SnapshotError::QuotaExceeded { bytes, quota });
    }
    Ok(())
}

#[cfg(test)]
#[path = "snapshot_create_quota_tests.rs"]
mod tests;
