//! Split test module.

use crate::*;
use bridgevm_qemu::QmpEvent;
use std::fs;
use std::os::unix::process::ExitStatusExt;
use std::process::Output;

use super::helpers::*;

#[test]
fn corrupt_deletion_metadata_surfaces_on_list_and_get() {
    let store = temp_store();
    let bundle = store.create_vm(&manifest("dev")).unwrap();
    fs::write(deletion_metadata_path(&bundle), "{not json").unwrap();

    assert!(matches!(store.list_vms(), Err(StorageError::Json(_))));
    assert!(matches!(store.get_vm("dev"), Err(StorageError::Json(_))));
}

#[test]
fn guest_tools_runner_metadata_points_at_transport_files_without_token_value() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();

    let token = store.guest_tools_token("dev").unwrap();
    let metadata = store.guest_tools_runner_metadata("dev").unwrap();
    let bundle = store.bundle_path("dev");

    assert_eq!(metadata.transport, "virtio-serial");
    assert_eq!(metadata.channel_name, "org.bridgevm.guest-tools.0");
    assert_eq!(
        metadata.socket_path,
        bundle.join("metadata").join("guest-tools.sock")
    );
    assert_eq!(
        metadata.token_path,
        bundle.join("metadata").join("guest-tools-token.json")
    );
    assert_eq!(metadata.token_created_at_unix, token.created_at_unix);
    assert_ne!(metadata.socket_path.display().to_string(), token.token);
    assert_ne!(metadata.token_path.display().to_string(), token.token);
}

#[test]
fn writes_guest_tools_runtime_metadata() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    let metadata = GuestToolsRuntimeMetadata {
        connected: true,
        guest_os: Some("linux".to_string()),
        agent_version: Some("1.0.0".to_string()),
        capabilities: vec!["heartbeat".to_string(), "guest-ip".to_string()],
        last_heartbeat_at_unix: Some(now_unix()),
        guest_ip_addresses: vec![GuestToolsIpAddressMetadata {
            address: "10.0.2.15".to_string(),
            interface: Some("eth0".to_string()),
        }],
        shared_folders: vec![GuestToolsSharedFolderMetadata {
            name: "workspace".to_string(),
            host_path_token: "share-token-1".to_string(),
            mounted_at_unix: now_unix(),
        }],
        metrics: Some(GuestToolsMetricsMetadata {
            cpu_percent: 12,
            memory_used_mib: 256,
            updated_at_unix: now_unix(),
        }),
        last_command_result: Some(GuestToolsCommandResultMetadata {
            request_id: "clipboard-1".to_string(),
            capability: Some("clipboard".to_string()),
            ok: true,
            error_code: None,
            message: Some("accepted".to_string()),
            result: Some(serde_json::json!({
                "text_length": 8,
                "changed": true
            })),
            metadata: Some(serde_json::json!({
                "handler": "clipboard"
            })),
            completed_at_unix: now_unix(),
        }),
        agent_update: Some(GuestToolsAgentUpdateMetadata {
            current_version: "1.0.0".to_string(),
            available_version: "1.1.0".to_string(),
            download_url: Some("https://updates.example/bridgevm-tools".to_string()),
            signature: Some("sig".to_string()),
            observed_at_unix: now_unix(),
        }),
        clipboard: Some(GuestToolsClipboardMetadata {
            text: "guest text".to_string(),
            updated_at_unix: now_unix(),
        }),
        updated_at_unix: now_unix(),
    };

    assert_eq!(store.guest_tools_runtime_metadata("dev").unwrap(), None);
    store
        .write_guest_tools_runtime_metadata("dev", &metadata)
        .unwrap();
    assert_eq!(
        store.guest_tools_runtime_metadata("dev").unwrap(),
        Some(metadata)
    );
}

