//! Stage a copied pair within quota before hashing or publishing its manifest.

use super::super::manifest_admission::encode;
use super::super::{quota, stage::CreateStage};
use crate::snapshot_pair::{
    sha256_file, sync_dir, write_file_atomically, SnapshotError, SnapshotManifest, DISK_NAME,
    MANIFEST_NAME, SNAPSHOT_FORMAT_VERSION, VARS_NAME,
};
use std::io;
use std::path::Path;

pub(super) fn fill(
    [disk, vars]: [&Path; 2],
    staging: &Path,
    vm_id: &str,
    quota_bytes: u64,
    mut copy: impl FnMut(&Path, &Path) -> io::Result<u64>,
    observe: &mut impl FnMut(CreateStage),
) -> Result<SnapshotManifest, SnapshotError> {
    let disk_bytes = copy(disk, &staging.join(DISK_NAME))?;
    observe(CreateStage::DiskSynced);
    quota::admit(disk_bytes, 0, quota_bytes)?;
    let vars_bytes = copy(vars, &staging.join(VARS_NAME))?;
    observe(CreateStage::VarsSynced);
    quota::admit(disk_bytes, vars_bytes, quota_bytes)?;

    let manifest = SnapshotManifest {
        format_version: SNAPSHOT_FORMAT_VERSION,
        vm_id: vm_id.to_string(),
        disk_bytes,
        disk_sha256: sha256_file(&staging.join(DISK_NAME))?,
        vars_bytes,
        vars_sha256: sha256_file(&staging.join(VARS_NAME))?,
    };
    // The manifest is written last and is what makes the directory valid.
    write_file_atomically(&staging.join(MANIFEST_NAME), encode(&manifest)?.as_bytes())?;
    observe(CreateStage::ManifestPublished);
    sync_dir(staging)?;
    observe(CreateStage::StagingDirectorySynced);
    Ok(manifest)
}
