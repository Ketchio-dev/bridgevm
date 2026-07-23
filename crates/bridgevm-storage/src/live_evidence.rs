//! Import, read and clear a preserved live-evidence bundle.

use crate::*;
use std::fs;
use std::path::Path;

impl VmStore {
    pub fn live_evidence_metadata(
        &self,
        vm_name: &str,
    ) -> Result<Option<VmLiveEvidenceMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = live_evidence_metadata_path(&bundle);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(read_json_required(&path)?))
    }

    pub fn import_live_evidence_bundle(
        &self,
        vm_name: &str,
        source: &Path,
    ) -> Result<VmLiveEvidenceMetadata, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let source = absolutize(source.to_path_buf());
        let preserved_path = live_evidence_preserved_path(&bundle);
        let source_canonical = fs::canonicalize(&source)?;
        let preserved_parent = preserved_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        fs::create_dir_all(&preserved_parent)?;
        let preserved_parent_canonical = fs::canonicalize(&preserved_parent)?;
        if source_canonical.starts_with(&preserved_parent_canonical) {
            return Err(StorageError::UnsupportedBundleEntry(source));
        }
        if preserved_path.exists() {
            fs::remove_dir_all(&preserved_path)?;
        }
        let copy_summary = copy_dir_all(&source, &preserved_path)?;
        let metadata = VmLiveEvidenceMetadata {
            vm: vm_name.to_string(),
            source,
            preserved_path: preserved_path.clone(),
            copied_file_count: copy_summary.file_count,
            copied_files: copy_summary.files,
            recorded_at_unix: now_unix(),
        };
        write_json_pretty_atomic(&live_evidence_metadata_path(&bundle), &metadata)?;
        Ok(metadata)
    }

    pub fn clear_live_evidence_metadata(&self, vm_name: &str) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let metadata_path = live_evidence_metadata_path(&bundle);
        if metadata_path.exists() {
            fs::remove_file(metadata_path)?;
        }
        let preserved_dir = bundle.join("metadata").join("live-evidence");
        if preserved_dir.exists() {
            fs::remove_dir_all(preserved_dir)?;
        }
        Ok(())
    }
}
