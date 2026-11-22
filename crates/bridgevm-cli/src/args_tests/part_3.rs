//! Tests split so no file exceeds 1000 lines.

use crate::test_support::*;
include!("part_3_2.rs");

#[test]
fn hvf_virtio_gpu_3d_host_preflight_cli_accepts_command() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "virtio-gpu-3d-host-preflight"]).unwrap();

    let Command::Hvf(HvfCommand::VirtioGpu3dHostPreflight(args)) = cli.command else {
        panic!("expected hvf virtio-gpu-3d-host-preflight command");
    };

    assert_eq!(args.protocol, VirtioGpu3dHostPreflightProtocolChoice::Venus);
}

#[test]
fn hvf_virtio_gpu_3d_host_preflight_cli_accepts_virgl_protocol() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "virtio-gpu-3d-host-preflight",
        "--protocol",
        "virgl",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::VirtioGpu3dHostPreflight(args)) = cli.command else {
        panic!("expected hvf virtio-gpu-3d-host-preflight command");
    };

    assert_eq!(args.protocol, VirtioGpu3dHostPreflightProtocolChoice::Virgl);
}

#[test]
fn virtio_gpu_trace_report_passes_p3_gate_on_complete_trace() {
    let path = unique_trace_path("bridgevm-cli-virtio-gpu-pass");
    fs::write(&path, complete_virtio_gpu_trace_sample()).unwrap();

    let report = analyze_virtio_gpu_trace(&path).unwrap();
    let _ = fs::remove_file(path);

    assert_eq!(report.events, 13);
    assert!(report.device_init);
    assert!(report.has_3d_backend());
    assert!(report.accepted_venus_features());
    assert!(report.accepted_version_1());
    assert!(report.capset_info_ok);
    assert!(report.venus_capset_info_ok);
    assert!(report.capset_ok);
    assert!(report.venus_capset_ok);
    assert!(report.blob_create_ok);
    assert!(report.ctx_create_ok);
    assert!(report.venus_ctx_create_ok);
    assert!(report.submit_3d_ok && report.submit_3d_nonzero_ok);
    assert!(report.backend_fence_parked);
    assert!(report.fence_lifecycle_observed());
    assert!(report
        .p3_blockers(VirtioGpuTraceProtocolChoice::Auto)
        .is_empty());
    assert!(report
        .p3_blockers(VirtioGpuTraceProtocolChoice::Venus)
        .is_empty());
}

#[test]
fn virtio_gpu_trace_report_protocol_gate_distinguishes_venus_and_virgl() {
    let path = unique_trace_path("bridgevm-cli-virtio-gpu-non-venus");
    fs::write(
        &path,
        r#"{"seq":1,"event":"device_init","backend_3d":true}
{"seq":2,"event":"driver_features","select":0,"accepted":25}
{"seq":3,"event":"driver_features","select":1,"accepted":1}
{"seq":4,"event":"queue_notify","valid":true}
{"seq":5,"event":"command","name":"GET_CAPSET_INFO","response_name":"OK_CAPSET_INFO","response_capset_id":1,"response_capset_max_version":1,"response_capset_max_size":64}
{"seq":6,"event":"command","name":"GET_CAPSET","response_name":"OK_CAPSET","capset_id":1,"capset_version":1}
{"seq":7,"event":"command","name":"RESOURCE_CREATE_3D","response_name":"OK_NODATA"}
{"seq":8,"event":"command","name":"RESOURCE_ATTACH_BACKING","response_name":"OK_NODATA"}
{"seq":9,"event":"command","name":"CTX_CREATE","response_name":"OK_NODATA","context_init":1}
{"seq":10,"event":"command","name":"SUBMIT_3D","response_name":"OK_NODATA","fenced":true,"submit_size":16}
{"seq":11,"event":"fence_create","ctx_id":1,"ring_idx":0,"fence_id":9,"backend_accepted":true,"outcome":"parked"}
{"seq":12,"event":"fence_deliver","ctx_id":1,"ring_idx":0,"fence_id":9,"used_len":24}
"#,
    )
    .unwrap();

    let report = analyze_virtio_gpu_trace(&path).unwrap();
    let _ = fs::remove_file(path);

    assert!(report.capset_info_ok);
    assert!(report.capset_ok);
    assert!(report.ctx_create_ok);
    assert!(!report.venus_capset_info_ok);
    assert!(!report.venus_capset_ok);
    assert!(!report.venus_ctx_create_ok);
    assert!(report.virgl_capset_info_ok);
    assert!(report.virgl_capset_ok);
    assert!(report.virgl_ctx_create_ok);
    assert!(report.resource_create_3d_ok);
    assert!(report.resource_attach_backing_ok);
    assert!(!report.blob_create_ok);
    assert!(report
        .p3_blockers(VirtioGpuTraceProtocolChoice::Auto)
        .is_empty());
    assert!(report
        .p3_blockers(VirtioGpuTraceProtocolChoice::Virgl)
        .is_empty());

    let venus_blockers = report.p3_blockers(VirtioGpuTraceProtocolChoice::Venus);
    assert!(venus_blockers
        .iter()
        .any(|blocker| blocker == "GET_CAPSET_INFO did not report VENUS capset id 4"));
    assert!(venus_blockers
        .iter()
        .any(|blocker| blocker == "missing successful GET_CAPSET for VENUS capset id 4"));
    assert!(venus_blockers
        .iter()
        .any(|blocker| { blocker == "missing CTX_CREATE with VENUS context_init low byte 4" }));
}
