//! Split out of main.rs by responsibility.

use crate::*;

pub(crate) fn print_snapshot_chain(chain: &bridgevm_storage::SnapshotChainMetadata) {
    print_active_disk(&chain.active_disk);
    if chain.disks.is_empty() {
        println!("No disk snapshot chain metadata");
        return;
    }
    for disk in &chain.disks {
        println!("Snapshot disk: {}", disk.snapshot);
        print_snapshot_disk_status(disk);
    }
}

pub(crate) fn print_active_disk(active_disk: &bridgevm_storage::ActiveDiskMetadata) {
    println!("Active disk source: {}", active_disk.source);
    if let Some(snapshot) = &active_disk.snapshot {
        println!("Active disk snapshot: {}", snapshot);
    }
    println!("Active disk: {}", active_disk.path.display());
    println!("Active disk format: {}", active_disk.format);
    println!("Active disk ready: {}", active_disk.exists);
    println!("Active disk activated: {}", active_disk.activated_at_unix);
}

pub(crate) fn print_snapshot_disk_status(metadata: &bridgevm_storage::SnapshotDiskMetadata) {
    println!("Snapshot disk overlay: {}", metadata.overlay_path.display());
    println!("Snapshot disk overlay ready: {}", metadata.overlay_exists);
    println!("Snapshot disk backing: {}", metadata.backing_path.display());
    println!("Snapshot disk backing format: {}", metadata.backing_format);
    println!("Snapshot disk backing ready: {}", metadata.backing_exists);
    println!(
        "Snapshot disk create command: {}",
        metadata.create_command.join(" ")
    );
}

pub(crate) fn print_snapshot_disk_create_status(
    metadata: &bridgevm_storage::SnapshotDiskCreateMetadata,
) {
    println!("Snapshot disk create executed: {}", metadata.executed);
    println!(
        "Snapshot disk create command: {}",
        metadata.command.join(" ")
    );
    if let Some(status) = &metadata.exit_status {
        println!("Snapshot disk create status: {}", status);
    }
    if !metadata.stdout.is_empty() {
        println!(
            "Snapshot disk create stdout: {}",
            metadata.stdout.trim_end()
        );
    }
    if !metadata.stderr.is_empty() {
        println!(
            "Snapshot disk create stderr: {}",
            metadata.stderr.trim_end()
        );
    }
    print_snapshot_disk_status(&metadata.disk);
}

pub(crate) fn print_snapshot_suspend_image_status(
    metadata: &bridgevm_storage::SnapshotSuspendImageMetadata,
) {
    println!("Suspend image: {}", metadata.image_path.display());
    println!("Suspend image format: {}", metadata.image_format);
    println!("Suspend image ready: {}", metadata.image_exists);
    println!("Suspend image prepared: {}", metadata.prepared_at_unix);
}

pub(crate) fn print_application_consistent_snapshot_preflight(
    metadata: &ApplicationConsistentSnapshotPreflightMetadata,
) {
    println!("Application-consistent preflight: {}", metadata.snapshot);
    println!("Guest tools connected: {}", metadata.connected);
    println!(
        "Required capabilities: {}",
        metadata.required_capabilities.join(", ")
    );
    println!(
        "Available capabilities: {}",
        if metadata.available_capabilities.is_empty() {
            "none".to_string()
        } else {
            metadata.available_capabilities.join(", ")
        }
    );
    println!(
        "Missing capabilities: {}",
        if metadata.missing_capabilities.is_empty() {
            "none".to_string()
        } else {
            metadata.missing_capabilities.join(", ")
        }
    );
    println!("Application-consistent ready: {}", metadata.ready);
    println!("Planned freeze: {}", metadata.planned_freeze_semantics);
    println!("Planned thaw: {}", metadata.planned_thaw_semantics);
    println!(
        "Guest tools runtime updated: {}",
        metadata
            .runtime_updated_at_unix
            .map_or("unknown".to_string(), |updated| updated.to_string())
    );
    println!("Preflight prepared: {}", metadata.prepared_at_unix);
}

