//! Split out of diskverifymetadata.rs to keep files under 600 lines.

use super::*;
use crate::*;

use bridgevm_config::read_manifest_bytes;
use bridgevm_config::slug;
use bridgevm_config::ConfigError;
use bridgevm_config::VmManifest;
use bridgevm_config::SCHEMA_VERSION;
use bridgevm_qemu::guest_tools_socket_path;
use bridgevm_qemu::QemuImgCommand;
use serde::Deserialize;
use serde::Serialize;
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Output;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiskVerifyMetadata {
    pub active_disk: ActiveDiskMetadata,
    pub command: Vec<String>,
    pub exit_status: String,
    pub report: serde_json::Value,
    pub stdout: String,
    pub stderr: String,
    pub verify_duration_microseconds: u64,
    pub verified_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskCompactMetadata {
    pub preparation: DiskPreparationMetadata,
    pub active_disk: ActiveDiskMetadata,
    pub command: Vec<String>,
    pub temp_path: PathBuf,
    pub backup_path: PathBuf,
    pub exit_status: String,
    pub stdout: String,
    pub stderr: String,
    pub original_size_bytes: u64,
    pub compacted_size_bytes: u64,
    pub compact_duration_microseconds: u64,
    pub compacted_at_unix: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnapshotKind {
    Disk,
    Suspend,
    ApplicationConsistent,
}

impl std::fmt::Display for SnapshotKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotKind::Disk => write!(f, "disk"),
            SnapshotKind::Suspend => write!(f, "suspend"),
            SnapshotKind::ApplicationConsistent => write!(f, "application-consistent"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VmStore {
    pub(crate) root: PathBuf,
}

impl Default for VmStore {
    fn default() -> Self {
        let root = env::var_os("BRIDGEVM_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".bridgevm")))
            .unwrap_or_else(|| PathBuf::from(".bridgevm"));
        Self::new(root)
    }
}

impl VmStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: absolutize(root.into()),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn vms_dir(&self) -> PathBuf {
        self.root.join("vms")
    }

    pub fn bundle_path(&self, name: &str) -> PathBuf {
        self.vms_dir().join(format!("{}.vmbridge", slug(name)))
    }

    pub fn ensure(&self) -> Result<(), StorageError> {
        fs::create_dir_all(self.vms_dir())?;
        Ok(())
    }

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

    pub fn clone_vm(
        &self,
        name: &str,
        new_name: &str,
        linked: bool,
    ) -> Result<VmCloneMetadata, StorageError> {
        self.clone_vm_with(name, new_name, linked, run_command)
    }

    pub(crate) fn clone_vm_with<F>(
        &self,
        name: &str,
        new_name: &str,
        linked: bool,
        mut run: F,
    ) -> Result<VmCloneMetadata, StorageError>
    where
        F: FnMut(&str, &[String]) -> Result<Output, std::io::Error>,
    {
        self.ensure()?;
        let (source, mut manifest) = self.get_vm(name)?;
        let source_active_disk = if linked {
            Some(self.active_disk_at(&source, &manifest)?)
        } else {
            None
        };
        manifest.name = new_name.to_string();
        manifest.network.hostname = format!("{}.bridgevm.local", slug(new_name));

        let output = self.bundle_path(new_name);
        if output.exists() {
            return Err(StorageError::AlreadyExists(new_name.to_string()));
        }

        if let Err(error) = copy_dir_all(&source, &output) {
            let _ = fs::remove_dir_all(&output);
            return Err(error);
        }

        let clone_result: Result<VmCloneMetadata, StorageError> = (|| {
            let mut backing_path = None;
            let mut backing_format = None;
            let mut create_command = None;
            if let Some(source_active_disk) = source_active_disk {
                if !source_active_disk.path.exists() {
                    return Err(StorageError::DiskMissing(source_active_disk.path));
                }
                let disks_dir = output.join("disks");
                if disks_dir.exists() {
                    fs::remove_dir_all(&disks_dir)?;
                }
                fs::create_dir_all(&disks_dir)?;
                let snapshot_disk_metadata_dir = output.join("metadata").join("snapshot-disks");
                if snapshot_disk_metadata_dir.exists() {
                    fs::remove_dir_all(snapshot_disk_metadata_dir)?;
                }
                let suspend_images_dir = output.join("suspend-images");
                if suspend_images_dir.exists() {
                    fs::remove_dir_all(suspend_images_dir)?;
                }

                manifest.storage.primary.path = "disks/root.qcow2".to_string();
                manifest.storage.primary.format = "qcow2".to_string();
                let overlay_path = output.join("disks").join("root.qcow2");
                let command = QemuImgCommand::create_backed_disk(
                    &overlay_path,
                    "qcow2",
                    source_active_disk.format.clone(),
                    &source_active_disk.path,
                )
                .render_shell_words();
                let command_output = run(&command[0], &command[1..]).map_err(|source| {
                    StorageError::LinkedCloneDiskCreateIo {
                        command: command.clone(),
                        source,
                    }
                })?;
                let stderr = String::from_utf8_lossy(&command_output.stderr).to_string();
                if !command_output.status.success() {
                    return Err(StorageError::LinkedCloneDiskCreateFailed {
                        command,
                        status: command_output.status.to_string(),
                        stderr,
                    });
                }
                let active_disk = ActiveDiskMetadata {
                    source: ActiveDiskSource::Primary,
                    snapshot: None,
                    path: overlay_path,
                    format: "qcow2".to_string(),
                    exists: true,
                    activated_at_unix: now_unix(),
                };
                self.write_active_disk_at(&output, &active_disk)?;
                self.write_snapshots_at(&output, &[])?;
                backing_path = Some(source_active_disk.path);
                backing_format = Some(source_active_disk.format);
                create_command = Some(command);
            } else {
                self.rebase_copied_bundle_metadata(&source, &output, &manifest)?;
            }
            // Make the clone an independent VM: drop the source's persisted
            // per-VM identity and transient runtime state so it is not a
            // network/identity duplicate and starts stopped/clean.
            self.reset_clone_runtime_identity(&output)?;
            manifest.write(&output.join("manifest.yaml"))?;
            let metadata = VmCloneMetadata {
                vm: manifest.name,
                source,
                output: output.clone(),
                linked,
                backing_path,
                backing_format,
                create_command,
                cloned_at_unix: now_unix(),
            };
            let metadata_dir = output.join("metadata");
            fs::create_dir_all(&metadata_dir)?;
            write_json_pretty_atomic(&metadata_dir.join("clone.json"), &metadata)?;
            Ok(metadata)
        })();

        if clone_result.is_err() {
            let _ = fs::remove_dir_all(&output);
        }
        clone_result
    }

    pub fn repair_metadata(&self, name: &str) -> Result<VmMetadataRepairMetadata, StorageError> {
        let (bundle, manifest) = self.get_vm(name)?;
        let mut actions = Vec::new();

        self.ensure_dir_for_repair(&bundle.join("metadata"), &mut actions)?;
        self.ensure_dir_for_repair(&bundle.join("disks"), &mut actions)?;
        self.ensure_dir_for_repair(&bundle.join("logs"), &mut actions)?;

        let state_path = bundle.join("metadata").join("state.json");
        match read_json_file::<VmRuntimeMetadata>(&state_path)? {
            Some(_) => {}
            None => {
                self.write_state_at(&bundle, VmRuntimeState::Stopped)?;
                actions.push(metadata_repair_action(
                    &state_path,
                    "repaired",
                    "wrote default stopped runtime state metadata",
                ));
            }
        }

        let snapshots_path = bundle.join("metadata").join("snapshots.json");
        let snapshots = match read_json_file::<Vec<SnapshotMetadata>>(&snapshots_path)? {
            Some(snapshots) => snapshots,
            None => {
                self.write_snapshots_at(&bundle, &[])?;
                actions.push(metadata_repair_action(
                    &snapshots_path,
                    "repaired",
                    "wrote empty snapshot list metadata",
                ));
                Vec::new()
            }
        };

        let active_disk_path = bundle.join("metadata").join("active-disk.json");
        let primary_active_disk = self.primary_active_disk_at(&bundle, &manifest);
        match read_json_file::<ActiveDiskMetadata>(&active_disk_path)? {
            Some(mut active_disk) => {
                let exists = active_disk.path.exists();
                if active_disk.exists != exists {
                    active_disk.exists = exists;
                    self.write_active_disk_at(&bundle, &active_disk)?;
                    actions.push(metadata_repair_action(
                        &active_disk_path,
                        "refreshed",
                        "updated active disk existence flag",
                    ));
                }
            }
            None => {
                self.write_active_disk_at(&bundle, &primary_active_disk)?;
                actions.push(metadata_repair_action(
                    &active_disk_path,
                    "repaired",
                    "wrote primary active disk metadata from manifest",
                ));
            }
        }

        let token_path = guest_tools_token_path(&bundle);
        match read_json_file::<GuestToolsTokenMetadata>(&token_path)? {
            Some(_) => {}
            None => {
                self.write_guest_tools_token_at(&bundle, &new_guest_tools_token()?)?;
                actions.push(metadata_repair_action(
                    &token_path,
                    "repaired",
                    "wrote new guest-tools token metadata",
                ));
            }
        }

        let primary_disk_path = bundle.join("metadata").join("primary-disk.json");
        match read_json_file::<DiskPreparationMetadata>(&primary_disk_path)? {
            Some(_) => {}
            None => {
                let primary_disk = primary_disk_preparation_metadata(&bundle, &manifest);
                write_json_pretty_atomic(&primary_disk_path, &primary_disk)?;
                actions.push(metadata_repair_action(
                    &primary_disk_path,
                    "repaired",
                    "wrote primary disk preparation metadata without creating a disk",
                ));
            }
        }

        for snapshot in snapshots {
            match snapshot.kind {
                SnapshotKind::Disk => {
                    let path = snapshot_disk_metadata_path(&bundle, &snapshot.name);
                    match read_json_file::<SnapshotDiskMetadata>(&path)? {
                        Some(mut metadata) => {
                            let overlay_exists = metadata.overlay_path.exists();
                            let backing_exists = metadata.backing_path.exists();
                            if metadata.overlay_exists != overlay_exists
                                || metadata.backing_exists != backing_exists
                            {
                                metadata.overlay_exists = overlay_exists;
                                metadata.backing_exists = backing_exists;
                                self.write_snapshot_disk_metadata_at(
                                    &bundle,
                                    &snapshot.name,
                                    &metadata,
                                )?;
                                actions.push(metadata_repair_action(
                                    &path,
                                    "refreshed",
                                    "updated snapshot disk existence flags",
                                ));
                            }
                        }
                        None => {
                            self.prepare_snapshot_disk_at(&bundle, &manifest, &snapshot.name)?;
                            actions.push(metadata_repair_action(
                                &path,
                                "repaired",
                                "wrote disk snapshot chain metadata from active disk",
                            ));
                        }
                    }
                }
                SnapshotKind::Suspend => {
                    let path = snapshot_suspend_image_metadata_path(&bundle, &snapshot.name);
                    match read_json_file::<SnapshotSuspendImageMetadata>(&path)? {
                        Some(mut metadata) => {
                            let image_exists = metadata.image_path.exists();
                            if metadata.image_exists != image_exists {
                                metadata.image_exists = image_exists;
                                self.write_snapshot_suspend_image_metadata_at(
                                    &bundle,
                                    &snapshot.name,
                                    &metadata,
                                )?;
                                actions.push(metadata_repair_action(
                                    &path,
                                    "refreshed",
                                    "updated suspend image existence flag",
                                ));
                            }
                        }
                        None => {
                            self.prepare_snapshot_suspend_image_at(&bundle, &snapshot.name)?;
                            actions.push(metadata_repair_action(
                                &path,
                                "repaired",
                                "wrote suspend image metadata",
                            ));
                        }
                    }
                }
                SnapshotKind::ApplicationConsistent => {
                    let path =
                        application_consistent_snapshot_preflight_path(&bundle, &snapshot.name);
                    if read_json_file::<ApplicationConsistentSnapshotPreflightMetadata>(&path)?
                        .is_none()
                    {
                        self.prepare_application_consistent_snapshot_preflight_at(
                            &bundle,
                            &snapshot.name,
                        )?;
                        actions.push(metadata_repair_action(
                            &path,
                            "repaired",
                            "wrote application-consistent snapshot preflight metadata",
                        ));
                    }
                }
            }
        }

        Ok(VmMetadataRepairMetadata {
            vm: manifest.name,
            bundle,
            repaired: !actions.is_empty(),
            actions,
            repaired_at_unix: now_unix(),
        })
    }

    pub fn migrate_manifest(
        &self,
        name: &str,
        dry_run: bool,
    ) -> Result<VmManifestMigrationMetadata, StorageError> {
        let bundle = self.bundle_path(name);
        let manifest_path = bundle.join("manifest.yaml");
        if !manifest_path.exists() || deletion_metadata_at(&bundle)?.is_some() {
            return Err(StorageError::NotFound(name.to_string()));
        }

        let raw_manifest = read_manifest_bytes(&manifest_path)?;
        let manifest_value: serde_yaml::Value =
            serde_yaml::from_slice(&raw_manifest).map_err(ConfigError::from)?;
        let from_schema = manifest_value
            .get("schemaVersion")
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or("<missing>")
            .to_string();
        if from_schema != SCHEMA_VERSION {
            return Err(StorageError::Config(ConfigError::UnsupportedSchema {
                expected: SCHEMA_VERSION,
                actual: from_schema,
            }));
        }

        let manifest: VmManifest =
            serde_yaml::from_slice(&raw_manifest).map_err(ConfigError::from)?;
        manifest.validate()?;

        let metadata_dir = bundle.join("metadata");
        let backup_path = metadata_dir.join("manifest-before-migration.yaml");
        let receipt_path = metadata_dir.join("manifest-migration.json");
        let migrated_at_unix = now_unix();

        let mut actions = vec![metadata_repair_action(
            &manifest_path,
            "validated",
            "manifest already uses the current schema",
        )];
        if dry_run {
            actions.push(metadata_repair_action(
                &receipt_path,
                "planned",
                "dry-run did not write migration receipt or manifest backup",
            ));
            return Ok(VmManifestMigrationMetadata {
                vm: manifest.name,
                bundle,
                manifest_path,
                from_schema,
                to_schema: SCHEMA_VERSION.to_string(),
                dry_run,
                migrated: false,
                backup_path: None,
                receipt_path: None,
                actions,
                migrated_at_unix,
            });
        }

        fs::create_dir_all(&metadata_dir)?;
        fs::copy(&manifest_path, &backup_path)?;
        actions.push(metadata_repair_action(
            &backup_path,
            "backed-up",
            "copied manifest before migration",
        ));

        let metadata = VmManifestMigrationMetadata {
            vm: manifest.name,
            bundle,
            manifest_path,
            from_schema,
            to_schema: SCHEMA_VERSION.to_string(),
            dry_run,
            migrated: false,
            backup_path: Some(backup_path),
            receipt_path: Some(receipt_path.clone()),
            actions,
            migrated_at_unix,
        };
        write_json_pretty_atomic(&receipt_path, &metadata)?;
        Ok(metadata)
    }

    pub(crate) fn ensure_dir_for_repair(
        &self,
        path: &Path,
        actions: &mut Vec<MetadataRepairAction>,
    ) -> Result<(), StorageError> {
        if !path.exists() {
            fs::create_dir_all(path)?;
            actions.push(metadata_repair_action(
                path,
                "created",
                "created missing VM bundle directory",
            ));
        }
        Ok(())
    }

    pub fn prepare_primary_disk(
        &self,
        name: &str,
    ) -> Result<DiskPreparationMetadata, StorageError> {
        let (bundle, manifest) = self.get_vm(name)?;
        let path = resolve_bundle_path(&bundle, &manifest.storage.primary.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let format = manifest.storage.primary.format.clone();
        let size = manifest.storage.primary.size.clone();
        let size_bytes = parse_size_bytes(&size);
        let mut exists = path.exists();
        let mut created = false;
        let mut create_command = None;

        if !exists {
            if format == "raw" {
                let file = fs::File::create(&path)?;
                if let Some(bytes) = size_bytes {
                    file.set_len(bytes)?;
                }
                exists = true;
                created = true;
            } else {
                create_command = Some(
                    QemuImgCommand::create_disk(&path, format.clone(), size.clone())
                        .render_shell_words(),
                );
            }
        }

        let metadata = DiskPreparationMetadata {
            path,
            format,
            size,
            size_bytes,
            exists,
            created,
            create_command,
            prepared_at_unix: now_unix(),
        };
        let metadata_dir = bundle.join("metadata");
        fs::create_dir_all(&metadata_dir)?;
        fs::write(
            metadata_dir.join("primary-disk.json"),
            serde_json::to_string_pretty(&metadata)?,
        )?;
        Ok(metadata)
    }

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

    pub fn prepare_active_disk(
        &self,
        name: &str,
    ) -> Result<(DiskPreparationMetadata, ActiveDiskMetadata), StorageError> {
        let (bundle, manifest) = self.get_vm(name)?;
        let active_disk = self.active_disk_at(&bundle, &manifest)?;
        if active_disk.source == ActiveDiskSource::Primary {
            let disk = self.prepare_primary_disk(name)?;
            let active_disk = ActiveDiskMetadata {
                exists: disk.exists,
                path: disk.path.clone(),
                format: disk.format.clone(),
                ..active_disk
            };
            self.write_active_disk_at(&bundle, &active_disk)?;
            return Ok((disk, active_disk));
        }

        let disk = DiskPreparationMetadata {
            path: active_disk.path.clone(),
            format: active_disk.format.clone(),
            size: manifest.storage.primary.size,
            size_bytes: None,
            exists: active_disk.path.exists(),
            created: false,
            create_command: None,
            prepared_at_unix: now_unix(),
        };
        let active_disk = ActiveDiskMetadata {
            exists: disk.exists,
            ..active_disk
        };
        self.write_active_disk_at(&bundle, &active_disk)?;
        Ok((disk, active_disk))
    }

    pub fn create_primary_disk(&self, name: &str) -> Result<DiskCreateMetadata, StorageError> {
        self.create_primary_disk_with(name, run_command)
    }

    pub fn inspect_primary_disk(&self, name: &str) -> Result<DiskInspectMetadata, StorageError> {
        self.inspect_primary_disk_with(name, run_command)
    }

    pub fn verify_active_disk(&self, name: &str) -> Result<DiskVerifyMetadata, StorageError> {
        self.verify_active_disk_with(name, run_command)
    }

    pub fn compact_active_disk(&self, name: &str) -> Result<DiskCompactMetadata, StorageError> {
        self.compact_active_disk_with(name, run_command)
    }

    pub(crate) fn verify_active_disk_with<F>(
        &self,
        name: &str,
        mut run: F,
    ) -> Result<DiskVerifyMetadata, StorageError>
    where
        F: FnMut(&str, &[String]) -> Result<Output, std::io::Error>,
    {
        let (_preparation, active_disk) = self.prepare_active_disk(name)?;
        if !active_disk.exists {
            return Err(StorageError::DiskMissing(active_disk.path));
        }
        if active_disk.format == "raw" {
            return Err(StorageError::DiskVerifyUnsupportedRaw(active_disk.path));
        }

        let command = QemuImgCommand::check_json(&active_disk.path).render_shell_words();
        let verify_started = Instant::now();
        let output =
            run(&command[0], &command[1..]).map_err(|source| StorageError::DiskVerifyIo {
                command: command.clone(),
                source,
            })?;
        let verify_duration_microseconds = duration_micros_u64(verify_started.elapsed());
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let status = output.status.to_string();
        if !output.status.success() {
            return Err(StorageError::DiskVerifyFailed {
                command,
                status,
                stderr,
            });
        }
        let report = serde_json::from_str(&stdout)?;
        let metadata = DiskVerifyMetadata {
            active_disk,
            command,
            exit_status: status,
            report,
            stdout,
            stderr,
            verify_duration_microseconds,
            verified_at_unix: now_unix(),
        };
        self.write_disk_verify_metadata(name, &metadata)?;
        Ok(metadata)
    }

    pub(crate) fn compact_active_disk_with<F>(
        &self,
        name: &str,
        mut run: F,
    ) -> Result<DiskCompactMetadata, StorageError>
    where
        F: FnMut(&str, &[String]) -> Result<Output, std::io::Error>,
    {
        let (bundle, _) = self.get_vm(name)?;
        let (preparation, active_disk) = self.prepare_active_disk(name)?;
        if !active_disk.exists {
            return Err(StorageError::DiskMissing(active_disk.path));
        }
        if active_disk.format == "raw" {
            return Err(StorageError::DiskCompactUnsupportedRaw(active_disk.path));
        }

        let original_size_bytes = fs::metadata(&active_disk.path)?.len();
        let compacted_at_unix = now_unix();
        let temp_path = active_disk
            .path
            .with_extension(format!("{}.compact.tmp", active_disk.format));
        let backup_path = active_disk.path.with_extension(format!(
            "{}.precompact-{compacted_at_unix}",
            active_disk.format
        ));
        if temp_path.exists() {
            fs::remove_file(&temp_path)?;
        }

        let command =
            QemuImgCommand::convert_compact(&active_disk.path, &temp_path, &active_disk.format)
                .render_shell_words();
        let compact_started = Instant::now();
        let output =
            run(&command[0], &command[1..]).map_err(|source| StorageError::DiskCompactIo {
                command: command.clone(),
                source,
            })?;
        let compact_duration_microseconds = duration_micros_u64(compact_started.elapsed());
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let status = output.status.to_string();
        if !output.status.success() {
            return Err(StorageError::DiskCompactFailed {
                command,
                status,
                stderr,
            });
        }
        if !temp_path.exists() {
            return Err(StorageError::DiskMissing(temp_path));
        }

        fs::rename(&active_disk.path, &backup_path)?;
        fs::rename(&temp_path, &active_disk.path)?;
        let compacted_size_bytes = fs::metadata(&active_disk.path)?.len();

        let active_disk = ActiveDiskMetadata {
            exists: true,
            ..active_disk
        };
        self.write_active_disk_at(&bundle, &active_disk)?;

        let metadata = DiskCompactMetadata {
            preparation: DiskPreparationMetadata {
                exists: active_disk.path.exists(),
                ..preparation
            },
            active_disk,
            command,
            temp_path,
            backup_path,
            exit_status: status,
            stdout,
            stderr,
            original_size_bytes,
            compacted_size_bytes,
            compact_duration_microseconds,
            compacted_at_unix,
        };
        self.write_disk_compact_metadata(name, &metadata)?;
        Ok(metadata)
    }

    pub(crate) fn inspect_primary_disk_with<F>(
        &self,
        name: &str,
        mut run: F,
    ) -> Result<DiskInspectMetadata, StorageError>
    where
        F: FnMut(&str, &[String]) -> Result<Output, std::io::Error>,
    {
        let preparation = self.prepare_primary_disk(name)?;
        if !preparation.exists {
            return Err(StorageError::DiskMissing(preparation.path));
        }
        if preparation.format == "raw" {
            return Err(StorageError::DiskInspectUnsupportedRaw(preparation.path));
        }

        let command = QemuImgCommand::info_json(&preparation.path).render_shell_words();
        let inspect_started = Instant::now();
        let output =
            run(&command[0], &command[1..]).map_err(|source| StorageError::DiskInspectIo {
                command: command.clone(),
                source,
            })?;
        let inspect_duration_microseconds = duration_micros_u64(inspect_started.elapsed());
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let status = output.status.to_string();
        if !output.status.success() {
            return Err(StorageError::DiskInspectFailed {
                command,
                status,
                stderr,
            });
        }
        let info = serde_json::from_str(&stdout)?;
        let metadata = DiskInspectMetadata {
            preparation,
            command,
            exit_status: status,
            info,
            stdout,
            stderr,
            inspect_duration_microseconds,
            inspected_at_unix: now_unix(),
        };
        self.write_disk_inspect_metadata(name, &metadata)?;
        Ok(metadata)
    }

    pub(crate) fn create_primary_disk_with<F>(
        &self,
        name: &str,
        mut run: F,
    ) -> Result<DiskCreateMetadata, StorageError>
    where
        F: FnMut(&str, &[String]) -> Result<Output, std::io::Error>,
    {
        let mut preparation = self.prepare_primary_disk(name)?;
        let command = preparation.create_command.clone();
        let Some(command_words) = command.clone() else {
            let metadata = DiskCreateMetadata {
                preparation,
                command,
                executed: false,
                exit_status: None,
                stdout: String::new(),
                stderr: String::new(),
                created_at_unix: now_unix(),
            };
            self.write_disk_create_metadata(name, &metadata)?;
            return Ok(metadata);
        };

        let output = run(&command_words[0], &command_words[1..]).map_err(|source| {
            StorageError::DiskCreateIo {
                command: command_words.clone(),
                source,
            }
        })?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let status = output.status.to_string();
        if !output.status.success() {
            return Err(StorageError::DiskCreateFailed {
                command: command_words,
                status,
                stderr,
            });
        }

        preparation = self.prepare_primary_disk(name)?;
        let metadata = DiskCreateMetadata {
            preparation,
            command,
            executed: true,
            exit_status: Some(status),
            stdout,
            stderr,
            created_at_unix: now_unix(),
        };
        self.write_disk_create_metadata(name, &metadata)?;
        Ok(metadata)
    }

    pub fn state(&self, name: &str) -> Result<VmRuntimeMetadata, StorageError> {
        let (bundle, _) = self.get_vm(name)?;
        self.state_at(&bundle)
    }

    pub fn transition_state(
        &self,
        name: &str,
        to: VmRuntimeState,
    ) -> Result<VmRuntimeMetadata, StorageError> {
        let (bundle, _) = self.get_vm(name)?;
        let current = self.state_at(&bundle)?;
        validate_transition(current.state, to)?;
        self.write_state_at(&bundle, to)
    }

    /// Write the runtime state UNCONDITIONALLY, bypassing the transition-validity
    /// check. For use only after an irreversible action has already made the new
    /// state the ground truth (e.g. the backend process has been killed, or a
    /// suspend snapshot committed): the recorded state must then reflect reality
    /// even if the prior state was unexpected — refusing the write here is what
    /// strands a dead backend recorded as `Running`.
    pub fn force_transition_state(
        &self,
        name: &str,
        to: VmRuntimeState,
    ) -> Result<VmRuntimeMetadata, StorageError> {
        let (bundle, _) = self.get_vm(name)?;
        self.write_state_at(&bundle, to)
    }

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

    pub fn application_consistent_snapshot_preflight_metadata(
        &self,
        vm_name: &str,
        snapshot_name: &str,
    ) -> Result<Option<ApplicationConsistentSnapshotPreflightMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = application_consistent_snapshot_preflight_path(&bundle, snapshot_name);
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

    pub fn runner_metadata(&self, vm_name: &str) -> Result<Option<RunnerMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = bundle.join("metadata").join("runner.json");
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(read_json_required(&path)?))
    }

    pub fn guest_tools_token(
        &self,
        vm_name: &str,
    ) -> Result<GuestToolsTokenMetadata, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        self.guest_tools_token_at(&bundle)
    }

    pub fn guest_tools_runner_metadata(
        &self,
        vm_name: &str,
    ) -> Result<GuestToolsRunnerMetadata, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let token = self.guest_tools_token_at(&bundle)?;
        Ok(GuestToolsRunnerMetadata {
            transport: "virtio-serial".to_string(),
            channel_name: GUEST_TOOLS_CHANNEL_NAME.to_string(),
            socket_path: guest_tools_socket_path(&bundle),
            token_path: guest_tools_token_path(&bundle),
            token_created_at_unix: token.created_at_unix,
        })
    }

    pub fn guest_tools_runtime_metadata(
        &self,
        vm_name: &str,
    ) -> Result<Option<GuestToolsRuntimeMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = guest_tools_runtime_path(&bundle);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(read_json_required(&path)?))
    }

    pub fn write_guest_tools_runtime_metadata(
        &self,
        vm_name: &str,
        metadata: &GuestToolsRuntimeMetadata,
    ) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        write_json_pretty_atomic(&guest_tools_runtime_path(&bundle), metadata)
    }

    pub fn runtime_resource_policy_metadata(
        &self,
        vm_name: &str,
    ) -> Result<Option<RuntimeResourcePolicyMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = runtime_resource_policy_path(&bundle);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(read_json_required(&path)?))
    }

    pub fn write_runtime_resource_policy_metadata(
        &self,
        vm_name: &str,
        metadata: &RuntimeResourcePolicyMetadata,
    ) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        write_json_pretty_atomic(&runtime_resource_policy_path(&bundle), metadata)
    }

    pub fn qmp_supervisor_metadata(
        &self,
        vm_name: &str,
    ) -> Result<Option<QmpSupervisorMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = qmp_supervisor_path(&bundle);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(read_json_required(&path)?))
    }

    pub fn write_qmp_supervisor_metadata(
        &self,
        vm_name: &str,
        metadata: &QmpSupervisorMetadata,
    ) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        write_json_pretty_atomic(&qmp_supervisor_path(&bundle), metadata)
    }

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

    pub fn write_runner_metadata(
        &self,
        vm_name: &str,
        metadata: &RunnerMetadata,
    ) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let dir = bundle.join("metadata");
        fs::create_dir_all(&dir)?;
        // Atomic (temp + rename): a torn write of runner.json would otherwise
        // leave invalid JSON that bricks every later lifecycle read.
        write_json_pretty_atomic(&dir.join("runner.json"), metadata)?;
        Ok(())
    }

    pub(crate) fn rebase_copied_bundle_metadata(
        &self,
        source: &Path,
        output: &Path,
        manifest: &VmManifest,
    ) -> Result<(), StorageError> {
        let mut active_disk = self.active_disk_at(output, manifest)?;
        active_disk.path = rebase_copied_path(&active_disk.path, source, output);
        active_disk.exists = active_disk.path.exists();
        self.write_active_disk_at(output, &active_disk)?;

        let snapshot_disk_metadata_dir = output.join("metadata").join("snapshot-disks");
        if snapshot_disk_metadata_dir.exists() {
            for entry in fs::read_dir(&snapshot_disk_metadata_dir)? {
                let path = entry?.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                if path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with("-create.json"))
                {
                    let mut metadata: SnapshotDiskCreateMetadata = read_json_required(&path)?;
                    rebase_snapshot_disk_metadata(&mut metadata.disk, source, output);
                    write_json_pretty_atomic(&path, &metadata)?;
                } else {
                    let mut metadata: SnapshotDiskMetadata = read_json_required(&path)?;
                    rebase_snapshot_disk_metadata(&mut metadata, source, output);
                    write_json_pretty_atomic(&path, &metadata)?;
                }
            }
        }

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

    /// Reset a freshly-copied clone bundle so it is an independent VM rather than
    /// a duplicate of the source on the network/host.
    ///
    /// A bundle copy duplicates everything, including per-VM identity and
    /// transient runtime state. For a clone we must:
    /// - drop the persisted per-VM identity (Apple VZ machine identifier and the
    ///   NAT MAC address) so the clone regenerates fresh identity on next launch
    ///   and is not a network/identity duplicate of the source;
    /// - issue a fresh guest-tools token (the source's credential must not be
    ///   reused);
    /// - drop transient runner metadata and reset runtime state to Stopped so the
    ///   clone starts clean (no inherited pid/log pointers into the source's run);
    /// - drop any inherited Fast Mode saved-state suspend image, which is keyed
    ///   by the source's name and identity and would otherwise leave the clone
    ///   marked suspended against state it can never restore.
    ///
    /// Snapshot/disk overlay metadata is rebased separately (full clone) or
    /// dropped (linked clone) by the callers.
    pub(crate) fn reset_clone_runtime_identity(&self, output: &Path) -> Result<(), StorageError> {
        let metadata_dir = output.join("metadata");

        // Per-VM identity persisted by the Apple VZ runner (machine identifier +
        // NAT MAC). Removing them makes the runner mint fresh identity on the
        // clone's next launch instead of cloning the source's.
        for identity_file in [
            metadata_dir.join("machine-identifier.bin"),
            metadata_dir.join("network-mac-address.txt"),
        ] {
            if identity_file.exists() {
                fs::remove_file(&identity_file)?;
            }
        }

        // Fresh guest-tools token: the clone must not share the source's credential.
        self.write_guest_tools_token_at(output, &new_guest_tools_token()?)?;

        // Transient runner metadata points at the source's run (pid, log files).
        let runner_path = metadata_dir.join("runner.json");
        if runner_path.exists() {
            fs::remove_file(&runner_path)?;
        }

        // Fast Mode saved-state images live under metadata/suspend-images and are
        // keyed by the source's name/identity; the clone cannot restore them.
        let fast_suspend_dir = metadata_dir.join("suspend-images");
        if fast_suspend_dir.exists() {
            fs::remove_dir_all(&fast_suspend_dir)?;
        }

        // The clone starts stopped/clean regardless of the source's live state.
        self.write_state_at(output, VmRuntimeState::Stopped)?;

        Ok(())
    }

    pub fn clear_runner_metadata(&self, vm_name: &str) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = bundle.join("metadata").join("runner.json");
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub(crate) fn write_disk_create_metadata(
        &self,
        vm_name: &str,
        metadata: &DiskCreateMetadata,
    ) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let dir = bundle.join("metadata");
        write_json_pretty_atomic(&dir.join("last-disk-create.json"), metadata)?;
        Ok(())
    }

    pub(crate) fn write_disk_inspect_metadata(
        &self,
        vm_name: &str,
        metadata: &DiskInspectMetadata,
    ) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let dir = bundle.join("metadata");
        write_json_pretty_atomic(&dir.join("last-disk-inspect.json"), metadata)?;
        Ok(())
    }

    pub(crate) fn write_disk_verify_metadata(
        &self,
        vm_name: &str,
        metadata: &DiskVerifyMetadata,
    ) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let dir = bundle.join("metadata");
        write_json_pretty_atomic(&dir.join("last-disk-verify.json"), metadata)?;
        Ok(())
    }

    pub(crate) fn write_disk_compact_metadata(
        &self,
        vm_name: &str,
        metadata: &DiskCompactMetadata,
    ) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let dir = bundle.join("metadata");
        write_json_pretty_atomic(&dir.join("last-disk-compact.json"), metadata)?;
        Ok(())
    }

    pub(crate) fn state_at(&self, bundle: &Path) -> Result<VmRuntimeMetadata, StorageError> {
        let path = bundle.join("metadata").join("state.json");
        if !path.exists() {
            return self.write_state_at(bundle, VmRuntimeState::Stopped);
        }
        read_json_required(&path)
    }

    pub(crate) fn write_state_at(
        &self,
        bundle: &Path,
        state: VmRuntimeState,
    ) -> Result<VmRuntimeMetadata, StorageError> {
        let metadata = VmRuntimeMetadata {
            state,
            updated_at_unix: now_unix(),
        };
        let dir = bundle.join("metadata");
        fs::create_dir_all(&dir)?;
        // Atomic (temp + rename): a torn write of state.json would otherwise
        // leave invalid JSON that bricks lifecycle reads.
        write_json_pretty_atomic(&dir.join("state.json"), &metadata)?;
        Ok(metadata)
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

    pub(crate) fn prepare_application_consistent_snapshot_preflight_at(
        &self,
        bundle: &Path,
        snapshot_name: &str,
    ) -> Result<ApplicationConsistentSnapshotPreflightMetadata, StorageError> {
        let runtime_path = guest_tools_runtime_path(bundle);
        let runtime: Option<GuestToolsRuntimeMetadata> = if runtime_path.exists() {
            Some(read_json_required(&runtime_path)?)
        } else {
            None
        };
        let required_capabilities = application_consistent_snapshot_required_capabilities();
        let available_capabilities = runtime
            .as_ref()
            .map(|runtime| runtime.capabilities.clone())
            .unwrap_or_default();
        let missing_capabilities = required_capabilities
            .iter()
            .filter(|required| {
                !available_capabilities
                    .iter()
                    .any(|available| available == *required)
            })
            .cloned()
            .collect::<Vec<_>>();
        let connected = runtime.as_ref().is_some_and(|runtime| runtime.connected);
        let ready = connected && missing_capabilities.is_empty();
        let metadata = ApplicationConsistentSnapshotPreflightMetadata {
            snapshot: snapshot_name.to_string(),
            connected,
            required_capabilities,
            available_capabilities,
            missing_capabilities,
            ready,
            planned_freeze_semantics: APPLICATION_CONSISTENT_FREEZE_SEMANTICS.to_string(),
            planned_thaw_semantics: APPLICATION_CONSISTENT_THAW_SEMANTICS.to_string(),
            runtime_updated_at_unix: runtime.as_ref().map(|runtime| runtime.updated_at_unix),
            prepared_at_unix: now_unix(),
        };
        write_json_pretty_atomic(
            &application_consistent_snapshot_preflight_path(bundle, snapshot_name),
            &metadata,
        )?;
        Ok(metadata)
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

    pub(crate) fn guest_tools_token_at(
        &self,
        bundle: &Path,
    ) -> Result<GuestToolsTokenMetadata, StorageError> {
        let path = guest_tools_token_path(bundle);
        if path.exists() {
            return read_json_required(&path);
        }

        let metadata = new_guest_tools_token()?;
        self.write_guest_tools_token_at(bundle, &metadata)?;
        Ok(metadata)
    }

    pub(crate) fn write_guest_tools_token_at(
        &self,
        bundle: &Path,
        metadata: &GuestToolsTokenMetadata,
    ) -> Result<(), StorageError> {
        write_json_pretty_atomic(&guest_tools_token_path(bundle), metadata)
    }
}
