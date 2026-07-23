//! Snapshot suspend images and the VM-scoped Fast Mode suspend image.

use crate::*;
use bridgevm_config::slug;
use std::fs;
use std::path::Path;

impl VmStore {
    pub fn snapshot_suspend_image_metadata(
        &self,
        vm_name: &str,
        snapshot_name: &str,
    ) -> Result<Option<SnapshotSuspendImageMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = snapshot_suspend_image_metadata_path(&bundle, snapshot_name);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(read_json_required(&path)?))
    }

    /// Record that a Fast Mode suspend image now exists at `image_path`,
    /// writing the VM-scoped suspend-image metadata. Used after a successful
    /// Fast Mode suspend.
    pub fn mark_fast_suspend_image_exists(
        &self,
        vm_name: &str,
        image_path: &Path,
    ) -> Result<FastSuspendImageMetadata, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let metadata = FastSuspendImageMetadata {
            vm: vm_name.to_string(),
            image_path: image_path.to_path_buf(),
            image_format: "apple-vz-saved-state-v1".to_string(),
            image_exists: image_path.exists(),
            updated_at_unix: now_unix(),
        };
        write_json_pretty_atomic(
            &fast_suspend_image_metadata_path(&bundle, vm_name),
            &metadata,
        )?;
        Ok(metadata)
    }

    /// Read the VM-scoped Fast Mode suspend-image metadata, if present.
    pub fn fast_suspend_image_metadata(
        &self,
        vm_name: &str,
    ) -> Result<Option<FastSuspendImageMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = fast_suspend_image_metadata_path(&bundle, vm_name);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(read_json_required(&path)?))
    }

    pub(crate) fn prepare_snapshot_suspend_image_at(
        &self,
        bundle: &Path,
        snapshot_name: &str,
    ) -> Result<SnapshotSuspendImageMetadata, StorageError> {
        let image_path = bundle
            .join("suspend-images")
            .join(format!("{}.bin", slug(snapshot_name)));
        if let Some(parent) = image_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let metadata = SnapshotSuspendImageMetadata {
            snapshot: snapshot_name.to_string(),
            image_path,
            image_format: "bridgevm-suspend-image-v1".to_string(),
            image_exists: false,
            prepared_at_unix: now_unix(),
        };
        self.write_snapshot_suspend_image_metadata_at(bundle, snapshot_name, &metadata)?;
        Ok(metadata)
    }

    pub(crate) fn write_snapshot_suspend_image_metadata_at(
        &self,
        bundle: &Path,
        snapshot_name: &str,
        metadata: &SnapshotSuspendImageMetadata,
    ) -> Result<(), StorageError> {
        write_json_pretty_atomic(
            &snapshot_suspend_image_metadata_path(bundle, snapshot_name),
            metadata,
        )
    }
}
