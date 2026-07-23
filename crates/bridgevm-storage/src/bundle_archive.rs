//! Tar export and extract with archive-path safety and format sniffing.

use crate::*;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub(crate) fn export_bundle_tar(
    source: &Path,
    output: &Path,
    metadata: &VmExportMetadata,
) -> Result<(), StorageError> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let staging = unique_temp_path("bridgevm-export-tar");
    let _staging_guard = TempDirGuard::new(staging.clone());
    copy_dir_all(source, &staging)?;
    let metadata_dir = staging.join("metadata");
    fs::create_dir_all(&metadata_dir)?;
    fs::write(
        metadata_dir.join("export.json"),
        serde_json::to_string_pretty(metadata)?,
    )?;

    let file = fs::File::create(output)?;
    let mut builder = tar::Builder::new(file);
    builder.append_dir_all(".", &staging)?;
    builder.finish()?;
    Ok(())
}

pub(crate) fn extract_bundle_tar(input: &Path, output: &Path) -> Result<(), StorageError> {
    fs::create_dir_all(output)?;
    let file = fs::File::open(input)?;
    let mut archive = tar::Archive::new(file);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let raw_path = entry.path()?.into_owned();
        let Some(relative_path) = safe_archive_path(&raw_path) else {
            return Err(StorageError::UnsafeArchiveEntry(raw_path));
        };
        if relative_path.as_os_str().is_empty() {
            continue;
        }
        let destination = output.join(&relative_path);
        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() {
            fs::create_dir_all(&destination)?;
        } else if entry_type.is_file() {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            entry.unpack(&destination)?;
        } else {
            return Err(StorageError::UnsupportedBundleEntry(raw_path));
        }
    }
    Ok(())
}

pub(crate) fn safe_archive_path(path: &Path) -> Option<PathBuf> {
    let mut safe = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(name) => safe.push(name),
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => return None,
        }
    }
    Some(safe)
}

pub(crate) fn is_tar_path(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("tar")
}

pub(crate) fn is_unsupported_archive_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("zip" | "tgz" | "gz")
    ) || name.ends_with(".tar.gz")
}
