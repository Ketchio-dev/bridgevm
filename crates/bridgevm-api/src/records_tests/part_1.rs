//! Tests split so no file exceeds 1000 lines.

use crate::test_support::*;

#[test]
fn lifecycle_plan_response_round_trips_as_json() {
    let response = BridgeVmResponse::LifecyclePlan {
        plan: LifecyclePlanRecord {
            vm: "legacy".to_string(),
            action: LifecycleAction::Resume,
            current_state: VmRuntimeState::Suspended,
            target_state: VmRuntimeState::Running,
            backend: "qemu-qmp".to_string(),
            metadata_only: true,
            executable: true,
            qmp_command: Some("cont".to_string()),
            socket_path: Some(PathBuf::from("/tmp/bridgevm/legacy/metadata/qmp.sock")),
            socket_available: true,
            qmp_supervisor: None,
            blockers: Vec::new(),
            notes: vec!["metadata-only lifecycle plan; no backend command was sent".to_string()],
        },
    };
    let json = serde_json::to_string(&response).unwrap();
    let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(response, decoded);
}

#[test]
fn snapshot_preflight_status_request_and_response_round_trip_as_json() {
    let request = BridgeVmRequest::SnapshotPreflightStatus {
        name: "dev".to_string(),
        consistency: SnapshotConsistency::ApplicationConsistent,
    };
    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("snapshot_preflight_status"));
    assert!(json.contains("application-consistent"));
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);

    let response = BridgeVmResponse::SnapshotPreflightStatus {
        preflight: SnapshotPreflightStatusRecord {
            vm: "dev".to_string(),
            consistency: SnapshotConsistency::ApplicationConsistent,
            backend_freeze_thaw_supported: false,
            guest_tools_connected: true,
            capabilities: vec!["fs-freeze".to_string(), "fs-thaw".to_string()],
            ready: false,
            blockers: vec![SnapshotPreflightBlockerRecord {
                code: "backend-freeze-thaw-unavailable".to_string(),
                message: "Freeze/thaw orchestration requires the bridgevmd-owned running backend; this offline preflight cannot drive the guest agent.".to_string(),
                path: None,
            }],
            checked_at_unix: 1,
        },
    };
    let json = serde_json::to_string(&response).unwrap();
    let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(response, decoded);
}

#[test]
fn application_consistent_snapshot_execution_request_and_response_round_trip_as_json() {
    let request = BridgeVmRequest::ExecuteApplicationConsistentSnapshot {
        vm: "dev".to_string(),
        name: "before-upgrade".to_string(),
        freeze_timeout_millis: Some(5_000),
    };
    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("execute_application_consistent_snapshot"));
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);

    let response = BridgeVmResponse::ApplicationConsistentSnapshotExecution {
        execution: ApplicationConsistentSnapshotExecutionRecord {
            vm: "dev".to_string(),
            snapshot: "before-upgrade".to_string(),
            freeze_request_id: "freeze-1".to_string(),
            thaw_request_id: "thaw-1".to_string(),
            pending_commands_after_freeze: 1,
            pending_commands_after_thaw: 2,
            snapshot_created_at_unix: 42,
            freeze_result: ApplicationConsistentSnapshotCommandResultRecord {
                request_id: "freeze-1".to_string(),
                capability: Some("fs-freeze".to_string()),
                ok: true,
                error_code: None,
                message: Some("freeze scaffold acknowledged".to_string()),
                completed_at_unix: 40,
            },
            thaw_result: ApplicationConsistentSnapshotCommandResultRecord {
                request_id: "thaw-1".to_string(),
                capability: Some("fs-thaw".to_string()),
                ok: true,
                error_code: None,
                message: Some("thaw scaffold acknowledged".to_string()),
                completed_at_unix: 41,
            },
            preflight_ready: true,
            note: "scaffold boundary".to_string(),
        },
    };
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("application_consistent_snapshot_execution"));
    let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(response, decoded);
}

