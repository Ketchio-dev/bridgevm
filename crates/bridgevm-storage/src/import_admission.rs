//! Deleted bundles remain preserved inputs, never hidden imported registrations.

use crate::import_paths::invalid;
use crate::{deletion_metadata_path, StorageError};
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

pub(crate) fn source(input: &Path) -> Result<(), StorageError> {
    if !input.is_dir() || !input.join("manifest.yaml").exists() {
        return Err(StorageError::InvalidImportBundle(input.to_path_buf()));
    }
    no_deletion_marker(input)
}

pub(crate) fn no_deletion_marker(bundle: &Path) -> Result<(), StorageError> {
    // Presence is enough: malformed records must not poison inventory either.
    // Check the owned copy too, before it can become a published registration.
    match fs::symlink_metadata(deletion_metadata_path(bundle)) {
        Ok(_) => Err(invalid(
            "cannot import a bundle with metadata/deletion.json; deletion records are preserved, not automatically cleared",
        )),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
