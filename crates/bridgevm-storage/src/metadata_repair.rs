//! Repairing missing or stale bundle metadata and recording manifest migration.

use crate::*;
use bridgevm_config::read_manifest_bytes;
use bridgevm_config::ConfigError;
use bridgevm_config::VmManifest;
use bridgevm_config::SCHEMA_VERSION;
use std::fs;
use std::path::Path;

pub(crate) fn metadata_repair_action(
    path: &Path,
    action: impl Into<String>,
    detail: impl Into<String>,
) -> MetadataRepairAction {
    MetadataRepairAction {
        path: path.to_path_buf(),
        action: action.into(),
        detail: detail.into(),
    }
}

impl VmStore {
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
}
