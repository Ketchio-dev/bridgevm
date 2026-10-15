//! Split test module.

use crate::*;
use std::fs;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::Output;

use super::helpers::*;

#[test]
fn linked_clone_requires_existing_source_active_disk() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();

    let error = store
        .clone_vm_with("dev", "dev-linked", true, |_program, _args| {
            panic!("missing backing should fail before qemu-img")
        })
        .unwrap_err();

    assert!(matches!(error, StorageError::DiskMissing(_)));
    assert!(!store.bundle_path("dev-linked").exists());
}

#[test]
fn linked_clone_reports_qemu_img_failure() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    let primary = store.prepare_primary_disk("dev").unwrap();
    fs::write(&primary.path, b"fake backing").unwrap();

    let error = store
        .clone_vm_with("dev", "dev-linked", true, |_program, _args| {
            Ok(Output {
                status: std::process::ExitStatus::from_raw(1 << 8),
                stdout: Vec::new(),
                stderr: b"qemu-img failed".to_vec(),
            })
        })
        .unwrap_err();

    let StorageError::LinkedCloneDiskCreateFailed {
        command,
        status,
        stderr,
    } = error
    else {
        panic!("expected linked clone qemu-img failure");
    };
    assert_eq!(
        command[..7],
        ["qemu-img", "create", "-f", "qcow2", "-F", "qcow2", "-b"]
    );
    assert!(status.contains('1'));
    assert_eq!(stderr, "qemu-img failed");
    assert!(!store.bundle_path("dev-linked").exists());
}

#[test]
fn exports_vm_bundle_copy_with_metadata() {
    let store = temp_store();
    let bundle = store.create_vm(&manifest("dev")).unwrap();
    store
        .create_snapshot("dev", "before-upgrade", SnapshotKind::Disk)
        .unwrap();
    fs::write(
        bundle.join("metadata").join("qmp.sock"),
        b"socket placeholder",
    )
    .unwrap();
    fs::write(bundle.join("metadata").join("export.lock"), b"locked").unwrap();
    let output = store.root().join("exports").join("dev.vmbridge");

    let export = store.export_vm("dev", &output).unwrap();
    assert_eq!(export.vm, "dev");
    assert_eq!(export.archive_format, "directory");
    assert!(export.copied_file_count >= 2);
    assert!(export.copied_files.contains(&"manifest.yaml".to_string()));
    assert!(export
        .copied_files
        .contains(&"metadata/snapshots.json".to_string()));
    assert!(export.manifest_preserved);
    assert!(export.metadata_preserved);
    assert!(output.join("manifest.yaml").exists());
    assert!(output.join("metadata").join("snapshots.json").exists());
    assert!(output.join("metadata").join("export.json").exists());
    assert!(!export
        .copied_files
        .contains(&"metadata/qmp.sock".to_string()));
    assert!(!export
        .copied_files
        .contains(&"metadata/export.lock".to_string()));
    assert!(!output.join("metadata").join("qmp.sock").exists());
    assert!(!output.join("metadata").join("export.lock").exists());

    let duplicate = store.export_vm("dev", &output).unwrap_err();
    assert!(matches!(duplicate, StorageError::ExportAlreadyExists(_)));
}

#[test]
fn rejects_export_output_at_or_inside_source_bundle() {
    let store = temp_store();
    let bundle = store.create_vm(&manifest("dev")).unwrap();

    let self_export = store.export_vm("dev", &bundle).unwrap_err();
    assert!(matches!(
        self_export,
        StorageError::ExportOutputInsideSource { .. }
    ));

    let nested_export = store
        .export_vm("dev", bundle.join("exports").join("dev.vmbridge"))
        .unwrap_err();
    assert!(matches!(
        nested_export,
        StorageError::ExportOutputInsideSource { .. }
    ));
}

