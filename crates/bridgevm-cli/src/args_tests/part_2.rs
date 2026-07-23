//! Tests split so no file exceeds 1000 lines.

use crate::test_support::*;

#[test]
fn hvf_mmio_read_emulation_probe_cli_defaults_to_no_emulation() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "mmio-read-emulation-probe"]).unwrap();

    let Command::Hvf(HvfCommand::MmioReadEmulationProbe(args)) = cli.command else {
        panic!("expected hvf mmio-read-emulation-probe command");
    };

    assert!(!args.allow_emulate);
}

#[test]
fn hvf_mmio_read_emulation_probe_cli_accepts_explicit_emulation_opt_in() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "mmio-read-emulation-probe",
        "--allow-emulate",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::MmioReadEmulationProbe(args)) = cli.command else {
        panic!("expected hvf mmio-read-emulation-probe command");
    };

    assert!(args.allow_emulate);
}

#[test]
fn hvf_mmio_write_emulation_probe_cli_defaults_to_no_emulation() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "mmio-write-emulation-probe"]).unwrap();

    let Command::Hvf(HvfCommand::MmioWriteEmulationProbe(args)) = cli.command else {
        panic!("expected hvf mmio-write-emulation-probe command");
    };

    assert!(!args.allow_emulate);
}

#[test]
fn hvf_mmio_write_emulation_probe_cli_accepts_explicit_emulation_opt_in() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "mmio-write-emulation-probe",
        "--allow-emulate",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::MmioWriteEmulationProbe(args)) = cli.command else {
        panic!("expected hvf mmio-write-emulation-probe command");
    };

    assert!(args.allow_emulate);
}

#[test]
fn hvf_mmio_serial_device_probe_cli_defaults_to_no_device() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "mmio-serial-device-probe"]).unwrap();

    let Command::Hvf(HvfCommand::MmioSerialDeviceProbe(args)) = cli.command else {
        panic!("expected hvf mmio-serial-device-probe command");
    };

    assert!(!args.allow_device);
}

#[test]
fn hvf_mmio_serial_device_probe_cli_accepts_explicit_device_opt_in() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "mmio-serial-device-probe",
        "--allow-device",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::MmioSerialDeviceProbe(args)) = cli.command else {
        panic!("expected hvf mmio-serial-device-probe command");
    };

    assert!(args.allow_device);
}

#[test]
fn hvf_mmio_rtc_device_probe_cli_defaults_to_no_device() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "mmio-rtc-device-probe"]).unwrap();

    let Command::Hvf(HvfCommand::MmioRtcDeviceProbe(args)) = cli.command else {
        panic!("expected hvf mmio-rtc-device-probe command");
    };

    assert!(!args.allow_device);
}

#[test]
fn hvf_mmio_rtc_device_probe_cli_accepts_explicit_device_opt_in() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "mmio-rtc-device-probe", "--allow-device"])
        .unwrap();

    let Command::Hvf(HvfCommand::MmioRtcDeviceProbe(args)) = cli.command else {
        panic!("expected hvf mmio-rtc-device-probe command");
    };

    assert!(args.allow_device);
}

#[test]
fn hvf_mmio_block_device_probe_cli_defaults_to_no_device() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "mmio-block-device-probe"]).unwrap();

    let Command::Hvf(HvfCommand::MmioBlockDeviceProbe(args)) = cli.command else {
        panic!("expected hvf mmio-block-device-probe command");
    };

    assert!(!args.allow_device);
}

#[test]
fn hvf_mmio_block_device_probe_cli_accepts_explicit_device_opt_in() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "mmio-block-device-probe",
        "--allow-device",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::MmioBlockDeviceProbe(args)) = cli.command else {
        panic!("expected hvf mmio-block-device-probe command");
    };

    assert!(args.allow_device);
}

#[test]
fn hvf_mmio_block_queue_probe_cli_defaults_to_no_device() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "mmio-block-queue-probe"]).unwrap();

    let Command::Hvf(HvfCommand::MmioBlockQueueProbe(args)) = cli.command else {
        panic!("expected hvf mmio-block-queue-probe command");
    };

    assert!(!args.allow_device);
    assert_eq!(args.disk, None);
    assert_eq!(args.iso, None);
    assert_eq!(args.writable_disk, None);
}

