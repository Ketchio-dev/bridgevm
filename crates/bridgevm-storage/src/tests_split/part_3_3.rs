//! Split test module.

use crate::*;
use bridgevm_config::ConfigError;
use bridgevm_config::SCHEMA_VERSION;
use std::fs;
use std::os::unix::process::ExitStatusExt;
use std::process::Output;

use super::helpers::*;

#[test]
fn creates_primary_qcow2_disk_with_injected_qemu_img_runner() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();

    let create = store
        .create_primary_disk_with("dev", |program, args| {
            assert_eq!(program, "qemu-img");
            assert_eq!(args[..3], ["create", "-f", "qcow2"]);
            fs::write(&args[3], b"fake qcow2")?;
            Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: b"created\n".to_vec(),
                stderr: Vec::new(),
            })
        })
        .unwrap();

    assert!(create.executed);
    assert_eq!(
        create.command.as_ref().unwrap()[..3],
        ["qemu-img", "create", "-f"]
    );
    assert!(create.preparation.exists);
    assert!(!create.preparation.created);
    assert_eq!(create.stdout, "created\n");
    assert!(store
        .bundle_path("dev")
        .join("metadata")
        .join("last-disk-create.json")
        .exists());
}

#[test]
fn reports_failed_primary_disk_create_command() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();

    let error = store
        .create_primary_disk_with("dev", |_program, _args| {
            Ok(Output {
                status: std::process::ExitStatus::from_raw(1 << 8),
                stdout: Vec::new(),
                stderr: b"qemu-img failed".to_vec(),
            })
        })
        .unwrap_err();

    let StorageError::DiskCreateFailed {
        command,
        status,
        stderr,
    } = error
    else {
        panic!("expected disk create failure");
    };
    assert_eq!(command[..3], ["qemu-img", "create", "-f"]);
    assert!(status.contains('1'));
    assert_eq!(stderr, "qemu-img failed");
}

#[test]
fn skips_primary_disk_create_when_prepare_already_made_raw_disk() {
    let store = temp_store();
    let mut raw_manifest = manifest("raw-dev");
    raw_manifest.storage.primary.format = "raw".to_string();
    raw_manifest.storage.primary.path = "disks/root.raw".to_string();
    raw_manifest.storage.primary.size = "1MiB".to_string();
    store.create_vm(&raw_manifest).unwrap();

    let create = store
        .create_primary_disk_with("raw-dev", |_program, _args| {
            panic!("raw disk creation should be handled by prepare")
        })
        .unwrap();

    assert!(!create.executed);
    assert!(create.command.is_none());
    assert!(create.preparation.exists);
    assert!(create.preparation.created);
}

#[test]
fn inspects_existing_primary_disk_with_injected_qemu_img_runner() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    store
        .create_primary_disk_with("dev", |_program, args| {
            fs::write(&args[3], b"fake qcow2")?;
            Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        })
        .unwrap();

    let inspect = store
        .inspect_primary_disk_with("dev", |program, args| {
            assert_eq!(program, "qemu-img");
            assert_eq!(args[0], "info");
            assert_eq!(args[1], "--output=json");
            Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: br#"{"format":"qcow2","virtual-size":85899345920}"#.to_vec(),
                stderr: Vec::new(),
            })
        })
        .unwrap();

    assert_eq!(inspect.command[..2], ["qemu-img", "info"]);
    assert_eq!(inspect.info["format"], "qcow2");
    assert_eq!(inspect.info["virtual-size"], 80 * 1024 * 1024 * 1024_u64);
    let metadata_path = store
        .bundle_path("dev")
        .join("metadata")
        .join("last-disk-inspect.json");
    assert!(metadata_path.exists());
    let recorded: DiskInspectMetadata =
        serde_json::from_str(&fs::read_to_string(metadata_path).unwrap()).unwrap();
    assert_eq!(
        recorded.inspect_duration_microseconds,
        inspect.inspect_duration_microseconds
    );
}

