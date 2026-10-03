//! Relocate typed operative metadata; retain historical execution evidence.

use crate::import_paths::{invalid, ImportPaths};
use crate::vm_clone::snapshot_records::{read_records, SnapshotDiskRecord};
use crate::*;
use bridgevm_config::VmManifest;
use bridgevm_qemu::QemuImgCommand;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(crate) struct DiskFiles {
    pub(crate) formats: BTreeMap<PathBuf, String>,
    pub(crate) backings: BTreeMap<PathBuf, PathBuf>,
}

fn register(files: &mut DiskFiles, path: PathBuf, format: &str) -> Result<(), StorageError> {
    if let Some(previous) = files.formats.insert(path, format.to_string()) {
        if previous != format {
            return Err(invalid("inconsistent imported disk formats"));
        }
    }
    Ok(())
}

fn disk(
    metadata: &mut SnapshotDiskMetadata,
    paths: &ImportPaths,
    files: &mut DiskFiles,
) -> Result<(), StorageError> {
    let overlay = paths.relative(&metadata.overlay_path)?;
    let backing = paths.relative(&metadata.backing_path)?;
    if let Some(previous) = files.backings.insert(overlay.clone(), backing.clone()) {
        if previous != backing {
            return Err(invalid("inconsistent imported snapshot backing identity"));
        }
    }
    register(files, overlay, &metadata.overlay_format)?;
    register(files, backing, &metadata.backing_format)?;
    let overlay_exists = paths.exists(&metadata.overlay_path)?;
    let backing_exists = paths.exists(&metadata.backing_path)?;
    if (metadata.overlay_exists && !overlay_exists) || (metadata.backing_exists && !backing_exists)
    {
        return Err(invalid(
            "recorded snapshot media is missing from copied bundle",
        ));
    }
    metadata.overlay_exists = overlay_exists;
    metadata.backing_exists = backing_exists;
    if metadata.overlay_exists && !metadata.backing_exists {
        return Err(invalid("imported snapshot overlay has no copied backing"));
    }
    metadata.overlay_path = paths.relocate(&metadata.overlay_path)?;
    metadata.backing_path = paths.relocate(&metadata.backing_path)?;
    metadata.create_command = QemuImgCommand::create_backed_disk(
        &metadata.overlay_path,
        metadata.overlay_format.clone(),
        metadata.backing_format.clone(),
        &metadata.backing_path,
    )
    .render_shell_words();
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum SuspendRecord {
    Snapshot(SnapshotSuspendImageMetadata),
    Fast(FastSuspendImageMetadata),
}

pub(crate) fn relocate(
    paths: &ImportPaths,
    manifest: &mut VmManifest,
    original_name: &str,
) -> Result<DiskFiles, StorageError> {
    let mut files = DiskFiles::default();
    let primary = paths.relative(Path::new(&manifest.storage.primary.path))?;
    register(
        &mut files,
        primary.clone(),
        &manifest.storage.primary.format,
    )?;
    manifest.storage.primary.path = primary.to_string_lossy().into_owned();
    if let Some(boot) = &mut manifest.boot {
        for value in [
            &mut boot.installer_image,
            &mut boot.kernel_path,
            &mut boot.initrd_path,
            &mut boot.macos_restore_image,
        ]
        .into_iter()
        .flatten()
        {
            *value = paths
                .relative(Path::new(value))?
                .to_string_lossy()
                .into_owned();
        }
    }
    let active_path = paths.staging.join("metadata/active-disk.json");
    if let Some(mut active) = read_json_file::<ActiveDiskMetadata>(&active_path)? {
        register(&mut files, paths.relative(&active.path)?, &active.format)?;
        let recorded_exists = active.exists;
        active.exists = paths.exists(&active.path)?;
        if (recorded_exists || active.source != ActiveDiskSource::Primary) && !active.exists {
            return Err(invalid("selected imported snapshot disk is not copied"));
        }
        active.path = paths.relocate(&active.path)?;
        write_json_pretty_atomic(&active_path, &active)?;
    }
    let directory = paths.staging.join("metadata/snapshot-disks");
    for (path, mut record) in read_records(&directory)? {
        match &mut record {
            SnapshotDiskRecord::Disk(value) => disk(value, paths, &mut files)?,
            SnapshotDiskRecord::Create(value) => disk(&mut value.disk, paths, &mut files)?,
        }
        write_json_pretty_atomic(&path, &record)?;
    }
    let creates = directory.join("creates");
    if creates.exists() {
        for entry in fs::read_dir(creates)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) == Some("json") {
                let mut receipt: SnapshotDiskCreateMetadata = read_json_required(&path)?;
                disk(&mut receipt.disk, paths, &mut files)?;
                write_json_pretty_atomic(&path, &receipt)?;
            }
        }
    }
    let suspend = paths.staging.join("metadata/suspend-images");
    if suspend.exists() {
        let entries = fs::read_dir(suspend)?.collect::<Result<Vec<_>, _>>()?;
        for entry in entries {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let mut record: SuspendRecord = read_json_required(&path)?;
            let (image_path, exists) = match &mut record {
                SuspendRecord::Snapshot(value) => (&mut value.image_path, &mut value.image_exists),
                SuspendRecord::Fast(value) => (&mut value.image_path, &mut value.image_exists),
            };
            let copied_exists = paths.exists(image_path)?;
            if *exists && !copied_exists {
                return Err(invalid(
                    "recorded suspend image is missing from copied bundle",
                ));
            }
            *exists = copied_exists;
            *image_path = paths.relocate(image_path)?;
            let destination = if let SuspendRecord::Fast(value) = &mut record {
                if value.vm != original_name {
                    return Err(invalid(
                        "Fast saved-state VM identity disagrees with imported manifest",
                    ));
                }
                value.vm = manifest.name.clone();
                fast_suspend_image_metadata_path(&paths.staging, &manifest.name)
            } else {
                path.clone()
            };
            if destination != path && destination.exists() {
                return Err(invalid(
                    "renamed Fast saved-state metadata destination is occupied",
                ));
            }
            write_json_pretty_atomic(&destination, &record)?;
            if destination != path {
                fs::remove_file(path)?;
            }
        }
    }
    Ok(files)
}