#[cfg(unix)]
#[test]
fn rejects_symlinks_while_exporting_vm_bundle() {
    use std::os::unix::fs::symlink;

    let store = temp_store();
    let bundle = store.create_vm(&manifest("dev")).unwrap();
    symlink("manifest.yaml", bundle.join("manifest-link.yaml")).unwrap();
    let output = store.root().join("exports").join("dev.vmbridge");

    let error = store.export_vm("dev", &output).unwrap_err();
    assert!(matches!(error, StorageError::UnsupportedBundleEntry(_)));
}

#[test]
fn imports_exported_vm_bundle_with_optional_rename() {
    let source = temp_store();
    source.create_vm(&manifest("dev")).unwrap();
    source
        .create_snapshot("dev", "before-upgrade", SnapshotKind::Disk)
        .unwrap();
    let output = source.root().join("exports").join("dev.vmbridge");
    source.export_vm("dev", &output).unwrap();

    let target = temp_store();
    let import = target.import_vm(&output, Some("dev-copy")).unwrap();
    assert_eq!(import.vm, "dev-copy");
    assert_eq!(import.original_name, "dev");
    assert_eq!(import.requested_name.as_deref(), Some("dev-copy"));
    assert_eq!(import.archive_format, "directory");
    assert!(import.manifest_identity_rewritten);
    assert!(import.manifest_preserved);
    assert!(import.metadata_preserved);
    assert!(import.copied_files.contains(&"manifest.yaml".to_string()));
    assert!(import.output.join("manifest.yaml").exists());
    assert!(import.output.join("metadata").join("import.json").exists());

    let (_, manifest) = target.get_vm("dev-copy").unwrap();
    assert_eq!(manifest.name, "dev-copy");
    assert_eq!(manifest.network.hostname, "dev-copy.bridgevm.local");
    assert_eq!(target.snapshots("dev-copy").unwrap().len(), 1);
}

#[test]
fn exports_and_imports_tar_vm_bundle_with_optional_rename() {
    let source = temp_store();
    let bundle = source.create_vm(&manifest("dev")).unwrap();
    source
        .create_snapshot("dev", "before-upgrade", SnapshotKind::Disk)
        .unwrap();
    fs::write(
        bundle.join("metadata").join("qmp.sock"),
        b"socket placeholder",
    )
    .unwrap();
    fs::write(bundle.join("metadata").join("export.lock"), b"locked").unwrap();
    let output = source.root().join("exports").join("dev.tar");
    let export = source.export_vm("dev", &output).unwrap();
    assert_eq!(export.output, output);
    assert_eq!(export.archive_format, "tar");
    assert!(output.is_file());
    assert!(!export
        .copied_files
        .contains(&"metadata/qmp.sock".to_string()));
    assert!(!export
        .copied_files
        .contains(&"metadata/export.lock".to_string()));

    let target = temp_store();
    let import = target.import_vm(&output, Some("dev-copy")).unwrap();
    assert_eq!(import.vm, "dev-copy");
    assert_eq!(import.source, output);
    assert_eq!(import.original_name, "dev");
    assert_eq!(import.requested_name.as_deref(), Some("dev-copy"));
    assert_eq!(import.archive_format, "tar");
    assert!(import.manifest_identity_rewritten);
    assert!(import.copied_files.contains(&"manifest.yaml".to_string()));
    assert!(import
        .copied_files
        .contains(&"metadata/export.json".to_string()));
    assert!(import.output.join("manifest.yaml").exists());
    assert!(import.output.join("metadata").join("export.json").exists());
    assert!(import.output.join("metadata").join("import.json").exists());
    assert!(!import.output.join("metadata").join("qmp.sock").exists());
    assert!(!import.output.join("metadata").join("export.lock").exists());

    let (_, manifest) = target.get_vm("dev-copy").unwrap();
    assert_eq!(manifest.name, "dev-copy");
    assert_eq!(manifest.network.hostname, "dev-copy.bridgevm.local");
    assert_eq!(target.snapshots("dev-copy").unwrap().len(), 1);
}

