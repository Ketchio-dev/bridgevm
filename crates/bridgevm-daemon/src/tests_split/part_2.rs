//! Split test module.

use super::helpers::*;
use crate::*;
use bridgevm_api::BridgeVmRequest;
use bridgevm_api::BridgeVmResponse;
use bridgevm_config::BootMode;
use bridgevm_config::Guest;
use bridgevm_config::VmManifest;
use bridgevm_config::VmMode;
use bridgevm_qemu::QemuError;
use bridgevm_storage::VmRuntimeState;
use std::fs;

#[test]
fn daemon_connection_lists_templates_for_dashboard_creation_flow() {
    let store = temp_store();

    let response = daemon_request(store, BridgeVmRequest::ListTemplates);
    let BridgeVmResponse::BootTemplates { templates } = response else {
        panic!("expected boot templates response");
    };

    let ubuntu = templates
        .iter()
        .find(|template| template.id == "ubuntu-arm64-installer")
        .expect("ubuntu arm64 installer template");
    assert_eq!(ubuntu.guest_os, "ubuntu");
    assert_eq!(ubuntu.guest_arch, "arm64");
    assert_eq!(ubuntu.mode, BootMode::LinuxInstaller);

    let json = serde_json::to_string(&BridgeVmResponse::BootTemplates { templates }).unwrap();
    assert!(json.contains(r#""type":"boot_templates""#));
    assert!(json.contains(r#""id":"ubuntu-arm64-installer""#));
    assert!(json.contains(r#""mode":"linux-installer""#));
}

#[test]
fn daemon_connection_reports_boot_media_status_for_dashboard_detail() {
    let store = temp_store();
    let source = store.root().join("fixtures").join("ubuntu.iso");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, b"fake installer").unwrap();

    let mut manifest = VmManifest::new(
        "ubuntu",
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
        installer_image: Some("installers/ubuntu-arm64.iso".to_string()),
        kernel_path: None,
        initrd_path: None,
        kernel_command_line: None,
        macos_restore_image: None,
    });
    store.create_vm(&manifest).unwrap();

    let response = daemon_request(
        store.clone(),
        BridgeVmRequest::ImportBootMedia {
            name: "ubuntu".to_string(),
            source,
            kind: None,
        },
    );
    let BridgeVmResponse::BootMediaImported { import } = response else {
        panic!("expected boot media import response");
    };
    assert_eq!(import.vm, "ubuntu");
    assert_eq!(import.kind, bridgevm_api::BootMediaKind::InstallerImage);
    assert_eq!(import.bytes, 14);

    let response = daemon_request(
        store,
        BridgeVmRequest::InspectBootMediaStatus {
            name: "ubuntu".to_string(),
        },
    );
    let BridgeVmResponse::BootMediaStatus { status } = response else {
        panic!("expected boot media status response");
    };
    assert_eq!(status.vm, "ubuntu");
    assert_eq!(status.entries.len(), 1);
    let entry = &status.entries[0];
    assert_eq!(entry.kind, bridgevm_api::BootMediaKind::InstallerImage);
    assert!(entry.path.ends_with("installers/ubuntu-arm64.iso"));
    assert!(entry.exists);
    assert_eq!(entry.bytes, Some(14));
    assert_eq!(entry.last_import.as_ref().unwrap().bytes, 14);

    let json = serde_json::to_string(&BridgeVmResponse::BootMediaStatus { status }).unwrap();
    assert!(json.contains(r#""type":"boot_media_status""#));
    assert!(json.contains(r#""kind":"installer-image""#));
    assert!(json.contains(r#""bytes":14"#));
    assert!(!json.contains("size_bytes"));
}

#[test]
fn daemon_connection_imports_boot_media_for_dashboard_detail() {
    let store = temp_store();
    let source = store.root().join("fixtures").join("ubuntu.iso");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, b"fake installer").unwrap();

    let mut manifest = VmManifest::new(
        "ubuntu",
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
        installer_image: Some("installers/ubuntu-arm64.iso".to_string()),
        kernel_path: None,
        initrd_path: None,
        kernel_command_line: None,
        macos_restore_image: None,
    });
    store.create_vm(&manifest).unwrap();

    let response = daemon_request(
        store.clone(),
        BridgeVmRequest::ImportBootMedia {
            name: "ubuntu".to_string(),
            source: source.clone(),
            kind: Some(bridgevm_api::BootMediaKind::InstallerImage),
        },
    );
    let BridgeVmResponse::BootMediaImported { import } = response else {
        panic!("expected boot media import response");
    };
    assert_eq!(import.vm, "ubuntu");
    assert_eq!(import.kind, bridgevm_api::BootMediaKind::InstallerImage);
    assert_eq!(import.source, source);
    assert!(import.destination.ends_with("installers/ubuntu-arm64.iso"));
    assert_eq!(import.bytes, 14);
    assert!(!import.replaced);
    assert_eq!(fs::read(&import.destination).unwrap(), b"fake installer");

    let json = serde_json::to_string(&BridgeVmResponse::BootMediaImported { import }).unwrap();
    assert!(json.contains(r#""type":"boot_media_imported""#));
    assert!(json.contains(r#""kind":"installer-image""#));
    assert!(json.contains(r#""bytes":14"#));

    let response = daemon_request(
        store,
        BridgeVmRequest::InspectBootMediaStatus {
            name: "ubuntu".to_string(),
        },
    );
    let BridgeVmResponse::BootMediaStatus { status } = response else {
        panic!("expected boot media status response");
    };
    let entry = status.entries.first().expect("boot media entry");
    assert!(entry.exists);
    assert_eq!(entry.bytes, Some(14));
    assert_eq!(entry.last_import.as_ref().unwrap().bytes, 14);
}

#[test]
fn daemon_connection_verifies_and_plans_boot_media_download_for_dashboard_detail() {
    let store = temp_store();
    let source = store.root().join("fixtures").join("ubuntu.iso");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, b"fake installer").unwrap();

    let mut manifest = VmManifest::new(
        "ubuntu",
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
        installer_image: Some("installers/ubuntu-arm64.iso".to_string()),
        kernel_path: None,
        initrd_path: None,
        kernel_command_line: None,
        macos_restore_image: None,
    });
    store.create_vm(&manifest).unwrap();

    let response = daemon_request(
        store.clone(),
        BridgeVmRequest::ImportBootMedia {
            name: "ubuntu".to_string(),
            source,
            kind: Some(bridgevm_api::BootMediaKind::InstallerImage),
        },
    );
    let BridgeVmResponse::BootMediaImported { import: _ } = response else {
        panic!("expected boot media import response");
    };
    let expected_sha256 =
        "941ef2fd249e8e3535908e3663515a85a291c538016f75be86032da473029b3e".to_string();

    let response = daemon_request(
        store.clone(),
        BridgeVmRequest::VerifyBootMedia {
            name: "ubuntu".to_string(),
            expected_sha256: expected_sha256.clone(),
            kind: Some(bridgevm_api::BootMediaKind::InstallerImage),
        },
    );
    let BridgeVmResponse::BootMediaVerified { verification } = response else {
        panic!("expected boot media verification response");
    };
    assert_eq!(verification.vm, "ubuntu");
    assert_eq!(
        verification.kind,
        bridgevm_api::BootMediaKind::InstallerImage
    );
    assert_eq!(verification.expected_sha256, expected_sha256);
    assert_eq!(verification.actual_sha256, expected_sha256);
    assert!(verification.verified);
    assert_eq!(verification.bytes, 14);

    let json =
        serde_json::to_string(&BridgeVmResponse::BootMediaVerified { verification }).unwrap();
    assert!(json.contains(r#""type":"boot_media_verified""#));
    assert!(json.contains(r#""kind":"installer-image""#));
    assert!(json.contains(r#""verified":true"#));

    let download_body = b"downloaded installer";
    let downloaded_sha256 =
        "462fbe30bef6a4c53bf4aa9514ec72707270a518e9f98b4aa348432a4fc9fc3c".to_string();
    let (download_url, server) = serve_one_http_response(download_body);

    let response = daemon_request(
        store.clone(),
        BridgeVmRequest::PlanBootMediaDownload {
            name: "ubuntu".to_string(),
            url: download_url.clone(),
            expected_sha256: Some(downloaded_sha256.clone()),
            kind: Some(bridgevm_api::BootMediaKind::InstallerImage),
        },
    );
    let BridgeVmResponse::BootMediaDownloadPlanned { plan } = response else {
        panic!("expected boot media download plan response");
    };
    assert_eq!(plan.vm, "ubuntu");
    assert_eq!(plan.kind, bridgevm_api::BootMediaKind::InstallerImage);
    assert_eq!(plan.url, download_url);
    assert_eq!(
        plan.expected_sha256.as_deref(),
        Some(downloaded_sha256.as_str())
    );
    assert!(plan.exists);
    assert_eq!(plan.bytes, Some(14));
    assert!(plan.last_import.is_some());
    assert!(plan.last_verification.is_some());

    let json = serde_json::to_string(&BridgeVmResponse::BootMediaDownloadPlanned { plan }).unwrap();
    assert!(json.contains(r#""type":"boot_media_download_planned""#));
    assert!(json.contains(r#""kind":"installer-image""#));
    assert!(json.contains(r#""planned_at_unix""#));

    let response = daemon_request(
        store.clone(),
        BridgeVmRequest::DownloadBootMedia {
            name: "ubuntu".to_string(),
            kind: Some(bridgevm_api::BootMediaKind::InstallerImage),
        },
    );
    server.join().expect("http test server should finish");
    let BridgeVmResponse::BootMediaDownloaded { download } = response else {
        panic!("expected boot media downloaded response");
    };
    assert_eq!(download.vm, "ubuntu");
    assert_eq!(download.kind, bridgevm_api::BootMediaKind::InstallerImage);
    assert!(download.replaced);
    assert_eq!(download.bytes, Some(download_body.len() as u64));
    assert_eq!(
        download.actual_sha256.as_deref(),
        Some(downloaded_sha256.as_str())
    );
    assert_eq!(download.verified, Some(true));
    assert!(download.downloaded);

    let response = daemon_request(
        store,
        BridgeVmRequest::InspectBootMediaStatus {
            name: "ubuntu".to_string(),
        },
    );
    let BridgeVmResponse::BootMediaStatus { status } = response else {
        panic!("expected boot media status response");
    };
    let entry = status.entries.first().expect("boot media entry");
    assert!(entry.last_verification.as_ref().unwrap().verified);
    assert!(entry.last_download.as_ref().unwrap().downloaded);
    assert_eq!(entry.last_download_plan.as_ref().unwrap().url, download_url);
}

#[test]
fn daemon_connection_returns_network_planner_errors() {
    let store = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();

    let response = daemon_request(
        store.clone(),
        BridgeVmRequest::AddPort {
            name: "legacy".to_string(),
            host: 0,
            guest: 22,
        },
    );
    let BridgeVmResponse::Error { message } = response else {
        panic!("expected network planner error");
    };
    assert!(message.contains("invalid port forward 0:22"));

    let response = daemon_request(
        store,
        BridgeVmRequest::ListPorts {
            name: "legacy".to_string(),
        },
    );
    let BridgeVmResponse::PortForwards { ports } = response else {
        panic!("expected port forwards response");
    };
    assert!(ports.forwards.is_empty());
}

#[test]
fn daemon_qemu_error_message_preserves_network_blocker_requirement() {
    let message = compatibility_qemu_command_error(QemuError::UnsupportedNetworkRequirement {
        mode: "advanced".to_string(),
        blocker: "qemu-advanced-network-requires-schema".to_string(),
        requirement:
            "Compatibility Mode QEMU requires an advanced network schema and launcher wiring before launch"
                .to_string(),
    });

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
fn daemon_fast_spawn_error_updates_runner_metadata_with_blocker() {
    let store = temp_store();
    store.create_vm(&fast_manifest("fast-linux")).unwrap();

    let response = daemon_request(
        store.clone(),
        BridgeVmRequest::RunBackend {
            name: "fast-linux".to_string(),
            spawn: true,
        },
    );
    let BridgeVmResponse::Error { message } = response else {
        panic!("expected Fast Mode spawn error");
    };
    assert!(
        message.contains("Fast Mode spawn requires BRIDGEVM_APPLE_VZ_RUNNER"),
        "{message}"
    );
    assert!(message.contains("launch blockers:"), "{message}");
    assert!(message.contains("missing-primary-disk"), "{message}");
    assert!(message.contains("apple-vz-runner-unavailable"), "{message}");

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
fn daemon_refuses_qemu_host_only_spawn_without_privileged_networking() {
    let store = temp_store();
    let mut manifest = compatibility_manifest("legacy");
    manifest.storage.primary.path = "disks/root.raw".to_string();
    manifest.storage.primary.format = "raw".to_string();
    manifest.storage.primary.size = "1MiB".to_string();
    manifest.network.mode = "host-only".to_string();
    store.create_vm(&manifest).unwrap();

    let response = daemon_request(
        store.clone(),
        BridgeVmRequest::RunBackend {
            name: "legacy".to_string(),
            spawn: true,
        },
    );
    let BridgeVmResponse::Error { message } = response else {
        panic!("expected host-only spawn readiness error");
    };

    assert!(
        message.contains("qemu-host-only-requires-privilege"),
        "{message}"
    );
    assert!(message.contains("vmnet-host"), "{message}");
    assert!(message.contains("com.apple.vm.networking"), "{message}");
    assert!(store.runner_metadata("legacy").unwrap().is_none());
}

#[test]
fn bundled_helper_discovery_uses_executable_siblings() {
    let store = temp_store();
    let helpers = store.root().join("BridgeVM.app/Contents/Helpers");
    fs::create_dir_all(&helpers).unwrap();
    let bridgevmd = helpers.join("bridgevmd");
    let apple_vz_runner = helpers.join("AppleVzRunner");
    write_executable(&bridgevmd, "#!/bin/sh\n");
    write_executable(&apple_vz_runner, "#!/bin/sh\n");

    assert_eq!(
        bundled_helper_path_from_exe(&bridgevmd, "AppleVzRunner"),
        Some(apple_vz_runner)
    );

    fs::remove_dir_all(store.root()).unwrap();
}

#[test]
fn entitlement_plist_requires_virtualization_true_value() {
    let true_plist = r#"
            <plist version="1.0">
            <dict>
              <key>com.apple.security.virtualization</key>
              <true/>
            </dict>
            </plist>
        "#;
    let false_plist = r#"
            <plist version="1.0">
            <dict>
              <key>com.apple.security.virtualization</key>
              <false/>
            </dict>
            </plist>
        "#;
    let missing_plist = r#"
            <plist version="1.0">
            <dict>
              <key>com.apple.security.app-sandbox</key>
              <true/>
            </dict>
            </plist>
        "#;

    assert!(entitlement_plist_has_true(
        true_plist,
        "com.apple.security.virtualization"
    ));
    assert!(!entitlement_plist_has_true(
        false_plist,
        "com.apple.security.virtualization"
    ));
    assert!(!entitlement_plist_has_true(
        missing_plist,
        "com.apple.security.virtualization"
    ));
}

#[test]
fn daemon_fast_spawn_preflight_failure_does_not_mutate_runtime_state() {
    let store = temp_store();
    store.create_vm(&ready_fast_manifest("fast-linux")).unwrap();
    let bundle = store.bundle_path("fast-linux");
    fs::create_dir_all(bundle.join("boot")).unwrap();
    fs::write(bundle.join("boot").join("vmlinuz"), b"kernel").unwrap();

    let lightvm_runner = store.root().join("fake-lightvm-runner");
    let apple_vz_runner = store.root().join("fake-AppleVzRunner");
    write_executable(&lightvm_runner, "#!/bin/sh\n");
    fs::write(&apple_vz_runner, b"not executable").unwrap();

    let mut state = DaemonState::new(store.clone());
    let error = state
        .spawn_fast_backend(
            "fast-linux",
            bundle.clone(),
            ready_fast_manifest("fast-linux"),
            FastModeSpawnConfig {
                lightvm_runner,
                apple_vz_runner: apple_vz_runner.clone(),
                stop_after_seconds: None,
                force_stop_grace_seconds: None,
                verify_apple_vz_runner_entitlement: false,
            },
        )
        .unwrap_err();
    let message = format!("{error:#}");

    assert!(
        message.contains("BRIDGEVM_APPLE_VZ_RUNNER/AppleVzRunner is not executable"),
        "{message}"
    );
    assert_eq!(
        store.state("fast-linux").unwrap().state,
        VmRuntimeState::Stopped
    );
    assert_eq!(store.runner_metadata("fast-linux").unwrap(), None);
    assert!(!state.children.contains_key("fast-linux"));
    assert!(
        !bundle.join("disks").join("root.raw").exists(),
        "preflight should fail before preparing the active disk"
    );

    fs::remove_dir_all(store.root()).unwrap();
}