#[test]
fn imports_live_evidence_bundle_into_vm_metadata() {
    let store = temp_store();
    let bundle = store.create_vm(&manifest("dev")).unwrap();
    let source = store.root().join("source-live-evidence");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("SUMMARY.txt"), "live evidence").unwrap();
    fs::write(source.join("viewer-frame.png"), "frame").unwrap();

    assert_eq!(store.live_evidence_metadata("dev").unwrap(), None);
    let metadata = store.import_live_evidence_bundle("dev", &source).unwrap();

    assert_eq!(metadata.vm, "dev");
    assert_eq!(metadata.source, source);
    assert_eq!(
        metadata.preserved_path,
        bundle.join("metadata").join("live-evidence").join("latest")
    );
    assert!(metadata.preserved_path.join("SUMMARY.txt").exists());
    assert!(metadata.preserved_path.join("viewer-frame.png").exists());
    assert!(metadata.copied_files.contains(&"SUMMARY.txt".to_string()));
    assert_eq!(store.live_evidence_metadata("dev").unwrap(), Some(metadata));

    store.clear_live_evidence_metadata("dev").unwrap();
    assert_eq!(store.live_evidence_metadata("dev").unwrap(), None);
    assert!(!bundle.join("metadata").join("live-evidence").exists());
}

#[test]
fn reads_legacy_guest_tools_runtime_without_shared_folders() {
    let store = temp_store();
    let bundle = store.create_vm(&manifest("dev")).unwrap();
    fs::create_dir_all(bundle.join("metadata")).unwrap();
    fs::write(
        guest_tools_runtime_path(&bundle),
        r#"{
  "connected": true,
  "guest_os": "linux",
  "agent_version": "1.0.0",
  "capabilities": ["heartbeat"],
  "last_heartbeat_at_unix": 1,
  "guest_ip_addresses": [],
  "metrics": null,
  "updated_at_unix": 2
}"#,
    )
    .unwrap();

    let runtime = store
        .guest_tools_runtime_metadata("dev")
        .unwrap()
        .expect("runtime metadata");

    assert!(runtime.connected);
    assert!(runtime.shared_folders.is_empty());
    assert!(runtime.last_command_result.is_none());
    assert!(runtime.clipboard.is_none());
}

#[test]
fn writes_qmp_supervisor_metadata() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    let metadata = QmpSupervisorMetadata {
        events: vec![QmpEvent {
            name: "BLOCK_JOB_COMPLETED".to_string(),
            data: Some(serde_json::json!({"device":"drive0"})),
        }],
        terminal_event: None,
        envelopes_read: 1,
        limit_reached: false,
        updated_at_unix: now_unix(),
    };

    assert_eq!(store.qmp_supervisor_metadata("dev").unwrap(), None);
    store
        .write_qmp_supervisor_metadata("dev", &metadata)
        .unwrap();
    assert_eq!(
        store.qmp_supervisor_metadata("dev").unwrap(),
        Some(metadata)
    );
    assert!(store
        .bundle_path("dev")
        .join("metadata")
        .join("qmp-supervisor.json")
        .exists());
}

#[test]
fn rejects_duplicate_and_missing_snapshots() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    store
        .create_snapshot("dev", "before-upgrade", SnapshotKind::Disk)
        .unwrap();

    let duplicate = store
        .create_snapshot("dev", "before-upgrade", SnapshotKind::Disk)
        .unwrap_err();
    assert!(matches!(
        duplicate,
        StorageError::SnapshotAlreadyExists { .. }
    ));

    let missing = store.restore_snapshot("dev", "missing").unwrap_err();
    assert!(matches!(missing, StorageError::SnapshotNotFound { .. }));
}

#[test]
fn suspend_snapshot_does_not_prepare_disk_chain_metadata() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    store
        .create_snapshot("dev", "paused", SnapshotKind::Suspend)
        .unwrap();

    assert!(store
        .snapshot_disk_metadata("dev", "paused")
        .unwrap()
        .is_none());
    let image = store
        .snapshot_suspend_image_metadata("dev", "paused")
        .unwrap()
        .expect("suspend image metadata");
    assert_eq!(image.snapshot, "paused");
    assert_eq!(image.image_format, "bridgevm-suspend-image-v1");
    assert!(!image.image_exists);
    assert!(image.image_path.ends_with("suspend-images/paused.bin"));
    assert!(store
        .bundle_path("dev")
        .join("metadata")
        .join("suspend-images")
        .join("paused.json")
        .exists());
}