#[test]
fn hvf_mmio_block_queue_probe_cli_accepts_explicit_device_opt_in() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "mmio-block-queue-probe",
        "--allow-device",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::MmioBlockQueueProbe(args)) = cli.command else {
        panic!("expected hvf mmio-block-queue-probe command");
    };

    assert!(args.allow_device);
    assert_eq!(args.disk, None);
    assert_eq!(args.iso, None);
    assert_eq!(args.writable_disk, None);
}

#[test]
fn hvf_mmio_block_queue_probe_cli_accepts_file_backing_disk() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "mmio-block-queue-probe",
        "--allow-device",
        "--disk",
        "/tmp/bridgevm-live-block.img",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::MmioBlockQueueProbe(args)) = cli.command else {
        panic!("expected hvf mmio-block-queue-probe command");
    };

    assert!(args.allow_device);
    assert_eq!(
        args.disk,
        Some(PathBuf::from("/tmp/bridgevm-live-block.img"))
    );
    assert_eq!(args.iso, None);
    assert_eq!(args.writable_disk, None);
}

#[test]
fn hvf_mmio_block_queue_probe_cli_accepts_read_only_iso_backing() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "mmio-block-queue-probe",
        "--allow-device",
        "--iso",
        "/tmp/Win11_Arm64.iso",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::MmioBlockQueueProbe(args)) = cli.command else {
        panic!("expected hvf mmio-block-queue-probe command");
    };

    assert!(args.allow_device);
    assert_eq!(args.disk, None);
    assert_eq!(args.iso, Some(PathBuf::from("/tmp/Win11_Arm64.iso")));
    assert_eq!(args.writable_disk, None);
}

#[test]
fn hvf_mmio_block_queue_probe_cli_accepts_writable_file_backing_disk() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "mmio-block-queue-probe",
        "--allow-device",
        "--writable-disk",
        "/tmp/bridgevm-writable-live-block.img",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::MmioBlockQueueProbe(args)) = cli.command else {
        panic!("expected hvf mmio-block-queue-probe command");
    };

    assert!(args.allow_device);
    assert_eq!(args.disk, None);
    assert_eq!(args.iso, None);
    assert_eq!(
        args.writable_disk,
        Some(PathBuf::from("/tmp/bridgevm-writable-live-block.img"))
    );
}

#[test]
fn hvf_virtio_block_request_model_probe_cli_parses() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "virtio-block-request-model-probe"]).unwrap();

    let Command::Hvf(HvfCommand::VirtioBlockRequestModelProbe) = cli.command else {
        panic!("expected hvf virtio-block-request-model-probe command");
    };
}

#[test]
fn hvf_virtio_block_file_backing_probe_cli_accepts_disk() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "virtio-block-file-backing-probe",
        "--disk",
        "/tmp/bridgevm-test.img",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::VirtioBlockFileBackingProbe(args)) = cli.command else {
        panic!("expected hvf virtio-block-file-backing-probe command");
    };

    assert_eq!(args.disk, PathBuf::from("/tmp/bridgevm-test.img"));
}

#[test]
fn hvf_virtio_block_writable_file_backing_probe_cli_accepts_disk() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "virtio-block-writable-file-backing-probe",
        "--disk",
        "/tmp/bridgevm-writable-test.img",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::VirtioBlockWritableFileBackingProbe(args)) = cli.command else {
        panic!("expected hvf virtio-block-writable-file-backing-probe command");
    };

    assert_eq!(args.disk, PathBuf::from("/tmp/bridgevm-writable-test.img"));
}

#[test]
fn hvf_virtio_block_iso_backing_probe_cli_accepts_iso() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "virtio-block-iso-backing-probe",
        "--iso",
        "/tmp/Win11_Arm64.iso",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::VirtioBlockIsoBackingProbe(args)) = cli.command else {
        panic!("expected hvf virtio-block-iso-backing-probe command");
    };

    assert_eq!(args.iso, PathBuf::from("/tmp/Win11_Arm64.iso"));
}

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
