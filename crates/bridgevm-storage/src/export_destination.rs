//! Occupied output admission and exclusive creation for tar export.

use crate::StorageError;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

pub(crate) fn ensure_absent(output: &Path) -> Result<(), StorageError> {
    match fs::symlink_metadata(output) {
        Ok(_) => Err(StorageError::ExportAlreadyExists(output.to_path_buf())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn create_tar(output: &Path) -> Result<fs::File, StorageError> {
    // Admission can race another ordinary export. Never follow or truncate an
    // entry created before this open, including a dangling symbolic link.
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|error| match error.kind() {
            ErrorKind::AlreadyExists => StorageError::ExportAlreadyExists(output.to_path_buf()),
            _ => error.into(),
        })
}
