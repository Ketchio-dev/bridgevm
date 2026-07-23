//! Tests split so no file exceeds 1000 lines.

use crate::test_support::*;

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
    assert!(report.submit_3d_ok);
    assert!(report.submit_3d_nonzero_ok);
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

#[test]
fn hvf_machine_plan_cli_accepts_installer_resources() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "machine-plan",
        "--installer",
        "ISO/Win11_25H2_English_Arm64_v2.iso",
        "--memory-gib",
        "8",
        "--vcpus",
        "6",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::MachinePlan(args)) = cli.command else {
        panic!("expected hvf machine-plan command");
    };

    assert_eq!(
        args.installer.as_deref(),
        Some(Path::new("ISO/Win11_25H2_English_Arm64_v2.iso"))
    );
    assert_eq!(args.memory_gib, 8);
    assert_eq!(args.vcpus, 6);
}

#[test]
fn hvf_windows_boot_disk_layout_probe_cli_accepts_disk_size_and_create() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "windows-boot-disk-layout-probe",
        "--disk",
        "/tmp/win11-arm-hvf.raw",
        "--size-gib",
        "8",
        "--create",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::WindowsBootDiskLayoutProbe(args)) = cli.command else {
        panic!("expected hvf windows-boot-disk-layout-probe command");
    };

    assert_eq!(args.disk, PathBuf::from("/tmp/win11-arm-hvf.raw"));
    assert_eq!(args.size_gib, 8);
    assert!(args.create);
}

#[test]
fn hvf_windows_firmware_handoff_probe_cli_accepts_firmware_vars_and_create_vars() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "windows-firmware-handoff-probe",
        "--firmware",
        "/tmp/AAVMF_CODE.fd",
        "--vars-template",
        "/tmp/AAVMF_VARS.fd",
        "--vars",
        "/tmp/win11-arm-vars.fd",
        "--create-vars",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::WindowsFirmwareHandoffProbe(args)) = cli.command else {
        panic!("expected hvf windows-firmware-handoff-probe command");
    };

    assert_eq!(args.firmware, PathBuf::from("/tmp/AAVMF_CODE.fd"));
    assert_eq!(
        args.vars_template,
        Some(PathBuf::from("/tmp/AAVMF_VARS.fd"))
    );
    assert_eq!(args.vars, Some(PathBuf::from("/tmp/win11-arm-vars.fd")));
    assert!(args.create_vars);
}

#[test]
fn hvf_windows_pflash_map_probe_cli_accepts_firmware_vars_and_create_vars() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "windows-pflash-map-probe",
        "--firmware",
        "/tmp/AAVMF_CODE.fd",
        "--vars-template",
        "/tmp/AAVMF_VARS.fd",
        "--vars",
        "/tmp/win11-arm-vars.fd",
        "--create-vars",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::WindowsPflashMapProbe(args)) = cli.command else {
        panic!("expected hvf windows-pflash-map-probe command");
    };

    assert_eq!(args.firmware, PathBuf::from("/tmp/AAVMF_CODE.fd"));
    assert_eq!(
        args.vars_template,
        Some(PathBuf::from("/tmp/AAVMF_VARS.fd"))
    );
    assert_eq!(args.vars, Some(PathBuf::from("/tmp/win11-arm-vars.fd")));
    assert!(args.create_vars);
}

#[test]
fn hvf_windows_pflash_hvf_map_probe_cli_accepts_firmware_vars_create_vars_and_allow_map() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "windows-pflash-hvf-map-probe",
        "--firmware",
        "/tmp/AAVMF_CODE.fd",
        "--vars-template",
        "/tmp/AAVMF_VARS.fd",
        "--vars",
        "/tmp/win11-arm-vars.fd",
        "--create-vars",
        "--allow-map",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::WindowsPflashHvfMapProbe(args)) = cli.command else {
        panic!("expected hvf windows-pflash-hvf-map-probe command");
    };

    assert_eq!(args.firmware, PathBuf::from("/tmp/AAVMF_CODE.fd"));
    assert_eq!(
        args.vars_template,
        Some(PathBuf::from("/tmp/AAVMF_VARS.fd"))
    );
    assert_eq!(args.vars, Some(PathBuf::from("/tmp/win11-arm-vars.fd")));
    assert!(args.create_vars);
    assert!(args.allow_map);
}

