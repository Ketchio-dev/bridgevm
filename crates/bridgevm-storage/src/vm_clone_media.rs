//! Full clones own their copied disk graph, including QCOW2 backing headers.

use super::{rebase_copied_path, snapshot_records};
use crate::import_metadata::DiskFiles;
use crate::import_paths::ImportPaths;
use crate::*;
use bridgevm_config::VmManifest;
use std::fs;
use std::path::Path;

impl VmStore {
    pub(crate) fn rebase_copied_bundle_metadata(
        &self,
        source: &Path,
        output: &Path,
        manifest: &VmManifest,
    ) -> Result<(), StorageError> {
        let mut active_disk = self.active_disk_at(output, manifest)?;
        let paths = ImportPaths::new(source, output, &fs::canonicalize(output)?)?;
        let mut files = DiskFiles::default();
        files.formats.insert(
            paths.relative(Path::new(&manifest.storage.primary.path))?,
            manifest.storage.primary.format.clone(),
        );
        let active_path =
            if active_disk.path == resolve_bundle_path(output, &manifest.storage.primary.path) {
                paths.relative(Path::new(&manifest.storage.primary.path))?
            } else {
                paths.relative(&active_disk.path)?
            };
        if files
            .formats
            .get(&active_path)
            .is_some_and(|format| *format != active_disk.format)
        {
            return Err(crate::import_paths::invalid(
                "full clone disk formats disagree",
            ));
        }
        files
            .formats
            .insert(active_path, active_disk.format.clone());
        // The copy owns every member. Validate the complete copied backing graph
        // before changing any headers; never reopen an external source backing.
        crate::import_qcow::relocate(&paths, files).map_err(|error| {
            crate::import_paths::invalid(format!(
                "full clone could not make the copied disk graph independent: {error}"
            ))
        })?;
        active_disk.path = rebase_copied_path(&active_disk.path, source, output);
        active_disk.exists = active_disk.path.exists();
        self.write_active_disk_at(output, &active_disk)?;

        snapshot_records::rebase(source, output)?;

        let suspend_image_metadata_dir = output.join("metadata").join("suspend-images");
        if suspend_image_metadata_dir.exists() {
            for entry in fs::read_dir(&suspend_image_metadata_dir)? {
                let path = entry?.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let mut metadata: SnapshotSuspendImageMetadata = read_json_required(&path)?;
                metadata.image_path = rebase_copied_path(&metadata.image_path, source, output);
                metadata.image_exists = metadata.image_path.exists();
                write_json_pretty_atomic(&path, &metadata)?;
            }
        }

        Ok(())
    }
}
