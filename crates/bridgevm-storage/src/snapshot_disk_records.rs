//! Read legacy disk records by their JSON shape and rebase retained receipts.

use crate::vm_clone::rebase_snapshot_disk_metadata;
use crate::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum SnapshotDiskRecord {
    Disk(SnapshotDiskMetadata),
    Create(SnapshotDiskCreateMetadata),
}

pub(crate) fn read_records(
    directory: &Path,
) -> Result<Vec<(PathBuf, SnapshotDiskRecord)>, StorageError> {
    let mut records = Vec::new();
    if directory.exists() {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                records.push((path.clone(), read_json_required(&path)?));
            }
        }
    }
    Ok(records)
}

pub(super) fn rebase(source: &Path, output: &Path) -> Result<(), StorageError> {
    let directory = output.join("metadata").join("snapshot-disks");
    for (path, mut record) in read_records(&directory)? {
        match &mut record {
            SnapshotDiskRecord::Disk(metadata) => {
                rebase_snapshot_disk_metadata(metadata, source, output);
            }
            SnapshotDiskRecord::Create(metadata) => {
                rebase_snapshot_disk_metadata(&mut metadata.disk, source, output);
            }
        }
        write_json_pretty_atomic(&path, &record)?;
    }
    let receipts = directory.join("creates");
    if receipts.exists() {
        for entry in fs::read_dir(receipts)? {
            let path = entry?.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let mut receipt: SnapshotDiskCreateMetadata = read_json_required(&path)?;
            rebase_snapshot_disk_metadata(&mut receipt.disk, source, output);
            write_json_pretty_atomic(&path, &receipt)?;
        }
    }
    Ok(())
}
