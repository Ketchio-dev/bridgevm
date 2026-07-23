//! Tests split so no file exceeds 1000 lines.

use crate::test_support::*;

#[test]
fn service_contract_serializes_as_stable_schema_marker() {
    let json = serde_json::to_value(BridgeVmServiceContract::json_over_uds()).unwrap();

    assert_eq!(
        json,
        serde_json::json!({
            "schema_id": "bridgevm.api/v1",
            "version": 1,
            "service": "bridgevm.api.v1.BridgeVmService",
            "request_type": "BridgeVmRequest",
            "response_type": "BridgeVmResponse",
            "transport": "json-ndjson-over-uds"
        })
    );
}

#[test]
fn doctor_request_and_response_round_trip_as_json() {
    let request = BridgeVmRequest::Doctor;
    let json = serde_json::to_string(&request).unwrap();
    assert_eq!(json, r#"{"type":"doctor"}"#);
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);

    let response = BridgeVmResponse::Doctor {
        store_root: PathBuf::from("/tmp/bridgevm"),
        vms_dir: PathBuf::from("/tmp/bridgevm/vms"),
        status: "OK".to_string(),
    };
    let json = serde_json::to_string(&response).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&json).unwrap(),
        serde_json::json!({
            "type": "doctor",
            "store_root": "/tmp/bridgevm",
            "vms_dir": "/tmp/bridgevm/vms",
            "status": "OK"
        })
    );
    let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(response, decoded);
}

#[test]
fn runtime_control_request_and_response_round_trip_as_json() {
    let request = BridgeVmRequest::RuntimeControl {
        name: "fast-linux".to_string(),
        command: "status".to_string(),
    };
    let json = serde_json::to_string(&request).unwrap();
    assert_eq!(
        json,
        r#"{"type":"runtime_control","name":"fast-linux","command":"status"}"#
    );
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);

    let response = BridgeVmResponse::RuntimeControl {
        control: RuntimeControlCommandRecord {
            vm: "fast-linux".to_string(),
            kind: "apple-vz-display".to_string(),
            socket_path: PathBuf::from("/tmp/bvm-vz-test.sock"),
            command: "status".to_string(),
            response: serde_json::json!({
                "ok": true,
                "state": "running",
                "display": {"width": 1024, "height": 768}
            }),
        },
    };
    let json = serde_json::to_string(&response).unwrap();
    let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(response, decoded);
}

#[test]
fn application_consistent_snapshot_response_round_trips_as_json() {
    let response = BridgeVmResponse::Snapshot {
        snapshot: SnapshotMetadata {
            name: "before-upgrade".to_string(),
            kind: SnapshotKind::ApplicationConsistent,
            created_at_unix: 1,
            vm_state: VmRuntimeState::Running,
        },
        disk: None,
        application_consistent_preflight: Some(ApplicationConsistentSnapshotPreflightMetadata {
            snapshot: "before-upgrade".to_string(),
            connected: true,
            required_capabilities: vec!["fs-freeze".to_string(), "fs-thaw".to_string()],
            available_capabilities: vec![
                "heartbeat".to_string(),
                "fs-freeze".to_string(),
                "fs-thaw".to_string(),
            ],
            missing_capabilities: Vec::new(),
            ready: true,
            planned_freeze_semantics: "daemon-owned guest-tools fs-freeze request".to_string(),
            planned_thaw_semantics: "daemon-owned guest-tools fs-thaw request".to_string(),
            runtime_updated_at_unix: Some(2),
            prepared_at_unix: 3,
        }),
    };
    let json = serde_json::to_string(&response).unwrap();
    let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(response, decoded);
}

#[test]
fn repair_metadata_response_round_trips_as_json() {
    let response = BridgeVmResponse::MetadataRepaired {
        repair: VmMetadataRepairMetadata {
            vm: "dev".to_string(),
            bundle: PathBuf::from("/tmp/dev.vmbridge"),
            repaired: true,
            actions: vec![bridgevm_storage::MetadataRepairAction {
                action: "repaired".to_string(),
                path: PathBuf::from("/tmp/dev.vmbridge/metadata/runtime.json"),
                detail: "wrote runtime metadata".to_string(),
            }],
            repaired_at_unix: 42,
        },
    };
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("metadata_repaired"));
    let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(response, decoded);
}

