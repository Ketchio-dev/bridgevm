//! Refuse an unexpected member type or size before hashing its contents.

use crate::snapshot_pair::{regular_file, snapshot_hash::sha256_reader, SnapshotError};
use std::io::Read;
use std::path::Path;

pub(super) fn verify(path: &Path, want_hash: &str, want_bytes: u64) -> Result<(), SnapshotError> {
    let file = regular_file::open(path)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let mismatch = |bytes| {
        SnapshotError::BadManifest(format!(
            "{name} contains {bytes} bytes, manifest declares {want_bytes}"
        ))
    };
    let measured = file.metadata()?.len();
    if measured != want_bytes {
        return Err(mismatch(measured));
    }
    // A growing member gets at most one byte beyond its declared size read;
    // a shrinking one is rejected by the byte count from the same descriptor.
    let (hash, bytes) = sha256_reader(file.take(want_bytes.saturating_add(1)))?;
    if bytes != want_bytes {
        return Err(mismatch(bytes));
    }
    if hash != want_hash {
        return Err(SnapshotError::HashMismatch {
            file: name.into_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "snapshot_verified_member_tests.rs"]
mod tests;
