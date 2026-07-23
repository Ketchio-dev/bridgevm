//! Tests split so no file exceeds 1000 lines.

use crate::test_support::*;

#[test]
fn handler_reports_guest_tools_policy_from_manifest() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-guest-tools-status-test-{}",
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
    manifest.integration.clipboard = false;
    manifest.integration.shared_folders = false;
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::GuestToolsStatus {
            name: "dev".to_string(),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::GuestToolsStatus { status } = response else {
        panic!("expected guest tools status response");
    };

    assert_eq!(status.vm, "dev");
    assert_eq!(status.tools, "required");
    assert!(status.token_created_at_unix > 0);
    assert!(status
        .capabilities
        .iter()
        .any(|capability| capability.name == "heartbeat"));
    assert!(status
        .capabilities
        .iter()
        .any(|capability| capability.name == "display-resize"));
    assert!(status
        .capabilities
        .iter()
        .any(|capability| capability.name == "applications"));
    assert!(status
        .capabilities
        .iter()
        .any(|capability| capability.name == "windows"));
    assert!(status
        .capabilities
        .iter()
        .any(|capability| capability.name == "agent-update"));
    assert!(!status
        .capabilities
        .iter()
        .any(|capability| capability.name == "clipboard"));
    assert!(!status
        .capabilities
        .iter()
        .any(|capability| capability.name == "shared-folders"));
    assert!(status.approved_shared_folders.is_empty());

    let response = handle_request(
        &store,
        BridgeVmRequest::GuestToolsToken {
            name: "dev".to_string(),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::GuestToolsToken { token } = response else {
        panic!("expected guest tools token response");
    };
    assert_eq!(token.vm, "dev");
    assert_eq!(token.token.len(), 64);
    assert_eq!(token.created_at_unix, status.token_created_at_unix);
}

#[test]
fn handler_reports_last_guest_tools_command_result_from_runtime() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-guest-tools-command-result-test-{}",
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
    store
        .write_guest_tools_runtime_metadata(
            "dev",
            &GuestToolsRuntimeMetadata {
                connected: true,
                guest_os: Some("linux".to_string()),
                agent_version: Some("1.0.0".to_string()),
                capabilities: vec!["applications".to_string()],
                last_heartbeat_at_unix: Some(1),
                guest_ip_addresses: Vec::new(),
                shared_folders: Vec::new(),
                metrics: None,
                last_command_result: Some(bridgevm_storage::GuestToolsCommandResultMetadata {
                    request_id: "apps-1".to_string(),
                    capability: Some("applications".to_string()),
                    ok: false,
                    error_code: Some("not-ready".to_string()),
                    message: Some("application inventory is not ready".to_string()),
                    result: Some(serde_json::json!({
                        "applications": [
                            {
                                "id": "terminal",
                                "name": "Terminal"
                            }
                        ]
                    })),
                    metadata: Some(serde_json::json!({
                        "scan_duration_ms": 12
                    })),
                    completed_at_unix: 42,
                }),
                agent_update: None,
                clipboard: None,
                updated_at_unix: 43,
            },
        )
        .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::GuestToolsStatus {
            name: "dev".to_string(),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::GuestToolsStatus { status } = response else {
        panic!("expected guest tools status response");
    };
    let result = status
        .runtime
        .expect("runtime metadata")
        .last_command_result
        .expect("last command result");

    assert_eq!(result.request_id, "apps-1");
    assert_eq!(result.capability.as_deref(), Some("applications"));
    assert!(!result.ok);
    assert_eq!(result.error_code.as_deref(), Some("not-ready"));
    assert_eq!(
        result.message.as_deref(),
        Some("application inventory is not ready")
    );
    assert_eq!(
        result.result,
        Some(serde_json::json!({
            "applications": [
                {
                    "id": "terminal",
                    "name": "Terminal"
                }
            ]
        }))
    );
    assert_eq!(
        result.metadata,
        Some(serde_json::json!({
            "scan_duration_ms": 12
        }))
    );
    assert_eq!(result.completed_at_unix, 42);
}

#[test]
fn handler_reports_guest_tools_agent_update_from_runtime() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-guest-tools-agent-update-test-{}",
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
    store
        .write_guest_tools_runtime_metadata(
            "dev",
            &GuestToolsRuntimeMetadata {
                connected: true,
                guest_os: Some("linux".to_string()),
                agent_version: Some("1.0.0".to_string()),
                capabilities: vec!["agent-update".to_string()],
                last_heartbeat_at_unix: Some(1),
                guest_ip_addresses: Vec::new(),
                shared_folders: Vec::new(),
                metrics: None,
                last_command_result: None,
                agent_update: Some(bridgevm_storage::GuestToolsAgentUpdateMetadata {
                    current_version: "1.0.0".to_string(),
                    available_version: "1.1.0".to_string(),
                    download_url: Some("https://updates.example/bridgevm-tools".to_string()),
                    signature: Some("signed".to_string()),
                    observed_at_unix: 42,
                }),
                clipboard: None,
                updated_at_unix: 43,
            },
        )
        .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::GuestToolsStatus {
            name: "dev".to_string(),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::GuestToolsStatus { status } = response else {
        panic!("expected guest tools status response");
    };
    let update = status
        .runtime
        .expect("runtime metadata")
        .agent_update
        .expect("agent update metadata");

    assert_eq!(update.current_version, "1.0.0");
    assert_eq!(update.available_version, "1.1.0");
    assert_eq!(
        update.download_url.as_deref(),
        Some("https://updates.example/bridgevm-tools")
    );
    assert_eq!(update.signature.as_deref(), Some("signed"));
    assert_eq!(update.observed_at_unix, 42);
}

#[test]
fn handler_reports_approved_shared_folders_from_manifest() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-approved-shares-test-{}",
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
    manifest.shared_folders = vec![
        SharedFolder {
            name: "workspace".to_string(),
            host_path: "/Users/me/project".to_string(),
            read_only: false,
            host_path_token: Some("share-token-workspace".to_string()),
        },
        SharedFolder {
            name: "downloads".to_string(),
            host_path: "/Users/me/Downloads".to_string(),
            read_only: true,
            host_path_token: None,
        },
    ];
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::GuestToolsStatus {
            name: "dev".to_string(),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::GuestToolsStatus { status } = response else {
        panic!("expected guest tools status response");
    };

    assert_eq!(status.approved_shared_folders.len(), 2);
    assert_eq!(status.approved_shared_folders[0].name, "workspace");
    assert_eq!(
        status.approved_shared_folders[0].host_path,
        "/Users/me/project"
    );
    assert_eq!(
        status.approved_shared_folders[0].host_path_token,
        "share-token-workspace"
    );
    assert!(!status.approved_shared_folders[0].read_only);
    assert_eq!(status.approved_shared_folders[0].approval, "required");
    assert_eq!(status.approved_shared_folders[1].name, "downloads");
    assert!(status.approved_shared_folders[1].read_only);
    assert!(status.approved_shared_folders[1]
        .host_path_token
        .starts_with("share-"));
}

#[test]
fn handler_updates_manifest_shared_folders() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-share-manifest-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let host_path = root.join("workspace");
    fs::create_dir_all(&host_path).unwrap();
    let host_path = fs::canonicalize(&host_path)
        .unwrap()
        .to_string_lossy()
        .to_string();
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
        BridgeVmRequest::AddShare {
            name: "dev".to_string(),
            share: "workspace".to_string(),
            host_path: host_path.clone(),
            read_only: true,
            host_path_token: Some("share-token-workspace".to_string()),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::SharedFolders { shares } = response else {
        panic!("expected shared folders response");
    };
    assert_eq!(shares.shared_folders.len(), 1);
    assert_eq!(shares.shared_folders[0].name, "workspace");
    assert_eq!(shares.shared_folders[0].host_path, host_path);
    assert!(shares.shared_folders[0].read_only);
    assert_eq!(
        shares.shared_folders[0].host_path_token,
        "share-token-workspace"
    );

    let (_, manifest) = store.get_vm("dev").unwrap();
    assert_eq!(manifest.shared_folders.len(), 1);
    assert_eq!(manifest.shared_folders[0].name, "workspace");
    assert_eq!(manifest.shared_folders[0].host_path, host_path);

    let response = handle_request(
        &store,
        BridgeVmRequest::RemoveShare {
            name: "dev".to_string(),
            share: "workspace".to_string(),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::SharedFolders { shares } = response else {
        panic!("expected shared folders response");
    };
    assert!(shares.shared_folders.is_empty());
}

#[test]
fn handler_generates_manifest_compatible_linux_tools_command() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-guest-tools-linux-command-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = VmStore::new(root);
    let mut manifest = VmManifest::new(
        "dev",
        VmMode::Compatibility,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    manifest.integration.clipboard = false;
    manifest.integration.shared_folders = false;
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();
    let token = store.guest_tools_token("dev").unwrap().token;

    let response = handle_request(
        &store,
        BridgeVmRequest::GuestToolsLinuxCommand {
            name: "dev".to_string(),
            transport: GuestToolsLinuxCommandTransport::Device,
            token_file: None,
            device: None,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::GuestToolsLinuxCommand { command } = response else {
        panic!("expected guest tools linux command response");
    };

    assert_eq!(command.vm, "dev");
    assert_eq!(command.transport, GuestToolsLinuxCommandTransport::Device);
    assert_eq!(command.command[0], "bridgevm-tools-linux");
    assert!(command
        .command
        .windows(2)
        .any(|pair| pair == ["--device", "/dev/virtio-ports/org.bridgevm.guest-tools.0"]));
    assert!(command.command.windows(2).any(|pair| {
        pair[0] == "--token-file" && pair[1].ends_with("metadata/guest-tools-token.json")
    }));
    assert!(command
        .command
        .windows(2)
        .any(|pair| pair == ["--capability", "heartbeat:1"]));
    assert!(command
        .command
        .windows(2)
        .any(|pair| pair == ["--capability", "guest-ip:1"]));
    assert!(command
        .command
        .windows(2)
        .any(|pair| pair == ["--capability", "time-sync:1"]));
    assert!(command
        .command
        .windows(2)
        .any(|pair| pair == ["--capability", "guest-metrics:1"]));
    assert!(!command
        .capabilities
        .iter()
        .any(|item| item == "clipboard:1"));
    assert!(!command
        .capabilities
        .iter()
        .any(|item| item == "shared-folders:1"));
    assert!(!command.command.iter().any(|word| word == &token));

    let socket_response = handle_request(
        &store,
        BridgeVmRequest::GuestToolsLinuxCommand {
            name: "dev".to_string(),
            transport: GuestToolsLinuxCommandTransport::Socket,
            token_file: Some(PathBuf::from("/run/bridgevm-token.json")),
            device: None,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::GuestToolsLinuxCommand { command } = socket_response else {
        panic!("expected guest tools linux command response");
    };
    assert!(command
        .command
        .windows(2)
        .any(|pair| { pair[0] == "--socket" && pair[1].ends_with("metadata/guest-tools.sock") }));
    assert!(command
        .command
        .windows(2)
        .any(|pair| pair == ["--token-file", "/run/bridgevm-token.json"]));
}

#[test]
fn handler_accepts_guest_tools_hello_against_manifest_policy() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-guest-tools-hello-test-{}",
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
    let token = store.guest_tools_token("dev").unwrap().token;

    let response = handle_request(
        &store,
        BridgeVmRequest::GuestToolsAcceptHello {
            name: "dev".to_string(),
            envelope: AgentEnvelope::new(valid_guest_hello(
                &token,
                &["clipboard", "display-resize"],
            )),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::GuestToolsSession { session } = response else {
        panic!("expected guest tools session response");
    };

    assert_eq!(session.vm, "dev");
    assert_eq!(session.guest_os, "linux");
    assert_eq!(session.agent_version.as_deref(), Some("1.0.0"));
    assert_eq!(session.capabilities.len(), 2);

    let error = handle_request(
        &store,
        BridgeVmRequest::GuestToolsAcceptHello {
            name: "dev".to_string(),
            envelope: AgentEnvelope::new(valid_guest_hello("wrong-token", &["clipboard"])),
        },
    )
    .into_result()
    .expect_err("wrong tools token should be rejected");
    assert!(error.contains("InvalidToolsToken"));
}

#[test]
fn handler_inspects_template_boot_media() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-boot-media-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = VmStore::new(root);
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
    manifest.boot =
        boot_template_by_id("ubuntu-arm64-installer").map(|template| template.as_boot());
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::InspectBootMedia {
            name: "ubuntu".to_string(),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::BootMedia { name, boot } = response else {
        panic!("expected boot media response");
    };

    assert_eq!(name, "ubuntu");
    assert_eq!(boot.mode, BootMode::LinuxInstaller);
    let installer = boot.installer_image.expect("expected installer image");
    assert!(installer.path.ends_with("installers/ubuntu-arm64.iso"));
    assert!(!installer.exists);
}

#[test]
fn handler_reports_metadata_safe_readiness_blockers_without_preparing_launch() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-readiness-report-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = VmStore::new(root);
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
    manifest.boot =
        boot_template_by_id("ubuntu-arm64-installer").map(|template| template.as_boot());
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::ReadinessReport {
            name: "ubuntu".to_string(),
            live_evidence: None,
            record_live_evidence: false,
            clear_live_evidence: false,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::ReadinessReport { report } = response else {
        panic!("expected readiness report response");
    };

    assert_eq!(report.vm, "ubuntu");
    assert_eq!(report.mode, VmMode::Fast);
    assert_eq!(report.state, VmRuntimeState::Stopped);
    assert!(report.metadata_only);
    assert!(report.live_e2e_required);
    assert!(report
        .notes
        .iter()
        .any(|note| note.contains("metadata-only preflight report")));
    assert!(report
        .notes
        .iter()
        .any(|note| note.contains("explicit opt-in live smoke")));
    assert!(report.evidence_requirements.iter().any(|requirement| {
        requirement.kind == "live-boot" && requirement.required && !requirement.proven
    }));
    assert!(report.evidence_requirements.iter().any(|requirement| {
        requirement.kind == "console" && requirement.required && !requirement.proven
    }));
    assert!(report.evidence_requirements.iter().any(|requirement| {
        requirement.kind == "guest-tools-effects" && requirement.required && !requirement.proven
    }));
    assert!(report.boot_media.as_ref().is_some_and(|status| {
        status.entries.iter().any(|entry| {
            entry.kind == BootMediaKind::InstallerImage
                && entry.path.ends_with("installers/ubuntu-arm64.iso")
                && !entry.exists
        })
    }));
    assert!(report
        .blockers
        .iter()
        .any(|blocker| blocker.starts_with("boot-media-missing:installer-image:")));
    assert!(report
        .blockers
        .iter()
        .any(|blocker| blocker.starts_with("active-disk-missing:")));
    assert!(report
        .blockers
        .iter()
        .any(|blocker| blocker == "launch-readiness-blocker:missing-primary-disk"));
    assert!(report
        .blockers
        .iter()
        .any(|blocker| blocker == "launch-readiness-blocker:missing-installer-image"));
    let pre_run_readiness = report
        .pre_run_launch_readiness
        .as_ref()
        .expect("expected pre-run launch readiness");
    assert!(!pre_run_readiness.ready);
    assert!(pre_run_readiness
        .blockers
        .iter()
        .any(|blocker| blocker.code == "missing-primary-disk"));
    assert!(pre_run_readiness
        .blockers
        .iter()
        .any(|blocker| blocker.code == "missing-installer-image"));
    assert!(!report
        .blockers
        .iter()
        .any(|blocker| blocker == "runner-metadata-missing"));
    assert!(store.runner_metadata("ubuntu").unwrap().is_none());
    assert!(!store
        .root()
        .join("vms")
        .join("ubuntu.vmbridge")
        .join("metadata")
        .join("primary-disk.json")
        .exists());
}

#[test]
fn compatibility_readiness_reports_missing_windows_firmware_dependencies() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-compat-firmware-readiness-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = VmStore::new(root);
    let mut manifest = VmManifest::new(
        "win11",
        VmMode::Compatibility,
        Guest {
            os: "windows".to_string(),
            version: Some("11".to_string()),
            arch: "arm64".to_string(),
        },
        "80GiB",
    );
    manifest.boot = Some(bridgevm_config::Boot {
        mode: BootMode::WindowsInstaller,
        installer_image: Some("installers/win11-arm.iso".to_string()),
        kernel_path: None,
        initrd_path: None,
        kernel_command_line: None,
        macos_restore_image: None,
    });
    manifest.firmware.tpm = true;
    manifest.firmware.secure_boot = true;
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::ReadinessReport {
            name: "win11".to_string(),
            live_evidence: None,
            record_live_evidence: false,
            clear_live_evidence: false,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::ReadinessReport { report } = response else {
        panic!("expected readiness report response");
    };
    let pre_run_readiness = report
        .pre_run_launch_readiness
        .as_ref()
        .expect("expected Compatibility Mode pre-run readiness");

    for code in [
        "missing-primary-disk",
        "missing-windows-installer-image",
        "missing-tpm-socket",
        "missing-secure-boot-vars",
    ] {
        assert!(
            pre_run_readiness
                .blockers
                .iter()
                .any(|blocker| blocker.code == code),
            "missing pre-run blocker {code}: {:?}",
            pre_run_readiness.blockers
        );
        assert!(
            report
                .blockers
                .iter()
                .any(|blocker| blocker == &format!("launch-readiness-blocker:{code}")),
            "missing report blocker {code}: {:?}",
            report.blockers
        );
    }

    let response = handle_request(
        &store,
        BridgeVmRequest::PrepareRun {
            name: "win11".to_string(),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::RunnerStatus {
        metadata: Some(metadata),
        ..
    } = response
    else {
        panic!("expected runner status");
    };
    let readiness = metadata
        .launch_readiness
        .as_ref()
        .expect("Compatibility dry-run includes launch readiness");
    assert!(!readiness.ready);
    assert!(readiness
        .blockers
        .iter()
        .any(|blocker| blocker.code == "missing-windows-installer-image"));
    assert!(readiness
        .blockers
        .iter()
        .any(|blocker| blocker.code == "missing-tpm-socket"));
    assert!(readiness
        .blockers
        .iter()
        .any(|blocker| blocker.code == "missing-secure-boot-vars"));
}