pub(crate) fn print_snapshot_preflight_status(metadata: &SnapshotPreflightStatusRecord) {
    println!("Snapshot preflight for {}", metadata.vm);
    println!("Consistency: {:?}", metadata.consistency);
    println!(
        "Backend freeze/thaw supported: {}",
        metadata.backend_freeze_thaw_supported
    );
    println!("Guest tools connected: {}", metadata.guest_tools_connected);
    println!(
        "Capabilities: {}",
        if metadata.capabilities.is_empty() {
            "none".to_string()
        } else {
            metadata.capabilities.join(", ")
        }
    );
    println!("Preflight ready: {}", metadata.ready);
    if metadata.blockers.is_empty() {
        println!("Blockers: none");
    } else {
        for blocker in &metadata.blockers {
            if let Some(path) = &blocker.path {
                println!(
                    "Blocker: {} - {} ({})",
                    blocker.code,
                    blocker.message,
                    path.display()
                );
            } else {
                println!("Blocker: {} - {}", blocker.code, blocker.message);
            }
        }
    }
    println!("Checked: {}", metadata.checked_at_unix);
}

pub(crate) fn print_application_consistent_snapshot_execution(
    execution: &ApplicationConsistentSnapshotExecutionRecord,
) {
    println!(
        "Application-consistent snapshot execution for {}",
        execution.vm
    );
    println!("Snapshot: {}", execution.snapshot);
    println!("Freeze request ID: {}", execution.freeze_request_id);
    println!("Thaw request ID: {}", execution.thaw_request_id);
    println!(
        "Pending after freeze: {}",
        execution.pending_commands_after_freeze
    );
    println!(
        "Pending after thaw: {}",
        execution.pending_commands_after_thaw
    );
    println!(
        "Freeze result: {} ({})",
        execution.freeze_result.ok,
        execution
            .freeze_result
            .message
            .as_deref()
            .unwrap_or("no message")
    );
    println!(
        "Thaw result: {} ({})",
        execution.thaw_result.ok,
        execution
            .thaw_result
            .message
            .as_deref()
            .unwrap_or("no message")
    );
    println!("Preflight ready: {}", execution.preflight_ready);
    println!("Snapshot created: {}", execution.snapshot_created_at_unix);
    println!("Note: {}", execution.note);
}

pub(crate) fn recommend(args: GuestArgs) -> Result<()> {
    let choice = GuestChoice {
        os: args.os,
        version: args.version,
        arch: args.arch,
    };
    let rec = recommend_mode(&choice);
    print_mode_recommendation(&rec, Some(&choice));
    Ok(())
}

