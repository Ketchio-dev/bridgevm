//! Tests split so no file exceeds 1000 lines.

use crate::test_support::*;

#[test]
fn application_consistent_snapshot_request_round_trips_as_json() {
    let request = BridgeVmRequest::CreateSnapshot {
        vm: "dev".to_string(),
        name: "before-upgrade".to_string(),
        kind: SnapshotKind::ApplicationConsistent,
    };
    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("application-consistent"));
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn snapshot_chain_request_round_trips_as_json() {
    let request = BridgeVmRequest::SnapshotChain {
        vm: "dev".to_string(),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn export_vm_request_round_trips_as_json() {
    let request = BridgeVmRequest::ExportVm {
        name: "dev".to_string(),
        output: PathBuf::from("dev.vmbridge"),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn import_vm_request_round_trips_as_json() {
    let request = BridgeVmRequest::ImportVm {
        input: PathBuf::from("dev.vmbridge"),
        name: Some("dev-copy".to_string()),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn clone_vm_request_round_trips_as_json() {
    let request = BridgeVmRequest::CloneVm {
        name: "dev".to_string(),
        new_name: "dev-copy".to_string(),
        linked: false,
    };
    let json = serde_json::to_string(&request).unwrap();
    assert_eq!(
        json,
        r#"{"type":"clone_vm","name":"dev","new_name":"dev-copy"}"#
    );
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn clone_vm_request_defaults_missing_linked_to_false() {
    let decoded: BridgeVmRequest =
        serde_json::from_str(r#"{"type":"clone_vm","name":"dev","new_name":"dev-copy"}"#).unwrap();
    assert_eq!(
        decoded,
        BridgeVmRequest::CloneVm {
            name: "dev".to_string(),
            new_name: "dev-copy".to_string(),
            linked: false,
        }
    );
}

#[test]
fn linked_clone_vm_request_round_trips_as_json() {
    let request = BridgeVmRequest::CloneVm {
        name: "dev".to_string(),
        new_name: "dev-copy".to_string(),
        linked: true,
    };
    let json = serde_json::to_string(&request).unwrap();
    assert_eq!(
        json,
        r#"{"type":"clone_vm","name":"dev","new_name":"dev-copy","linked":true}"#
    );
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn repair_metadata_request_round_trips_as_json() {
    let request = BridgeVmRequest::RepairMetadata {
        name: "dev".to_string(),
    };
    let json = serde_json::to_string(&request).unwrap();
    assert_eq!(json, r#"{"type":"repair_metadata","name":"dev"}"#);
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn migrate_manifest_request_round_trips_as_json() {
    let request = BridgeVmRequest::MigrateManifest {
        name: "dev".to_string(),
        dry_run: true,
    };
    let json = serde_json::to_string(&request).unwrap();
    assert_eq!(
        json,
        r#"{"type":"migrate_manifest","name":"dev","dry_run":true}"#
    );
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn plan_network_request_round_trips_as_json() {
    let request = BridgeVmRequest::PlanNetwork {
        name: "dev".to_string(),
    };
    let json = serde_json::to_string(&request).unwrap();
    assert_eq!(json, r#"{"type":"plan_network","name":"dev"}"#);
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn diagnostic_bundle_request_round_trips_as_json() {
    let request = BridgeVmRequest::CreateDiagnosticBundle {
        name: "dev".to_string(),
        output: PathBuf::from("diagnostics"),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn performance_baseline_request_round_trips_as_json() {
    let request = BridgeVmRequest::CreatePerformanceBaseline {
        name: "dev".to_string(),
        output: PathBuf::from("performance"),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn performance_sample_request_round_trips_as_json() {
    let request = BridgeVmRequest::CreatePerformanceSample {
        name: "dev".to_string(),
        output: PathBuf::from("performance"),
        artifact_bytes: Some(4096),
        iterations: Some(3),
        sync: true,
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn guest_tools_requests_round_trip_as_json() {
    let status = BridgeVmRequest::GuestToolsStatus {
        name: "dev".to_string(),
    };
    let json = serde_json::to_string(&status).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(status, decoded);

    let token = BridgeVmRequest::GuestToolsToken {
        name: "dev".to_string(),
    };
    let json = serde_json::to_string(&token).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(token, decoded);

    let accept = BridgeVmRequest::GuestToolsAcceptHello {
        name: "dev".to_string(),
        envelope: AgentEnvelope::new(valid_guest_hello("token-1", &["clipboard"])),
    };
    let json = serde_json::to_string(&accept).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(accept, decoded);

    let send = BridgeVmRequest::GuestToolsSendCommand {
        name: "dev".to_string(),
        envelope: AgentEnvelope::with_request_id(
            AgentMessage::SetClipboard {
                text: "hello".to_string(),
            },
            "clipboard-1",
        ),
    };
    let json = serde_json::to_string(&send).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(send, decoded);

    let mount_approved_share = BridgeVmRequest::GuestToolsMountApprovedShare {
        name: "dev".to_string(),
        share: "workspace".to_string(),
        request_id: Some("mount-workspace-1".to_string()),
    };
    let json = serde_json::to_string(&mount_approved_share).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(mount_approved_share, decoded);

    let linux_command = BridgeVmRequest::GuestToolsLinuxCommand {
        name: "dev".to_string(),
        transport: GuestToolsLinuxCommandTransport::Socket,
        token_file: Some(PathBuf::from("/run/bridgevm-token.json")),
        device: None,
    };
    let json = serde_json::to_string(&linux_command).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(linux_command, decoded);
}

#[test]
fn handler_create_preserves_debian_apple_vz_linux_kernel_raw_template_manifest() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-vz-template-create-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = VmStore::new(root);
    handle_request(
        &store,
        BridgeVmRequest::CreateVmFromTemplate {
            name: "vz-template".to_string(),
            template_id: "debian-arm64-apple-vz-linux-kernel-raw".to_string(),
        },
    )
    .into_result()
    .unwrap();

    let (_, stored) = store.get_vm("vz-template").unwrap();
    assert_eq!(stored.mode, VmMode::Fast);
    assert_eq!(stored.guest.os, "debian");
    assert_eq!(stored.guest.arch, "arm64");
    assert_eq!(stored.storage.primary.path, "disks/root.raw");
    assert_eq!(stored.storage.primary.format, "raw");
    assert_eq!(stored.storage.primary.size, "64MiB");
    let boot = stored.boot.expect("boot");
    assert_eq!(boot.mode, BootMode::LinuxKernel);
    assert_eq!(boot.kernel_path.as_deref(), Some("boot/vmlinuz"));
    assert_eq!(boot.initrd_path.as_deref(), Some("boot/initrd"));
    assert_eq!(
        boot.kernel_command_line.as_deref(),
        Some("console=hvc0 priority=low")
    );
}

#[test]
fn handler_create_preserves_ubuntu_apple_vz_linux_kernel_raw_template_manifest() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-ubuntu-vz-template-create-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = VmStore::new(root);
    handle_request(
        &store,
        BridgeVmRequest::CreateVmFromTemplate {
            name: "ubuntu-vz-template".to_string(),
            template_id: "ubuntu-arm64-apple-vz-linux-kernel-raw".to_string(),
        },
    )
    .into_result()
    .unwrap();

    let (_, stored) = store.get_vm("ubuntu-vz-template").unwrap();
    assert_eq!(stored.mode, VmMode::Fast);
    assert_eq!(stored.guest.os, "ubuntu");
    assert_eq!(stored.guest.arch, "arm64");
    assert_eq!(stored.storage.primary.path, "disks/root.raw");
    assert_eq!(stored.storage.primary.format, "raw");
    assert_eq!(stored.storage.primary.size, "32GiB");
    let boot = stored.boot.expect("boot");
    assert_eq!(boot.mode, BootMode::LinuxKernel);
    assert_eq!(boot.kernel_path.as_deref(), Some("boot/vmlinuz"));
    assert_eq!(boot.initrd_path.as_deref(), Some("boot/initrd"));
    assert_eq!(
        boot.kernel_command_line.as_deref(),
        Some("console=hvc0 root=/dev/vda2 rw systemd.unit=graphical.target")
    );
}

#[test]
fn handler_rejects_duplicate_shared_folder_names() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-share-duplicate-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = VmStore::new(root);
    let mut manifest = VmManifest::new(
        "dev",
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    manifest.shared_folders = vec![SharedFolder {
        name: "workspace".to_string(),
        host_path: "/Users/me/project".to_string(),
        read_only: false,
        host_path_token: None,
    }];
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();

    let error = handle_request(
        &store,
        BridgeVmRequest::AddShare {
            name: "dev".to_string(),
            share: "workspace".to_string(),
            host_path: "/Users/me/other".to_string(),
            read_only: false,
            host_path_token: None,
        },
    )
    .into_result()
    .expect_err("duplicate shared folder should fail");
    assert!(error.contains("duplicate shared folder name 'workspace'"));
}

#[test]
fn handler_rejects_boot_media_write_destination_outside_bundle() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-media-path-safety-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let source = root.join("source.iso");
    fs::create_dir_all(&root).unwrap();
    fs::write(&source, b"fake installer").unwrap();
    let store = VmStore::new(root.clone());
    let mut manifest = VmManifest::new(
        "unsafe",
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    manifest.boot = Some(bridgevm_config::Boot {
        mode: BootMode::LinuxInstaller,
        installer_image: Some("../escaped.iso".to_string()),
        kernel_path: None,
        initrd_path: None,
        kernel_command_line: None,
        macos_restore_image: None,
    });
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();

    let import_error = handle_request(
        &store,
        BridgeVmRequest::ImportBootMedia {
            name: "unsafe".to_string(),
            source: source.clone(),
            kind: None,
        },
    )
    .into_result()
    .expect_err("escaping import destination should be rejected");
    assert!(import_error.contains("outside VM bundle"));

    let plan_error = handle_request(
        &store,
        BridgeVmRequest::PlanBootMediaDownload {
            name: "unsafe".to_string(),
            url: "https://example.invalid/ubuntu.iso".to_string(),
            expected_sha256: None,
            kind: None,
        },
    )
    .into_result()
    .expect_err("escaping download destination should be rejected");
    assert!(plan_error.contains("outside VM bundle"));

    assert!(!root.join("vms").join("escaped.iso").exists());
    assert!(!store
        .bundle_path("unsafe")
        .join("metadata")
        .join("boot-media")
        .join("installer-image.json")
        .exists());
    assert!(!store
        .bundle_path("unsafe")
        .join("metadata")
        .join("boot-media")
        .join("installer-image-download.json")
        .exists());
}

#[test]
fn handler_qemu_args_error_preserves_network_blocker_requirement() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-qemu-network-blocker-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = VmStore::new(root);
    let mut manifest = VmManifest::new(
        "legacy",
        VmMode::Compatibility,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "x86_64".to_string(),
        },
        "64GiB",
    );
    manifest.network.mode = "advanced".to_string();
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();

    let message = handle_request(
        &store,
        BridgeVmRequest::QemuArgs {
            name: "legacy".to_string(),
        },
    )
    .into_result()
    .expect_err("advanced QEMU args should expose launch blocker");

    assert!(
        message.contains("failed to build Compatibility Mode QEMU command"),
        "{message}"
    );
    assert!(
        message.contains("QEMU launch blocker qemu-advanced-network-requires-schema"),
        "{message}"
    );
    assert!(
        message.contains("requirement: Compatibility Mode QEMU requires an advanced network schema and launcher wiring before launch"),
        "{message}"
    );
}

#[test]
fn handler_prepare_run_error_preserves_network_blocker_requirement() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-prepare-network-blocker-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = VmStore::new(root);
    let mut manifest = VmManifest::new(
        "legacy",
        VmMode::Compatibility,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "x86_64".to_string(),
        },
        "64GiB",
    );
    manifest.network.mode = "advanced".to_string();
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();

    let message = handle_request(
        &store,
        BridgeVmRequest::PrepareRun {
            name: "legacy".to_string(),
        },
    )
    .into_result()
    .expect_err("advanced prepare-run should expose launch blocker");

    assert!(
        message.contains("failed to build Compatibility Mode QEMU command"),
        "{message}"
    );
    assert!(
        message.contains("QEMU launch blocker qemu-advanced-network-requires-schema"),
        "{message}"
    );
    assert!(
        message.contains("requirement: Compatibility Mode QEMU requires an advanced network schema and launcher wiring before launch"),
        "{message}"
    );
}

#[test]
fn handler_refuses_qemu_host_only_spawn_without_privileged_networking() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-qemu-host-only-spawn-blocker-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = VmStore::new(root);
    let mut manifest = VmManifest::new(
        "legacy",
        VmMode::Compatibility,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "x86_64".to_string(),
        },
        "1MiB",
    );
    manifest.storage.primary.format = "raw".to_string();
    manifest.storage.primary.path = "disks/root.raw".to_string();
    manifest.network.mode = "host-only".to_string();
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();

    let message = handle_request(
        &store,
        BridgeVmRequest::RunBackend {
            name: "legacy".to_string(),
            spawn: true,
        },
    )
    .into_result()
    .expect_err("host-only spawn should require privileged vmnet support");

    assert!(
        message.contains("qemu-host-only-requires-privilege"),
        "{message}"
    );
    assert!(message.contains("vmnet-host"), "{message}");
    assert!(message.contains("com.apple.vm.networking"), "{message}");
    assert!(store.runner_metadata("legacy").unwrap().is_none());
}

#[test]
fn handler_refuses_qemu_bridged_spawn_without_privileged_networking() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-qemu-bridged-spawn-blocker-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = VmStore::new(root);
    let mut manifest = VmManifest::new(
        "legacy",
        VmMode::Compatibility,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "x86_64".to_string(),
        },
        "1MiB",
    );
    manifest.storage.primary.format = "raw".to_string();
    manifest.storage.primary.path = "disks/root.raw".to_string();
    manifest.network.mode = "bridged".to_string();
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();

    let message = handle_request(
        &store,
        BridgeVmRequest::RunBackend {
            name: "legacy".to_string(),
            spawn: true,
        },
    )
    .into_result()
    .expect_err("bridged spawn should require privileged vmnet support");

    assert!(
        message.contains("qemu-bridged-requires-privilege"),
        "{message}"
    );
    assert!(message.contains("vmnet-bridged"), "{message}");
    assert!(message.contains("com.apple.vm.networking"), "{message}");
    assert!(store.runner_metadata("legacy").unwrap().is_none());
}

#[test]
fn handler_rejects_port_forwards_outside_nat_networking() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-port-mode-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = VmStore::new(root);
    let mut manifest = VmManifest::new(
        "legacy",
        VmMode::Compatibility,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "x86_64".to_string(),
        },
        "64GiB",
    );
    manifest.network.mode = "host-only".to_string();
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();

    let error = handle_request(
        &store,
        BridgeVmRequest::AddPort {
            name: "legacy".to_string(),
            host: 3000,
            guest: 3000,
        },
    )
    .into_result()
    .expect_err("host-only port forward should fail");
    assert!(error.contains("host-only networking does not support port forwarding"));

    let (_, manifest) = store.get_vm("legacy").unwrap();
    assert!(manifest.network.forwards.is_empty());
}

#[test]
fn handler_rejects_open_without_guest_port_forward() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-open-missing-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = VmStore::new(root);
    let manifest = VmManifest::new(
        "legacy",
        VmMode::Compatibility,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "x86_64".to_string(),
        },
        "64GiB",
    );
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();

    let error = handle_request(
        &store,
        BridgeVmRequest::OpenPort {
            name: "legacy".to_string(),
            guest: 80,
            scheme: Some("http".to_string()),
        },
    )
    .into_result()
    .expect_err("missing forwarded guest port should fail");
    assert!(error.contains("no host port is forwarded to guest port 80"));
}

#[test]
fn handler_fast_spawn_error_updates_runner_metadata_with_blocker() {
    let _guard = APPLE_VZ_RUNNER_ENV_LOCK.lock().unwrap();
    let _env = EnvVarGuard::capture("BRIDGEVM_APPLE_VZ_RUNNER");
    std::env::remove_var("BRIDGEVM_APPLE_VZ_RUNNER");

    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-fast-spawn-blocker-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = VmStore::new(root);
    let manifest = VmManifest::new(
        "fast-linux",
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

    let error = handle_request(
        &store,
        BridgeVmRequest::RunBackend {
            name: "fast-linux".to_string(),
            spawn: true,
        },
    )
    .into_result()
    .unwrap_err();

    assert!(
        error.contains("Fast Mode spawn requires BRIDGEVM_APPLE_VZ_RUNNER"),
        "{error}"
    );
    assert!(error.contains("launch blockers:"), "{error}");
    assert!(error.contains("missing-primary-disk"), "{error}");
    assert!(error.contains("apple-vz-runner-unavailable"), "{error}");
    let metadata = store
        .runner_metadata("fast-linux")
        .unwrap()
        .expect("Fast spawn blocker writes dry-run runner metadata");
    assert!(metadata.dry_run);
    assert_eq!(metadata.engine, "lightvm");
    let readiness = metadata
        .launch_readiness
        .expect("Fast Mode runner metadata includes launch readiness");
    assert!(!readiness.ready);
    assert!(readiness
        .blockers
        .iter()
        .any(|blocker| blocker.code == "apple-vz-runner-unavailable"));
}

#[test]
fn handler_preserves_export_hardening_error_messages() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-export-hardening-test-{}",
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
    let bundle = store.bundle_path("dev");

    let error = handle_request(
        &store,
        BridgeVmRequest::ExportVm {
            name: "dev".to_string(),
            output: bundle.join("exports").join("dev.vmbridge"),
        },
    )
    .into_result()
    .unwrap_err();

    assert!(
        error.contains("export output must not be the source bundle or inside it"),
        "unexpected export error: {error}"
    );
}

#[test]
fn handler_preserves_import_hardening_error_messages() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-import-hardening-test-{}",
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

    let error = handle_request(
        &store,
        BridgeVmRequest::ImportVm {
            input: store.bundle_path("dev"),
            name: None,
        },
    )
    .into_result()
    .unwrap_err();

    assert!(
        error.contains("import input conflicts with the destination store"),
        "unexpected import error: {error}"
    );
}

#[test]
fn handler_reapply_runtime_resources_rejects_stopped_vm() {
    let (store, name) = fast_test_store("runtime-resource-stopped");

    let message = handle_request(
        &store,
        BridgeVmRequest::ReapplyRuntimeResources {
            name,
            visibility: RuntimeResourceVisibility::Foreground,
        },
    )
    .into_result()
    .expect_err("stopped VM should reject runtime resource reapply");

    assert!(message.contains("requires a running VM"));
}

#[test]
fn compatibility_suspend_requires_running_qmp_socket() {
    let store = VmStore::new(unique_test_root("compat-suspend-no-sock"));
    let manifest = VmManifest::new(
        "compat",
        VmMode::Compatibility,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "x86_64".to_string(),
        },
        "40GiB",
    );
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();
    // No QMP socket present -> suspend should report the socket is unavailable.
    let error = suspend_backend(&store, "compat").unwrap_err();
    assert!(
        error.contains("QMP socket unavailable"),
        "unexpected error: {error}"
    );
    let _ = std::fs::remove_dir_all(store.root());
}

#[test]
fn compatibility_resume_requires_suspend_marker() {
    let store = VmStore::new(unique_test_root("compat-resume-no-marker"));
    let manifest = VmManifest::new(
        "compat",
        VmMode::Compatibility,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "x86_64".to_string(),
        },
        "40GiB",
    );
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();
    let error = resume_backend(&store, "compat").unwrap_err();
    assert!(
        error.contains("no saved Compatibility Mode state to resume from"),
        "unexpected error: {error}"
    );
    let _ = std::fs::remove_dir_all(store.root());
}
