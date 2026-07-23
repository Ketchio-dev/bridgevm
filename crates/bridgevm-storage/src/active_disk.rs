//! Which disk is active — primary, snapshot overlay or backing — and the chain view.

use crate::*;
use bridgevm_config::VmManifest;
use std::fs;
use std::path::Path;

impl VmStore {
    pub fn active_disk(&self, name: &str) -> Result<ActiveDiskMetadata, StorageError> {
        let (bundle, manifest) = self.get_vm(name)?;
        self.active_disk_at(&bundle, &manifest)
    }

    pub fn snapshot_chain(&self, name: &str) -> Result<SnapshotChainMetadata, StorageError> {
        let (bundle, manifest) = self.get_vm(name)?;
        let active_disk = self.active_disk_at(&bundle, &manifest)?;
        let mut disks = Vec::new();
        let dir = bundle.join("metadata").join("snapshot-disks");
        if dir.exists() {
            for entry in fs::read_dir(dir)? {
                let path = entry?.path();
                if path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with("-create.json"))
                {
                    continue;
                }
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let mut disk: SnapshotDiskMetadata = read_json_required(&path)?;
                disk.overlay_exists = disk.overlay_path.exists();
                disk.backing_exists = disk.backing_path.exists();
                disks.push(disk);
            }
        }
        disks.sort_by(|a, b| a.snapshot.cmp(&b.snapshot));
        Ok(SnapshotChainMetadata { active_disk, disks })
    }

    pub(crate) fn active_disk_at(
        &self,
        bundle: &Path,
        manifest: &VmManifest,
    ) -> Result<ActiveDiskMetadata, StorageError> {
        let path = bundle.join("metadata").join("active-disk.json");
        if !path.exists() {
            return Ok(self.primary_active_disk_at(bundle, manifest));
        }

        let mut active_disk: ActiveDiskMetadata = read_json_required(&path)?;
        active_disk.exists = active_disk.path.exists();
        Ok(active_disk)
    }

    pub(crate) fn primary_active_disk_at(
        &self,
        bundle: &Path,
        manifest: &VmManifest,
    ) -> ActiveDiskMetadata {
        let path = resolve_bundle_path(bundle, &manifest.storage.primary.path);
        ActiveDiskMetadata {
            source: ActiveDiskSource::Primary,
            snapshot: None,
            exists: path.exists(),
            path,
            format: manifest.storage.primary.format.clone(),
            activated_at_unix: now_unix(),
        }
    }

    pub(crate) fn snapshot_overlay_active_disk_at(
        &self,
        snapshot_name: &str,
        disk: &SnapshotDiskMetadata,
        exists: bool,
    ) -> ActiveDiskMetadata {
        ActiveDiskMetadata {
            source: ActiveDiskSource::SnapshotOverlay,
            snapshot: Some(snapshot_name.to_string()),
            path: disk.overlay_path.clone(),
            format: disk.overlay_format.clone(),
            exists,
            activated_at_unix: now_unix(),
        }
    }

    pub(crate) fn snapshot_backing_active_disk_at(
        &self,
        snapshot_name: &str,
        disk: &SnapshotDiskMetadata,
        exists: bool,
    ) -> ActiveDiskMetadata {
        ActiveDiskMetadata {
            source: ActiveDiskSource::SnapshotBacking,
            snapshot: Some(snapshot_name.to_string()),
            path: disk.backing_path.clone(),
            format: disk.backing_format.clone(),
            exists,
            activated_at_unix: now_unix(),
        }
    }

    pub(crate) fn write_active_disk_at(
        &self,
        bundle: &Path,
        metadata: &ActiveDiskMetadata,
    ) -> Result<(), StorageError> {
        write_json_pretty_atomic(&bundle.join("metadata").join("active-disk.json"), metadata)
    }
}
