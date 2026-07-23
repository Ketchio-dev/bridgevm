//! Disk-snapshot overlay preparation and creation.

use crate::*;
use bridgevm_config::slug;
use bridgevm_config::VmManifest;
use bridgevm_qemu::QemuImgCommand;
use std::fs;
use std::path::Path;
use std::process::Output;

impl VmStore {
    pub fn snapshot_disk_metadata(
        &self,
        vm_name: &str,
        snapshot_name: &str,
    ) -> Result<Option<SnapshotDiskMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = snapshot_disk_metadata_path(&bundle, snapshot_name);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(read_json_required(&path)?))
    }

    pub fn create_snapshot_disk(
        &self,
        vm_name: &str,
        snapshot_name: &str,
    ) -> Result<SnapshotDiskCreateMetadata, StorageError> {
        self.create_snapshot_disk_with(vm_name, snapshot_name, run_command)
    }

    pub(crate) fn create_snapshot_disk_with<F>(
        &self,
        vm_name: &str,
        snapshot_name: &str,
        mut run: F,
    ) -> Result<SnapshotDiskCreateMetadata, StorageError>
    where
        F: FnMut(&str, &[String]) -> Result<Output, std::io::Error>,
    {
        let (bundle, _) = self.get_vm(vm_name)?;
        let mut disk = self
            .snapshot_disk_metadata(vm_name, snapshot_name)?
            .ok_or_else(|| StorageError::SnapshotDiskMetadataNotFound {
                vm: vm_name.to_string(),
                snapshot: snapshot_name.to_string(),
            })?;
        disk.backing_exists = disk.backing_path.exists();
        disk.overlay_exists = disk.overlay_path.exists();
        if !disk.backing_exists {
            self.write_snapshot_disk_metadata_at(&bundle, snapshot_name, &disk)?;
            return Err(StorageError::SnapshotDiskBackingMissing(
                disk.backing_path.clone(),
            ));
        }

        let command = disk.create_command.clone();
        if disk.overlay_exists {
            let active_disk =
                self.snapshot_overlay_active_disk_at(snapshot_name, &disk, disk.overlay_exists);
            self.write_active_disk_at(&bundle, &active_disk)?;
            let metadata = SnapshotDiskCreateMetadata {
                snapshot: snapshot_name.to_string(),
                disk,
                command,
                executed: false,
                exit_status: None,
                stdout: String::new(),
                stderr: String::new(),
                created_at_unix: now_unix(),
            };
            self.write_snapshot_disk_create_metadata_at(&bundle, snapshot_name, &metadata)?;
            return Ok(metadata);
        }

        let output = run(&command[0], &command[1..]).map_err(|source| {
            StorageError::SnapshotDiskCreateIo {
                command: command.clone(),
                source,
            }
        })?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let status = output.status.to_string();
        if !output.status.success() {
            return Err(StorageError::SnapshotDiskCreateFailed {
                command,
                status,
                stderr,
            });
        }

        disk.overlay_exists = disk.overlay_path.exists();
        disk.backing_exists = disk.backing_path.exists();
        self.write_snapshot_disk_metadata_at(&bundle, snapshot_name, &disk)?;
        let active_disk =
            self.snapshot_overlay_active_disk_at(snapshot_name, &disk, disk.overlay_exists);
        self.write_active_disk_at(&bundle, &active_disk)?;
        let metadata = SnapshotDiskCreateMetadata {
            snapshot: snapshot_name.to_string(),
            disk,
            command,
            executed: true,
            exit_status: Some(status),
            stdout,
            stderr,
            created_at_unix: now_unix(),
        };
        self.write_snapshot_disk_create_metadata_at(&bundle, snapshot_name, &metadata)?;
        Ok(metadata)
    }

    pub(crate) fn prepare_snapshot_disk_at(
        &self,
        bundle: &Path,
        manifest: &VmManifest,
        snapshot_name: &str,
    ) -> Result<SnapshotDiskMetadata, StorageError> {
        let active_disk = self.active_disk_at(bundle, manifest)?;
        let backing_path = active_disk.path;
        let overlay_path = bundle
            .join("disks")
            .join("snapshots")
            .join(format!("{}.qcow2", slug(snapshot_name)));
        if let Some(parent) = overlay_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let backing_format = active_disk.format;
        let create_command = QemuImgCommand::create_backed_disk(
            &overlay_path,
            "qcow2",
            backing_format.clone(),
            &backing_path,
        )
        .render_shell_words();
        let metadata = SnapshotDiskMetadata {
            snapshot: snapshot_name.to_string(),
            overlay_exists: overlay_path.exists(),
            overlay_path,
            overlay_format: "qcow2".to_string(),
            backing_path: backing_path.clone(),
            backing_format,
            backing_exists: backing_path.exists(),
            create_command,
            prepared_at_unix: now_unix(),
        };

        let path = snapshot_disk_metadata_path(bundle, snapshot_name);
        write_json_pretty_atomic(&path, &metadata)?;
        Ok(metadata)
    }

    pub(crate) fn write_snapshot_disk_metadata_at(
        &self,
        bundle: &Path,
        snapshot_name: &str,
        metadata: &SnapshotDiskMetadata,
    ) -> Result<(), StorageError> {
        write_json_pretty_atomic(
            &snapshot_disk_metadata_path(bundle, snapshot_name),
            metadata,
        )
    }

    pub(crate) fn write_snapshot_disk_create_metadata_at(
        &self,
        bundle: &Path,
        snapshot_name: &str,
        metadata: &SnapshotDiskCreateMetadata,
    ) -> Result<(), StorageError> {
        write_json_pretty_atomic(
            &snapshot_disk_create_metadata_path(bundle, snapshot_name),
            metadata,
        )
    }
}
