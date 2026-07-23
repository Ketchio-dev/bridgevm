//! Tests split so no file exceeds 1000 lines.

use crate::test_support::*;

#[test]
fn handler_reports_snapshot_preflight_status_from_guest_tools_runtime() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-snapshot-preflight-test-{}",
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
                agent_version: Some("0.1.0".to_string()),
                capabilities: vec!["fs-freeze".to_string(), "fs-thaw".to_string()],
                last_heartbeat_at_unix: Some(1),
                guest_ip_addresses: Vec::new(),
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
        BridgeVmRequest::SnapshotPreflightStatus {
            name: "dev".to_string(),
            consistency: SnapshotConsistency::ApplicationConsistent,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::SnapshotPreflightStatus { preflight } = response else {
        panic!("expected snapshot preflight response");
    };
    assert_eq!(preflight.vm, "dev");
    assert_eq!(
        preflight.consistency,
        SnapshotConsistency::ApplicationConsistent
    );
    assert!(preflight.guest_tools_connected);
    assert_eq!(
        preflight.capabilities,
        vec!["fs-freeze".to_string(), "fs-thaw".to_string()]
    );
    assert!(!preflight.backend_freeze_thaw_supported);
    assert!(!preflight.ready);
    assert_eq!(
        preflight.blockers[0].code,
        "backend-freeze-thaw-unavailable"
    );
}

#[test]
fn handler_restores_suspend_snapshot_metadata() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-suspend-restore-test-{}",
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
        .transition_state("dev", VmRuntimeState::Running)
        .unwrap();
    store
        .transition_state("dev", VmRuntimeState::Suspended)
        .unwrap();
    handle_request(
        &store,
        BridgeVmRequest::CreateSnapshot {
            vm: "dev".to_string(),
            name: "paused".to_string(),
            kind: SnapshotKind::Suspend,
        },
    )
    .into_result()
    .unwrap();
    let image = store
        .snapshot_suspend_image_metadata("dev", "paused")
        .unwrap()
        .unwrap();
    fs::write(&image.image_path, b"fake suspend image").unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::RestoreSnapshot {
            vm: "dev".to_string(),
            name: "paused".to_string(),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::SnapshotRestored { restore } = response else {
        panic!("expected snapshot restore response");
    };
    assert_eq!(restore.snapshot, "paused");
    assert_eq!(restore.restored_state, VmRuntimeState::Suspended);
    assert!(restore.active_disk.is_none());
    assert!(restore.suspend_image.unwrap().image_exists);
}

#[test]
fn handler_creates_application_consistent_snapshot_preflight_metadata() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-application-consistent-snapshot-test-{}",
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
                capabilities: vec![
                    "heartbeat".to_string(),
                    "fs-freeze".to_string(),
                    "fs-thaw".to_string(),
                ],
                last_heartbeat_at_unix: Some(1),
                guest_ip_addresses: Vec::new(),
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
        BridgeVmRequest::CreateSnapshot {
            vm: "dev".to_string(),
            name: "app-ready".to_string(),
            kind: SnapshotKind::ApplicationConsistent,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::Snapshot {
        snapshot,
        disk,
        application_consistent_preflight,
    } = response
    else {
        panic!("expected snapshot response");
    };
    let preflight = application_consistent_preflight.expect("preflight metadata");

    assert_eq!(snapshot.kind, SnapshotKind::ApplicationConsistent);
    assert!(disk.is_none());
    assert!(preflight.connected);
    assert!(preflight.ready);
    assert!(preflight.missing_capabilities.is_empty());
    assert_eq!(preflight.runtime_updated_at_unix, Some(2));
}

#[test]
fn handler_exports_vm_bundle() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-export-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let output = root.join("exports").join("dev.vmbridge");
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
        BridgeVmRequest::ExportVm {
            name: "dev".to_string(),
            output: output.clone(),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::Exported { export } = response else {
        panic!("expected export response");
    };
    assert_eq!(export.vm, "dev");
    assert_eq!(export.archive_format, "directory");
    assert!(export.manifest_preserved);
    assert!(export.copied_files.contains(&"manifest.yaml".to_string()));
    assert!(output.join("manifest.yaml").exists());
}