#[test]
fn application_consistent_snapshot_records_not_ready_preflight_without_runtime() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    let snapshot = store
        .create_snapshot("dev", "app-ready", SnapshotKind::ApplicationConsistent)
        .unwrap();

    assert_eq!(snapshot.kind, SnapshotKind::ApplicationConsistent);
    assert!(store
        .snapshot_disk_metadata("dev", "app-ready")
        .unwrap()
        .is_none());
    let preflight = store
        .application_consistent_snapshot_preflight_metadata("dev", "app-ready")
        .unwrap()
        .expect("application-consistent preflight metadata");
    assert!(!preflight.connected);
    assert!(!preflight.ready);
    assert_eq!(
        preflight.required_capabilities,
        vec!["fs-freeze".to_string(), "fs-thaw".to_string()]
    );
    assert_eq!(
        preflight.missing_capabilities,
        vec!["fs-freeze".to_string(), "fs-thaw".to_string()]
    );
    assert!(preflight.available_capabilities.is_empty());
    assert!(preflight.runtime_updated_at_unix.is_none());
    assert!(preflight
        .planned_freeze_semantics
        .contains("daemon-owned guest-tools fs-freeze request"));
    assert!(preflight
        .planned_thaw_semantics
        .contains("daemon-owned guest-tools fs-thaw request"));
    assert!(store
        .bundle_path("dev")
        .join("metadata")
        .join("application-consistent-snapshots")
        .join("app-ready.json")
        .exists());
}

#[test]
fn application_consistent_snapshot_records_ready_preflight_from_runtime() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    let runtime = GuestToolsRuntimeMetadata {
        connected: true,
        guest_os: Some("linux".to_string()),
        agent_version: Some("1.0.0".to_string()),
        capabilities: vec![
            "heartbeat".to_string(),
            "fs-freeze".to_string(),
            "fs-thaw".to_string(),
        ],
        last_heartbeat_at_unix: Some(3),
        guest_ip_addresses: Vec::new(),
        shared_folders: Vec::new(),
        metrics: None,
        last_command_result: None,
        agent_update: None,
        clipboard: None,
        updated_at_unix: 4,
    };
    store
        .write_guest_tools_runtime_metadata("dev", &runtime)
        .unwrap();

    store
        .create_snapshot("dev", "app-ready", SnapshotKind::ApplicationConsistent)
        .unwrap();
    let preflight = store
        .application_consistent_snapshot_preflight_metadata("dev", "app-ready")
        .unwrap()
        .expect("application-consistent preflight metadata");

    assert!(preflight.connected);
    assert!(preflight.ready);
    assert!(preflight.missing_capabilities.is_empty());
    assert_eq!(preflight.runtime_updated_at_unix, Some(4));
    assert_eq!(preflight.available_capabilities, runtime.capabilities);
}

#[test]
fn restore_suspend_snapshot_requires_recorded_image() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    store
        .transition_state("dev", VmRuntimeState::Running)
        .unwrap();
    store
        .transition_state("dev", VmRuntimeState::Suspended)
        .unwrap();
    store
        .create_snapshot("dev", "paused", SnapshotKind::Suspend)
        .unwrap();

    let missing = store.restore_snapshot("dev", "paused").unwrap_err();
    assert!(matches!(
        missing,
        StorageError::SnapshotSuspendImageMissing(_)
    ));
    assert!(
        !store
            .snapshot_suspend_image_metadata("dev", "paused")
            .unwrap()
            .unwrap()
            .image_exists
    );
}

#[test]
fn restore_suspend_snapshot_records_suspend_image_metadata() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    store
        .transition_state("dev", VmRuntimeState::Running)
        .unwrap();
    store
        .transition_state("dev", VmRuntimeState::Suspended)
        .unwrap();
    store
        .create_snapshot("dev", "paused", SnapshotKind::Suspend)
        .unwrap();
    let image = store
        .snapshot_suspend_image_metadata("dev", "paused")
        .unwrap()
        .unwrap();
    fs::write(&image.image_path, b"fake suspend image").unwrap();

    let restore = store.restore_snapshot("dev", "paused").unwrap();
    assert_eq!(restore.snapshot, "paused");
    assert_eq!(restore.restored_state, VmRuntimeState::Suspended);
    assert!(restore.active_disk.is_none());
    let restored_image = restore.suspend_image.as_ref().expect("suspend image");
    assert!(restored_image.image_exists);
    assert_eq!(restored_image.image_path, image.image_path);
    assert_eq!(store.last_restore("dev").unwrap(), Some(restore));
}

