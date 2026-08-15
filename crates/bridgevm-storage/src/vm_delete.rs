//! Deleting a VM, whole or metadata-only. Split out of vm_registry.rs.

use crate::*;
use std::fs;
use std::path::PathBuf;

impl VmStore {
    pub fn delete_vm(&self, name: &str) -> Result<PathBuf, StorageError> {
        let (bundle, _) = self.get_vm(name)?;
        fs::remove_dir_all(&bundle)?;
        Ok(bundle)
    }

    pub fn delete_vm_metadata_only(&self, name: &str) -> Result<VmDeletionMetadata, StorageError> {
        let (bundle, manifest) = self.get_vm(name)?;
        let metadata_dir = bundle.join("metadata");
        fs::create_dir_all(&metadata_dir)?;
        let manifest_path = bundle.join("manifest.yaml");
        let manifest_backup = metadata_dir.join("deleted-manifest.yaml");
        fs::copy(&manifest_path, &manifest_backup)?;
        let metadata_path = deletion_metadata_path(&bundle);
        let metadata = VmDeletionMetadata {
            vm: manifest.name,
            bundle,
            manifest_backup,
            metadata_path: metadata_path.clone(),
            deleted_at_unix: now_unix(),
            metadata_only: true,
        };
        write_json_pretty_atomic(&metadata_path, &metadata)?;
        Ok(metadata)
    }
}
