use crate::test_support::*;

#[test]
fn hvf_virtio_gpu_trace_report_cli_accepts_trace_and_gate_flag() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "virtio-gpu-trace-report",
        "--trace",
        "/tmp/bridgevm-virtio-gpu.jsonl",
        "--protocol",
        "virgl",
        "--require-p3-gate",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::VirtioGpuTraceReport(args)) = cli.command else {
        panic!("expected hvf virtio-gpu-trace-report command");
    };

    assert_eq!(args.trace, PathBuf::from("/tmp/bridgevm-virtio-gpu.jsonl"));
    assert_eq!(args.protocol, VirtioGpuTraceProtocolChoice::Virgl);
    assert!(args.require_p3_gate);
}

#[test]
fn hvf_title_gate_report_cli_accepts_repeated_manifests() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "title-gate-report",
        "--title-manifest",
        "/tmp/ppsspp.json",
        "--title-manifest",
        "/tmp/heaven.json",
        "--guest-logs",
        "/tmp/guest-logs",
        "--trace",
        "/tmp/virtio-gpu.jsonl",
        "--pre-run-state",
        "/tmp/pre-run.json",
        "--json-output",
        "/tmp/title-gates.json",
        "--require-title-gates",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::TitleGateReport(args)) = cli.command else {
        panic!("expected hvf title-gate-report command");
    };
    assert_eq!(
        args.manifests,
        vec![
            PathBuf::from("/tmp/ppsspp.json"),
            PathBuf::from("/tmp/heaven.json")
        ]
    );
    assert_eq!(args.guest_logs, PathBuf::from("/tmp/guest-logs"));
    assert!(args.require_title_gates);
}

#[test]
fn virtio_gpu_trace_report_groups_bounded_submit_failures_and_accepts_old_jsonl() {
    let path = unique_trace_path("bridgevm-cli-virtio-gpu-submit-failures");
    fs::write(
        &path,
        r#"{"seq":1,"event":"command","name":"SUBMIT_3D","response_name":"ERR_UNSPEC","duration_ns":3000,"ctx_id":7,"submit_size":64,"submit_first_command_id":43,"renderer_status":22,"renderer_command_id":43,"renderer_resource_id":14,"renderer_resource_found":true,"renderer_resource_backed":false}
{"seq":2,"event":"command","name":"SUBMIT_3D","response_name":"ERR_UNSPEC","duration_ns":1000,"ctx_id":7,"submit_size":64,"submit_first_command_id":43,"renderer_status":22,"renderer_command_id":43,"renderer_resource_id":14,"renderer_resource_found":true,"renderer_resource_backed":false}
{"seq":3,"event":"command","name":"SUBMIT_3D","response_name":"ERR_UNSPEC","submit_size":16,"submit_first_command_id":8}
"#,
    )
    .unwrap();

    let report = analyze_virtio_gpu_trace(&path).unwrap();
    let _ = fs::remove_file(path);

    assert_eq!(report.command_percentile_us(50, 100), 1.0);
    assert_eq!(report.command_percentile_us(95, 100), 3.0);
    assert_eq!(report.submit_3d_failures.len(), 2);
    let transfer = report
        .submit_3d_failures
        .iter()
        .find(|(failure, _)| failure.command_id == Some(43))
        .unwrap();
    assert_eq!(*transfer.1, 2);
    assert_eq!(transfer.0.renderer_status, Some(22));
    assert_eq!(transfer.0.resource_id, Some(14));
    assert_eq!(transfer.0.resource_found, Some(true));
    assert_eq!(transfer.0.resource_backed, Some(false));
    let old = report
        .submit_3d_failures
        .iter()
        .find(|(failure, _)| failure.command_id == Some(8))
        .unwrap();
    assert_eq!(*old.1, 1);
    assert_eq!(old.0.renderer_status, None);
}

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
