//! Tests split so no file exceeds 1000 lines.

use crate::test_support::*;

#[test]
fn handler_rejects_ssh_plan_without_target() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-ssh-missing-test-{}",
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
        BridgeVmRequest::SshPlan {
            name: "dev".to_string(),
            user: None,
        },
    );
    let BridgeVmResponse::Error { message } = response else {
        panic!("expected error response");
    };
    assert!(message.contains("no SSH target available"));
}

#[test]
fn handler_creates_redacted_diagnostic_bundle() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-diagnostics-test-{}",
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
    let token = store.guest_tools_token("legacy").unwrap().token;
    let bundle = store.bundle_path("legacy");
    fs::write(
        bundle.join("logs").join("qemu.log"),
        format!("booted with token {token}\n"),
    )
    .unwrap();
    fs::write(
        bundle.join("metadata").join("secrets.json"),
        r#"{"password":"open-sesame","nested":{"authorization":"Bearer abc"}}"#,
    )
    .unwrap();
    fs::create_dir_all(bundle.join("metadata").join("boot-media")).unwrap();
    fs::write(
        bundle.join("metadata").join("boot-media").join("download-plan.json"),
        r#"{"url":"https://example.invalid/ubuntu.iso?sig=secret#section","command":["curl","https://example.invalid/ubuntu.iso?sig=secret"]}"#,
    )
    .unwrap();
    fs::write(
        bundle.join("metadata").join("qmp-supervisor.json"),
        r#"{"events":[{"event":"RESUME"}],"terminal_event":null,"envelopes_read":1,"limit_reached":false,"updated_at_unix":1}"#,
    )
    .unwrap();
    let oversized_log = bundle.join("logs").join("oversized.log");
    let mut oversized = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&oversized_log)
        .unwrap();
    oversized.write_all(token.as_bytes()).unwrap();
    oversized.set_len(MAX_DIAGNOSTIC_FILE_BYTES + 1).unwrap();
    fs::write(bundle.join("metadata").join("diagnostics.lock"), "locked").unwrap();
    fs::create_dir_all(bundle.join("disks")).unwrap();
    fs::write(bundle.join("disks").join("root.qcow2"), "not copied").unwrap();

    let output = store.root().join("diagnostics-output");
    let response = handle_request(
        &store,
        BridgeVmRequest::CreateDiagnosticBundle {
            name: "legacy".to_string(),
            output,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::DiagnosticBundle { bundle } = response else {
        panic!("expected diagnostic bundle response");
    };

    assert!(bundle.output.exists());
    assert!(bundle.files.contains(&PathBuf::from("manifest.yaml")));
    assert!(bundle
        .files
        .contains(&PathBuf::from("metadata/guest-tools-token.json")));
    assert!(bundle.files.contains(&PathBuf::from("logs/qemu.log")));
    assert!(bundle.files.contains(&PathBuf::from("logs/oversized.log")));
    assert!(bundle
        .files
        .contains(&PathBuf::from("metadata/qmp-supervisor.json")));
    assert!(bundle
        .files
        .contains(&PathBuf::from("diagnostic-bundle.json")));
    assert!(!bundle
        .files
        .contains(&PathBuf::from("metadata/diagnostics.lock")));
    assert!(!bundle.files.contains(&PathBuf::from("disks/root.qcow2")));
    for file in &bundle.files {
        assert!(
            file.is_relative(),
            "diagnostic metadata should only report relative paths: {}",
            file.display()
        );
        assert!(
            !file
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir)),
            "diagnostic metadata should not report parent-directory paths: {}",
            file.display()
        );
        let file_name = file
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        assert!(
            !file_name.ends_with(".sock") && !file_name.ends_with(".lock"),
            "diagnostic metadata should not report socket or lock files: {}",
            file.display()
        );
    }

    for file in &bundle.files {
        let content = fs::read_to_string(bundle.output.join(file)).unwrap();
        assert!(
            !content.contains(&token),
            "diagnostic file leaked guest tools token: {}",
            file.display()
        );
    }
    let token_metadata =
        fs::read_to_string(bundle.output.join("metadata/guest-tools-token.json")).unwrap();
    assert!(token_metadata.contains("<redacted>"));
    let log = fs::read_to_string(bundle.output.join("logs/qemu.log")).unwrap();
    assert!(log.contains("<redacted>"));
    let oversized_log = fs::read_to_string(bundle.output.join("logs/oversized.log")).unwrap();
    assert!(oversized_log.contains("diagnostic file omitted"));
    assert!(oversized_log.contains("16777216-byte safety limit"));
    assert!(!oversized_log.contains(&token));
    let secrets = fs::read_to_string(bundle.output.join("metadata/secrets.json")).unwrap();
    assert!(!secrets.contains("open-sesame"));
    assert!(!secrets.contains("Bearer abc"));
    assert!(secrets.contains("<redacted>"));
    let download_plan = fs::read_to_string(
        bundle
            .output
            .join("metadata")
            .join("boot-media")
            .join("download-plan.json"),
    )
    .unwrap();
    assert!(!download_plan.contains("sig=secret"));
    assert!(download_plan.contains("https://example.invalid/ubuntu.iso?<redacted>#section"));
    assert!(download_plan.contains("https://example.invalid/ubuntu.iso?<redacted>"));
}

