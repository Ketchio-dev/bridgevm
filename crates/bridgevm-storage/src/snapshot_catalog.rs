//! Snapshot list, creation, restore and the last-restore receipt.

use crate::*;
use std::fs;
use std::path::Path;

impl VmStore {
    pub fn create_snapshot(
        &self,
        vm_name: &str,
        snapshot_name: &str,
        kind: SnapshotKind,
    ) -> Result<SnapshotMetadata, StorageError> {
        let (bundle, manifest) = self.get_vm(vm_name)?;
        let _lock = MetadataLock::acquire(&bundle, "snapshots.lock")?;
        let state = self.state_at(&bundle)?.state;
        let mut snapshots = self.snapshots(vm_name)?;
        if snapshots
            .iter()
            .any(|snapshot| snapshot.name == snapshot_name)
        {
            return Err(StorageError::SnapshotAlreadyExists {
                vm: vm_name.to_string(),
                snapshot: snapshot_name.to_string(),
            });
        }
        let snapshot = SnapshotMetadata {
            name: snapshot_name.to_string(),
            kind,
            created_at_unix: now_unix(),
            vm_state: state,
        };
        snapshots.push(snapshot.clone());
        snapshots.sort_by(|a, b| a.name.cmp(&b.name));
        self.write_snapshots_at(&bundle, &snapshots)?;
        match kind {
            SnapshotKind::Disk => {
                self.prepare_snapshot_disk_at(&bundle, &manifest, snapshot_name)?;
            }
            SnapshotKind::Suspend => {
                self.prepare_snapshot_suspend_image_at(&bundle, snapshot_name)?;
            }
            SnapshotKind::ApplicationConsistent => {
                self.prepare_application_consistent_snapshot_preflight_at(&bundle, snapshot_name)?;
            }
        }
        Ok(snapshot)
    }

    pub fn snapshots(&self, vm_name: &str) -> Result<Vec<SnapshotMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = bundle.join("metadata").join("snapshots.json");
        if !path.exists() {
            return Ok(Vec::new());
        }
        read_json_required(&path)
    }

    pub fn restore_snapshot(
        &self,
        vm_name: &str,
        snapshot_name: &str,
    ) -> Result<SnapshotRestoreMetadata, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let snapshot = self
            .snapshots(vm_name)?
            .into_iter()
            .find(|snapshot| snapshot.name == snapshot_name)
            .ok_or_else(|| StorageError::SnapshotNotFound {
                vm: vm_name.to_string(),
                snapshot: snapshot_name.to_string(),
            })?;
        let active_disk = if snapshot.kind == SnapshotKind::Disk {
            let disk = self
                .snapshot_disk_metadata(vm_name, snapshot_name)?
                .ok_or_else(|| StorageError::SnapshotDiskMetadataNotFound {
                    vm: vm_name.to_string(),
                    snapshot: snapshot_name.to_string(),
                })?;
            if !disk.backing_path.exists() {
                return Err(StorageError::SnapshotDiskBackingMissing(disk.backing_path));
            }
            let active_disk = self.snapshot_backing_active_disk_at(snapshot_name, &disk, true);
            self.write_active_disk_at(&bundle, &active_disk)?;
            Some(active_disk)
        } else {
            None
        };
        let suspend_image = if snapshot.kind == SnapshotKind::Suspend {
            let mut suspend_image = self
                .snapshot_suspend_image_metadata(vm_name, snapshot_name)?
                .ok_or_else(|| StorageError::SnapshotSuspendImageMetadataNotFound {
                    vm: vm_name.to_string(),
                    snapshot: snapshot_name.to_string(),
                })?;
            suspend_image.image_exists = suspend_image.image_path.exists();
            self.write_snapshot_suspend_image_metadata_at(&bundle, snapshot_name, &suspend_image)?;
            if !suspend_image.image_exists {
                return Err(StorageError::SnapshotSuspendImageMissing(
                    suspend_image.image_path,
                ));
            }
            Some(suspend_image)
        } else {
            None
        };
        let restore = SnapshotRestoreMetadata {
            snapshot: snapshot.name,
            restored_at_unix: now_unix(),
            restored_state: snapshot.vm_state,
            active_disk,
            suspend_image,
        };
        self.write_state_at(&bundle, restore.restored_state)?;
        let dir = bundle.join("metadata");
        fs::create_dir_all(&dir)?;
        fs::write(
            dir.join("last-restore.json"),
            serde_json::to_string_pretty(&restore)?,
        )?;
        Ok(restore)
    }

    pub fn last_restore(
        &self,
        vm_name: &str,
    ) -> Result<Option<SnapshotRestoreMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = bundle.join("metadata").join("last-restore.json");
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(read_json_required(&path)?))
    }

    pub(crate) fn write_snapshots_at(
        &self,
        bundle: &Path,
        snapshots: &[SnapshotMetadata],
    ) -> Result<(), StorageError> {
        let dir = bundle.join("metadata");
        fs::create_dir_all(&dir)?;
        write_json_pretty_atomic(&dir.join("snapshots.json"), snapshots)?;
        Ok(())
    }
}
