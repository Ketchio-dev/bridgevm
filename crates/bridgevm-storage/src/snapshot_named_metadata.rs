//! Exact display-name identity for snapshot metadata keyed by filesystem slugs.

use super::read_named_metadata;
use crate::*;

impl VmStore {
    pub fn snapshot_disk_metadata(
        &self,
        vm_name: &str,
        snapshot_name: &str,
    ) -> Result<Option<SnapshotDiskMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = snapshot_disk_metadata_path(&bundle, snapshot_name);
        read_named_metadata(&path, snapshot_name, |metadata: &SnapshotDiskMetadata| {
            &metadata.snapshot
        })
    }

    pub fn snapshot_suspend_image_metadata(
        &self,
        vm_name: &str,
        snapshot_name: &str,
    ) -> Result<Option<SnapshotSuspendImageMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = snapshot_suspend_image_metadata_path(&bundle, snapshot_name);
        read_named_metadata(
            &path,
            snapshot_name,
            |metadata: &SnapshotSuspendImageMetadata| &metadata.snapshot,
        )
    }

    pub fn application_consistent_snapshot_preflight_metadata(
        &self,
        vm_name: &str,
        snapshot_name: &str,
    ) -> Result<Option<ApplicationConsistentSnapshotPreflightMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = application_consistent_snapshot_preflight_path(&bundle, snapshot_name);
        read_named_metadata(
            &path,
            snapshot_name,
            |metadata: &ApplicationConsistentSnapshotPreflightMetadata| &metadata.snapshot,
        )
    }
}
