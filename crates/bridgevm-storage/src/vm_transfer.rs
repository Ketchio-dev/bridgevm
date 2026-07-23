//! Export a bundle and import one, with path-conflict guards.

use crate::*;
use bridgevm_config::slug;
use bridgevm_config::VmManifest;
use std::fs;
use std::path::Path;

impl VmStore {
    pub fn export_vm(
        &self,
        name: &str,
        output: impl AsRef<Path>,
    ) -> Result<VmExportMetadata, StorageError> {
        let (bundle, manifest) = self.get_vm(name)?;
        let output = output.as_ref().to_path_buf();
        let source = fs::canonicalize(&bundle)?;
        let resolved_output = resolve_path_for_new(&output)?;
        if is_same_or_descendant(&resolved_output, &source) {
            return Err(StorageError::ExportOutputInsideSource {
                source_bundle: bundle,
                output,
            });
        }
        if output.exists() {
            return Err(StorageError::ExportAlreadyExists(output));
        }
        let archive_format = if is_tar_path(&output) {
            "tar"
        } else if is_unsupported_archive_path(&output) {
            return Err(StorageError::UnsupportedArchiveFormat(output));
        } else {
            "directory"
        };
        let copy_summary = summarize_bundle_copy(&bundle)?;
        let metadata = VmExportMetadata {
            vm: manifest.name,
            source: bundle,
            output: output.clone(),
            archive_format: archive_format.to_string(),
            copied_file_count: copy_summary.file_count,
            copied_files: copy_summary.files,
            manifest_preserved: copy_summary.manifest_preserved,
            metadata_preserved: copy_summary.metadata_preserved,
            exported_at_unix: now_unix(),
        };
        if is_tar_path(&output) {
            export_bundle_tar(&metadata.source, &output, &metadata)?;
        } else {
            copy_dir_all(&metadata.source, &output)?;
            let metadata_dir = output.join("metadata");
            fs::create_dir_all(&metadata_dir)?;
            fs::write(
                metadata_dir.join("export.json"),
                serde_json::to_string_pretty(&metadata)?,
            )?;
        }
        Ok(metadata)
    }

    pub fn import_vm(
        &self,
        input: impl AsRef<Path>,
        name_override: Option<&str>,
    ) -> Result<VmImportMetadata, StorageError> {
        self.ensure()?;
        let input = input.as_ref().to_path_buf();
        if input.is_file() {
            if !is_tar_path(&input) {
                return Err(StorageError::UnsupportedArchiveFormat(input));
            }
            let store_resolved = fs::canonicalize(&self.root)?;
            let input_resolved = fs::canonicalize(&input)?;
            if is_same_or_descendant(&input_resolved, &store_resolved) {
                return Err(StorageError::ImportPathConflict {
                    input,
                    output: self.vms_dir(),
                });
            }
            let staging = unique_temp_path("bridgevm-import-tar");
            let _staging_guard = TempDirGuard::new(staging.clone());
            extract_bundle_tar(&input, &staging)?;
            return self.import_vm_bundle(&staging, &input, name_override);
        }
        self.import_vm_bundle(&input, &input, name_override)
    }

    pub(crate) fn import_vm_bundle(
        &self,
        input: &Path,
        metadata_source: &Path,
        name_override: Option<&str>,
    ) -> Result<VmImportMetadata, StorageError> {
        let manifest_path = input.join("manifest.yaml");
        if !input.is_dir() || !manifest_path.exists() {
            return Err(StorageError::InvalidImportBundle(input.to_path_buf()));
        }

        let mut manifest = VmManifest::read(&manifest_path)?;
        let original_name = manifest.name.clone();
        let requested_name = name_override.map(str::to_string);
        if let Some(name) = name_override {
            manifest.name = name.to_string();
            manifest.network.hostname = format!("{}.bridgevm.local", slug(name));
        }

        let output = self.bundle_path(&manifest.name);
        let input_resolved = fs::canonicalize(input)?;
        let output_resolved = resolve_path_for_new(&output)?;
        let store_resolved = fs::canonicalize(&self.root)?;
        if is_same_or_descendant(&output_resolved, &input_resolved)
            || is_same_or_descendant(&input_resolved, &output_resolved)
            || is_same_or_descendant(&input_resolved, &store_resolved)
        {
            return Err(StorageError::ImportPathConflict {
                input: input.to_path_buf(),
                output,
            });
        }
        if output.exists() {
            return Err(StorageError::AlreadyExists(manifest.name));
        }

        let copy_summary = copy_dir_all(input, &output)?;
        manifest.write(&output.join("manifest.yaml"))?;
        let metadata = VmImportMetadata {
            vm: manifest.name,
            original_name,
            requested_name,
            source: metadata_source.to_path_buf(),
            output: output.clone(),
            archive_format: if is_tar_path(metadata_source) {
                "tar".to_string()
            } else {
                "directory".to_string()
            },
            copied_file_count: copy_summary.file_count,
            copied_files: copy_summary.files,
            manifest_preserved: copy_summary.manifest_preserved,
            metadata_preserved: copy_summary.metadata_preserved,
            manifest_identity_rewritten: name_override.is_some(),
            imported_at_unix: now_unix(),
        };
        let metadata_dir = output.join("metadata");
        fs::create_dir_all(&metadata_dir)?;
        fs::write(
            metadata_dir.join("import.json"),
            serde_json::to_string_pretty(&metadata)?,
        )?;
        Ok(metadata)
    }
}