#[test]
fn rejects_inspection_when_primary_disk_is_missing() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();

    let error = store
        .inspect_primary_disk_with("dev", |_program, _args| {
            panic!("missing disk should fail before qemu-img info")
        })
        .unwrap_err();

    assert!(matches!(error, StorageError::DiskMissing(_)));
}

#[test]
fn reports_failed_primary_disk_inspection_command() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    store
        .create_primary_disk_with("dev", |_program, args| {
            fs::write(&args[3], b"fake qcow2")?;
            Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        })
        .unwrap();

    let error = store
        .inspect_primary_disk_with("dev", |_program, _args| {
            Ok(Output {
                status: std::process::ExitStatus::from_raw(1 << 8),
                stdout: Vec::new(),
                stderr: b"bad image".to_vec(),
            })
        })
        .unwrap_err();

    let StorageError::DiskInspectFailed {
        command,
        status,
        stderr,
    } = error
    else {
        panic!("expected disk inspect failure");
    };
    assert_eq!(command[..2], ["qemu-img", "info"]);
    assert!(status.contains('1'));
    assert_eq!(stderr, "bad image");
}

#[test]
fn verifies_active_qcow2_disk_with_injected_qemu_img_runner() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    store
        .create_primary_disk_with("dev", |_program, args| {
            fs::write(&args[3], b"fake qcow2")?;
            Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        })
        .unwrap();

    let verify = store
        .verify_active_disk_with("dev", |program, args| {
            assert_eq!(program, "qemu-img");
            assert_eq!(args[0], "check");
            assert_eq!(args[1], "--output=json");
            Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: br#"{"image-end-offset":4096,"check-errors":0}"#.to_vec(),
                stderr: Vec::new(),
            })
        })
        .unwrap();

    assert_eq!(verify.command[..2], ["qemu-img", "check"]);
    assert_eq!(verify.active_disk.source, ActiveDiskSource::Primary);
    assert_eq!(verify.report["check-errors"], 0);
    assert_eq!(verify.report["image-end-offset"], 4096);
    let metadata_path = store
        .bundle_path("dev")
        .join("metadata")
        .join("last-disk-verify.json");
    assert!(metadata_path.exists());
    let recorded: DiskVerifyMetadata =
        serde_json::from_str(&fs::read_to_string(metadata_path).unwrap()).unwrap();
    assert_eq!(
        recorded.verify_duration_microseconds,
        verify.verify_duration_microseconds
    );
}

#[test]
fn rejects_verification_for_raw_disks() {
    let store = temp_store();
    let mut raw_manifest = manifest("raw-dev");
    raw_manifest.storage.primary.format = "raw".to_string();
    raw_manifest.storage.primary.path = "disks/root.raw".to_string();
    raw_manifest.storage.primary.size = "1MiB".to_string();
    store.create_vm(&raw_manifest).unwrap();

    let error = store
        .verify_active_disk_with("raw-dev", |_program, _args| {
            panic!("raw disk verification should fail before qemu-img check")
        })
        .unwrap_err();

    assert!(matches!(error, StorageError::DiskVerifyUnsupportedRaw(_)));
}

#[test]
fn reports_failed_disk_verification_command() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    store
        .create_primary_disk_with("dev", |_program, args| {
            fs::write(&args[3], b"fake qcow2")?;
            Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        })
        .unwrap();

    let error = store
        .verify_active_disk_with("dev", |_program, _args| {
            Ok(Output {
                status: std::process::ExitStatus::from_raw(2 << 8),
                stdout: br#"{"check-errors":1}"#.to_vec(),
                stderr: b"check failed".to_vec(),
            })
        })
        .unwrap_err();

    let StorageError::DiskVerifyFailed {
        command,
        status,
        stderr,
    } = error
    else {
        panic!("expected disk verify failure");
    };
    assert_eq!(command[..2], ["qemu-img", "check"]);
    assert!(status.contains('2'));
    assert_eq!(stderr, "check failed");
}

