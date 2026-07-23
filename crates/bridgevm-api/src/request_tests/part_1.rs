//! Tests split so no file exceeds 1000 lines.

use crate::test_support::*;

#[test]
fn service_contract_does_not_wrap_existing_json_request_shape() {
    let request = BridgeVmRequest::ListVms;
    let json = serde_json::to_string(&request).unwrap();

    assert_eq!(json, r#"{"type":"list_vms"}"#);
    assert!(!json.contains("schema_id"));
    assert!(!json.contains("version"));

    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn create_vm_request_keeps_wire_shape_while_bounding_enum_size() {
    let manifest = VmManifest::new(
        "wire-shape",
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: Some("24.04".to_string()),
            arch: "arm64".to_string(),
        },
        "32GiB",
    );
    let request = BridgeVmRequest::create_vm(manifest.clone());

    // Box is an in-memory implementation detail: peers still receive the
    // manifest object directly under the existing `manifest` key.
    let json = serde_json::to_value(&request).unwrap();
    assert_eq!(json["type"], "create_vm");
    assert_eq!(json["manifest"]["name"], manifest.name);
    assert_eq!(json["manifest"]["guest"]["os"], manifest.guest.os);
    assert!(json["manifest"].get("value").is_none());
    assert_eq!(
        serde_json::from_value::<BridgeVmRequest>(json).unwrap(),
        request
    );

    assert!(std::mem::size_of::<BridgeVmRequest>() <= 256);
}

#[test]
fn request_round_trips_as_json() {
    let request = BridgeVmRequest::RecommendMode {
        choice: GuestChoice {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn list_templates_request_round_trips_as_json() {
    let request = BridgeVmRequest::ListTemplates;
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn create_vm_from_template_request_round_trips_as_json() {
    let request = BridgeVmRequest::CreateVmFromTemplate {
        name: "try-vz-linux".to_string(),
        template_id: "debian-arm64-apple-vz-linux-kernel-raw".to_string(),
    };
    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains(r#""type":"create_vm_from_template""#));
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn inspect_boot_media_request_round_trips_as_json() {
    let request = BridgeVmRequest::InspectBootMedia {
        name: "ubuntu".to_string(),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn import_boot_media_request_round_trips_as_json() {
    let request = BridgeVmRequest::ImportBootMedia {
        name: "ubuntu".to_string(),
        source: PathBuf::from("ubuntu.iso"),
        kind: Some(BootMediaKind::InstallerImage),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn inspect_boot_media_status_request_round_trips_as_json() {
    let request = BridgeVmRequest::InspectBootMediaStatus {
        name: "ubuntu".to_string(),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn verify_boot_media_request_round_trips_as_json() {
    let request = BridgeVmRequest::VerifyBootMedia {
        name: "ubuntu".to_string(),
        expected_sha256: "0".repeat(64),
        kind: Some(BootMediaKind::InstallerImage),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn plan_boot_media_download_request_round_trips_as_json() {
    let request = BridgeVmRequest::PlanBootMediaDownload {
        name: "ubuntu".to_string(),
        url: "https://example.invalid/ubuntu.iso".to_string(),
        expected_sha256: Some("0".repeat(64)),
        kind: Some(BootMediaKind::InstallerImage),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn download_boot_media_request_round_trips_as_json() {
    let request = BridgeVmRequest::DownloadBootMedia {
        name: "ubuntu".to_string(),
        kind: Some(BootMediaKind::InstallerImage),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn port_requests_round_trip_as_json() {
    for request in [
        BridgeVmRequest::ListPorts {
            name: "legacy".to_string(),
        },
        BridgeVmRequest::AddPort {
            name: "legacy".to_string(),
            host: 3000,
            guest: 3000,
        },
        BridgeVmRequest::RemovePort {
            name: "legacy".to_string(),
            host: 3000,
            guest: 3000,
        },
        BridgeVmRequest::OpenPort {
            name: "legacy".to_string(),
            guest: 3000,
            scheme: Some("http".to_string()),
        },
    ] {
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }
}

#[test]
fn share_requests_round_trip_as_json() {
    for request in [
        BridgeVmRequest::ListShares {
            name: "dev".to_string(),
        },
        BridgeVmRequest::AddShare {
            name: "dev".to_string(),
            share: "workspace".to_string(),
            host_path: "/Users/me/project".to_string(),
            read_only: true,
            host_path_token: Some("share-token-workspace".to_string()),
        },
        BridgeVmRequest::RemoveShare {
            name: "dev".to_string(),
            share: "workspace".to_string(),
        },
    ] {
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }
}

#[test]
fn run_backend_request_round_trips_as_json() {
    let request = BridgeVmRequest::RunBackend {
        name: "legacy".to_string(),
        spawn: false,
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn suspend_backend_request_round_trips_as_json() {
    let request = BridgeVmRequest::SuspendBackend {
        name: "legacy".to_string(),
    };
    let json = serde_json::to_string(&request).unwrap();
    assert_eq!(json, r#"{"type":"suspend_backend","name":"legacy"}"#);
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn resume_backend_request_round_trips_as_json() {
    let request = BridgeVmRequest::ResumeBackend {
        name: "legacy".to_string(),
    };
    let json = serde_json::to_string(&request).unwrap();
    assert_eq!(json, r#"{"type":"resume_backend","name":"legacy"}"#);
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn reapply_runtime_resources_request_round_trips_as_json() {
    let request = BridgeVmRequest::ReapplyRuntimeResources {
        name: "fast-linux".to_string(),
        visibility: RuntimeResourceVisibility::Background,
    };
    let json = serde_json::to_string(&request).unwrap();
    assert_eq!(
        json,
        r#"{"type":"reapply_runtime_resources","name":"fast-linux","visibility":"background"}"#
    );
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn prepare_disk_request_round_trips_as_json() {
    let request = BridgeVmRequest::PrepareDisk {
        name: "legacy".to_string(),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn create_disk_request_round_trips_as_json() {
    let request = BridgeVmRequest::CreateDisk {
        name: "legacy".to_string(),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn inspect_disk_request_round_trips_as_json() {
    let request = BridgeVmRequest::InspectDisk {
        name: "legacy".to_string(),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn verify_disk_request_round_trips_as_json() {
    let request = BridgeVmRequest::VerifyDisk {
        name: "legacy".to_string(),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn compact_disk_request_round_trips_as_json() {
    let request = BridgeVmRequest::CompactDisk {
        name: "legacy".to_string(),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn view_logs_request_round_trips_as_json() {
    let request = BridgeVmRequest::ViewLogs {
        name: "legacy".to_string(),
        kind: VmLogKind::Qemu,
        max_bytes: Some(4096),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn ssh_plan_request_round_trips_as_json() {
    let request = BridgeVmRequest::SshPlan {
        name: "dev".to_string(),
        user: Some("ubuntu".to_string()),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn stop_backend_request_round_trips_as_json() {
    let request = BridgeVmRequest::StopBackend {
        name: "legacy".to_string(),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn qmp_control_requests_round_trip_as_json() {
    for request in [
        BridgeVmRequest::QmpStop {
            name: "legacy".to_string(),
        },
        BridgeVmRequest::QmpCont {
            name: "legacy".to_string(),
        },
    ] {
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }
}

#[test]
fn lifecycle_plan_request_round_trips_as_json() {
    let request = BridgeVmRequest::LifecyclePlan {
        name: "legacy".to_string(),
        action: LifecycleAction::Suspend,
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn restart_vm_request_round_trips_as_json() {
    let request = BridgeVmRequest::RestartVm {
        name: "legacy".to_string(),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn restore_snapshot_request_round_trips_as_json() {
    let request = BridgeVmRequest::RestoreSnapshot {
        vm: "dev".to_string(),
        name: "before-upgrade".to_string(),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn create_snapshot_disk_request_round_trips_as_json() {
    let request = BridgeVmRequest::CreateSnapshotDisk {
        vm: "dev".to_string(),
        name: "before-upgrade".to_string(),
    };
    let json = serde_json::to_string(&request).unwrap();
    let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(request, decoded);
}
