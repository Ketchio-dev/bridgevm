//! Full and linked clone, metadata rebase, clone identity reset.

use crate::*;
use bridgevm_config::slug;
use bridgevm_config::VmManifest;
use bridgevm_qemu::QemuImgCommand;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Output;

pub(crate) fn rebase_copied_path(path: &Path, source: &Path, output: &Path) -> PathBuf {
    path.strip_prefix(source)
        .map(|relative| output.join(relative))
        .unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn rebase_snapshot_disk_metadata(
    metadata: &mut SnapshotDiskMetadata,
    source: &Path,
    output: &Path,
) {
    metadata.overlay_path = rebase_copied_path(&metadata.overlay_path, source, output);
    metadata.backing_path = rebase_copied_path(&metadata.backing_path, source, output);
    metadata.overlay_exists = metadata.overlay_path.exists();
    metadata.backing_exists = metadata.backing_path.exists();
    metadata.create_command = QemuImgCommand::create_backed_disk(
        &metadata.overlay_path,
        metadata.overlay_format.clone(),
        metadata.backing_format.clone(),
        &metadata.backing_path,
    )
    .render_shell_words();
}

impl VmStore {
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
}
