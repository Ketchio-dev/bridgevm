//! Tests split so no file exceeds 1000 lines.

use crate::test_support::*;

#[test]
fn create_keeps_qcow2_defaults_without_template_storage() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "create",
        "plain-linux",
        "--os",
        "ubuntu",
        "--arch",
        "arm64",
    ])
    .unwrap();
    let Command::Create(args) = cli.command else {
        panic!("expected create command");
    };

    let manifest = manifest_for_create(args).expect("manifest");
    assert_eq!(manifest.mode, VmMode::Fast);
    assert_eq!(manifest.storage.primary.path, "disks/root.qcow2");
    assert_eq!(manifest.storage.primary.format, "qcow2");
    assert_eq!(manifest.storage.primary.size, DEFAULT_PRIMARY_DISK_SIZE);
}

#[test]
fn socket_request_for_plain_template_create_uses_daemon_template_api() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "create",
        "try-vz-linux",
        "--template",
        "debian-arm64-apple-vz-linux-kernel-raw",
    ])
    .unwrap();
    let Command::Create(args) = cli.command else {
        panic!("expected create command");
    };

    let request = request_for(Command::Create(args)).expect("request");
    let BridgeVmRequest::CreateVmFromTemplate { name, template_id } = request else {
        panic!("expected create-from-template request");
    };
    assert_eq!(name, "try-vz-linux");
    assert_eq!(template_id, "debian-arm64-apple-vz-linux-kernel-raw");
}

#[test]
fn hvf_windows_plan_cli_accepts_installer_path() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "windows-plan",
        "--installer",
        "ISO/Win11_25H2_English_Arm64_v2.iso",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::WindowsPlan(args)) = cli.command else {
        panic!("expected hvf windows-plan command");
    };

    assert_eq!(
        args.installer.as_deref(),
        Some(Path::new("ISO/Win11_25H2_English_Arm64_v2.iso"))
    );
}

#[test]
fn hvf_host_capabilities_cli_parses() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "host-capabilities"]).unwrap();

    let Command::Hvf(HvfCommand::HostCapabilities) = cli.command else {
        panic!("expected hvf host-capabilities command");
    };
}

#[test]
fn hvf_vm_probe_cli_defaults_to_no_create() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "vm-probe"]).unwrap();

    let Command::Hvf(HvfCommand::VmProbe(args)) = cli.command else {
        panic!("expected hvf vm-probe command");
    };

    assert!(!args.allow_create);
}

#[test]
fn hvf_vm_probe_cli_accepts_explicit_create_opt_in() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "vm-probe", "--allow-create"]).unwrap();

    let Command::Hvf(HvfCommand::VmProbe(args)) = cli.command else {
        panic!("expected hvf vm-probe command");
    };

    assert!(args.allow_create);
}

#[test]
fn hvf_vcpu_probe_cli_accepts_explicit_create_opt_in() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "vcpu-probe", "--allow-create"]).unwrap();

    let Command::Hvf(HvfCommand::VcpuProbe(args)) = cli.command else {
        panic!("expected hvf vcpu-probe command");
    };

    assert!(args.allow_create);
}

#[test]
fn hvf_vcpu_run_probe_cli_defaults_to_no_run() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "vcpu-run-probe"]).unwrap();

    let Command::Hvf(HvfCommand::VcpuRunProbe(args)) = cli.command else {
        panic!("expected hvf vcpu-run-probe command");
    };

    assert!(!args.allow_run);
}

#[test]
fn hvf_vcpu_run_probe_cli_accepts_explicit_run_opt_in() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "vcpu-run-probe", "--allow-run"]).unwrap();

    let Command::Hvf(HvfCommand::VcpuRunProbe(args)) = cli.command else {
        panic!("expected hvf vcpu-run-probe command");
    };

    assert!(args.allow_run);
}

#[test]
fn hvf_interrupt_timer_probe_cli_defaults_to_no_probe() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "interrupt-timer-probe"]).unwrap();

    let Command::Hvf(HvfCommand::InterruptTimerProbe(args)) = cli.command else {
        panic!("expected hvf interrupt-timer-probe command");
    };

    assert!(!args.allow_interrupt_timer);
}