#[test]
fn handler_imports_vm_bundle() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-import-source-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let output = root.join("exports").join("dev.vmbridge");
    let source = VmStore::new(root);
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
    handle_request(&source, BridgeVmRequest::create_vm(manifest))
        .into_result()
        .unwrap();
    handle_request(
        &source,
        BridgeVmRequest::ExportVm {
            name: "dev".to_string(),
            output: output.clone(),
        },
    )
    .into_result()
    .unwrap();

    let mut import_root = std::env::temp_dir();
    import_root.push(format!(
        "bridgevm-api-import-target-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let target = VmStore::new(import_root);
    let response = handle_request(
        &target,
        BridgeVmRequest::ImportVm {
            input: output,
            name: Some("dev-copy".to_string()),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::Imported { import } = response else {
        panic!("expected import response");
    };
    assert_eq!(import.vm, "dev-copy");
    assert_eq!(import.original_name, "dev");
    assert_eq!(import.requested_name.as_deref(), Some("dev-copy"));
    assert!(import.manifest_identity_rewritten);
    assert_eq!(record_for(&target, "dev-copy").unwrap().name, "dev-copy");
}

#[test]
fn handler_restarts_vm_through_stop_then_start_state() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-restart-test-{}",
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
    handle_request(
        &store,
        BridgeVmRequest::TransitionVm {
            name: "dev".to_string(),
            state: VmRuntimeState::Running,
        },
    )
    .into_result()
    .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::RestartVm {
            name: "dev".to_string(),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::State { name, metadata } = response else {
        panic!("expected restart state response");
    };

    assert_eq!(name, "dev");
    assert_eq!(metadata.state, VmRuntimeState::Running);
    assert_eq!(store.state("dev").unwrap().state, VmRuntimeState::Running);
    assert!(store.runner_metadata("dev").unwrap().is_none());
}

#[test]
fn handler_clones_vm_bundle_with_new_manifest_identity() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-clone-test-{}",
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
        BridgeVmRequest::CloneVm {
            name: "dev".to_string(),
            new_name: "dev-copy".to_string(),
            linked: false,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::Cloned { clone } = response else {
        panic!("expected clone response");
    };

    assert_eq!(clone.vm, "dev-copy");
    assert!(clone.output.join("manifest.yaml").exists());
    assert!(clone.output.join("metadata").join("clone.json").exists());
    let (clone_bundle, manifest) = store.get_vm("dev-copy").unwrap();
    assert_eq!(manifest.name, "dev-copy");
    assert_eq!(manifest.network.hostname, "dev-copy.bridgevm.local");

    // The clone is a distinct VM and the source is left unchanged.
    let (source_bundle, source_manifest) = store.get_vm("dev").unwrap();
    assert_eq!(source_manifest.name, "dev");
    assert_eq!(source_manifest.network.hostname, "dev.bridgevm.local");
    assert_ne!(source_bundle, clone_bundle);
    assert_eq!(
        store.state("dev-copy").unwrap().state,
        VmRuntimeState::Stopped
    );
}

#[test]
fn handler_reapplies_runtime_resources_for_background_fast_vm() {
    let _battery = EnvVarGuard::set("BRIDGEVM_FORCE_ON_BATTERY", "0");
    let (store, name) = fast_test_store("runtime-resource-policy");
    store
        .transition_state(&name, VmRuntimeState::Running)
        .unwrap();
    store
        .write_runner_metadata(
            &name,
            &RunnerMetadata {
                engine: "lightvm".to_string(),
                pid: Some(42),
                command: vec!["lightvm-runner".to_string()],
                log_path: PathBuf::from("logs/lightvm.log"),
                started_at_unix: now_unix(),
                dry_run: false,
                launch_spec_path: None,
                guest_tools: None,
                disk: None,
                active_disk: None,
                launch_readiness: None,
                runtime_control: None,
            },
        )
        .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::ReapplyRuntimeResources {
            name: name.clone(),
            visibility: RuntimeResourceVisibility::Background,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::RuntimeResourcePolicy { policy } = response else {
        panic!("expected runtime resource policy");
    };

    assert_eq!(policy.vm, name);
    assert_eq!(policy.visibility, RuntimeResourceVisibility::Background);
    assert_eq!(policy.state, VmRuntimeState::Running);
    assert!(!policy.on_battery);
    assert_eq!(policy.memory, "2048");
    assert_eq!(policy.cpu, "1");
    assert_eq!(policy.display_fps_cap, "10");
    assert!(!policy.live_applied);
    assert!(!policy.runtime_control_acknowledged);
    assert_eq!(
        policy.live_apply_blockers[0].code,
        "runtime-control-unavailable"
    );
    assert_eq!(
        store
            .runtime_resource_policy_metadata(&policy.vm)
            .unwrap()
            .as_ref(),
        Some(&policy)
    );
}

#[test]
fn handler_acknowledges_runtime_policy_when_display_control_reads_it() {
    let _battery = EnvVarGuard::set("BRIDGEVM_FORCE_ON_BATTERY", "0");
    let (store, name) = fast_test_store("runtime-resource-policy-ack");
    let socket_path = {
        let mut path = PathBuf::from("/tmp");
        path.push(format!(
            "bridgevm-api-policy-ack-{}-{}.sock",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        path
    };
    let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
    let server = std::thread::spawn({
        let expected_name = name.clone();
        move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut request)
                .unwrap();
            let request: serde_json::Value = serde_json::from_str(&request).unwrap();
            assert_eq!(
                request.get("command").and_then(serde_json::Value::as_str),
                Some("policy")
            );
            stream
                .write_all(
                    serde_json::json!({
                        "ok": true,
                        "policy": {
                            "vm": expected_name,
                            "visibility": "background",
                            "display_fps_cap": "10"
                        },
                        "supported_commands": ["status", "stop", "policy", "pacing"]
                    })
                    .to_string()
                    .as_bytes(),
                )
                .unwrap();
            stream.write_all(b"\n").unwrap();
        }
    });
    store
        .transition_state(&name, VmRuntimeState::Running)
        .unwrap();
    store
        .write_runner_metadata(
            &name,
            &RunnerMetadata {
                engine: "lightvm".to_string(),
                pid: Some(42),
                command: vec!["lightvm-runner".to_string()],
                log_path: PathBuf::from("logs/lightvm.log"),
                started_at_unix: now_unix(),
                dry_run: false,
                launch_spec_path: None,
                guest_tools: None,
                disk: None,
                active_disk: None,
                launch_readiness: None,
                runtime_control: Some(RuntimeControlMetadata {
                    kind: "apple-vz-display".to_string(),
                    socket_path: socket_path.clone(),
                    commands: vec![
                        "status".to_string(),
                        "stop".to_string(),
                        "policy".to_string(),
                        "pacing".to_string(),
                    ],
                }),
            },
        )
        .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::ReapplyRuntimeResources {
            name: name.clone(),
            visibility: RuntimeResourceVisibility::Background,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::RuntimeResourcePolicy { policy } = response else {
        panic!("expected runtime resource policy");
    };

    assert_eq!(policy.vm, name);
    assert_eq!(policy.visibility, RuntimeResourceVisibility::Background);
    assert!(policy.runtime_control_acknowledged);
    assert!(!policy.live_applied);
    assert_eq!(
        store
            .runtime_resource_policy_metadata(&policy.vm)
            .unwrap()
            .as_ref(),
        Some(&policy)
    );
    server.join().unwrap();
    let _ = fs::remove_file(socket_path);
}

#[test]
fn handler_sends_runtime_control_command_to_recorded_socket() {
    let (store, name) = fast_test_store("runtime-control-command");
    let socket_path = {
        let mut path = PathBuf::from("/tmp");
        path.push(format!(
            "bridgevm-api-rc-{}-{}.sock",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        path
    };
    let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
    let server = std::thread::spawn({
        let expected_name = name.clone();
        move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut request)
                .unwrap();
            let request: serde_json::Value = serde_json::from_str(&request).unwrap();
            assert_eq!(
                request.get("command").and_then(serde_json::Value::as_str),
                Some("status")
            );
            stream
                .write_all(
                    serde_json::json!({
                        "ok": true,
                        "vm": expected_name,
                        "state": "running",
                        "stopping": false,
                        "display": {"width": 1024, "height": 768},
                        "supported_commands": ["status", "stop", "policy", "pacing"]
                    })
                    .to_string()
                    .as_bytes(),
                )
                .unwrap();
            stream.write_all(b"\n").unwrap();
        }
    });

    store
        .write_runner_metadata(
            &name,
            &RunnerMetadata {
                engine: "lightvm".to_string(),
                pid: Some(42),
                command: vec!["lightvm-runner".to_string()],
                log_path: PathBuf::from("logs/lightvm.log"),
                started_at_unix: now_unix(),
                dry_run: false,
                launch_spec_path: None,
                guest_tools: None,
                disk: None,
                active_disk: None,
                launch_readiness: None,
                runtime_control: Some(RuntimeControlMetadata {
                    kind: "apple-vz-display".to_string(),
                    socket_path: socket_path.clone(),
                    commands: vec![
                        "status".to_string(),
                        "stop".to_string(),
                        "policy".to_string(),
                        "pacing".to_string(),
                    ],
                }),
            },
        )
        .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::RuntimeControl {
            name: name.clone(),
            command: "status".to_string(),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::RuntimeControl { control } = response else {
        panic!("expected runtime control response");
    };

    assert_eq!(control.vm, name);
    assert_eq!(control.kind, "apple-vz-display");
    assert_eq!(control.socket_path, socket_path);
    assert_eq!(control.command, "status");
    assert_eq!(
        control
            .response
            .get("state")
            .and_then(serde_json::Value::as_str),
        Some("running")
    );
    server.join().unwrap();
    let _ = fs::remove_file(socket_path);
}