#[test]
fn hvf_windows_reset_vector_entry_probe_cli_accepts_firmware_vars_create_vars_and_allow_entry() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "windows-reset-vector-entry-probe",
        "--firmware",
        "/tmp/AAVMF_CODE.fd",
        "--vars-template",
        "/tmp/AAVMF_VARS.fd",
        "--vars",
        "/tmp/win11-arm-vars.fd",
        "--create-vars",
        "--allow-entry",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::WindowsResetVectorEntryProbe(args)) = cli.command else {
        panic!("expected hvf windows-reset-vector-entry-probe command");
    };

    assert_eq!(args.firmware, PathBuf::from("/tmp/AAVMF_CODE.fd"));
    assert_eq!(
        args.vars_template,
        Some(PathBuf::from("/tmp/AAVMF_VARS.fd"))
    );
    assert_eq!(args.vars, Some(PathBuf::from("/tmp/win11-arm-vars.fd")));
    assert!(args.create_vars);
    assert!(args.allow_entry);
}

#[test]
fn hvf_windows_firmware_run_loop_probe_cli_accepts_firmware_vars_create_vars_and_loop_bounds() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "windows-firmware-run-loop-probe",
        "--firmware",
        "/tmp/AAVMF_CODE.fd",
        "--vars-template",
        "/tmp/AAVMF_VARS.fd",
        "--vars",
        "/tmp/win11-arm-vars.fd",
        "--create-vars",
        "--allow-loop",
        "--max-exits",
        "12",
        "--guest-ram-mib",
        "128",
        "--watchdog-ms",
        "250",
        "--map-low-pflash-alias",
        "--seed-diagnostic-vector",
        "--seed-guest-ram-diagnostic-vector",
        "--seed-executable-diagnostic-vector",
        "--try-recommended-vector-base-vbar",
        "--continue-after-recommended-vector-base-vbar",
        "--repair-low-vector-diagnostic-page",
        "--remap-low-vector-to-recommended-vector",
        "--continue-after-low-vector-repair",
        "--restore-low-vector-slot-before-eret",
        "--wire-interrupt-timer",
        "--iso",
        "/tmp/Win11_Arm64.iso",
        "--writable-disk",
        "/tmp/windows-arm.raw",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::WindowsFirmwareRunLoopProbe(args)) = cli.command else {
        panic!("expected hvf windows-firmware-run-loop-probe command");
    };

    assert_eq!(args.firmware, PathBuf::from("/tmp/AAVMF_CODE.fd"));
    assert_eq!(
        args.vars_template,
        Some(PathBuf::from("/tmp/AAVMF_VARS.fd"))
    );
    assert_eq!(args.vars, Some(PathBuf::from("/tmp/win11-arm-vars.fd")));
    assert!(args.create_vars);
    assert!(args.allow_loop);
    assert_eq!(args.max_exits, 12);
    assert_eq!(args.guest_ram_mib, 128);
    assert_eq!(args.watchdog_ms, 250);
    assert!(args.map_low_pflash_alias);
    assert!(args.seed_diagnostic_vector);
    assert!(args.seed_guest_ram_diagnostic_vector);
    assert!(args.seed_executable_diagnostic_vector);
    assert!(args.try_recommended_vector_base_vbar);
    assert!(args.continue_after_recommended_vector_base_vbar);
    assert!(args.repair_low_vector_diagnostic_page);
    assert!(args.remap_low_vector_to_recommended_vector);
    assert!(args.continue_after_low_vector_repair);
    assert!(args.restore_low_vector_slot_before_eret);
    assert!(args.wire_interrupt_timer);
    assert_eq!(args.iso, Some(PathBuf::from("/tmp/Win11_Arm64.iso")));
    assert_eq!(
        args.writable_disk,
        Some(PathBuf::from("/tmp/windows-arm.raw"))
    );
}

