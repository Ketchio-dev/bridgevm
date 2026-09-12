use crate::bundle_directory_permissions::{self as permissions, DirectoryModes};
use crate::*;
use std::fs;
use std::path::Path;

pub(crate) fn extract_bundle_tar(input: &Path, output: &Path) -> Result<(), StorageError> {
    permissions::create(output)?;
    let file = fs::File::open(input)?;
    let mut archive = tar::Archive::new(file);
    let mut modes = DirectoryModes::default();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let raw_path = entry.path()?.into_owned();
        let Some(relative_path) = safe_archive_path(&raw_path) else {
            return Err(StorageError::UnsafeArchiveEntry(raw_path));
        };
        let entry_type = entry.header().entry_type();
        if relative_path.as_os_str().is_empty() {
            if entry_type.is_dir() {
                modes.record(output.to_path_buf(), entry.header().mode()?)?;
            }
            continue;
        }
        let destination = output.join(&relative_path);
        if entry_type.is_dir() {
            modes.record(destination, entry.header().mode()?)?;
        } else if entry_type.is_file() {
            if let Some(parent) = destination.parent() {
                permissions::create(parent)?;
            }
            entry.unpack(&destination)?;
        } else {
            return Err(StorageError::UnsupportedBundleEntry(raw_path));
        }
    }
    modes.apply()?;
    Ok(())
}
