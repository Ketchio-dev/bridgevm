//! Bind capacity metadata to the same bytes used to verify each snapshot hash.

use super::{SnapshotError, SnapshotManifest, DISK_NAME, MANIFEST_NAME, VARS_NAME};
use std::path::Path;

/// Read and verify a snapshot without restoring it.
pub fn verify_snapshot(dir: &Path) -> Result<SnapshotManifest, SnapshotError> {
    let manifest = super::manifest_read::read(&dir.join(MANIFEST_NAME))?;
    for (name, want_hash, want_bytes) in [
        (DISK_NAME, &manifest.disk_sha256, manifest.disk_bytes),
        (VARS_NAME, &manifest.vars_sha256, manifest.vars_bytes),
    ] {
        member::verify(&dir.join(name), want_hash, want_bytes)?;
    }
    Ok(manifest)
}

#[path = "snapshot_verified_member.rs"]
mod member;