#[test]
fn compacts_active_qcow2_disk_with_injected_qemu_img_runner() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    store
        .create_primary_disk_with("dev", |_program, args| {
            fs::write(&args[3], b"fake qcow2 image with slack space")?;
            Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        })
        .unwrap();

    let compact = store
        .compact_active_disk_with("dev", |program, args| {
            assert_eq!(program, "qemu-img");
            assert_eq!(args[..3], ["convert", "-O", "qcow2"]);
            fs::write(&args[4], b"small qcow2")?;
            Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: b"compacted\n".to_vec(),
                stderr: Vec::new(),
            })
        })
        .unwrap();

    assert_eq!(compact.command[..3], ["qemu-img", "convert", "-O"]);
    assert_eq!(fs::read(&compact.active_disk.path).unwrap(), b"small qcow2");
    assert!(compact.backup_path.exists());
    assert!(!compact.temp_path.exists());
    assert!(compact.original_size_bytes > compact.compacted_size_bytes);
    assert_eq!(compact.stdout, "compacted\n");
    assert!(store
        .bundle_path("dev")
        .join("metadata")
        .join("last-disk-compact.json")
        .exists());
}

#[test]
fn rejects_compaction_for_raw_disks() {
    let store = temp_store();
    let mut raw_manifest = manifest("raw-dev");
    raw_manifest.storage.primary.format = "raw".to_string();
    raw_manifest.storage.primary.path = "disks/root.raw".to_string();
    raw_manifest.storage.primary.size = "1MiB".to_string();
    store.create_vm(&raw_manifest).unwrap();
    store.prepare_primary_disk("raw-dev").unwrap();

    let error = store
        .compact_active_disk_with("raw-dev", |_program, _args| {
            panic!("raw disk should fail before qemu-img convert")
        })
        .unwrap_err();

    assert!(matches!(error, StorageError::DiskCompactUnsupportedRaw(_)));
}

#[test]
fn reports_failed_disk_compaction_command() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    store
        .create_primary_disk_with("dev", |_program, args| {
            fs::write(&args[3], b"fake qcow2")?;
            Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        })
        .unwrap();

    let error = store
        .compact_active_disk_with("dev", |_program, _args| {
            Ok(Output {
                status: std::process::ExitStatus::from_raw(1 << 8),
                stdout: Vec::new(),
                stderr: b"convert failed".to_vec(),
            })
        })
        .unwrap_err();

    let StorageError::DiskCompactFailed {
        command,
        status,
        stderr,
    } = error
    else {
        panic!("expected disk compact failure");
    };
    assert_eq!(command[..3], ["qemu-img", "convert", "-O"]);
    assert!(status.contains('1'));
    assert_eq!(stderr, "convert failed");
}

#[test]
fn rejects_invalid_lifecycle_transition() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();

    let error = store
        .transition_state("dev", VmRuntimeState::Suspended)
        .unwrap_err();
    assert!(matches!(
        error,
        StorageError::InvalidStateTransition {
            from: VmRuntimeState::Stopped,
            to: VmRuntimeState::Suspended
        }
    ));
}

#[test]
fn manifest_migration_dry_run_does_not_write_receipt_or_backup() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();

    let migration = store.migrate_manifest("dev", true).unwrap();

    assert!(migration.dry_run);
    assert!(!migration.migrated);
    assert_eq!(migration.from_schema, SCHEMA_VERSION);
    assert_eq!(migration.to_schema, SCHEMA_VERSION);
    assert!(migration.backup_path.is_none());
    assert!(migration.receipt_path.is_none());
    let bundle = store.bundle_path("dev");
    assert!(!bundle
        .join("metadata")
        .join("manifest-before-migration.yaml")
        .exists());
    assert!(!bundle
        .join("metadata")
        .join("manifest-migration.json")
        .exists());
}