#[test]
fn hvf_windows_firmware_device_discovery_probe_cli_accepts_firmware_media_and_loop_flags() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "windows-firmware-device-discovery-probe",
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

    let Command::Hvf(HvfCommand::WindowsFirmwareDeviceDiscoveryProbe(args)) = cli.command else {
        panic!("expected hvf windows-firmware-device-discovery-probe command");
    };

    assert_eq!(args.firmware, PathBuf::from("/tmp/AAVMF_CODE.fd"));
    assert_eq!(
        args.vars_template,
        Some(PathBuf::from("/tmp/AAVMF_VARS.fd"))
    );
    assert_eq!(args.vars, Some(PathBuf::from("/tmp/win11-arm-vars.fd")));
    assert!(args.create_vars);
    assert!(args.allow_loop);
    assert_eq!(args.max_exits, 16);
    assert_eq!(args.guest_ram_mib, 128);
    assert_eq!(args.watchdog_ms, 250);
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
fn hvf_windows_platform_description_probe_cli_accepts_memory_and_vcpus() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "hvf",
        "windows-platform-description-probe",
        "--memory-gib",
        "8",
        "--vcpus",
        "6",
    ])
    .unwrap();

    let Command::Hvf(HvfCommand::WindowsPlatformDescriptionProbe(args)) = cli.command else {
        panic!("expected hvf windows-platform-description-probe command");
    };

    assert_eq!(args.memory_gib, 8);
    assert_eq!(args.vcpus, 6);
}

#[test]
fn hvf_windows_xhci_hid_boot_key_probe_cli_parses() {
    let cli = Cli::try_parse_from(["bridgevm", "hvf", "windows-xhci-hid-boot-key-probe"]).unwrap();

    let Command::Hvf(HvfCommand::WindowsXhciHidBootKeyProbe) = cli.command else {
        panic!("expected hvf windows-xhci-hid-boot-key-probe command");
    };
}

#[test]
fn display_cli_accepts_optional_display_size() {
    let cli = Cli::try_parse_from([
        "bridgevm", "display", "dev", "--width", "1440", "--height", "900",
    ])
    .unwrap();
    let Command::Display(args) = cli.command else {
        panic!("expected display command");
    };

    assert_eq!(args.name, "dev");
    assert_eq!(args.display_size().unwrap(), Some((1440, 900)));
}

#[test]
fn display_cli_requires_width_and_height_together() {
    let cli = Cli::try_parse_from(["bridgevm", "display", "dev", "--width", "1440"]).unwrap();
    let Command::Display(args) = cli.command else {
        panic!("expected display command");
    };

    let error = args.display_size().expect_err("missing height must fail");
    assert!(error
        .to_string()
        .contains("--width and --height must be provided together"));
}

#[test]
fn local_metadata_repair_calls_store() {
    let store = unique_store("bridgevm-cli-metadata-repair-test");
    store.create_vm(&test_manifest("dev")).unwrap();

    metadata(
        &store,
        MetadataCommand {
            command: MetadataSubcommand::Repair(VmNameArgs {
                name: "dev".to_string(),
            }),
        },
    )
    .unwrap();
}

#[test]
fn local_metadata_migrate_manifest_calls_store() {
    let store = unique_store("bridgevm-cli-manifest-migration-test");
    store.create_vm(&test_manifest("dev")).unwrap();

    metadata(
        &store,
        MetadataCommand {
            command: MetadataSubcommand::MigrateManifest(ManifestMigrateArgs {
                name: "dev".to_string(),
                dry_run: true,
            }),
        },
    )
    .unwrap();
}

#[test]
fn local_metadata_manifest_schema_prints_v1_contract() {
    metadata(
        &unique_store("bridgevm-cli-manifest-schema-test"),
        MetadataCommand {
            command: MetadataSubcommand::ManifestSchema,
        },
    )
    .unwrap();
}

#[test]
fn local_metadata_validate_manifest_reads_without_store_mutation() {
    let store = unique_store("bridgevm-cli-manifest-validate-test");
    let manifest_path = store.root().join("manifest.yaml");
    fs::create_dir_all(store.root()).unwrap();
    test_manifest("dev").write(&manifest_path).unwrap();

    metadata(
        &store,
        MetadataCommand {
            command: MetadataSubcommand::ValidateManifest(ManifestValidateArgs {
                path: manifest_path,
            }),
        },
    )
    .unwrap();
}