#[test]
fn repairs_missing_core_and_snapshot_metadata_without_creating_disks() {
    let store = temp_store();
    let bundle = store.create_vm(&manifest("dev")).unwrap();
    store
        .create_snapshot("dev", "disk-snap", SnapshotKind::Disk)
        .unwrap();
    store
        .create_snapshot("dev", "suspend-snap", SnapshotKind::Suspend)
        .unwrap();
    store
        .create_snapshot("dev", "app-snap", SnapshotKind::ApplicationConsistent)
        .unwrap();

    let state_path = bundle.join("metadata").join("state.json");
    let snapshots_path = bundle.join("metadata").join("snapshots.json");
    let active_disk_path = bundle.join("metadata").join("active-disk.json");
    let token_path = guest_tools_token_path(&bundle);
    let primary_disk_path = bundle.join("metadata").join("primary-disk.json");
    let disk_snapshot_path = snapshot_disk_metadata_path(&bundle, "disk-snap");
    let suspend_path = snapshot_suspend_image_metadata_path(&bundle, "suspend-snap");
    let app_path = application_consistent_snapshot_preflight_path(&bundle, "app-snap");
    let disk_path = bundle.join("disks").join("root.qcow2");

    fs::remove_file(&state_path).unwrap();
    fs::remove_file(&active_disk_path).unwrap();
    fs::remove_file(&token_path).unwrap();
    fs::remove_file(&disk_snapshot_path).unwrap();
    fs::remove_file(&suspend_path).unwrap();
    fs::remove_file(&app_path).unwrap();
    assert!(snapshots_path.exists());
    assert!(!disk_path.exists());

    let repair = store.repair_metadata("dev").unwrap();

    assert!(repair.repaired);
    assert_eq!(repair.vm, "dev");
    assert_eq!(repair.bundle, bundle);
    assert!(repair
        .actions
        .iter()
        .any(|action| action.path == state_path));
    assert!(repair
        .actions
        .iter()
        .any(|action| action.path == active_disk_path));
    assert!(repair
        .actions
        .iter()
        .any(|action| action.path == token_path));
    assert!(repair
        .actions
        .iter()
        .any(|action| action.path == primary_disk_path));
    assert!(repair
        .actions
        .iter()
        .any(|action| action.path == disk_snapshot_path));
    assert!(repair
        .actions
        .iter()
        .any(|action| action.path == suspend_path));
    assert!(repair.actions.iter().any(|action| action.path == app_path));

    assert_eq!(store.state("dev").unwrap().state, VmRuntimeState::Stopped);
    assert!(store
        .active_disk("dev")
        .unwrap()
        .path
        .ends_with("disks/root.qcow2"));
    assert_eq!(store.guest_tools_token("dev").unwrap().token.len(), 64);
    assert!(store
        .snapshot_disk_metadata("dev", "disk-snap")
        .unwrap()
        .is_some());
    assert!(store
        .snapshot_suspend_image_metadata("dev", "suspend-snap")
        .unwrap()
        .is_some());
    assert!(store
        .application_consistent_snapshot_preflight_metadata("dev", "app-snap")
        .unwrap()
        .is_some());
    assert!(primary_disk_path.exists());
    assert!(!disk_path.exists());
}

#[test]
fn repair_metadata_is_noop_when_metadata_is_healthy() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    store.prepare_primary_disk("dev").unwrap();
    store
        .create_snapshot("dev", "disk-snap", SnapshotKind::Disk)
        .unwrap();

    let repair = store.repair_metadata("dev").unwrap();

    assert!(!repair.repaired);
    assert!(repair.actions.is_empty());
}

#[test]
fn repair_metadata_reports_corrupt_json_without_replacing_it() {
    let store = temp_store();
    let bundle = store.create_vm(&manifest("dev")).unwrap();
    let token_path = guest_tools_token_path(&bundle);
    fs::write(&token_path, b"not json").unwrap();

    let error = store.repair_metadata("dev").unwrap_err();

    assert!(matches!(error, StorageError::Json(_)));
    assert_eq!(fs::read(&token_path).unwrap(), b"not json");
}

