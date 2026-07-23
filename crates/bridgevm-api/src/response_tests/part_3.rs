//! Tests split so no file exceeds 1000 lines.

use crate::test_support::*;

#[test]
fn handler_prepares_compatibility_run() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-prepare-test-{}",
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

    let response = handle_request(
        &store,
        BridgeVmRequest::PrepareRun {
            name: "legacy".to_string(),
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
    assert!(metadata.dry_run);
    assert_eq!(metadata.pid, None);
    assert_eq!(metadata.command.first().unwrap(), "qemu-system-x86_64");
    let guest_tools = metadata.guest_tools.expect("guest tools metadata");
    assert_eq!(guest_tools.transport, "virtio-serial");
    assert_eq!(guest_tools.channel_name, "org.bridgevm.guest-tools.0");
    assert!(guest_tools
        .socket_path
        .ends_with("metadata/guest-tools.sock"));
    assert!(guest_tools
        .token_path
        .ends_with("metadata/guest-tools-token.json"));
    let token = store.guest_tools_token("legacy").unwrap().token;
    assert!(!metadata.command.join(" ").contains(&token));
}

#[test]
fn handler_renders_qemu_host_only_args() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-qemu-host-only-test-{}",
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

    let response = handle_request(
        &store,
        BridgeVmRequest::QemuArgs {
            name: "legacy".to_string(),
        },
    )
    .into_result()
    .expect("host-only QEMU args should render");
    let BridgeVmResponse::QemuCommand { command } = response else {
        panic!("expected qemu command");
    };

    assert!(command.args.iter().any(|arg| arg == "vmnet-host,id=net0"));
}

#[test]
fn handler_plans_qemu_bridged_network_blocker_without_launching() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-network-plan-bridged-test-{}",
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
    manifest.network.mode = "bridged".to_string();
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::PlanNetwork {
            name: "legacy".to_string(),
        },
    )
    .into_result()
    .expect("network plan should return blockers as data");
    let BridgeVmResponse::NetworkPlanned { plan } = response else {
        panic!("expected network plan response");
    };

    assert!(plan.dry_run);
    assert!(!plan.executable);
    assert_eq!(plan.backend, "qemu");
    assert_eq!(plan.mode, "bridged");
    assert!(plan
        .blockers
        .iter()
        .any(|blocker| blocker.code == "qemu-bridged-requires-privilege"
            && blocker.message.contains("com.apple.vm.networking")));
    assert!(store.runner_metadata("legacy").unwrap().is_none());
}

#[test]
fn handler_plans_qemu_host_only_privilege_blocker_without_launching() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-network-plan-host-only-privilege-test-{}",
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

    let response = handle_request(
        &store,
        BridgeVmRequest::PlanNetwork {
            name: "legacy".to_string(),
        },
    )
    .into_result()
    .expect("network plan should return privilege blockers as data");
    let BridgeVmResponse::NetworkPlanned { plan } = response else {
        panic!("expected network plan response");
    };

    assert!(plan.dry_run);
    assert!(!plan.executable);
    assert_eq!(plan.backend, "qemu");
    assert_eq!(plan.mode, "host-only");
    assert!(plan
        .capabilities
        .as_ref()
        .is_some_and(|capabilities| capabilities.requires_privileged_helper));
    assert!(plan.blockers.iter().any(|blocker| blocker.code
        == "qemu-host-only-requires-privilege"
        && blocker.message.contains("vmnet-host")
        && blocker.message.contains("com.apple.vm.networking")));
    assert!(store.runner_metadata("legacy").unwrap().is_none());
}

#[test]
fn handler_plans_host_only_port_forward_blocker_without_mutation() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-network-plan-host-only-test-{}",
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
    manifest.network.forwards.push(PortForward {
        host: 3000,
        guest: 3000,
    });
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::PlanNetwork {
            name: "legacy".to_string(),
        },
    )
    .into_result()
    .expect("network plan should return blockers as data");
    let BridgeVmResponse::NetworkPlanned { plan } = response else {
        panic!("expected network plan response");
    };

    assert!(plan.dry_run);
    assert!(!plan.executable);
    assert_eq!(plan.mode, "host-only");
    assert_eq!(plan.port_forwards[0].host, 3000);
    assert!(plan
        .blockers
        .iter()
        .any(|blocker| blocker.code == "unsupported-port-forwarding"));
    let (_, manifest) = store.get_vm("legacy").unwrap();
    assert_eq!(manifest.network.forwards.len(), 1);
}

