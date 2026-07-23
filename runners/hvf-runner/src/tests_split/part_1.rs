//! Split test module.

use crate::*;
use clap::Parser;
use std::path::Path;
use std::path::PathBuf;

#[test]
fn parses_windows_firmware_run_loop_low_vector_continue_flag() {
    let args = Args::try_parse_from([
        "hvf-runner",
        "--windows-firmware-run-loop-probe",
        "--firmware",
        "/tmp/AAVMF_CODE.fd",
        "--vars-template",
        "/tmp/AAVMF_VARS.fd",
        "--vars",
        "/tmp/win11-arm-vars.fd",
        "--create-vars",
        "--allow-loop",
        "--map-low-pflash-alias",
        "--try-recommended-vector-base-vbar",
        "--continue-after-recommended-vector-base-vbar",
        "--repair-low-vector-diagnostic-page",
        "--remap-low-vector-to-recommended-vector",
        "--continue-after-low-vector-repair",
        "--restore-low-vector-slot-before-eret",
        "--wire-interrupt-timer",
    ])
    .unwrap();

    assert!(args.windows_firmware_run_loop_probe);
    assert_eq!(args.firmware, Some(PathBuf::from("/tmp/AAVMF_CODE.fd")));
    assert_eq!(
        args.vars_template,
        Some(PathBuf::from("/tmp/AAVMF_VARS.fd"))
    );
    assert_eq!(args.vars, Some(PathBuf::from("/tmp/win11-arm-vars.fd")));
    assert!(args.create_vars);
    assert!(args.allow_loop);
    assert!(args.map_low_pflash_alias);
    assert!(args.try_recommended_vector_base_vbar);
    assert!(args.continue_after_recommended_vector_base_vbar);
    assert!(args.repair_low_vector_diagnostic_page);
    assert!(args.remap_low_vector_to_recommended_vector);
    assert!(args.continue_after_low_vector_repair);
    assert!(args.restore_low_vector_slot_before_eret);
    assert!(args.wire_interrupt_timer);
}

#[test]
fn parses_windows_firmware_device_discovery_probe_media_flags() {
    let args = Args::try_parse_from([
        "hvf-runner",
        "--windows-firmware-device-discovery-probe",
        "--firmware",
        "/tmp/AAVMF_CODE.fd",
        "--vars-template",
        "/tmp/AAVMF_VARS.fd",
        "--vars",
        "/tmp/win11-arm-vars.fd",
        "--create-vars",
        "--allow-loop",
        "--max-exits",
        "16",
        "--guest-ram-mib",
        "128",
        "--watchdog-ms",
        "250",
        "--map-low-pflash-alias",
        "--repair-low-vector-diagnostic-page",
        "--continue-after-low-vector-repair",
        "--wire-interrupt-timer",
        "--iso",
        "/tmp/Win11_Arm64.iso",
        "--writable-disk",
        "/tmp/windows-arm.raw",
    ])
    .unwrap();

    assert!(args.windows_firmware_device_discovery_probe);
    assert_eq!(args.firmware, Some(PathBuf::from("/tmp/AAVMF_CODE.fd")));
    assert_eq!(
        args.vars_template,
        Some(PathBuf::from("/tmp/AAVMF_VARS.fd"))
    );
    assert_eq!(args.vars, Some(PathBuf::from("/tmp/win11-arm-vars.fd")));
    assert!(args.create_vars);
    assert!(args.allow_loop);
    assert_eq!(args.max_exits, 16);
    assert_eq!(args.guest_ram_mib, 128);
    assert_eq!(args.watchdog_ms, Some(250));
    assert!(args.map_low_pflash_alias);
    assert!(args.repair_low_vector_diagnostic_page);
    assert!(args.continue_after_low_vector_repair);
    assert!(args.wire_interrupt_timer);
    assert_eq!(args.iso, Some(PathBuf::from("/tmp/Win11_Arm64.iso")));
    assert_eq!(
        args.writable_disk,
        Some(PathBuf::from("/tmp/windows-arm.raw"))
    );
}

#[test]
fn parses_windows_xhci_hid_boot_key_probe_flag() {
    let args = Args::try_parse_from(["hvf-runner", "--windows-xhci-hid-boot-key-probe"]).unwrap();

    assert!(args.windows_xhci_hid_boot_key_probe);
}