#[test]
fn rejects_duplicate_tar_imports() {
    let source = temp_store();
    source.create_vm(&manifest("dev")).unwrap();
    let output = source.root().join("exports").join("dev.tar");
    source.export_vm("dev", &output).unwrap();

    let target = temp_store();
    target.import_vm(&output, None).unwrap();
    let duplicate = target.import_vm(&output, None).unwrap_err();
    assert!(matches!(duplicate, StorageError::AlreadyExists(_)));
}

#[test]
fn rejects_tar_import_with_parent_directory_entry() {
    let tar_path = unique_temp_path("bridgevm-parent-test").with_extension("tar");
    write_raw_tar_entry(&tar_path, "../manifest.yaml", b'0', None, b"name: evil\n");

    let store = temp_store();
    let error = store.import_vm(&tar_path, None).unwrap_err();
    let _ = fs::remove_file(&tar_path);
    assert!(matches!(error, StorageError::UnsafeArchiveEntry(_)));
}

#[test]
fn rejects_tar_import_with_symlink_entry() {
    let tar_path = unique_temp_path("bridgevm-symlink-test").with_extension("tar");
    {
        let file = fs::File::create(&tar_path).unwrap();
        let mut builder = tar::Builder::new(file);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        header.set_link_name("manifest.yaml").unwrap();
        header.set_cksum();
        builder
            .append_data(&mut header, "manifest-link.yaml", &[][..])
            .unwrap();
        builder.finish().unwrap();
    }

    let store = temp_store();
    let error = store.import_vm(&tar_path, None).unwrap_err();
    let _ = fs::remove_file(&tar_path);
    assert!(matches!(error, StorageError::UnsupportedBundleEntry(_)));
}

#[test]
fn rejects_duplicate_and_invalid_imports() {
    let source = temp_store();
    source.create_vm(&manifest("dev")).unwrap();
    let output = source.root().join("exports").join("dev.vmbridge");
    source.export_vm("dev", &output).unwrap();

    let target = temp_store();
    target.import_vm(&output, None).unwrap();
    let duplicate = target.import_vm(&output, None).unwrap_err();
    assert!(matches!(duplicate, StorageError::AlreadyExists(_)));

    let invalid = target.import_vm(target.root().join("missing.vmbridge"), None);
    assert!(matches!(invalid, Err(StorageError::InvalidImportBundle(_))));
}

#[test]
fn rejects_imports_from_destination_store_or_same_bundle() {
    let store = temp_store();
    let bundle = store.create_vm(&manifest("dev")).unwrap();

    let self_import = store.import_vm(&bundle, None);
    assert!(matches!(
        self_import,
        Err(StorageError::ImportPathConflict { .. })
    ));

    let output = store.root().join("exports").join("dev.vmbridge");
    store.export_vm("dev", &output).unwrap();
    let internal_import = store.import_vm(&output, Some("dev-copy"));
    assert!(matches!(
        internal_import,
        Err(StorageError::ImportPathConflict { .. })
    ));
}

#[test]
fn writes_runner_metadata() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    let metadata = RunnerMetadata {
        engine: "fullvm".to_string(),
        pid: Some(42),
        command: vec!["qemu-system-x86_64".to_string()],
        log_path: PathBuf::from("logs/qemu.log"),
        started_at_unix: now_unix(),
        dry_run: false,
        launch_spec_path: None,
        guest_tools: None,
        disk: None,
        active_disk: None,
        launch_readiness: Some(LaunchReadinessMetadata {
            ready: false,
            blockers: vec![LaunchReadinessBlockerMetadata {
                code: "missing-primary-disk".to_string(),
                message: "Primary disk is missing.".to_string(),
                path: Some(PathBuf::from("disks/root.qcow2")),
                capability: None,
            }],
        }),
        runtime_control: None,
    };

    store.write_runner_metadata("dev", &metadata).unwrap();
    assert_eq!(store.runner_metadata("dev").unwrap(), Some(metadata));

    store.clear_runner_metadata("dev").unwrap();
    assert_eq!(store.runner_metadata("dev").unwrap(), None);
}