#[test]
fn handler_updates_port_forwards_and_qemu_args() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-port-test-{}",
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

    let response = handle_request(
        &store,
        BridgeVmRequest::AddPort {
            name: "legacy".to_string(),
            host: 3000,
            guest: 3000,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::PortForwards { ports } = response else {
        panic!("expected port forwards response");
    };
    assert_eq!(ports.vm, "legacy");
    assert_eq!(ports.forwards.len(), 1);
    assert_eq!(ports.forwards[0].host, 3000);
    assert_eq!(ports.forwards[0].guest, 3000);

    let duplicate = handle_request(
        &store,
        BridgeVmRequest::AddPort {
            name: "legacy".to_string(),
            host: 3000,
            guest: 8080,
        },
    )
    .into_result()
    .expect_err("duplicate host port should fail");
    assert!(duplicate.contains("host port 3000"));

    let response = handle_request(
        &store,
        BridgeVmRequest::PrepareRun {
            name: "legacy".to_string(),
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
    assert!(metadata
        .command
        .iter()
        .any(|word| word.contains("hostfwd=tcp::3000-:3000")));

    let response = handle_request(
        &store,
        BridgeVmRequest::RemovePort {
            name: "legacy".to_string(),
            host: 3000,
            guest: 3000,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::PortForwards { ports } = response else {
        panic!("expected port forwards response");
    };
    assert!(ports.forwards.is_empty());

    let (_, manifest) = store.get_vm("legacy").unwrap();
    assert!(manifest.network.forwards.is_empty());
}

#[test]
fn handler_plans_ssh_from_port_forward_for_compatibility_mode() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-ssh-test-{}",
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
    manifest.network.forwards.push(PortForward {
        host: 2222,
        guest: 22,
    });
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::SshPlan {
            name: "legacy".to_string(),
            user: Some("ubuntu".to_string()),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::SshPlan { plan } = response else {
        panic!("expected ssh plan");
    };
    assert_eq!(plan.source, SshPlanSource::PortForward);
    assert_eq!(
        plan.command,
        vec![
            "ssh".to_string(),
            "-p".to_string(),
            "2222".to_string(),
            "ubuntu@127.0.0.1".to_string()
        ]
    );

    store
        .write_guest_tools_runtime_metadata(
            "legacy",
            &GuestToolsRuntimeMetadata {
                connected: true,
                guest_os: Some("linux".to_string()),
                agent_version: Some("1.0.0".to_string()),
                capabilities: vec!["guest-ip".to_string()],
                last_heartbeat_at_unix: Some(1),
                guest_ip_addresses: vec![GuestToolsIpAddressMetadata {
                    address: "10.0.2.15".to_string(),
                    interface: Some("eth0".to_string()),
                }],
                shared_folders: Vec::new(),
                metrics: None,
                last_command_result: None,
                agent_update: None,
                clipboard: None,
                updated_at_unix: 2,
            },
        )
        .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::SshPlan {
            name: "legacy".to_string(),
            user: Some("ubuntu".to_string()),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::SshPlan { plan } = response else {
        panic!("expected ssh plan");
    };
    assert_eq!(plan.source, SshPlanSource::PortForward);
    assert_eq!(plan.command.last().unwrap(), "ubuntu@127.0.0.1");
}

#[test]
fn handler_plans_open_from_guest_port_forward() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-open-test-{}",
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
    manifest.network.forwards.push(PortForward {
        host: 18080,
        guest: 80,
    });
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::OpenPort {
            name: "legacy".to_string(),
            guest: 80,
            scheme: Some("HTTPS".to_string()),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::OpenPortPlan { plan } = response else {
        panic!("expected open port plan");
    };
    assert_eq!(plan.scheme, "https");
    assert_eq!(plan.guest_port, 80);
    assert_eq!(plan.host_port, 18080);
    assert_eq!(plan.url, "https://127.0.0.1:18080");
    assert_eq!(
        plan.command,
        vec!["open".to_string(), "https://127.0.0.1:18080".to_string()]
    );
}

#[test]
fn handler_plans_ssh_from_connected_guest_tools_ip() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-ssh-ip-test-{}",
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
                capabilities: vec!["guest-ip".to_string()],
                last_heartbeat_at_unix: Some(1),
                guest_ip_addresses: vec![
                    GuestToolsIpAddressMetadata {
                        address: "127.0.0.1".to_string(),
                        interface: Some("lo".to_string()),
                    },
                    GuestToolsIpAddressMetadata {
                        address: "10.0.2.15".to_string(),
                        interface: Some("eth0".to_string()),
                    },
                ],
                shared_folders: Vec::new(),
                metrics: None,
                last_command_result: None,
                agent_update: None,
                clipboard: None,
                updated_at_unix: 2,
            },
        )
        .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::SshPlan {
            name: "dev".to_string(),
            user: Some("ubuntu".to_string()),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::SshPlan { plan } = response else {
        panic!("expected ssh plan");
    };
    assert_eq!(plan.source, SshPlanSource::GuestToolsIp);
    assert_eq!(
        plan.command,
        vec!["ssh".to_string(), "ubuntu@10.0.2.15".to_string()]
    );
}

#[test]
fn handler_views_bounded_vm_log_tail() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-log-view-test-{}",
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
        "80GiB",
    );
    handle_request(&store, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();
    let (bundle, _) = store.get_vm("legacy").unwrap();
    fs::create_dir_all(bundle.join("logs")).unwrap();
    fs::write(
        bundle.join("logs").join("qemu.log"),
        "first\nsecond\nthird\n",
    )
    .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::ViewLogs {
            name: "legacy".to_string(),
            kind: VmLogKind::Qemu,
            max_bytes: Some(12),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::LogsViewed { log } = response else {
        panic!("expected log view response");
    };
    assert_eq!(log.vm, "legacy");
    assert_eq!(log.kind, VmLogKind::Qemu);
    assert!(log.exists);
    assert_eq!(log.bytes, 19);
    assert_eq!(log.returned_bytes, 12);
    assert!(log.truncated);
    assert_eq!(log.content, "econd\nthird\n");
}