#[test]
fn launch_builds_installed_boot_wrapper_args() {
    let args = Args::try_parse_from([
        "hvf-runner",
        "--launch",
        "--target",
        "/tmp/win.raw",
        "--placeholder-nsid1",
        "/tmp/placeholder.raw",
        "--vars",
        "/tmp/vars.fd",
        "--evidence-dir",
        "/tmp/evidence",
        "--watchdog-ms",
        "12345",
        "--max-reboots",
        "3",
        "--ram-mib",
        "6144",
        "--smp-cpus",
        "4",
        "--boot-timer-ramfb-ms",
        "250",
        "--boot-timer-desktop-agent",
        "--agent-service-control",
        "/tmp/app.ctl",
        "--agent-service-command",
        "whoami /user",
        "--agent-clipboard-sync",
        "--agent-share-host",
        "/tmp/share",
        "--agent-share-guest",
        "C:\\bridgevm-share",
        "--agent-share-ms",
        "2500",
        "--agent-share-max-kb",
        "32768",
        "--enable-xhci",
        "--virtio-net",
        "--daily",
        "--release",
        "--print-policy",
    ])
    .unwrap();

    assert!(args.launch);
    assert_eq!(
        installed_boot_launch_args(&args, Path::new("/work")).unwrap(),
        vec![
            "--target",
            "/tmp/win.raw",
            "--vars",
            "/tmp/vars.fd",
            "--evidence-dir",
            "/tmp/evidence",
            "--placeholder-nsid1",
            "/tmp/placeholder.raw",
            "--watchdog-ms",
            "12345",
            "--max-reboots",
            "3",
            "--ram-mib",
            "6144",
            "--smp-cpus",
            "4",
            "--boot-timer-ramfb-ms",
            "250",
            "--boot-timer-desktop-agent",
            "--agent-service-control",
            "/tmp/app.ctl",
            "--agent-service-command",
            "whoami /user",
            "--agent-clipboard-sync",
            "--agent-share-host",
            "/tmp/share",
            "--agent-share-guest",
            "C:\\bridgevm-share",
            "--agent-share-ms",
            "2500",
            "--agent-share-max-kb",
            "32768",
            "--enable-xhci",
            "--virtio-net",
            "--daily",
            "--release",
            "--print-policy",
        ]
    );
}

#[test]
fn launch_forwards_explicit_no_watchdog_policy() {
    let args = Args::try_parse_from([
        "hvf-runner",
        "--launch",
        "--target",
        "/tmp/win.raw",
        "--vars",
        "/tmp/vars.fd",
        "--evidence-dir",
        "/tmp/evidence",
        "--no-watchdog",
    ])
    .unwrap();

    assert!(args.no_watchdog);
    let wrapper_args = installed_boot_launch_args(&args, Path::new("/work")).unwrap();
    assert!(wrapper_args.iter().any(|arg| arg == "--no-watchdog"));
    assert!(!wrapper_args.iter().any(|arg| arg == "--watchdog-ms"));
}

#[test]
fn launch_accepts_disk_as_target_alias() {
    let args = Args::try_parse_from([
        "hvf-runner",
        "--launch",
        "--disk",
        "/tmp/win.raw",
        "--vars",
        "/tmp/vars.fd",
        "--evidence-dir",
        "/tmp/evidence",
        "--print-policy",
    ])
    .unwrap();

    let wrapper_args = installed_boot_launch_args(&args, Path::new("/work")).unwrap();
    assert_eq!(&wrapper_args[0..2], ["--target", "/tmp/win.raw"]);
}