#[test]
fn hvf_interrupt_timer_probe_cli_accepts_explicit_opt_in() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "interrupt-timer-probe",
        "--allow-interrupt-timer",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::InterruptTimerProbe(args)) = cli.command else {
        panic!("expected hvf interrupt-timer-probe command");
    };

    assert!(args.allow_interrupt_timer);
}

#[test]
fn hvf_vtimer_exit_probe_cli_defaults_to_no_probe() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "vtimer-exit-probe"]).unwrap();

    let Command::Hvf(HvfCommand::VtimerExitProbe(args)) = cli.command else {
        panic!("expected hvf vtimer-exit-probe command");
    };

    assert!(!args.allow_vtimer_exit);
}

#[test]
fn hvf_vtimer_exit_probe_cli_accepts_explicit_opt_in() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "vtimer-exit-probe",
        "--allow-vtimer-exit",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::VtimerExitProbe(args)) = cli.command else {
        panic!("expected hvf vtimer-exit-probe command");
    };

    assert!(args.allow_vtimer_exit);
}

#[test]
fn hvf_memory_map_probe_cli_defaults_to_no_map() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "memory-map-probe"]).unwrap();

    let Command::Hvf(HvfCommand::MemoryMapProbe(args)) = cli.command else {
        panic!("expected hvf memory-map-probe command");
    };

    assert!(!args.allow_map);
}

#[test]
fn hvf_memory_map_probe_cli_accepts_explicit_map_opt_in() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "memory-map-probe", "--allow-map"]).unwrap();

    let Command::Hvf(HvfCommand::MemoryMapProbe(args)) = cli.command else {
        panic!("expected hvf memory-map-probe command");
    };

    assert!(args.allow_map);
}

#[test]
fn hvf_guest_entry_probe_cli_defaults_to_no_entry() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "guest-entry-probe"]).unwrap();

    let Command::Hvf(HvfCommand::GuestEntryProbe(args)) = cli.command else {
        panic!("expected hvf guest-entry-probe command");
    };

    assert!(!args.allow_entry);
}

#[test]
fn hvf_guest_entry_probe_cli_accepts_explicit_entry_opt_in() {
    let cli =
        Cli::try_parse_from(["bridgevm", "hvf", "guest-entry-probe", "--allow-entry"]).unwrap();

    let Command::Hvf(HvfCommand::GuestEntryProbe(args)) = cli.command else {
        panic!("expected hvf guest-entry-probe command");
    };

    assert!(args.allow_entry);
}

#[test]
fn hvf_guest_exit_loop_probe_cli_defaults_to_no_loop() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "guest-exit-loop-probe"]).unwrap();

    let Command::Hvf(HvfCommand::GuestExitLoopProbe(args)) = cli.command else {
        panic!("expected hvf guest-exit-loop-probe command");
    };

    assert!(!args.allow_loop);
}

#[test]
fn hvf_guest_exit_loop_probe_cli_accepts_explicit_loop_opt_in() {
    let cli =
        Cli::try_parse_from(["bridgevm", "hvf", "guest-exit-loop-probe", "--allow-loop"]).unwrap();

    let Command::Hvf(HvfCommand::GuestExitLoopProbe(args)) = cli.command else {
        panic!("expected hvf guest-exit-loop-probe command");
    };

    assert!(args.allow_loop);
}

#[test]
fn hvf_mmio_read_probe_cli_defaults_to_no_mmio() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "mmio-read-probe"]).unwrap();

    let Command::Hvf(HvfCommand::MmioReadProbe(args)) = cli.command else {
        panic!("expected hvf mmio-read-probe command");
    };

    assert!(!args.allow_mmio);
}

#[test]
fn hvf_mmio_read_probe_cli_accepts_explicit_mmio_opt_in() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "mmio-read-probe", "--allow-mmio"]).unwrap();

    let Command::Hvf(HvfCommand::MmioReadProbe(args)) = cli.command else {
        panic!("expected hvf mmio-read-probe command");
    };

    assert!(args.allow_mmio);
}