#[test]
fn network_planned_response_round_trips_as_json() {
    let response = BridgeVmResponse::NetworkPlanned {
        plan: NetworkPlanRecord {
            vm: "dev".to_string(),
            backend: "qemu".to_string(),
            mode: "bridged".to_string(),
            hostname: "dev.bridgevm.local".to_string(),
            dry_run: true,
            executable: false,
            port_forwards: Vec::new(),
            capabilities: Some(NetworkCapabilitiesRecord {
                guest_outbound: true,
                host_to_guest: true,
                guest_to_host: true,
                host_visible_hostname: true,
                supports_port_forwarding: false,
                requires_privileged_helper: true,
            }),
            blockers: vec![NetworkPlanBlockerRecord {
                code: "qemu-bridged-requires-privilege".to_string(),
                message: "Compatibility Mode QEMU bridged networking uses vmnet-bridged, which requires the qemu process to run as root or carry the com.apple.vm.networking entitlement"
                    .to_string(),
            }],
            notes: vec!["dry-run network plan".to_string()],
        },
    };
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("network_planned"));
    let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(response, decoded);
}

#[test]
fn performance_sample_response_round_trips_as_json() {
    let response = BridgeVmResponse::PerformanceSample {
        sample: PerformanceSampleMetadata {
            vm: "dev".to_string(),
            source: PathBuf::from("/tmp/dev.vmbridge"),
            output: PathBuf::from("/tmp/performance"),
            artifact: PathBuf::from("/tmp/performance/performance-sample.json"),
            probe: PathBuf::from("/tmp/performance/probe-1.bin"),
            probes: vec![
                PathBuf::from("/tmp/performance/probe-1.bin"),
                PathBuf::from("/tmp/performance/probe-2.bin"),
            ],
            artifact_bytes: 4096,
            iterations: 2,
            sync: true,
            iteration_results: vec![
                PerformanceSampleIterationRecord {
                    iteration: 1,
                    probe: PathBuf::from("/tmp/performance/probe-1.bin"),
                    bytes: 4096,
                    write_latency_microseconds: 120,
                    sync: true,
                },
                PerformanceSampleIterationRecord {
                    iteration: 2,
                    probe: PathBuf::from("/tmp/performance/probe-2.bin"),
                    bytes: 4096,
                    write_latency_microseconds: 110,
                    sync: true,
                },
            ],
            created_at_unix: 42,
            state: VmRuntimeMetadata {
                state: VmRuntimeState::Running,
                updated_at_unix: 40,
            },
            runner: None,
            guest_tools: GuestToolsStatusRecord {
                vm: "dev".to_string(),
                tools: "bridgevm-agent".to_string(),
                token_created_at_unix: 39,
                capabilities: Vec::new(),
                approved_shared_folders: Vec::new(),
                runtime: None,
            },
            metrics: Some(GuestToolsMetricsMetadata {
                cpu_percent: 7,
                memory_used_mib: 512,
                updated_at_unix: 41,
            }),
            measurements: vec![PerformanceMeasurementRecord {
                name: "sample_write_latency_microseconds".to_string(),
                value: 115,
                unit: "microseconds".to_string(),
                source: "bridgevm.performance_sample".to_string(),
                metadata_only: false,
            }],
            notes: vec!["metadata-safe performance sample".to_string()],
        },
    };
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("performance_sample"));
    let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(response, decoded);
}

