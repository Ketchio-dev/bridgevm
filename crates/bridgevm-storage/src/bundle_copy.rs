//! Recursive bundle copy and enumeration with unsupported-entry rejection.

use crate::*;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BundleCopySummary {
    pub(crate) file_count: u64,
    pub(crate) files: Vec<String>,
    pub(crate) manifest_preserved: bool,
    pub(crate) metadata_preserved: bool,
}

pub(crate) fn summarize_bundle_copy(from: &Path) -> Result<BundleCopySummary, StorageError> {
    let mut files = Vec::new();
    collect_regular_files(from, from, &mut files)?;
    files.sort();
    Ok(BundleCopySummary {
        file_count: files.len() as u64,
        manifest_preserved: files.iter().any(|path| path == "manifest.yaml"),
        metadata_preserved: files.iter().any(|path| path.starts_with("metadata/")),
        files,
    })
}

pub(crate) fn collect_regular_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<String>,
) -> Result<(), StorageError> {
    let metadata = fs::symlink_metadata(current)?;
    if !metadata.file_type().is_dir() {
        return Err(StorageError::UnsupportedBundleEntry(current.to_path_buf()));
    }
    let mut entries = fs::read_dir(current)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if should_skip_bundle_copy_path(&path) {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_regular_files(root, &path, files)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| StorageError::UnsupportedBundleEntry(path.clone()))?;
            files.push(relative.to_string_lossy().replace('\\', "/"));
        } else {
            return Err(StorageError::UnsupportedBundleEntry(path));
        }
    }
    Ok(())
}

pub(crate) fn copy_dir_all(from: &Path, to: &Path) -> Result<BundleCopySummary, StorageError> {
    let metadata = fs::symlink_metadata(from)?;
    if !metadata.file_type().is_dir() {
        return Err(StorageError::UnsupportedBundleEntry(from.to_path_buf()));
    }
    fs::create_dir_all(to)?;
    let mut copied_files = Vec::new();
    copy_dir_all_inner(from, from, to, &mut copied_files)?;
    copied_files.sort();
    Ok(BundleCopySummary {
        file_count: copied_files.len() as u64,
        manifest_preserved: copied_files.iter().any(|path| path == "manifest.yaml"),
        metadata_preserved: copied_files
            .iter()
            .any(|path| path.starts_with("metadata/")),
        files: copied_files,
    })
}

pub(crate) fn copy_dir_all_inner(
    root: &Path,
    from: &Path,
    to: &Path,
    copied_files: &mut Vec<String>,
) -> Result<(), StorageError> {
    let mut entries = fs::read_dir(from)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let from_path = entry.path();
        if should_skip_bundle_copy_path(&from_path) {
            continue;
        }
        let to_path = to.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            fs::create_dir_all(&to_path)?;
            copy_dir_all_inner(root, &from_path, &to_path, copied_files)?;
        } else if file_type.is_file() {
            fs::copy(&from_path, &to_path)?;
            let relative = from_path
                .strip_prefix(root)
                .map_err(|_| StorageError::UnsupportedBundleEntry(from_path.clone()))?;
            copied_files.push(relative.to_string_lossy().replace('\\', "/"));
        } else {
            return Err(StorageError::UnsupportedBundleEntry(from_path));
        }
    }
    Ok(())
}

pub(crate) fn should_skip_bundle_copy_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".sock") || name.ends_with(".lock"))
}
