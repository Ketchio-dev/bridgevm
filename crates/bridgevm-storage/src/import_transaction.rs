//! Validate and relocate an exclusive copy, then atomically publish it.

use crate::import_paths::{invalid, ImportPaths};
use crate::*;
use bridgevm_config::{slug, VmManifest};
use std::fs;
use std::io::Read;
use std::path::Path;

fn same_contents(source: &Path, copied: &Path) -> Result<(), StorageError> {
    let mut source = fs::File::open(source)?;
    let mut copied = fs::File::open(copied)?;
    let mut left = [0; 65536];
    let mut right = [0; 65536];
    loop {
        let a = source.read(&mut left)?;
        let b = copied.read(&mut right)?;
        if a != b || left[..a] != right[..b] {
            return Err(invalid("bundle copy content changed during import"));
        }
        if a == 0 {
            return Ok(());
        }
    }
}

pub(crate) fn import(
    store: &VmStore,
    input: &Path,
    metadata_source: &Path,
    name: Option<&str>,
) -> Result<VmImportMetadata, StorageError> {
    if !input.is_dir() || !input.join("manifest.yaml").exists() {
        return Err(StorageError::InvalidImportBundle(input.to_path_buf()));
    }
    let _ownership = crate::bundle_ownership::acquire(input)?;
    let mut manifest = VmManifest::read(&input.join("manifest.yaml"))?;
    let original_name = manifest.name.clone();
    if let Some(name) = name {
        manifest.name = name.to_string();
        manifest.network.hostname = format!("{}.bridgevm.local", slug(name));
    }
    let output = store.bundle_path(&manifest.name);
    let input_resolved = fs::canonicalize(input)?;
    let output_resolved = resolve_path_for_new(&output)?;
    let store_resolved = fs::canonicalize(store.root())?;
    if is_same_or_descendant(&output_resolved, &input_resolved)
        || is_same_or_descendant(&input_resolved, &output_resolved)
        || is_same_or_descendant(&input_resolved, &store_resolved)
    {
        return Err(StorageError::ImportPathConflict {
            input: input.to_path_buf(),
            output,
        });
    }
    if fs::symlink_metadata(&output).is_ok() {
        return Err(StorageError::AlreadyExists(manifest.name));
    }
    let guard = crate::import_staging::Staging::create(&store.vms_dir())?;
    let staging = guard.bundle.clone();
    let summary = copy_dir_owned(input, &staging)?;
    let modes = guard.writable()?;
    for relative in &summary.files {
        same_contents(&input.join(relative), &staging.join(relative))?;
    }
    let paths = ImportPaths::new(input, &staging, &output_resolved)?;
    let disks = crate::import_metadata::relocate(&paths, &mut manifest, &original_name)?;
    crate::import_qcow::relocate(&paths, disks)?;
    manifest.write(&staging.join("manifest.yaml"))?;
    let metadata = VmImportMetadata {
        vm: manifest.name.clone(),
        original_name,
        requested_name: name.map(str::to_string),
        source: metadata_source.to_path_buf(),
        output: output.clone(),
        archive_format: if is_tar_path(metadata_source) {
            "tar"
        } else {
            "directory"
        }
        .to_string(),
        copied_file_count: summary.file_count,
        copied_files: summary.files,
        manifest_preserved: summary.manifest_preserved,
        metadata_preserved: summary.metadata_preserved,
        manifest_identity_rewritten: name.is_some(),
        imported_at_unix: now_unix(),
    };
    write_json_pretty_atomic(&staging.join("metadata/import.json"), &metadata)?;
    crate::import_staging::Staging::restore(modes)?;
    crate::bundle_directory_permissions::copy_mode(input, &staging)?;
    crate::import_publish::publish(&staging, &output).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            StorageError::AlreadyExists(manifest.name)
        } else {
            StorageError::Io(error)
        }
    })?;
    Ok(metadata)
}