#[test]
fn runner_metadata_rejects_oversized_json_before_decode() {
    let store = temp_store();
    let bundle = store.create_vm(&manifest("dev")).unwrap();
    let path = bundle.join("metadata").join("runner.json");
    fs::write(&path, vec![b'x'; MAX_METADATA_JSON_BYTES as usize + 1]).unwrap();

    let error = store.runner_metadata("dev").unwrap_err();
    assert!(matches!(
        error,
        StorageError::MetadataTooLarge {
            path: error_path,
            actual,
            maximum: MAX_METADATA_JSON_BYTES
        } if error_path == path && actual == MAX_METADATA_JSON_BYTES + 1
    ));
}

#[test]
fn runner_metadata_rejects_sparse_oversized_json_before_allocation() {
    let store = temp_store();
    let bundle = store.create_vm(&manifest("dev")).unwrap();
    let path = bundle.join("metadata").join("runner.json");
    let file = fs::File::create(&path).unwrap();
    file.set_len(512 * 1024 * 1024).unwrap();

    let error = store.runner_metadata("dev").unwrap_err();

    assert!(matches!(
        error,
        StorageError::MetadataTooLarge {
            path: error_path,
            actual: 536_870_912,
            maximum: MAX_METADATA_JSON_BYTES
        } if error_path == path
    ));
}

#[test]
fn writes_runtime_resource_policy_metadata() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    let metadata = RuntimeResourcePolicyMetadata {
        vm: "dev".to_string(),
        mode: "fast".to_string(),
        profile: "automatic".to_string(),
        visibility: RuntimeResourceVisibility::Background,
        state: VmRuntimeState::Running,
        on_battery: true,
        memory: "2048".to_string(),
        cpu: "1".to_string(),
        display_fps_cap: "10".to_string(),
        rationale: "Battery or background throttling active.".to_string(),
        live_applied: false,
        runtime_control_acknowledged: false,
        live_apply_blockers: vec![RuntimeResourcePolicyBlocker {
            code: "runtime-control-unavailable".to_string(),
            message: "No live runtime control channel is available.".to_string(),
        }],
        updated_at_unix: now_unix(),
    };

    store
        .write_runtime_resource_policy_metadata("dev", &metadata)
        .unwrap();
    assert_eq!(
        store.runtime_resource_policy_metadata("dev").unwrap(),
        Some(metadata)
    );
}

#[test]
fn prepares_primary_disk_metadata_for_qcow2_and_raw() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();

    let qcow2 = store.prepare_primary_disk("dev").unwrap();
    assert_eq!(qcow2.format, "qcow2");
    assert!(!qcow2.exists);
    assert!(!qcow2.created);
    assert_eq!(qcow2.size_bytes, Some(80 * 1024 * 1024 * 1024));
    assert_eq!(
        qcow2.create_command.as_ref().unwrap()[..3],
        ["qemu-img", "create", "-f"]
    );
    assert!(store
        .bundle_path("dev")
        .join("metadata")
        .join("primary-disk.json")
        .exists());

    let mut raw_manifest = manifest("raw-dev");
    raw_manifest.storage.primary.format = "raw".to_string();
    raw_manifest.storage.primary.path = "disks/root.raw".to_string();
    raw_manifest.storage.primary.size = "1MiB".to_string();
    store.create_vm(&raw_manifest).unwrap();

    let raw = store.prepare_primary_disk("raw-dev").unwrap();
    assert!(raw.exists);
    assert!(raw.created);
    assert_eq!(raw.size_bytes, Some(1024 * 1024));
    assert_eq!(fs::metadata(raw.path).unwrap().len(), 1024 * 1024);
}
#[path = "concurrency.rs"]
mod concurrency;
