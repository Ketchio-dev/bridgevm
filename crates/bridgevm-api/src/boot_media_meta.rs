//! Split out of lib.rs by responsibility.

use crate::*;

pub(crate) fn push_boot_media_candidate(
    candidates: &mut Vec<(BootMediaKind, PathBuf)>,
    kind: BootMediaKind,
    path: Option<&AppleVzPathSpec>,
) {
    if let Some(path) = path {
        candidates.push((kind, PathBuf::from(&path.path)));
    }
}

pub(crate) fn ensure_boot_media_write_destination_in_bundle(
    bundle: &std::path::Path,
    destination: &std::path::Path,
    kind: BootMediaKind,
) -> Result<(), String> {
    let bundle = normalize_absolute_path(bundle);
    let destination = normalize_absolute_path(destination);
    if destination.starts_with(&bundle) {
        Ok(())
    } else {
        Err(format!(
            "boot media {kind} destination {} is outside VM bundle {}",
            destination.display(),
            bundle.display()
        ))
    }
}

pub(crate) fn normalize_absolute_path(path: &std::path::Path) -> PathBuf {
    use std::path::Component;

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

pub(crate) fn push_boot_media_status_entry(
    entries: &mut Vec<BootMediaStatusEntry>,
    bundle: &std::path::Path,
    kind: BootMediaKind,
    path: Option<&AppleVzPathSpec>,
) -> Result<(), String> {
    let Some(path) = path else {
        return Ok(());
    };
    let path = PathBuf::from(&path.path);
    let file_metadata = fs::metadata(&path).ok();
    let exists = file_metadata
        .as_ref()
        .is_some_and(std::fs::Metadata::is_file);
    let bytes = file_metadata
        .filter(std::fs::Metadata::is_file)
        .map(|metadata| metadata.len());
    entries.push(BootMediaStatusEntry {
        kind,
        path,
        exists,
        bytes,
        last_import: read_boot_media_import_metadata(bundle, kind)?,
        last_verification: read_boot_media_verification_metadata(bundle, kind)?,
        last_download_plan: read_boot_media_download_plan_metadata(bundle, kind)?,
        last_download: read_boot_media_download_result_metadata(bundle, kind)?,
    });
    Ok(())
}

pub(crate) fn write_boot_media_import_metadata(
    bundle: &std::path::Path,
    metadata: &BootMediaImportMetadata,
) -> Result<(), String> {
    let path = boot_media_import_metadata_path(bundle, metadata.kind);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create boot media metadata directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::write(
        &path,
        serde_json::to_string_pretty(metadata).map_err(|error| error.to_string())?,
    )
    .map_err(|error| {
        format!(
            "failed to write boot media metadata {}: {error}",
            path.display()
        )
    })
}

pub(crate) fn read_boot_media_import_metadata(
    bundle: &std::path::Path,
    kind: BootMediaKind,
) -> Result<Option<BootMediaImportMetadata>, String> {
    let path = boot_media_import_metadata_path(bundle, kind);
    if !path.exists() {
        return Ok(None);
    }
    read_boot_media_metadata_json(&path, "boot media metadata").map(Some)
}

pub(crate) fn boot_media_import_metadata_path(
    bundle: &std::path::Path,
    kind: BootMediaKind,
) -> PathBuf {
    bundle
        .join("metadata")
        .join("boot-media")
        .join(format!("{kind}.json"))
}

pub(crate) fn write_boot_media_verification_metadata(
    bundle: &std::path::Path,
    metadata: &BootMediaVerificationMetadata,
) -> Result<(), String> {
    let path = boot_media_verification_metadata_path(bundle, metadata.kind);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create boot media metadata directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::write(
        &path,
        serde_json::to_string_pretty(metadata).map_err(|error| error.to_string())?,
    )
    .map_err(|error| {
        format!(
            "failed to write boot media verification metadata {}: {error}",
            path.display()
        )
    })
}

pub(crate) fn read_boot_media_verification_metadata(
    bundle: &std::path::Path,
    kind: BootMediaKind,
) -> Result<Option<BootMediaVerificationMetadata>, String> {
    let path = boot_media_verification_metadata_path(bundle, kind);
    if !path.exists() {
        return Ok(None);
    }
    read_boot_media_metadata_json(&path, "boot media verification metadata").map(Some)
}

pub(crate) fn boot_media_verification_metadata_path(
    bundle: &std::path::Path,
    kind: BootMediaKind,
) -> PathBuf {
    bundle
        .join("metadata")
        .join("boot-media")
        .join(format!("{kind}-verify.json"))
}

pub(crate) fn write_boot_media_download_plan_metadata(
    bundle: &std::path::Path,
    metadata: &BootMediaDownloadPlanMetadata,
) -> Result<(), String> {
    let path = boot_media_download_plan_metadata_path(bundle, metadata.kind);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create boot media metadata directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::write(
        &path,
        serde_json::to_string_pretty(metadata).map_err(|error| error.to_string())?,
    )
    .map_err(|error| {
        format!(
            "failed to write boot media download plan metadata {}: {error}",
            path.display()
        )
    })
}

pub(crate) fn read_boot_media_download_plan_metadata(
    bundle: &std::path::Path,
    kind: BootMediaKind,
) -> Result<Option<BootMediaDownloadPlanMetadata>, String> {
    let path = boot_media_download_plan_metadata_path(bundle, kind);
    if !path.exists() {
        return Ok(None);
    }
    read_boot_media_metadata_json(&path, "boot media download plan metadata").map(Some)
}

pub(crate) fn boot_media_download_plan_metadata_path(
    bundle: &std::path::Path,
    kind: BootMediaKind,
) -> PathBuf {
    bundle
        .join("metadata")
        .join("boot-media")
        .join(format!("{kind}-download.json"))
}

pub(crate) fn write_boot_media_download_result_metadata(
    bundle: &std::path::Path,
    metadata: &BootMediaDownloadResultMetadata,
) -> Result<(), String> {
    let path = boot_media_download_result_metadata_path(bundle, metadata.kind);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create boot media metadata directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::write(
        &path,
        serde_json::to_string_pretty(metadata).map_err(|error| error.to_string())?,
    )
    .map_err(|error| {
        format!(
            "failed to write boot media download result metadata {}: {error}",
            path.display()
        )
    })
}

pub(crate) fn read_boot_media_download_result_metadata(
    bundle: &std::path::Path,
    kind: BootMediaKind,
) -> Result<Option<BootMediaDownloadResultMetadata>, String> {
    let path = boot_media_download_result_metadata_path(bundle, kind);
    if !path.exists() {
        return Ok(None);
    }
    read_boot_media_metadata_json(&path, "boot media download result metadata").map(Some)
}

pub(crate) fn read_boot_media_metadata_json<T: DeserializeOwned>(
    path: &Path,
    label: &str,
) -> Result<T, String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| {
            file.take(MAX_BOOT_MEDIA_METADATA_BYTES + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|error| format!("failed to read {label} {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_BOOT_MEDIA_METADATA_BYTES {
        return Err(format!(
            "{label} {} exceeds the {MAX_BOOT_MEDIA_METADATA_BYTES}-byte limit",
            path.display()
        ));
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid {label} {}: {error}", path.display()))
}

pub(crate) fn boot_media_download_result_metadata_path(
    bundle: &std::path::Path,
    kind: BootMediaKind,
) -> PathBuf {
    bundle
        .join("metadata")
        .join("boot-media")
        .join(format!("{kind}-download-result.json"))
}

pub(crate) fn boot_media_download_temp_path(destination: &std::path::Path) -> PathBuf {
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("boot-media");
    destination.with_file_name(format!(".{file_name}.download"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_media_metadata_reader_rejects_oversized_json_before_decode() {
        let root = std::env::temp_dir().join(format!(
            "bridgevm-api-oversized-boot-media-metadata-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = boot_media_import_metadata_path(&root, BootMediaKind::InstallerImage);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            vec![b'x'; MAX_BOOT_MEDIA_METADATA_BYTES as usize + 1],
        )
        .unwrap();

        let error = read_boot_media_import_metadata(&root, BootMediaKind::InstallerImage)
            .expect_err("oversized metadata must be rejected");
        assert!(error.contains("exceeds the 1048576-byte limit"));
        assert!(error.contains(&path.display().to_string()));

        let _ = fs::remove_dir_all(root);
    }
}
