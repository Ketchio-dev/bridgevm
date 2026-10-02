//! Bind capacity metadata to the same bytes used to verify each snapshot hash.

use super::{
    sha256_file_and_size, SnapshotError, SnapshotManifest, DISK_NAME, MANIFEST_NAME, VARS_NAME,
};
use std::path::Path;

/// Read and verify a snapshot without restoring it.
pub fn verify_snapshot(dir: &Path) -> Result<SnapshotManifest, SnapshotError> {
    let manifest = super::manifest_read::read(&dir.join(MANIFEST_NAME))?;
    for (name, want_hash, want_bytes) in [
        (DISK_NAME, &manifest.disk_sha256, manifest.disk_bytes),
        (VARS_NAME, &manifest.vars_sha256, manifest.vars_bytes),
    ] {
        let (hash, bytes) = sha256_file_and_size(&dir.join(name))?;
        if &hash != want_hash {
            return Err(SnapshotError::HashMismatch {
                file: name.to_string(),
            });
        }
        if bytes != want_bytes {
            return Err(SnapshotError::BadManifest(format!(
                "{name} contains {bytes} bytes, manifest declares {want_bytes}"
            )));
        }
    }
    Ok(manifest)
}
