use crate::{should_skip_bundle_copy_path, StorageError};
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
pub(crate) fn acquire(
    root: &Path,
) -> Result<bridgevm_media::media_lease::MediaLease, StorageError> {
    let mut paths = Vec::new();
    collect(root, &mut paths)?;
    Ok(bridgevm_media::media_lease::MediaLease::acquire(
        paths.iter().map(PathBuf::as_path),
    )?)
}

#[cfg(not(unix))]
pub(crate) fn acquire(_: &Path) -> Result<(), StorageError> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "bundle copying requires cooperative media ownership on this platform",
    )
    .into())
}

fn collect(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), StorageError> {
    if !fs::symlink_metadata(directory)?.file_type().is_dir() {
        return Err(StorageError::UnsupportedBundleEntry(
            directory.to_path_buf(),
        ));
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if should_skip_bundle_copy_path(&path) {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect(&path, paths)?;
        } else if kind.is_file() {
            paths.push(path);
        } else {
            return Err(StorageError::UnsupportedBundleEntry(path));
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
#[path = "bundle_ownership_tests.rs"]
mod tests;