#[test]
fn concurrent_snapshot_creates_keep_valid_snapshot_metadata() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();

    let first = store.clone();
    let second = store.clone();
    let first = std::thread::spawn(move || {
        first
            .create_snapshot("dev", "first", SnapshotKind::Disk)
            .unwrap();
    });
    let second = std::thread::spawn(move || {
        second
            .create_snapshot("dev", "second", SnapshotKind::Suspend)
            .unwrap();
    });

    first.join().unwrap();
    second.join().unwrap();

    let snapshots = store.snapshots("dev").unwrap();
    assert_eq!(snapshots.len(), 2);
    assert!(snapshots.iter().any(|snapshot| snapshot.name == "first"));
    assert!(snapshots.iter().any(|snapshot| snapshot.name == "second"));
}

#[test]
fn refuses_snapshot_disk_create_when_backing_is_missing() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    store
        .create_snapshot("dev", "before-upgrade", SnapshotKind::Disk)
        .unwrap();

    let error = store
        .create_snapshot_disk_with("dev", "before-upgrade", |_program, _args| {
            panic!("missing backing should fail before qemu-img")
        })
        .unwrap_err();

    assert!(matches!(error, StorageError::SnapshotDiskBackingMissing(_)));
}

#[test]
fn creates_snapshot_overlay_with_injected_qemu_img_runner() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    let primary = store.prepare_primary_disk("dev").unwrap();
    fs::write(&primary.path, b"fake backing").unwrap();
    store
        .create_snapshot("dev", "before-upgrade", SnapshotKind::Disk)
        .unwrap();

    let create = store
        .create_snapshot_disk_with("dev", "before-upgrade", |program, args| {
            assert_eq!(program, "qemu-img");
            assert_eq!(args[..6], ["create", "-f", "qcow2", "-F", "qcow2", "-b"]);
            fs::write(&args[7], b"fake overlay")?;
            Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: b"overlay created\n".to_vec(),
                stderr: Vec::new(),
            })
        })
        .unwrap();

    assert!(create.executed);
    assert!(create.disk.overlay_exists);
    assert!(create.disk.backing_exists);
    assert_eq!(create.stdout, "overlay created\n");
    assert!(store
        .bundle_path("dev")
        .join("metadata")
        .join("snapshot-disks")
        .join("before-upgrade-create.json")
        .exists());
    let active = store.active_disk("dev").unwrap();
    assert_eq!(active.source, ActiveDiskSource::SnapshotOverlay);
    assert_eq!(active.snapshot.as_deref(), Some("before-upgrade"));
    assert_eq!(active.path, create.disk.overlay_path);
    let chain = store.snapshot_chain("dev").unwrap();
    assert_eq!(chain.active_disk, active);
    assert_eq!(chain.disks.len(), 1);
    assert_eq!(chain.disks[0].snapshot, "before-upgrade");
}

#[test]
fn skips_snapshot_disk_create_when_overlay_exists() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    let primary = store.prepare_primary_disk("dev").unwrap();
    fs::write(&primary.path, b"fake backing").unwrap();
    store
        .create_snapshot("dev", "before-upgrade", SnapshotKind::Disk)
        .unwrap();
    let disk = store
        .snapshot_disk_metadata("dev", "before-upgrade")
        .unwrap()
        .unwrap();
    fs::write(&disk.overlay_path, b"fake overlay").unwrap();

    let create = store
        .create_snapshot_disk_with("dev", "before-upgrade", |_program, _args| {
            panic!("existing overlay should skip qemu-img")
        })
        .unwrap();

    assert!(!create.executed);
    assert!(create.disk.overlay_exists);
    let active = store.active_disk("dev").unwrap();
    assert_eq!(active.source, ActiveDiskSource::SnapshotOverlay);
    assert_eq!(active.path, disk.overlay_path);
}

#[test]
fn snapshot_chain_uses_active_disk_and_restore_rewinds_to_backing() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    let primary = store.prepare_primary_disk("dev").unwrap();
    fs::write(&primary.path, b"fake backing").unwrap();
    store
        .create_snapshot("dev", "base", SnapshotKind::Disk)
        .unwrap();
    let base = store
        .create_snapshot_disk_with("dev", "base", |_program, args| {
            fs::write(&args[7], b"fake base overlay")?;
            Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        })
        .unwrap();

    store
        .create_snapshot("dev", "after-base", SnapshotKind::Disk)
        .unwrap();
    let after_base = store
        .snapshot_disk_metadata("dev", "after-base")
        .unwrap()
        .unwrap();
    assert_eq!(after_base.backing_path, base.disk.overlay_path);
    assert_eq!(after_base.backing_format, "qcow2");

    let restore = store.restore_snapshot("dev", "after-base").unwrap();
    let active = restore.active_disk.expect("disk restore active disk");
    assert_eq!(active.source, ActiveDiskSource::SnapshotBacking);
    assert_eq!(active.snapshot.as_deref(), Some("after-base"));
    assert_eq!(active.path, base.disk.overlay_path);
    assert_eq!(store.active_disk("dev").unwrap(), active);
}