pub(crate) fn hvf(command: HvfCommand) -> Result<()> {
    match command {
        HvfCommand::WindowsPlan(args) => {
            let plan = plan_windows_11_arm_no_qemu(args.installer);
            print!("{}", plan.render_text());
            Ok(())
        }
        HvfCommand::MachinePlan(args) => {
            if args.memory_gib == 0 {
                bail!("--memory-gib must be greater than zero");
            }
            if args.vcpus == 0 {
                bail!("--vcpus must be greater than zero");
            }
            let plan = plan_windows_11_arm_hvf_machine(HvfMachinePlanOptions {
                installer: args.installer,
                memory_gib: args.memory_gib,
                vcpu_count: args.vcpus,
            });
            print!("{}", plan.render_text());
            Ok(())
        }
        HvfCommand::WindowsBootDiskLayoutProbe(args) => {
            if args.size_gib == 0 {
                bail!("--size-gib must be greater than zero");
            }
            let probe = probe_windows_11_arm_boot_disk_layout(WindowsArmBootDiskLayoutOptions {
                disk_path: args.disk,
                size_gib: args.size_gib,
                create: args.create,
            });
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::WindowsFirmwareHandoffProbe(args) => {
            let probe =
                probe_windows_11_arm_uefi_firmware_handoff(WindowsArmUefiFirmwareHandoffOptions {
                    firmware_path: args.firmware,
                    vars_template_path: args.vars_template,
                    vars_path: args.vars,
                    create_vars: args.create_vars,
                });
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::WindowsPflashMapProbe(args) => {
            let probe = probe_windows_11_arm_uefi_pflash_map(WindowsArmUefiPflashMapOptions {
                firmware_path: args.firmware,
                vars_template_path: args.vars_template,
                vars_path: args.vars,
                create_vars: args.create_vars,
            });
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::WindowsPflashHvfMapProbe(args) => {
            let allow_map = args.allow_map
                || env::var("BRIDGEVM_HVF_ALLOW_UEFI_PFLASH_MAP").as_deref() == Ok("1");
            let probe = probe_windows_11_arm_uefi_pflash_hvf_map(
                WindowsArmUefiPflashMapOptions {
                    firmware_path: args.firmware,
                    vars_template_path: args.vars_template,
                    vars_path: args.vars,
                    create_vars: args.create_vars,
                },
                allow_map,
            );
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::WindowsResetVectorEntryProbe(args) => {
            let allow_entry = args.allow_entry
                || env::var("BRIDGEVM_HVF_ALLOW_UEFI_RESET_VECTOR_ENTRY").as_deref() == Ok("1");
            let probe = probe_windows_11_arm_uefi_reset_vector_entry(
                WindowsArmUefiPflashMapOptions {
                    firmware_path: args.firmware,
                    vars_template_path: args.vars_template,
                    vars_path: args.vars,
                    create_vars: args.create_vars,
                },
                allow_entry,
            );
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::WindowsFirmwareRunLoopProbe(args) => {
            if args.max_exits == 0 {
                bail!("--max-exits must be greater than zero");
            }
            if args.guest_ram_mib == 0 {
                bail!("--guest-ram-mib must be greater than zero");
            }
            if args.watchdog_ms == 0 {
                bail!("--watchdog-ms must be greater than zero");
            }
            let allow_loop = args.allow_loop
                || env::var("BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP").as_deref() == Ok("1");
            let probe =
                probe_windows_11_arm_uefi_firmware_run_loop(WindowsArmUefiFirmwareRunLoopOptions {
                    pflash: WindowsArmUefiPflashMapOptions {
                        firmware_path: args.firmware,
                        vars_template_path: args.vars_template,
                        vars_path: args.vars,
                        create_vars: args.create_vars,
                    },
                    execution: WindowsArmUefiFirmwareRunLoopExecutionOptions {
                        allow_loop,
                        requested_exits: args.max_exits,
                        guest_ram_mib: args.guest_ram_mib,
                        watchdog_timeout_ms: args.watchdog_ms,
                        map_low_pflash_alias: args.map_low_pflash_alias,
                        seed_diagnostic_vector: args.seed_diagnostic_vector,
                        seed_guest_ram_diagnostic_vector: args.seed_guest_ram_diagnostic_vector,
                        seed_executable_diagnostic_vector: args.seed_executable_diagnostic_vector,
                        try_recommended_vector_base_vbar: args.try_recommended_vector_base_vbar,
                        continue_after_recommended_vector_base_vbar: args
                            .continue_after_recommended_vector_base_vbar,
                        repair_low_vector_diagnostic_page: args.repair_low_vector_diagnostic_page,
                        remap_low_vector_to_recommended_vector: args
                            .remap_low_vector_to_recommended_vector,
                        continue_after_low_vector_repair: args.continue_after_low_vector_repair,
                        restore_low_vector_slot_before_eret: args
                            .restore_low_vector_slot_before_eret,
                        wire_interrupt_timer: args.wire_interrupt_timer,
                        stop_at_first_post_repair_device_boundary: false,
                        installer_iso_path: args.iso,
                        writable_target_disk_path: args.writable_disk,
                    },
                });
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::WindowsFirmwareDeviceDiscoveryProbe(args) => {
            if args.max_exits == 0 {
                bail!("--max-exits must be greater than zero");
            }
            if args.guest_ram_mib == 0 {
                bail!("--guest-ram-mib must be greater than zero");
            }
            if args.watchdog_ms == 0 {
                bail!("--watchdog-ms must be greater than zero");
            }
            let allow_loop = args.allow_loop
                || env::var("BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP").as_deref() == Ok("1");
            let probe = probe_windows_11_arm_uefi_firmware_device_discovery(
                WindowsArmUefiFirmwareRunLoopOptions {
                    pflash: WindowsArmUefiPflashMapOptions {
                        firmware_path: args.firmware,
                        vars_template_path: args.vars_template,
                        vars_path: args.vars,
                        create_vars: args.create_vars,
                    },
                    execution: WindowsArmUefiFirmwareRunLoopExecutionOptions {
                        allow_loop,
                        requested_exits: args.max_exits,
                        guest_ram_mib: args.guest_ram_mib,
                        watchdog_timeout_ms: args.watchdog_ms,
                        map_low_pflash_alias: args.map_low_pflash_alias,
                        seed_diagnostic_vector: args.seed_diagnostic_vector,
                        seed_guest_ram_diagnostic_vector: args.seed_guest_ram_diagnostic_vector,
                        seed_executable_diagnostic_vector: args.seed_executable_diagnostic_vector,
                        try_recommended_vector_base_vbar: args.try_recommended_vector_base_vbar,
                        continue_after_recommended_vector_base_vbar: args
                            .continue_after_recommended_vector_base_vbar,
                        repair_low_vector_diagnostic_page: args.repair_low_vector_diagnostic_page,
                        remap_low_vector_to_recommended_vector: args
                            .remap_low_vector_to_recommended_vector,
                        continue_after_low_vector_repair: args.continue_after_low_vector_repair,
                        restore_low_vector_slot_before_eret: args
                            .restore_low_vector_slot_before_eret,
                        wire_interrupt_timer: args.wire_interrupt_timer,
                        stop_at_first_post_repair_device_boundary: false,
                        installer_iso_path: args.iso,
                        writable_target_disk_path: args.writable_disk,
                    },
                },
            );
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::WindowsPlatformDescriptionProbe(args) => {
            if args.memory_gib == 0 {
                bail!("--memory-gib must be greater than zero");
            }
            if args.vcpus == 0 {
                bail!("--vcpus must be greater than zero");
            }
            let probe =
                probe_windows_11_arm_platform_description(WindowsArmPlatformDescriptionOptions {
                    guest_ram_bytes: u64::from(args.memory_gib) * 1024 * 1024 * 1024,
                    vcpu_count: args.vcpus,
                });
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::WindowsXhciHidBootKeyProbe => {
            let probe = probe_windows_11_arm_xhci_hid_boot_key_report();
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::HostCapabilities => {
            let capabilities = query_hvf_host_capabilities();
            print!("{}", capabilities.render_text());
            Ok(())
        }
        HvfCommand::VmProbe(args) => {
            let allow_create =
                args.allow_create || env::var("BRIDGEVM_HVF_ALLOW_VM_CREATE").as_deref() == Ok("1");
            let probe = probe_hvf_vm_create(allow_create);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::VcpuProbe(args) => {
            let allow_create =
                args.allow_create || env::var("BRIDGEVM_HVF_ALLOW_VM_CREATE").as_deref() == Ok("1");
            let probe = probe_hvf_vcpu_create(allow_create);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::VcpuRunProbe(args) => {
            let allow_run =
                args.allow_run || env::var("BRIDGEVM_HVF_ALLOW_VCPU_RUN").as_deref() == Ok("1");
            let probe = probe_hvf_vcpu_run(allow_run);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::InterruptTimerProbe(args) => {
            let allow_probe = args.allow_interrupt_timer
                || env::var("BRIDGEVM_HVF_ALLOW_INTERRUPT_TIMER").as_deref() == Ok("1");
            let probe = probe_hvf_interrupt_timer(allow_probe);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::VtimerExitProbe(args) => {
            let allow_probe =
                args.allow_vtimer_exit || env_truthy("BRIDGEVM_HVF_ALLOW_VTIMER_EXIT");
            let probe = probe_hvf_vtimer_exit(allow_probe);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::MemoryMapProbe(args) => {
            let allow_map =
                args.allow_map || env::var("BRIDGEVM_HVF_ALLOW_MEMORY_MAP").as_deref() == Ok("1");
            let probe = probe_hvf_memory_map(allow_map);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::GuestEntryProbe(args) => {
            let allow_entry = args.allow_entry
                || env::var("BRIDGEVM_HVF_ALLOW_GUEST_ENTRY").as_deref() == Ok("1");
            let probe = probe_hvf_guest_entry(allow_entry);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::GuestExitLoopProbe(args) => {
            let allow_loop =
                args.allow_loop || env::var("BRIDGEVM_HVF_ALLOW_EXIT_LOOP").as_deref() == Ok("1");
            let probe = probe_hvf_guest_exit_loop(allow_loop);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::MmioReadProbe(args) => {
            let allow_mmio =
                args.allow_mmio || env::var("BRIDGEVM_HVF_ALLOW_MMIO_READ").as_deref() == Ok("1");
            let probe = probe_hvf_mmio_read_exit(allow_mmio);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::MmioReadEmulationProbe(args) => {
            let allow_emulate = args.allow_emulate
                || env::var("BRIDGEVM_HVF_ALLOW_MMIO_EMULATION").as_deref() == Ok("1");
            let probe = probe_hvf_mmio_read_emulation(allow_emulate);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::MmioWriteEmulationProbe(args) => {
            let allow_emulate = args.allow_emulate
                || env::var("BRIDGEVM_HVF_ALLOW_MMIO_WRITE_EMULATION").as_deref() == Ok("1");
            let probe = probe_hvf_mmio_write_emulation(allow_emulate);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::MmioSerialDeviceProbe(args) => {
            let allow_device = args.allow_device
                || env::var("BRIDGEVM_HVF_ALLOW_MMIO_SERIAL_DEVICE").as_deref() == Ok("1");
            let probe = probe_hvf_mmio_serial_device(allow_device);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::MmioRtcDeviceProbe(args) => {
            let allow_device = args.allow_device
                || env::var("BRIDGEVM_HVF_ALLOW_MMIO_RTC_DEVICE").as_deref() == Ok("1");
            let probe = probe_hvf_mmio_rtc_device(allow_device);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::MmioBlockDeviceProbe(args) => {
            let allow_device = args.allow_device
                || env::var("BRIDGEVM_HVF_ALLOW_MMIO_BLOCK_DEVICE").as_deref() == Ok("1");
            let probe = probe_hvf_mmio_block_device(allow_device);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::MmioBlockQueueProbe(args) => {
            let backing_selectors = usize::from(args.disk.is_some())
                + usize::from(args.iso.is_some())
                + usize::from(args.writable_disk.is_some());
            if backing_selectors > 1 {
                bail!("--disk, --iso, and --writable-disk are mutually exclusive for hvf mmio-block-queue-probe");
            }
            let allow_device = args.allow_device
                || env::var("BRIDGEVM_HVF_ALLOW_MMIO_BLOCK_QUEUE").as_deref() == Ok("1");
            let probe =
                probe_hvf_mmio_block_queue(allow_device, args.disk, args.iso, args.writable_disk);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::VirtioBlockRequestModelProbe => {
            let probe = probe_virtio_block_request_model();
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::VirtioBlockFileBackingProbe(args) => {
            let probe = probe_virtio_block_file_backing(args.disk);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::VirtioBlockWritableFileBackingProbe(args) => {
            let probe = probe_virtio_block_writable_file_backing(args.disk);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::VirtioBlockIsoBackingProbe(args) => {
            let probe = probe_virtio_block_iso_backing(args.iso);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::VirtioGpu3dHostPreflight(args) => {
            let probe = probe_virtio_gpu_3d_host_preflight_for(args.protocol.into());
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::VirtioGpuTraceReport(args) => {
            let report = analyze_virtio_gpu_trace(&args.trace)?;
            let blockers = report.p3_blockers(args.protocol);
            print_virtio_gpu_trace_report(&args.trace, args.protocol, &report, &blockers);
            if args.require_p3_gate && !blockers.is_empty() {
                bail!("virtio-gpu P3 trace gate failed: {}", blockers.join("; "));
            }
            Ok(())
        }
        HvfCommand::TitleGateReport(args) => run_title_gate_report(args),
    }
}

pub(crate) const VIRTIO_GPU_TRACE_FEATURE_VIRGL: u64 = 1 << 0;
pub(crate) const VIRTIO_GPU_TRACE_FEATURE_RESOURCE_BLOB: u64 = 1 << 3;
pub(crate) const VIRTIO_GPU_TRACE_FEATURE_CONTEXT_INIT: u64 = 1 << 4;
pub(crate) const VIRTIO_TRACE_FEATURE_VERSION_1: u64 = 1 << 0;
pub(crate) const VIRTIO_GPU_TRACE_CAPSET_VIRGL: u64 = 1;
pub(crate) const VIRTIO_GPU_TRACE_CAPSET_VIRGL2: u64 = 2;
pub(crate) const VIRTIO_GPU_TRACE_CAPSET_VENUS: u64 = 4;

#[derive(Debug)]
pub(crate) struct TitleGateManifest {
    pub(crate) path: PathBuf,
    pub(crate) id: String,
    pub(crate) api: String,
    pub(crate) architecture: String,
    pub(crate) executable: Option<String>,
    pub(crate) working_directory: Option<String>,
    pub(crate) arguments: Vec<String>,
    pub(crate) executable_sha256: Option<String>,
    pub(crate) log: PathBuf,
    pub(crate) pass_marker: String,
    pub(crate) minimum_runtime_seconds: u64,
    pub(crate) required_modules: Vec<String>,
    pub(crate) require_main_window: bool,
    pub(crate) minimum_resource_flushes: u64,
}

#[derive(Debug)]
pub(crate) struct TitleGateResult {
    pub(crate) manifest: TitleGateManifest,
    pub(crate) log_path: PathBuf,
    pub(crate) log_sha256: Option<String>,
    pub(crate) fresh_log: bool,
    pub(crate) elapsed_ms: Option<u64>,
    pub(crate) resource_flushes: u64,
    pub(crate) blockers: Vec<String>,
}

impl TitleGateResult {
    pub(crate) fn passed(&self) -> bool {
        self.blockers.is_empty()
    }

    pub(crate) fn as_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.manifest.id,
            "api": self.manifest.api,
            "architecture": self.manifest.architecture,
            "executable": self.manifest.executable,
            "working_directory": self.manifest.working_directory,
            "arguments": self.manifest.arguments,
            "manifest": self.manifest.path,
            "log": self.log_path,
            "log_sha256": self.log_sha256,
            "fresh_log": self.fresh_log,
            "elapsed_ms": self.elapsed_ms,
            "resource_flushes": self.resource_flushes,
            "minimum_resource_flushes": self.manifest.minimum_resource_flushes,
            "passed": self.passed(),
            "blockers": self.blockers,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hvf_virtio_block_iso_backing_probe_cli_requires_iso() {
        let error =
            Cli::try_parse_from(["bridgevm", "hvf", "virtio-block-iso-backing-probe"]).unwrap_err();

        assert!(error.to_string().contains("--iso"));
    }
}