#[test]
fn readiness_report_request_and_response_round_trips_as_json() {
    let request = BridgeVmRequest::ReadinessReport {
        name: "ubuntu".to_string(),
        live_evidence: None,
        record_live_evidence: false,
        clear_live_evidence: false,
    };
    let json = serde_json::to_string(&request).unwrap();
    assert_eq!(json, r#"{"type":"readiness_report","name":"ubuntu"}"#);
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);

    let request = BridgeVmRequest::ReadinessReport {
        name: "ubuntu".to_string(),
        live_evidence: Some(PathBuf::from("/tmp/live-evidence")),
        record_live_evidence: true,
        clear_live_evidence: false,
    };
    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains(r#""record_live_evidence":true"#));
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);

    let request = BridgeVmRequest::ReadinessReport {
        name: "ubuntu".to_string(),
        live_evidence: None,
        record_live_evidence: false,
        clear_live_evidence: true,
    };
    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains(r#""clear_live_evidence":true"#));
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);

    let response = BridgeVmResponse::ReadinessReport {
        report: VmReadinessReport {
            vm: "ubuntu".to_string(),
            mode: VmMode::Fast,
            state: VmRuntimeState::Stopped,
            metadata_only: true,
            live_e2e_required: true,
            live_evidence: None,
            evidence_requirements: vec![VmEvidenceRequirement {
                kind: "live-boot".to_string(),
                required: true,
                proven: false,
                note: "requires preserved opt-in serial or graphical boot progress evidence from Apple VZ or QEMU"
                    .to_string(),
            }],
            boot_media: None,
            boot_media_error: Some("missing boot metadata".to_string()),
            snapshot_chain: None,
            snapshot_chain_error: None,
            runner: None,
            pre_run_launch_readiness: Some(LaunchReadinessMetadata {
                ready: false,
                blockers: vec![LaunchReadinessBlockerMetadata {
                    code: "missing-primary-disk".to_string(),
                    message: "primary disk is missing".to_string(),
                    path: None,
                    capability: None,
                }],
            }),
            qmp_supervisor: None,
            runner_error: None,
            blockers: vec!["boot-media-status-error:missing boot metadata".to_string()],
            notes: vec![
                "metadata-only preflight report; no VM, QEMU, Apple VZ, console, or guest workload was started".to_string(),
                "live E2E boot, console, and guest-tools effects still require the explicit opt-in live smoke evidence path".to_string(),
            ],
        },
    };
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("readiness_report"));
    let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(response, decoded);
}

#[test]
fn live_boot_requirement_needs_progress_evidence_not_only_a_bundle() {
    let evidence =
        |serial: bool, graphical_boot: bool, viewer: bool, qmp: bool| VmLiveEvidenceVerification {
            path: PathBuf::from("/tmp/evidence"),
            backend: "apple-virtualization-framework".to_string(),
            vm_name: "ubuntu".to_string(),
            boot_mode: "linux-kernel".to_string(),
            disk_format: "raw".to_string(),
            network: "nat".to_string(),
            serial_sentinel_required: serial,
            serial_sentinel_proven: serial,
            graphical_boot_progress_proven: graphical_boot,
            viewer_evidence_proven: viewer,
            qmp_evidence_proven: qmp,
            guest_tools_effects_proven: false,
            summary: "synthetic test evidence".to_string(),
        };

    let launch_only = evidence(false, false, false, false);
    let launch_only_requirements = metadata_safe_live_evidence_requirements(Some(&launch_only));
    assert!(launch_only_requirements.iter().any(|requirement| {
        requirement.kind == "live-boot" && requirement.required && !requirement.proven
    }));

    let serial_progress = evidence(true, false, false, false);
    let serial_requirements = metadata_safe_live_evidence_requirements(Some(&serial_progress));
    assert!(serial_requirements.iter().any(|requirement| {
        requirement.kind == "live-boot" && requirement.required && requirement.proven
    }));

    let graphical_progress = evidence(false, true, false, false);
    let graphical_requirements =
        metadata_safe_live_evidence_requirements(Some(&graphical_progress));
    assert!(graphical_requirements.iter().any(|requirement| {
        requirement.kind == "live-boot" && requirement.required && requirement.proven
    }));

    for console_only_evidence in [
        evidence(false, false, true, false),
        evidence(false, false, false, true),
    ] {
        let requirements = metadata_safe_live_evidence_requirements(Some(&console_only_evidence));
        assert!(requirements.iter().any(|requirement| {
            requirement.kind == "live-boot" && requirement.required && !requirement.proven
        }));
        assert!(requirements.iter().any(|requirement| {
            requirement.kind == "console" && requirement.required && requirement.proven
        }));
    }
}

