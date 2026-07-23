//! Create, list, get and delete a VM bundle and its manifest.

use crate::*;
use bridgevm_config::VmManifest;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub(crate) fn deletion_metadata_at(
    bundle: &Path,
) -> Result<Option<VmDeletionMetadata>, StorageError> {
    read_json_file(&deletion_metadata_path(bundle))
}

impl VmStore {
    pub fn create_vm(&self, manifest: &VmManifest) -> Result<PathBuf, StorageError> {
        self.ensure()?;
        let bundle = self.bundle_path(&manifest.name);
        if bundle.exists() {
            return Err(StorageError::AlreadyExists(manifest.name.clone()));
        }
        fs::create_dir_all(bundle.join("disks"))?;
        fs::create_dir_all(bundle.join("logs"))?;
        fs::create_dir_all(bundle.join("metadata"))?;
        manifest.write(&bundle.join("manifest.yaml"))?;
        self.write_state_at(&bundle, VmRuntimeState::Stopped)?;
        self.write_snapshots_at(&bundle, &[])?;
        let active_disk = self.primary_active_disk_at(&bundle, manifest);
        self.write_active_disk_at(&bundle, &active_disk)?;
        self.write_guest_tools_token_at(&bundle, &new_guest_tools_token()?)?;
        Ok(bundle)
    }

    pub fn list_vms(&self) -> Result<Vec<(PathBuf, VmManifest)>, StorageError> {
        self.ensure()?;
        let mut vms = Vec::new();
        for entry in fs::read_dir(self.vms_dir())? {
            let path = entry?.path();
            let manifest_path = path.join("manifest.yaml");
            if manifest_path.exists() && deletion_metadata_at(&path)?.is_none() {
                vms.push((path, VmManifest::read(&manifest_path)?));
            }
        }
        vms.sort_by(|a, b| a.1.name.cmp(&b.1.name));
        Ok(vms)
    }

    pub fn get_vm(&self, name: &str) -> Result<(PathBuf, VmManifest), StorageError> {
        let bundle = self.bundle_path(name);
        let manifest_path = bundle.join("manifest.yaml");
        if !manifest_path.exists() || deletion_metadata_at(&bundle)?.is_some() {
            return Err(StorageError::NotFound(name.to_string()));
        }
        Ok((bundle, VmManifest::read(&manifest_path)?))
    }

    pub fn get_vm_with_active_disk(
        &self,
        name: &str,
    ) -> Result<(PathBuf, VmManifest, ActiveDiskMetadata), StorageError> {
        let (bundle, mut manifest) = self.get_vm(name)?;
        let active_disk = self.active_disk_at(&bundle, &manifest)?;
        manifest.storage.primary.path = active_disk.path.display().to_string();
        manifest.storage.primary.format = active_disk.format.clone();
        Ok((bundle, manifest, active_disk))
    }

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