#[test]
fn manifest_migration_writes_receipt_and_backup_for_current_schema() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();

    let migration = store.migrate_manifest("dev", false).unwrap();

    assert!(!migration.dry_run);
    assert!(!migration.migrated);
    assert_eq!(migration.from_schema, SCHEMA_VERSION);
    assert_eq!(migration.to_schema, SCHEMA_VERSION);
    assert!(migration.backup_path.as_ref().unwrap().exists());
    assert!(migration.receipt_path.as_ref().unwrap().exists());
    let receipt =
        read_json_file::<VmManifestMigrationMetadata>(migration.receipt_path.as_ref().unwrap())
            .unwrap()
            .expect("manifest migration receipt");
    assert_eq!(receipt.vm, "dev");
    assert_eq!(receipt.manifest_path, migration.manifest_path);
}

#[test]
fn manifest_migration_rejects_future_schema_without_receipt() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    let bundle = store.bundle_path("dev");
    let manifest_path = bundle.join("manifest.yaml");
    let mut raw_manifest = fs::read_to_string(&manifest_path).unwrap();
    raw_manifest = raw_manifest.replace(SCHEMA_VERSION, "bridgevm.io/v99");
    fs::write(&manifest_path, raw_manifest).unwrap();

    let error = store.migrate_manifest("dev", false).unwrap_err();

    assert!(matches!(
        error,
        StorageError::Config(ConfigError::UnsupportedSchema { .. })
    ));
    assert!(!bundle
        .join("metadata")
        .join("manifest-before-migration.yaml")
        .exists());
    assert!(!bundle
        .join("metadata")
        .join("manifest-migration.json")
        .exists());
}

#[test]
fn manifest_migration_rejects_malformed_yaml_without_receipt() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    let bundle = store.bundle_path("dev");
    let manifest_path = bundle.join("manifest.yaml");
    fs::write(
        &manifest_path,
        "schemaVersion: bridgevm.io/v1\nname: [not-valid-yaml\n",
    )
    .unwrap();

    let error = store.migrate_manifest("dev", false).unwrap_err();

    assert!(matches!(error, StorageError::Config(ConfigError::Yaml(_))));
    assert!(!bundle
        .join("metadata")
        .join("manifest-before-migration.yaml")
        .exists());
    assert!(!bundle
        .join("metadata")
        .join("manifest-migration.json")
        .exists());
}

#[test]
fn manifest_migration_rejects_oversized_manifest_without_receipt() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    let bundle = store.bundle_path("dev");
    fs::write(
        bundle.join("manifest.yaml"),
        vec![b'x'; bridgevm_config::MAX_MANIFEST_BYTES as usize + 1],
    )
    .unwrap();

    let error = store.migrate_manifest("dev", false).unwrap_err();

    assert!(matches!(
        error,
        StorageError::Config(ConfigError::ManifestTooLarge { .. })
    ));
    assert!(!bundle
        .join("metadata")
        .join("manifest-before-migration.yaml")
        .exists());
    assert!(!bundle
        .join("metadata")
        .join("manifest-migration.json")
        .exists());
}

#[test]
fn manifest_migration_ignores_metadata_deleted_vm() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    let deletion = store.delete_vm_metadata_only("dev").unwrap();

    let error = store.migrate_manifest("dev", false).unwrap_err();

    assert!(matches!(error, StorageError::NotFound(name) if name == "dev"));
    assert!(deletion.metadata_path.exists());
    let bundle = store.bundle_path("dev");
    assert!(!bundle
        .join("metadata")
        .join("manifest-before-migration.yaml")
        .exists());
    assert!(!bundle
        .join("metadata")
        .join("manifest-migration.json")
        .exists());
}

#[test]
fn allows_stopping_suspended_vm() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    store
        .transition_state("dev", VmRuntimeState::Running)
        .unwrap();
    store
        .transition_state("dev", VmRuntimeState::Suspended)
        .unwrap();

    let state = store
        .transition_state("dev", VmRuntimeState::Stopped)
        .unwrap();
    assert_eq!(state.state, VmRuntimeState::Stopped);
}