#[test]
fn migrate_manifest_response_round_trips_as_json() {
    let response = BridgeVmResponse::ManifestMigrated {
        migration: VmManifestMigrationMetadata {
            vm: "dev".to_string(),
            bundle: PathBuf::from("/tmp/dev.vmbridge"),
            manifest_path: PathBuf::from("/tmp/dev.vmbridge/manifest.yaml"),
            from_schema: "bridgevm.io/v1".to_string(),
            to_schema: "bridgevm.io/v1".to_string(),
            dry_run: false,
            migrated: false,
            backup_path: Some(PathBuf::from(
                "/tmp/dev.vmbridge/metadata/manifest-before-migration.yaml",
            )),
            receipt_path: Some(PathBuf::from(
                "/tmp/dev.vmbridge/metadata/manifest-migration.json",
            )),
            actions: vec![bridgevm_storage::MetadataRepairAction {
                action: "validated".to_string(),
                path: PathBuf::from("/tmp/dev.vmbridge/manifest.yaml"),
                detail: "manifest already uses the current schema".to_string(),
            }],
            migrated_at_unix: 42,
        },
    };
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("manifest_migrated"));
    let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(response, decoded);
}

#[test]
fn handler_creates_vm_record() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = VmStore::new(root);
    let manifest = VmManifest::new(
        "dev",
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    );

    let response = handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();
    let BridgeVmResponse::Vm { vm } = response else {
        panic!("expected VM response");
    };
    assert_eq!(vm.name, "dev");
    assert_eq!(vm.state, "stopped");
}

#[test]
fn handler_repairs_metadata() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-metadata-repair-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = VmStore::new(root);
    let manifest = VmManifest::new(
        "dev",
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::RepairMetadata {
            name: "dev".to_string(),
        },
    )
    .into_result()
    .unwrap();

    let BridgeVmResponse::MetadataRepaired { repair } = response else {
        panic!("expected metadata repair response");
    };
    assert_eq!(repair.vm, "dev");
    assert_eq!(repair.bundle, store.bundle_path("dev"));
    assert!(repair.repaired);
    assert!(repair
        .actions
        .iter()
        .any(|action| action.path.ends_with("metadata/primary-disk.json")));
}

#[test]
fn handler_migrates_manifest_metadata_boundary() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-manifest-migration-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = VmStore::new(root);
    let manifest = VmManifest::new(
        "dev",
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::MigrateManifest {
            name: "dev".to_string(),
            dry_run: false,
        },
    )
    .into_result()
    .unwrap();

    let BridgeVmResponse::ManifestMigrated { migration } = response else {
        panic!("expected manifest migration response");
    };
    assert_eq!(migration.vm, "dev");
    assert_eq!(migration.from_schema, "bridgevm.io/v1");
    assert_eq!(migration.to_schema, "bridgevm.io/v1");
    assert!(migration.backup_path.as_ref().unwrap().exists());
    assert!(migration.receipt_path.as_ref().unwrap().exists());
}

#[test]
fn handler_lists_boot_templates() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-template-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = VmStore::new(root);

    let response = handle_request(&store, BridgeVmRequest::ListTemplates)
        .into_result()
        .unwrap();
    let BridgeVmResponse::BootTemplates { templates } = response else {
        panic!("expected boot templates response");
    };

    assert!(templates
        .iter()
        .any(|template| template.id == "ubuntu-arm64-installer"));
    let ubuntu_vz_template = templates
        .iter()
        .find(|template| template.id == "ubuntu-arm64-apple-vz-linux-kernel-raw")
        .expect("Ubuntu Apple VZ linux-kernel raw template");
    assert_eq!(ubuntu_vz_template.mode, BootMode::LinuxKernel);
    assert_eq!(
        ubuntu_vz_template.kernel_path.as_deref(),
        Some("boot/vmlinuz")
    );
    assert_eq!(
        ubuntu_vz_template.initrd_path.as_deref(),
        Some("boot/initrd")
    );
    assert_eq!(
        ubuntu_vz_template.kernel_command_line.as_deref(),
        Some("console=hvc0 root=/dev/vda2 rw systemd.unit=graphical.target")
    );
    let ubuntu_storage = ubuntu_vz_template
        .storage
        .as_ref()
        .expect("Ubuntu storage defaults");
    assert_eq!(ubuntu_storage.primary.path, "disks/root.raw");
    assert_eq!(ubuntu_storage.primary.format, "raw");
    assert_eq!(ubuntu_storage.primary.size, "32GiB");
    let vz_template = templates
        .iter()
        .find(|template| template.id == "debian-arm64-apple-vz-linux-kernel-raw")
        .expect("Apple VZ linux-kernel raw template");
    assert_eq!(vz_template.mode, BootMode::LinuxKernel);
    assert_eq!(vz_template.kernel_path.as_deref(), Some("boot/vmlinuz"));
    assert_eq!(vz_template.initrd_path.as_deref(), Some("boot/initrd"));
    assert_eq!(
        vz_template.kernel_command_line.as_deref(),
        Some("console=hvc0 priority=low")
    );
    let storage = vz_template.storage.as_ref().expect("storage defaults");
    assert_eq!(storage.primary.path, "disks/root.raw");
    assert_eq!(storage.primary.format, "raw");
    assert_eq!(storage.primary.size, "64MiB");
    assert!(templates
        .iter()
        .any(|template| template.id == "macos-restore"));
}
