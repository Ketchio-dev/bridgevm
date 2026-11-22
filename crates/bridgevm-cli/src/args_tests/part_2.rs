//! Tests split so no file exceeds 1000 lines.

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
