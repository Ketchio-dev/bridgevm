//! Primary and active disk preparation metadata, and size parsing.

use crate::*;
use bridgevm_config::VmManifest;
use bridgevm_qemu::QemuImgCommand;
use std::fs;
use std::path::Path;

pub(crate) fn primary_disk_preparation_metadata(
    bundle: &Path,
    manifest: &VmManifest,
) -> DiskPreparationMetadata {
    let path = resolve_bundle_path(bundle, &manifest.storage.primary.path);
    let format = manifest.storage.primary.format.clone();
    let size = manifest.storage.primary.size.clone();
    let exists = path.exists();
    let create_command = if !exists && format != "raw" {
        Some(QemuImgCommand::create_disk(&path, format.clone(), size.clone()).render_shell_words())
    } else {
        None
    };
    DiskPreparationMetadata {
        path,
        format,
        size: size.clone(),
        size_bytes: parse_size_bytes(&size),
        exists,
        created: false,
        create_command,
        prepared_at_unix: now_unix(),
    }
}

pub(crate) fn parse_size_bytes(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    let units = [
        ("GiB", 1024_u64.pow(3)),
        ("G", 1024_u64.pow(3)),
        ("MiB", 1024_u64.pow(2)),
        ("M", 1024_u64.pow(2)),
        ("KiB", 1024),
        ("K", 1024),
        ("B", 1),
    ];
    for (suffix, multiplier) in units {
        if let Some(number) = trimmed.strip_suffix(suffix) {
            // checked_mul: a huge value would otherwise panic (debug) or wrap
            // (release) into a wrong set_len size. Overflow -> None.
            return number
                .trim()
                .parse::<u64>()
                .ok()
                .and_then(|n| n.checked_mul(multiplier));
        }
    }
    trimmed.parse::<u64>().ok()
}

impl VmStore {
    pub fn prepare_primary_disk(
        &self,
        name: &str,
    ) -> Result<DiskPreparationMetadata, StorageError> {
        let (bundle, manifest) = self.get_vm(name)?;
        let path = resolve_bundle_path(&bundle, &manifest.storage.primary.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let format = manifest.storage.primary.format.clone();
        let size = manifest.storage.primary.size.clone();
        let size_bytes = parse_size_bytes(&size);
        let mut exists = path.exists();
        let mut created = false;
        let mut create_command = None;

        if !exists {
            if format == "raw" {
                let file = fs::File::create(&path)?;
                if let Some(bytes) = size_bytes {
                    file.set_len(bytes)?;
                }
                exists = true;
                created = true;
            } else {
                create_command = Some(
                    QemuImgCommand::create_disk(&path, format.clone(), size.clone())
                        .render_shell_words(),
                );
            }
        }

        let metadata = DiskPreparationMetadata {
            path,
            format,
            size,
            size_bytes,
            exists,
            created,
            create_command,
            prepared_at_unix: now_unix(),
        };
        let metadata_dir = bundle.join("metadata");
        fs::create_dir_all(&metadata_dir)?;
        fs::write(
            metadata_dir.join("primary-disk.json"),
            serde_json::to_string_pretty(&metadata)?,
        )?;
        Ok(metadata)
    }

    pub fn prepare_active_disk(
        &self,
        name: &str,
    ) -> Result<(DiskPreparationMetadata, ActiveDiskMetadata), StorageError> {
        let (bundle, manifest) = self.get_vm(name)?;
        let active_disk = self.active_disk_at(&bundle, &manifest)?;
        if active_disk.source == ActiveDiskSource::Primary {
            let disk = self.prepare_primary_disk(name)?;
            let active_disk = ActiveDiskMetadata {
                exists: disk.exists,
                path: disk.path.clone(),
                format: disk.format.clone(),
                ..active_disk
            };
            self.write_active_disk_at(&bundle, &active_disk)?;
            return Ok((disk, active_disk));
        }

        let disk = DiskPreparationMetadata {
            path: active_disk.path.clone(),
            format: active_disk.format.clone(),
            size: manifest.storage.primary.size,
            size_bytes: None,
            exists: active_disk.path.exists(),
            created: false,
            create_command: None,
            prepared_at_unix: now_unix(),
        };
        let active_disk = ActiveDiskMetadata {
            exists: disk.exists,
            ..active_disk
        };
        self.write_active_disk_at(&bundle, &active_disk)?;
        Ok((disk, active_disk))
    }
}
