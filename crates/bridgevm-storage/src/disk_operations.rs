//! qemu-img create, inspect, verify and compact against the active disk, plus receipts.

use crate::*;
use bridgevm_qemu::QemuImgCommand;
use std::fs;
use std::process::Output;
use std::time::Instant;

impl VmStore {
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
}
