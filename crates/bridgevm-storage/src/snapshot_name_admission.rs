//! Name admission and exact identity when reading slug-keyed snapshot metadata.

use crate::*;
use bridgevm_config::slug;
use serde::de::DeserializeOwned;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub(super) fn validate(name: &str) -> Result<String, StorageError> {
    let normalized = slug(name);
    if normalized.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "snapshot name must contain at least one ASCII letter or digit",
        )
        .into());
    }
    Ok(normalized)
}

pub(super) fn admit(
    vm: &str,
    name: &str,
    normalized: &str,
    snapshots: &[SnapshotMetadata],
) -> Result<(), StorageError> {
    if snapshots
        .iter()
        .any(|snapshot| slug(&snapshot.name) == normalized)
    {
        return Err(StorageError::SnapshotAlreadyExists {
            vm: vm.to_string(),
            snapshot: name.to_string(),
        });
    }
    Ok(())
}

fn read_named_metadata<T: DeserializeOwned>(
    path: &Path,
    name: &str,
    identity: impl Fn(&T) -> &str,
) -> Result<Option<T>, StorageError> {
    let metadata = read_json_file(path)?;
    Ok(metadata.filter(|metadata| identity(metadata) == name))
}

pub(crate) fn snapshot_disk_create_metadata_path(bundle: &Path, name: &str) -> PathBuf {
    bundle
        .join("metadata/snapshot-disks/creates")
        .join(format!("{}.json", slug(name)))
}

#[path = "snapshot_named_metadata.rs"]
mod metadata;

pub(super) fn admit_metadata_path(
    bundle: &Path,
    name: &str,
    kind: SnapshotKind,
) -> Result<(), StorageError> {
    let path = match kind {
        SnapshotKind::Disk => snapshot_disk_metadata_path(bundle, name),
        SnapshotKind::Suspend => snapshot_suspend_image_metadata_path(bundle, name),
        SnapshotKind::ApplicationConsistent => {
            application_consistent_snapshot_preflight_path(bundle, name)
        }
    };
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "snapshot metadata path is already occupied: {}",
                path.display()
            ),
        )
        .into()),
    }
}