#[test]
fn launch_rejects_agent_share_maximum_outside_wrapper_range() {
    let args = Args::try_parse_from([
        "hvf-runner",
        "--launch",
        "--target",
        "/tmp/win.raw",
        "--vars",
        "/tmp/vars.fd",
        "--evidence-dir",
        "/tmp/evidence",
        "--agent-service-control",
        "/tmp/app.ctl",
        "--agent-share-host",
        "/tmp/share",
        "--agent-share-guest",
        "C:\\bridgevm-share",
        "--agent-share-max-kb",
        "1048577",
    ])
    .unwrap();

    let error = installed_boot_launch_args(&args, Path::new("/work"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("integer from 1 to 1048576"));
}

#[test]
fn launch_forwards_pause_resume_proof_control() {
    let args = Args::try_parse_from([
        "hvf-runner",
        "--launch",
        "--target",
        "/tmp/win.raw",
        "--vars",
        "/tmp/vars.fd",
        "--evidence-dir",
        "/tmp/evidence",
        "--host-pause-resume-proof-ms",
        "1500",
    ])
    .unwrap();

    let wrapper_args = installed_boot_launch_args(&args, Path::new("/work")).unwrap();
    assert!(wrapper_args
        .windows(2)
        .any(|pair| pair == ["--host-pause-resume-proof-ms", "1500"]));
}

#[test]
fn launch_forwards_audited_buffered_nvme_diagnostic() {
    let args = Args::try_parse_from([
        "hvf-runner",
        "--launch",
        "--target",
        "/tmp/win.raw",
        "--vars",
        "/tmp/vars.fd",
        "--evidence-dir",
        "/tmp/evidence",
        "--nvme-buffered-io",
    ])
    .unwrap();

    let wrapper_args = installed_boot_launch_args(&args, Path::new("/work")).unwrap();
    assert!(wrapper_args.iter().any(|arg| arg == "--nvme-buffered-io"));
}

#[test]
fn launch_requires_vars_and_evidence_dir() {
    let args =
        Args::try_parse_from(["hvf-runner", "--launch", "--target", "/tmp/win.raw"]).unwrap();

    let error = installed_boot_launch_args(&args, Path::new("/work"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("--vars"));
}

#[test]
fn launch_resolves_relative_paths_from_the_invocation_directory() {
    let args = Args::try_parse_from([
        "hvf-runner",
        "--launch",
        "--repo-root",
        "../repo",
        "--target",
        "media/win.raw",
        "--vars",
        "state/vars.fd",
        "--evidence-dir",
        "evidence",
        "--gpu-trace",
        "evidence/gpu.jsonl",
        "--agent-service-control",
        "state/app.ctl",
        "--agent-share-host",
        "share",
        "--agent-share-guest",
        "C:\\share",
    ])
    .unwrap();
    let invocation_dir = Path::new("/tmp/invocation");

    assert_eq!(
        launch_repo_root(&args, invocation_dir),
        PathBuf::from("/tmp/invocation/../repo")
    );
    assert_eq!(
        installed_boot_launch_args(&args, invocation_dir).unwrap(),
        vec![
            "--target",
            "/tmp/invocation/media/win.raw",
            "--vars",
            "/tmp/invocation/state/vars.fd",
            "--evidence-dir",
            "/tmp/invocation/evidence",
            "--agent-service-control",
            "/tmp/invocation/state/app.ctl",
            "--agent-share-host",
            "/tmp/invocation/share",
            "--agent-share-guest",
            "C:\\share",
            "--gpu-trace",
            "/tmp/invocation/evidence/gpu.jsonl",
        ]
    );
}

#[test]
fn launch_rejects_ambiguous_target_aliases() {
    let args = Args::try_parse_from([
        "hvf-runner",
        "--launch",
        "--target",
        "/tmp/a.raw",
        "--disk",
        "/tmp/b.raw",
        "--vars",
        "/tmp/vars.fd",
        "--evidence-dir",
        "/tmp/evidence",
    ])
    .unwrap();

    let error = installed_boot_launch_args(&args, Path::new("/work"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("only one"));
}

#[test]
fn launch_rejects_agent_extras_without_service_control() {
    let args = Args::try_parse_from([
        "hvf-runner",
        "--launch",
        "--target",
        "/tmp/win.raw",
        "--vars",
        "/tmp/vars.fd",
        "--evidence-dir",
        "/tmp/evidence",
        "--agent-clipboard-sync",
    ])
    .unwrap();

    let error = installed_boot_launch_args(&args, Path::new("/work"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("require --agent-service-control"));
}

#[test]
fn launch_rejects_unpaired_agent_share_paths() {
    let args = Args::try_parse_from([
        "hvf-runner",
        "--launch",
        "--target",
        "/tmp/win.raw",
        "--vars",
        "/tmp/vars.fd",
        "--evidence-dir",
        "/tmp/evidence",
        "--agent-service-control",
        "/tmp/app.ctl",
        "--agent-share-host",
        "/tmp/share",
    ])
    .unwrap();

    let error = installed_boot_launch_args(&args, Path::new("/work"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("must be provided together"));
}