#[test]
fn handler_creates_metadata_only_performance_baseline() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-performance-test-{}",
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

    let bundle = store.bundle_path("dev");
    let runner = RunnerMetadata {
        engine: "lightvm".to_string(),
        pid: Some(42),
        command: vec![
            "lightvm-runner".to_string(),
            "--vm".to_string(),
            "dev".to_string(),
        ],
        log_path: bundle.join("logs").join("runner.log"),
        started_at_unix: 10,
        dry_run: false,
        launch_spec_path: None,
        guest_tools: None,
        disk: None,
        active_disk: None,
        launch_readiness: None,
        runtime_control: None,
    };
    store.write_runner_metadata("dev", &runner).unwrap();
    store
        .write_guest_tools_runtime_metadata(
            "dev",
            &GuestToolsRuntimeMetadata {
                connected: true,
                guest_os: Some("linux".to_string()),
                agent_version: Some("1.0.0".to_string()),
                capabilities: vec!["guest-metrics".to_string()],
                last_heartbeat_at_unix: Some(11),
                guest_ip_addresses: vec![GuestToolsIpAddressMetadata {
                    address: "10.0.2.15".to_string(),
                    interface: Some("eth0".to_string()),
                }],
                shared_folders: Vec::new(),
                metrics: Some(GuestToolsMetricsMetadata {
                    cpu_percent: 7,
                    memory_used_mib: 512,
                    updated_at_unix: 12,
                }),
                last_command_result: None,
                agent_update: None,
                clipboard: None,
                updated_at_unix: 13,
            },
        )
        .unwrap();

    let output = store.root().join("performance-output");
    let response = handle_request(
        &store,
        BridgeVmRequest::CreatePerformanceBaseline {
            name: "dev".to_string(),
            output,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::PerformanceBaseline { baseline } = response else {
        panic!("expected performance baseline response");
    };

    assert!(baseline.output.exists());
    assert!(baseline.artifact.exists());
    assert!(baseline.metadata_only);
    assert_eq!(baseline.state.state, VmRuntimeState::Running);
    assert_eq!(baseline.runner.as_ref().unwrap().engine, "lightvm");
    assert_eq!(baseline.metrics.as_ref().unwrap().cpu_percent, 7);
    assert_eq!(baseline.metrics.as_ref().unwrap().memory_used_mib, 512);
    assert_measurement(&baseline.measurements, "guest_cpu_percent", 7, "percent");
    assert_measurement(&baseline.measurements, "guest_memory_used_mib", 512, "MiB");
    assert!(baseline
        .measurements
        .iter()
        .any(|measurement| measurement.name == "runner_observed_uptime_seconds"));
    assert!(baseline
        .notes
        .iter()
        .any(|note| note.contains("metadata-only")));

    let artifact = fs::read_to_string(baseline.output.join("performance-baseline.json")).unwrap();
    let decoded: PerformanceBaselineMetadata = serde_json::from_str(&artifact).unwrap();
    assert_eq!(decoded.vm, "dev");
    assert_eq!(decoded.runner.unwrap().engine, "lightvm");
    assert_measurement(&decoded.measurements, "guest_cpu_percent", 7, "percent");
    assert_eq!(decoded.metrics.unwrap().memory_used_mib, 512);
}

#[test]
fn handler_creates_host_side_performance_sample() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-performance-sample-test-{}",
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

    let output = store.root().join("performance-sample-output");
    let response = handle_request(
        &store,
        BridgeVmRequest::CreatePerformanceSample {
            name: "dev".to_string(),
            output,
            artifact_bytes: Some(4096),
            iterations: Some(3),
            sync: false,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::PerformanceSample { sample } = response else {
        panic!("expected performance sample response");
    };

    assert!(sample.output.exists());
    assert!(sample.artifact.exists());
    assert!(sample.probe.exists());
    assert_eq!(sample.probes.len(), 3);
    assert_eq!(sample.iteration_results.len(), 3);
    assert_eq!(sample.artifact_bytes, 4096);
    assert_eq!(sample.iterations, 3);
    assert!(!sample.sync);
    for probe in &sample.probes {
        assert!(probe.exists());
        assert_eq!(fs::metadata(probe).unwrap().len(), 4096);
    }
    assert_non_metadata_measurement(
        &sample.measurements,
        "host_artifact_write_bytes",
        4096,
        "bytes",
    );
    assert_non_metadata_measurement(
        &sample.measurements,
        "host_artifact_write_iterations",
        3,
        "count",
    );
    assert_non_metadata_measurement(
        &sample.measurements,
        "host_artifact_write_total_bytes",
        12_288,
        "bytes",
    );
    assert_non_metadata_measurement_exists(
        &sample.measurements,
        "host_artifact_write_latency_microseconds",
        "microseconds",
    );
    assert_non_metadata_measurement_exists(
        &sample.measurements,
        "host_artifact_write_latency_p50_microseconds",
        "microseconds",
    );
    assert_non_metadata_measurement_exists(
        &sample.measurements,
        "bridgevm_state_read_latency_microseconds",
        "microseconds",
    );
    assert_non_metadata_measurement_exists(
        &sample.measurements,
        "bridgevm_runner_metadata_read_latency_microseconds",
        "microseconds",
    );
    assert_non_metadata_measurement_exists(
        &sample.measurements,
        "bridgevm_guest_tools_status_inspect_latency_microseconds",
        "microseconds",
    );
    assert!(!sample
        .measurements
        .iter()
        .any(|measurement| measurement.name == "disk_inspect_duration_microseconds"));
    assert!(sample
        .notes
        .iter()
        .any(|note| note.contains("disk inspect duration skipped")));
    assert!(sample.measurements.iter().any(|measurement| {
        measurement.name == "sample_generation_duration_microseconds"
            && measurement.unit == "microseconds"
            && !measurement.metadata_only
    }));

    let artifact = fs::read_to_string(sample.output.join("performance-sample.json")).unwrap();
    let decoded: PerformanceSampleMetadata = serde_json::from_str(&artifact).unwrap();
    assert_eq!(decoded.vm, "dev");
    assert_eq!(decoded.probes.len(), 3);
    assert_eq!(decoded.iteration_results.len(), 3);
    assert_non_metadata_measurement(
        &decoded.measurements,
        "host_artifact_write_bytes",
        4096,
        "bytes",
    );
}

#[test]
fn handler_prepares_fast_run_without_qemu() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-fast-prepare-test-{}",
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

    let response = handle_request(
        &store,
        BridgeVmRequest::PrepareRun {
            name: "fast-linux".to_string(),
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
    assert_eq!(metadata.engine, "lightvm");
    assert_eq!(metadata.command.first().unwrap(), "lightvm-runner");
    let readiness = metadata
        .launch_readiness
        .expect("Fast Mode runner metadata includes launch readiness");
    assert!(!readiness.ready);
    assert!(readiness
        .blockers
        .iter()
        .any(|blocker| blocker.code == "missing-primary-disk"));
}

#[test]
fn handler_plans_lifecycle_qmp_without_connecting_to_backend() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-lifecycle-plan-test-{}",
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
    handle_request(
        &store,
        BridgeVmRequest::TransitionVm {
            name: "legacy".to_string(),
            state: VmRuntimeState::Running,
        },
    )
    .into_result()
    .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::LifecyclePlan {
            name: "legacy".to_string(),
            action: LifecycleAction::Suspend,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::LifecyclePlan { plan } = response else {
        panic!("expected lifecycle plan");
    };
    assert_eq!(plan.backend, "qemu-qmp");
    assert_eq!(plan.qmp_command.as_deref(), Some("stop"));
    assert!(!plan.socket_available);
    assert!(!plan.executable);
    assert!(plan
        .blockers
        .iter()
        .any(|blocker| blocker.starts_with("qmp-socket-unavailable:")));

    let socket_path = plan.socket_path.clone().expect("qmp socket path");
    fs::write(&socket_path, b"fake qmp presence marker").unwrap();
    let response = handle_request(
        &store,
        BridgeVmRequest::LifecyclePlan {
            name: "legacy".to_string(),
            action: LifecycleAction::Suspend,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::LifecyclePlan { plan } = response else {
        panic!("expected lifecycle plan");
    };
    assert!(plan.socket_available);
    assert!(plan.executable);
    assert!(plan.blockers.is_empty());
}

#[test]
fn handler_plans_fast_lifecycle_as_metadata_only_blocked() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-fast-lifecycle-plan-test-{}",
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

    let response = handle_request(
        &store,
        BridgeVmRequest::LifecyclePlan {
            name: "fast-linux".to_string(),
            action: LifecycleAction::Suspend,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::LifecyclePlan { plan } = response else {
        panic!("expected lifecycle plan");
    };
    assert_eq!(plan.backend, "apple-vz");
    assert!(plan.metadata_only);
    // Not executable here because Stopped -> Suspended is an invalid direct
    // transition (the suspend backend itself goes Stopped -> Running ->
    // Suspended). Fast suspend/resume is no longer reported as unimplemented.
    assert!(!plan.executable);
    assert!(!plan
        .blockers
        .contains(&"fast-mode-suspend-resume-backend-unimplemented".to_string()));
    assert!(plan
        .blockers
        .iter()
        .any(|blocker| blocker.starts_with("invalid-lifecycle-transition:")));

    let response = handle_request(
        &store,
        BridgeVmRequest::LifecyclePlan {
            name: "fast-linux".to_string(),
            action: LifecycleAction::Resume,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::LifecyclePlan { plan } = response else {
        panic!("expected lifecycle plan");
    };
    assert_eq!(plan.target_state, VmRuntimeState::Running);
    assert!(!plan.executable);
    assert!(plan
        .blockers
        .iter()
        .any(|blocker| blocker == "invalid-lifecycle-transition:stopped->running"));
}

#[test]
fn handler_fast_lifecycle_plan_requires_existing_runner_for_valid_transition() {
    let _guard = APPLE_VZ_RUNNER_ENV_LOCK.lock().unwrap();
    let _env = EnvVarGuard::capture("BRIDGEVM_APPLE_VZ_RUNNER");
    std::env::remove_var("BRIDGEVM_APPLE_VZ_RUNNER");

    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-fast-lifecycle-runner-plan-test-{}",
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
    handle_request(
        &store,
        BridgeVmRequest::TransitionVm {
            name: "fast-linux".to_string(),
            state: VmRuntimeState::Running,
        },
    )
    .into_result()
    .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::LifecyclePlan {
            name: "fast-linux".to_string(),
            action: LifecycleAction::Suspend,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::LifecyclePlan { plan } = response else {
        panic!("expected lifecycle plan");
    };
    assert_eq!(plan.backend, "apple-vz");
    assert!(!plan.executable);
    assert!(plan.blockers.iter().any(|blocker| {
        blocker.starts_with("apple-vz-runner-unavailable:set BRIDGEVM_APPLE_VZ_RUNNER")
    }));

    let runner = store.root().join("AppleVzRunner");
    fs::write(&runner, b"fake runner").unwrap();
    std::env::set_var("BRIDGEVM_APPLE_VZ_RUNNER", &runner);

    let response = handle_request(
        &store,
        BridgeVmRequest::LifecyclePlan {
            name: "fast-linux".to_string(),
            action: LifecycleAction::Suspend,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::LifecyclePlan { plan } = response else {
        panic!("expected lifecycle plan");
    };
    assert!(plan.executable);
    assert!(plan.blockers.is_empty());

    let _ = std::fs::remove_dir_all(store.root());
}

#[test]
fn handler_stops_dry_run_backend() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-stop-test-{}",
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
    handle_request(
        &store,
        BridgeVmRequest::RunBackend {
            name: "legacy".to_string(),
            spawn: false,
        },
    )
    .into_result()
    .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::StopBackend {
            name: "legacy".to_string(),
        },
    )
    .into_result()
    .unwrap();
    assert_eq!(
        response,
        BridgeVmResponse::RunnerStatus {
            metadata: None,
            qmp_supervisor: None
        }
    );
    assert_eq!(store.runner_metadata("legacy").unwrap(), None);
    assert_eq!(
        store.state("legacy").unwrap().state,
        VmRuntimeState::Stopped
    );
}

#[test]
fn handler_restores_snapshot_metadata() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-restore-test-{}",
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
    let disk = store.prepare_primary_disk("dev").unwrap();
    fs::write(&disk.path, b"fake backing").unwrap();
    handle_request(
        &store,
        BridgeVmRequest::CreateSnapshot {
            vm: "dev".to_string(),
            name: "before-upgrade".to_string(),
            kind: SnapshotKind::Disk,
        },
    )
    .into_result()
    .unwrap();

    let response = handle_request(
        &store,
        BridgeVmRequest::RestoreSnapshot {
            vm: "dev".to_string(),
            name: "before-upgrade".to_string(),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::SnapshotRestored { restore } = response else {
        panic!("expected snapshot restore response");
    };
    assert_eq!(restore.snapshot, "before-upgrade");
    assert_eq!(restore.restored_state, VmRuntimeState::Stopped);
    assert!(restore.active_disk.is_some());
}
