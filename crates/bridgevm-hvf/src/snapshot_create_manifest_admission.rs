//! Creation may publish only metadata that the shared manifest reader accepts.

use super::{SnapshotError, SnapshotManifest, SNAPSHOT_FORMAT_VERSION};
use crate::snapshot_pair::manifest_read::MANIFEST_LIMIT;
use std::{fs, io, path::Path};

pub(super) fn admit(
    [disk, vars]: [&Path; 2],
    vm_id: &str,
    quota: u64,
) -> Result<(), SnapshotError> {
    let disk_bytes = fs::metadata(disk)?.len();
    let vars_bytes = fs::metadata(vars)?.len();
    super::quota::admit(disk_bytes, vars_bytes, quota)?;
    // Creation always writes two 64-character SHA-256 digests. Their contents
    // cannot change the encoded size, so no file copy or hash is needed here.
    encode(&SnapshotManifest {
        format_version: SNAPSHOT_FORMAT_VERSION,
        vm_id: vm_id.to_string(),
        disk_bytes,
        disk_sha256: "0".repeat(64),
        vars_bytes,
        vars_sha256: "0".repeat(64),
    })?;
    Ok(())
}

pub(super) fn encode(manifest: &SnapshotManifest) -> Result<String, SnapshotError> {
    let text = manifest.to_json();
    if text.len() as u64 > MANIFEST_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "snapshot manifest exceeds the 64 KiB size limit",
        )
        .into());
    }
    Ok(text)
}

#[cfg(test)]
#[path = "snapshot_create_manifest_tests.rs"]
mod tests;