#[test]
fn handler_imports_template_boot_media() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-media-import-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let source = root.join("source.iso");
    fs::create_dir_all(&root).unwrap();
    fs::write(&source, b"fake installer").unwrap();
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
        BridgeVmRequest::ImportBootMedia {
            name: "ubuntu".to_string(),
            source: source.clone(),
            kind: None,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::BootMediaImported { import } = response else {
        panic!("expected boot media import response");
    };

    assert_eq!(import.vm, "ubuntu");
    assert_eq!(import.kind, BootMediaKind::InstallerImage);
    assert_eq!(import.source, source);
    assert!(import.destination.ends_with("installers/ubuntu-arm64.iso"));
    assert_eq!(import.bytes, 14);
    assert!(!import.replaced);
    assert!(import.imported_at_unix > 0);
    assert_eq!(fs::read(&import.destination).unwrap(), b"fake installer");
    assert!(store
        .root()
        .join("vms")
        .join("ubuntu.vmbridge")
        .join("metadata")
        .join("boot-media")
        .join("installer-image.json")
        .exists());

    let response = handle_request(
        &store,
        BridgeVmRequest::InspectBootMedia {
            name: "ubuntu".to_string(),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::BootMedia { boot, .. } = response else {
        panic!("expected boot media response");
    };
    assert!(boot.installer_image.unwrap().exists);

    let response = handle_request(
        &store,
        BridgeVmRequest::InspectBootMediaStatus {
            name: "ubuntu".to_string(),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::BootMediaStatus { status } = response else {
        panic!("expected boot media status response");
    };
    assert_eq!(status.vm, "ubuntu");
    assert_eq!(status.entries.len(), 1);
    let entry = &status.entries[0];
    assert_eq!(entry.kind, BootMediaKind::InstallerImage);
    assert!(entry.exists);
    assert_eq!(entry.bytes, Some(14));
    assert_eq!(entry.last_import.as_ref().unwrap().bytes, 14);

    let expected_sha256 = sha256_file(&entry.path).unwrap();
    let response = handle_request(
        &store,
        BridgeVmRequest::VerifyBootMedia {
            name: "ubuntu".to_string(),
            expected_sha256: expected_sha256.clone(),
            kind: None,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::BootMediaVerified { verification } = response else {
        panic!("expected boot media verification response");
    };
    assert_eq!(verification.kind, BootMediaKind::InstallerImage);
    assert_eq!(verification.expected_sha256, expected_sha256);
    assert_eq!(verification.actual_sha256, expected_sha256);
    assert!(verification.verified);
    assert!(store
        .root()
        .join("vms")
        .join("ubuntu.vmbridge")
        .join("metadata")
        .join("boot-media")
        .join("installer-image-verify.json")
        .exists());

    let response = handle_request(
        &store,
        BridgeVmRequest::InspectBootMediaStatus {
            name: "ubuntu".to_string(),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::BootMediaStatus { status } = response else {
        panic!("expected boot media status response");
    };
    let verification = status.entries[0].last_verification.as_ref().unwrap();
    assert!(verification.verified);
    assert_eq!(verification.actual_sha256, expected_sha256);

    let response = handle_request(
        &store,
        BridgeVmRequest::PlanBootMediaDownload {
            name: "ubuntu".to_string(),
            url: "https://example.invalid/ubuntu.iso".to_string(),
            expected_sha256: Some(expected_sha256.clone()),
            kind: None,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::BootMediaDownloadPlanned { plan } = response else {
        panic!("expected boot media download plan response");
    };
    assert_eq!(plan.vm, "ubuntu");
    assert_eq!(plan.kind, BootMediaKind::InstallerImage);
    assert_eq!(plan.url, "https://example.invalid/ubuntu.iso");
    assert_eq!(
        plan.expected_sha256.as_deref(),
        Some(expected_sha256.as_str())
    );
    assert!(plan.exists);
    assert_eq!(plan.bytes, Some(14));
    assert!(plan.last_import.is_some());
    assert!(plan.last_verification.is_some());
    assert!(store
        .root()
        .join("vms")
        .join("ubuntu.vmbridge")
        .join("metadata")
        .join("boot-media")
        .join("installer-image-download.json")
        .exists());

    let response = handle_request(
        &store,
        BridgeVmRequest::InspectBootMediaStatus {
            name: "ubuntu".to_string(),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::BootMediaStatus { status } = response else {
        panic!("expected boot media status response");
    };
    let download_plan = status.entries[0].last_download_plan.as_ref().unwrap();
    assert_eq!(download_plan.url, "https://example.invalid/ubuntu.iso");
    assert_eq!(
        download_plan.expected_sha256.as_deref(),
        Some(expected_sha256.as_str())
    );
}

#[test]
fn handler_executes_planned_boot_media_download_and_reports_status() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "bridgevm-api-media-download-test-{}",
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

    let body = b"downloaded installer";
    let expected_sha256 = format!("{:x}", Sha256::digest(body));
    let (url, server) = serve_one_http_response(body);

    let response = handle_request(
        &store,
        BridgeVmRequest::PlanBootMediaDownload {
            name: "ubuntu".to_string(),
            url: url.clone(),
            expected_sha256: Some(expected_sha256.clone()),
            kind: None,
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::BootMediaDownloadPlanned { plan } = response else {
        panic!("expected boot media download plan response");
    };
    assert_eq!(plan.url, url);
    assert!(!plan.exists);
    assert_eq!(plan.bytes, None);

    let response = handle_request(
        &store,
        BridgeVmRequest::DownloadBootMedia {
            name: "ubuntu".to_string(),
            kind: None,
        },
    )
    .into_result()
    .unwrap();
    server.join().expect("http test server should finish");
    let BridgeVmResponse::BootMediaDownloaded { download } = response else {
        panic!("expected boot media downloaded response");
    };
    assert_eq!(download.vm, "ubuntu");
    assert_eq!(download.kind, BootMediaKind::InstallerImage);
    assert_eq!(download.url, plan.url);
    assert_eq!(download.destination, plan.destination);
    assert_eq!(fs::read(&download.destination).unwrap(), body);
    assert_eq!(download.bytes, Some(body.len() as u64));
    assert!(!download.replaced);
    assert_eq!(
        download.expected_sha256.as_deref(),
        Some(expected_sha256.as_str())
    );
    assert_eq!(
        download.actual_sha256.as_deref(),
        Some(expected_sha256.as_str())
    );
    assert_eq!(download.verified, Some(true));
    assert!(download.downloaded);
    assert!(download.downloaded_at_unix > 0);
    assert!(store
        .root()
        .join("vms")
        .join("ubuntu.vmbridge")
        .join("metadata")
        .join("boot-media")
        .join("installer-image-download-result.json")
        .exists());

    let response = handle_request(
        &store,
        BridgeVmRequest::InspectBootMediaStatus {
            name: "ubuntu".to_string(),
        },
    )
    .into_result()
    .unwrap();
    let BridgeVmResponse::BootMediaStatus { status } = response else {
        panic!("expected boot media status response");
    };
    let entry = &status.entries[0];
    assert!(entry.exists);
    assert_eq!(entry.bytes, Some(body.len() as u64));
    let last_download = entry.last_download.as_ref().unwrap();
    assert!(last_download.downloaded);
    assert_eq!(last_download.verified, Some(true));
    assert_eq!(
        last_download.actual_sha256.as_deref(),
        Some(expected_sha256.as_str())
    );
}