#[test]
fn full_clone_rebases_copied_active_and_snapshot_disk_metadata() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    let primary = store.prepare_primary_disk("dev").unwrap();
    fs::write(&primary.path, b"fake backing").unwrap();
    store
        .create_snapshot("dev", "base", SnapshotKind::Disk)
        .unwrap();
    let source_overlay = store
        .create_snapshot_disk_with("dev", "base", |_program, args| {
            fs::write(&args[7], b"fake overlay")?;
            Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        })
        .unwrap()
        .disk
        .overlay_path;

    let clone = store.clone_vm("dev", "dev-copy", false).unwrap();
    assert!(!clone.linked);
    let clone_bundle = store.bundle_path("dev-copy");
    let active = store.active_disk("dev-copy").unwrap();
    assert_eq!(active.source, ActiveDiskSource::SnapshotOverlay);
    assert!(active.path.starts_with(&clone_bundle));
    assert!(active.path.ends_with("disks/snapshots/base.qcow2"));
    assert!(active.exists);
    assert_ne!(active.path, source_overlay);

    let disk = store
        .snapshot_disk_metadata("dev-copy", "base")
        .unwrap()
        .expect("copied snapshot disk metadata");
    assert!(disk.overlay_path.starts_with(&clone_bundle));
    assert!(disk.backing_path.starts_with(&clone_bundle));
    assert!(disk.overlay_exists);
    assert!(disk.backing_exists);
    assert_eq!(
        disk.create_command[7],
        disk.backing_path.display().to_string()
    );
    assert_eq!(
        disk.create_command[8],
        disk.overlay_path.display().to_string()
    );
}

#[test]
fn linked_clone_creates_overlay_backed_by_source_active_disk() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    let primary = store.prepare_primary_disk("dev").unwrap();
    fs::write(&primary.path, b"fake backing").unwrap();
    store
        .create_snapshot("dev", "source-only", SnapshotKind::Disk)
        .unwrap();

    let clone = store
        .clone_vm_with("dev", "dev-linked", true, |program, args| {
            assert_eq!(program, "qemu-img");
            assert_eq!(args[..6], ["create", "-f", "qcow2", "-F", "qcow2", "-b"]);
            assert_eq!(args[6], primary.path.display().to_string());
            fs::write(&args[7], b"fake linked overlay")?;
            Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: b"linked overlay created\n".to_vec(),
                stderr: Vec::new(),
            })
        })
        .unwrap();

    let clone_bundle = store.bundle_path("dev-linked");
    assert!(clone.linked);
    assert_eq!(clone.backing_path.as_ref(), Some(&primary.path));
    assert_eq!(clone.backing_format.as_deref(), Some("qcow2"));
    assert_eq!(
        clone.create_command.as_ref().unwrap()[..7],
        ["qemu-img", "create", "-f", "qcow2", "-F", "qcow2", "-b"]
    );
    let (_, manifest) = store.get_vm("dev-linked").unwrap();
    assert_eq!(manifest.name, "dev-linked");
    assert_eq!(manifest.network.hostname, "dev-linked.bridgevm.local");
    assert_eq!(manifest.storage.primary.path, "disks/root.qcow2");
    assert_eq!(manifest.storage.primary.format, "qcow2");

    let active = store.active_disk("dev-linked").unwrap();
    assert_eq!(active.source, ActiveDiskSource::Primary);
    assert_eq!(active.path, clone_bundle.join("disks").join("root.qcow2"));
    assert_eq!(fs::read(&active.path).unwrap(), b"fake linked overlay");
    assert!(store.snapshots("dev-linked").unwrap().is_empty());
    assert!(store.snapshot_chain("dev-linked").unwrap().disks.is_empty());
    assert!(!clone_bundle
        .join("metadata")
        .join("snapshot-disks")
        .exists());
    assert!(!clone_bundle.join("suspend-images").exists());
}
