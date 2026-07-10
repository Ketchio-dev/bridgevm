use anyhow::{bail, Context, Result};
use bridgevm_hvf::{
    plan_windows_11_arm_hvf_machine, plan_windows_11_arm_no_qemu, probe_hvf_guest_entry,
    probe_hvf_guest_exit_loop, probe_hvf_interrupt_timer, probe_hvf_memory_map,
    probe_hvf_mmio_block_device, probe_hvf_mmio_block_queue, probe_hvf_mmio_read_emulation,
    probe_hvf_mmio_read_exit, probe_hvf_mmio_rtc_device, probe_hvf_mmio_serial_device,
    probe_hvf_mmio_write_emulation, probe_hvf_vcpu_create, probe_hvf_vcpu_run, probe_hvf_vm_create,
    probe_hvf_vtimer_exit, probe_virtio_block_file_backing, probe_virtio_block_iso_backing,
    probe_virtio_block_request_model, probe_virtio_block_writable_file_backing,
    probe_windows_11_arm_boot_disk_layout, probe_windows_11_arm_platform_description,
    probe_windows_11_arm_uefi_firmware_device_discovery,
    probe_windows_11_arm_uefi_firmware_handoff, probe_windows_11_arm_uefi_firmware_run_loop,
    probe_windows_11_arm_uefi_pflash_hvf_map, probe_windows_11_arm_uefi_pflash_map,
    probe_windows_11_arm_uefi_reset_vector_entry, probe_windows_11_arm_xhci_hid_boot_key_report,
    query_hvf_host_capabilities, HvfMachinePlanOptions, WindowsArmBootDiskLayoutOptions,
    WindowsArmPlatformDescriptionOptions, WindowsArmUefiFirmwareHandoffOptions,
    WindowsArmUefiFirmwareRunLoopExecutionOptions, WindowsArmUefiFirmwareRunLoopOptions,
    WindowsArmUefiPflashMapOptions, WINDOWS_ARM_BOOT_DISK_DEFAULT_SIZE_GIB,
};
use clap::Parser;
use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Parser)]
#[command(
    name = "hvf-runner",
    about = "BridgeVM Apple Hypervisor.framework runner R&D boundary"
)]
struct Args {
    #[arg(long)]
    windows_plan: bool,
    #[arg(long)]
    machine_plan: bool,
    #[arg(long)]
    windows_boot_disk_layout_probe: bool,
    #[arg(long)]
    windows_firmware_handoff_probe: bool,
    #[arg(long)]
    windows_pflash_map_probe: bool,
    #[arg(long)]
    windows_pflash_hvf_map_probe: bool,
    #[arg(long)]
    windows_reset_vector_entry_probe: bool,
    #[arg(long)]
    windows_firmware_run_loop_probe: bool,
    #[arg(long)]
    windows_firmware_device_discovery_probe: bool,
    #[arg(long)]
    windows_platform_description_probe: bool,
    #[arg(long)]
    windows_xhci_hid_boot_key_probe: bool,
    #[arg(long)]
    host_capabilities: bool,
    #[arg(long)]
    vm_probe: bool,
    #[arg(long)]
    vcpu_probe: bool,
    #[arg(long)]
    allow_create: bool,
    #[arg(long)]
    vcpu_run_probe: bool,
    #[arg(long)]
    allow_run: bool,
    #[arg(long)]
    interrupt_timer_probe: bool,
    #[arg(long)]
    allow_interrupt_timer: bool,
    #[arg(long)]
    vtimer_exit_probe: bool,
    #[arg(long)]
    allow_vtimer_exit: bool,
    #[arg(long)]
    memory_map_probe: bool,
    #[arg(long)]
    allow_map: bool,
    #[arg(long)]
    guest_entry_probe: bool,
    #[arg(long)]
    allow_entry: bool,
    #[arg(long)]
    guest_exit_loop_probe: bool,
    #[arg(long)]
    allow_loop: bool,
    #[arg(long)]
    mmio_read_probe: bool,
    #[arg(long)]
    allow_mmio: bool,
    #[arg(long)]
    mmio_read_emulation_probe: bool,
    #[arg(long)]
    mmio_write_emulation_probe: bool,
    #[arg(long)]
    mmio_serial_device_probe: bool,
    #[arg(long)]
    mmio_rtc_device_probe: bool,
    #[arg(long)]
    mmio_block_device_probe: bool,
    #[arg(long)]
    mmio_block_queue_probe: bool,
    #[arg(long)]
    virtio_block_request_model_probe: bool,
    #[arg(long)]
    virtio_block_file_backing_probe: bool,
    #[arg(long)]
    virtio_block_writable_file_backing_probe: bool,
    #[arg(long)]
    virtio_block_iso_backing_probe: bool,
    #[arg(long)]
    allow_emulate: bool,
    #[arg(long)]
    allow_device: bool,
    #[arg(long, value_name = "PATH")]
    installer: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    disk: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    target: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    placeholder_nsid1: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    iso: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    writable_disk: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    firmware: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    vars_template: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    vars: Option<PathBuf>,
    #[arg(long, value_name = "DIR")]
    evidence_dir: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    repo_root: Option<PathBuf>,
    #[arg(long)]
    create: bool,
    #[arg(long)]
    create_vars: bool,
    #[arg(long, default_value_t = 6)]
    memory_gib: u32,
    #[arg(long, default_value_t = WINDOWS_ARM_BOOT_DISK_DEFAULT_SIZE_GIB)]
    size_gib: u32,
    #[arg(long, default_value_t = 4)]
    vcpus: u8,
    #[arg(long, default_value_t = 8)]
    max_exits: u32,
    #[arg(long, default_value_t = 64)]
    guest_ram_mib: u32,
    #[arg(long)]
    watchdog_ms: Option<u64>,
    #[arg(long)]
    max_reboots: Option<u32>,
    #[arg(long)]
    ram_mib: Option<u32>,
    #[arg(long)]
    smp_cpus: Option<u8>,
    #[arg(long)]
    boot_timer: bool,
    #[arg(long)]
    boot_timer_ramfb_ms: Option<u32>,
    #[arg(long)]
    boot_timer_desktop_checksum64: Option<String>,
    #[arg(long)]
    boot_timer_desktop_agent: bool,
    #[arg(long)]
    enable_xhci: bool,
    #[arg(long)]
    virtio_net: bool,
    #[arg(long)]
    virtio_gpu_3d: bool,
    #[arg(long)]
    virtio_gpu_device_id: Option<String>,
    #[arg(long, value_name = "PATH")]
    gpu_trace: Option<PathBuf>,
    #[arg(long)]
    gpu_trace_protocol: Option<String>,
    #[arg(long)]
    require_gpu_trace_gate: bool,
    #[arg(long, value_name = "DIR")]
    viogpu3d_dir: Option<PathBuf>,
    #[arg(long)]
    require_viogpu3d_readiness: bool,
    #[arg(long)]
    daily: bool,
    #[arg(long)]
    release: bool,
    #[arg(long)]
    skip_build: bool,
    #[arg(long)]
    print_policy: bool,
    #[arg(long)]
    map_low_pflash_alias: bool,
    #[arg(long)]
    seed_diagnostic_vector: bool,
    #[arg(long)]
    seed_guest_ram_diagnostic_vector: bool,
    #[arg(long)]
    seed_executable_diagnostic_vector: bool,
    #[arg(long)]
    try_recommended_vector_base_vbar: bool,
    #[arg(long)]
    continue_after_recommended_vector_base_vbar: bool,
    #[arg(long)]
    repair_low_vector_diagnostic_page: bool,
    #[arg(long)]
    remap_low_vector_to_recommended_vector: bool,
    #[arg(long)]
    continue_after_low_vector_repair: bool,
    #[arg(long)]
    restore_low_vector_slot_before_eret: bool,
    #[arg(long)]
    wire_interrupt_timer: bool,
    #[arg(long)]
    launch: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.launch {
        return launch_installed_windows(&args);
    }

    if args.windows_plan {
        let plan = plan_windows_11_arm_no_qemu(args.installer);
        print!("{}", plan.render_text());
        return Ok(());
    }

    if args.machine_plan {
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
        return Ok(());
    }

    if args.windows_boot_disk_layout_probe {
        if args.size_gib == 0 {
            bail!("--size-gib must be greater than zero");
        }
        let disk = args.disk.clone().ok_or_else(|| {
            anyhow::anyhow!("--disk is required for --windows-boot-disk-layout-probe")
        })?;
        let probe = probe_windows_11_arm_boot_disk_layout(WindowsArmBootDiskLayoutOptions {
            disk_path: disk,
            size_gib: args.size_gib,
            create: args.create,
        });
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.windows_firmware_handoff_probe {
        let firmware = args.firmware.clone().ok_or_else(|| {
            anyhow::anyhow!("--firmware is required for --windows-firmware-handoff-probe")
        })?;
        let probe =
            probe_windows_11_arm_uefi_firmware_handoff(WindowsArmUefiFirmwareHandoffOptions {
                firmware_path: firmware,
                vars_template_path: args.vars_template.clone(),
                vars_path: args.vars.clone(),
                create_vars: args.create_vars,
            });
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.windows_pflash_map_probe {
        let firmware = args.firmware.clone().ok_or_else(|| {
            anyhow::anyhow!("--firmware is required for --windows-pflash-map-probe")
        })?;
        let probe = probe_windows_11_arm_uefi_pflash_map(WindowsArmUefiPflashMapOptions {
            firmware_path: firmware,
            vars_template_path: args.vars_template.clone(),
            vars_path: args.vars.clone(),
            create_vars: args.create_vars,
        });
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.windows_pflash_hvf_map_probe {
        let firmware = args.firmware.clone().ok_or_else(|| {
            anyhow::anyhow!("--firmware is required for --windows-pflash-hvf-map-probe")
        })?;
        let allow_map =
            args.allow_map || env::var("BRIDGEVM_HVF_ALLOW_UEFI_PFLASH_MAP").as_deref() == Ok("1");
        let probe = probe_windows_11_arm_uefi_pflash_hvf_map(
            WindowsArmUefiPflashMapOptions {
                firmware_path: firmware,
                vars_template_path: args.vars_template.clone(),
                vars_path: args.vars.clone(),
                create_vars: args.create_vars,
            },
            allow_map,
        );
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.windows_reset_vector_entry_probe {
        let firmware = args.firmware.clone().ok_or_else(|| {
            anyhow::anyhow!("--firmware is required for --windows-reset-vector-entry-probe")
        })?;
        let allow_entry = args.allow_entry
            || env::var("BRIDGEVM_HVF_ALLOW_UEFI_RESET_VECTOR_ENTRY").as_deref() == Ok("1");
        let probe = probe_windows_11_arm_uefi_reset_vector_entry(
            WindowsArmUefiPflashMapOptions {
                firmware_path: firmware,
                vars_template_path: args.vars_template.clone(),
                vars_path: args.vars.clone(),
                create_vars: args.create_vars,
            },
            allow_entry,
        );
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.windows_firmware_run_loop_probe {
        if args.max_exits == 0 {
            bail!("--max-exits must be greater than zero");
        }
        if args.guest_ram_mib == 0 {
            bail!("--guest-ram-mib must be greater than zero");
        }
        let watchdog_ms = args.watchdog_ms.unwrap_or(100);
        if watchdog_ms == 0 {
            bail!("--watchdog-ms must be greater than zero");
        }
        let firmware = args.firmware.clone().ok_or_else(|| {
            anyhow::anyhow!("--firmware is required for --windows-firmware-run-loop-probe")
        })?;
        let allow_loop = args.allow_loop
            || env::var("BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP").as_deref() == Ok("1");
        let probe =
            probe_windows_11_arm_uefi_firmware_run_loop(WindowsArmUefiFirmwareRunLoopOptions {
                pflash: WindowsArmUefiPflashMapOptions {
                    firmware_path: firmware,
                    vars_template_path: args.vars_template.clone(),
                    vars_path: args.vars.clone(),
                    create_vars: args.create_vars,
                },
                execution: WindowsArmUefiFirmwareRunLoopExecutionOptions {
                    allow_loop,
                    requested_exits: args.max_exits,
                    guest_ram_mib: args.guest_ram_mib,
                    watchdog_timeout_ms: watchdog_ms,
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
                    restore_low_vector_slot_before_eret: args.restore_low_vector_slot_before_eret,
                    wire_interrupt_timer: args.wire_interrupt_timer,
                    stop_at_first_post_repair_device_boundary: false,
                    installer_iso_path: args.iso.clone(),
                    writable_target_disk_path: args.writable_disk.clone(),
                },
            });
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.windows_firmware_device_discovery_probe {
        if args.max_exits == 0 {
            bail!("--max-exits must be greater than zero");
        }
        if args.guest_ram_mib == 0 {
            bail!("--guest-ram-mib must be greater than zero");
        }
        let watchdog_ms = args.watchdog_ms.unwrap_or(100);
        if watchdog_ms == 0 {
            bail!("--watchdog-ms must be greater than zero");
        }
        let firmware = args.firmware.clone().ok_or_else(|| {
            anyhow::anyhow!("--firmware is required for --windows-firmware-device-discovery-probe")
        })?;
        let allow_loop = args.allow_loop
            || env::var("BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP").as_deref() == Ok("1");
        let probe = probe_windows_11_arm_uefi_firmware_device_discovery(
            WindowsArmUefiFirmwareRunLoopOptions {
                pflash: WindowsArmUefiPflashMapOptions {
                    firmware_path: firmware,
                    vars_template_path: args.vars_template.clone(),
                    vars_path: args.vars.clone(),
                    create_vars: args.create_vars,
                },
                execution: WindowsArmUefiFirmwareRunLoopExecutionOptions {
                    allow_loop,
                    requested_exits: args.max_exits,
                    guest_ram_mib: args.guest_ram_mib,
                    watchdog_timeout_ms: watchdog_ms,
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
                    restore_low_vector_slot_before_eret: args.restore_low_vector_slot_before_eret,
                    wire_interrupt_timer: args.wire_interrupt_timer,
                    stop_at_first_post_repair_device_boundary: false,
                    installer_iso_path: args.iso.clone(),
                    writable_target_disk_path: args.writable_disk.clone(),
                },
            },
        );
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.windows_platform_description_probe {
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
        return Ok(());
    }

    if args.windows_xhci_hid_boot_key_probe {
        let probe = probe_windows_11_arm_xhci_hid_boot_key_report();
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.host_capabilities {
        let capabilities = query_hvf_host_capabilities();
        print!("{}", capabilities.render_text());
        return Ok(());
    }

    if args.vm_probe {
        let allow_create =
            args.allow_create || env::var("BRIDGEVM_HVF_ALLOW_VM_CREATE").as_deref() == Ok("1");
        let probe = probe_hvf_vm_create(allow_create);
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.vcpu_probe {
        let allow_create =
            args.allow_create || env::var("BRIDGEVM_HVF_ALLOW_VM_CREATE").as_deref() == Ok("1");
        let probe = probe_hvf_vcpu_create(allow_create);
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.vcpu_run_probe {
        let allow_run =
            args.allow_run || env::var("BRIDGEVM_HVF_ALLOW_VCPU_RUN").as_deref() == Ok("1");
        let probe = probe_hvf_vcpu_run(allow_run);
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.interrupt_timer_probe {
        let allow_probe = args.allow_interrupt_timer
            || env::var("BRIDGEVM_HVF_ALLOW_INTERRUPT_TIMER").as_deref() == Ok("1");
        let probe = probe_hvf_interrupt_timer(allow_probe);
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.vtimer_exit_probe {
        let allow_probe = args.allow_vtimer_exit || env_truthy("BRIDGEVM_HVF_ALLOW_VTIMER_EXIT");
        let probe = probe_hvf_vtimer_exit(allow_probe);
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.memory_map_probe {
        let allow_map =
            args.allow_map || env::var("BRIDGEVM_HVF_ALLOW_MEMORY_MAP").as_deref() == Ok("1");
        let probe = probe_hvf_memory_map(allow_map);
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.guest_entry_probe {
        let allow_entry =
            args.allow_entry || env::var("BRIDGEVM_HVF_ALLOW_GUEST_ENTRY").as_deref() == Ok("1");
        let probe = probe_hvf_guest_entry(allow_entry);
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.guest_exit_loop_probe {
        let allow_loop =
            args.allow_loop || env::var("BRIDGEVM_HVF_ALLOW_EXIT_LOOP").as_deref() == Ok("1");
        let probe = probe_hvf_guest_exit_loop(allow_loop);
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.mmio_read_probe {
        let allow_mmio =
            args.allow_mmio || env::var("BRIDGEVM_HVF_ALLOW_MMIO_READ").as_deref() == Ok("1");
        let probe = probe_hvf_mmio_read_exit(allow_mmio);
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.mmio_read_emulation_probe {
        let allow_emulate = args.allow_emulate
            || env::var("BRIDGEVM_HVF_ALLOW_MMIO_EMULATION").as_deref() == Ok("1");
        let probe = probe_hvf_mmio_read_emulation(allow_emulate);
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.mmio_write_emulation_probe {
        let allow_emulate = args.allow_emulate
            || env::var("BRIDGEVM_HVF_ALLOW_MMIO_WRITE_EMULATION").as_deref() == Ok("1");
        let probe = probe_hvf_mmio_write_emulation(allow_emulate);
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.mmio_serial_device_probe {
        let allow_device = args.allow_device
            || env::var("BRIDGEVM_HVF_ALLOW_MMIO_SERIAL_DEVICE").as_deref() == Ok("1");
        let probe = probe_hvf_mmio_serial_device(allow_device);
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.mmio_rtc_device_probe {
        let allow_device = args.allow_device
            || env::var("BRIDGEVM_HVF_ALLOW_MMIO_RTC_DEVICE").as_deref() == Ok("1");
        let probe = probe_hvf_mmio_rtc_device(allow_device);
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.mmio_block_device_probe {
        let allow_device = args.allow_device
            || env::var("BRIDGEVM_HVF_ALLOW_MMIO_BLOCK_DEVICE").as_deref() == Ok("1");
        let probe = probe_hvf_mmio_block_device(allow_device);
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.mmio_block_queue_probe {
        let backing_selectors = usize::from(args.disk.is_some())
            + usize::from(args.iso.is_some())
            + usize::from(args.writable_disk.is_some());
        if backing_selectors > 1 {
            bail!("--disk, --iso, and --writable-disk are mutually exclusive for --mmio-block-queue-probe");
        }
        let allow_device = args.allow_device
            || env::var("BRIDGEVM_HVF_ALLOW_MMIO_BLOCK_QUEUE").as_deref() == Ok("1");
        let probe = probe_hvf_mmio_block_queue(
            allow_device,
            args.disk.clone(),
            args.iso.clone(),
            args.writable_disk.clone(),
        );
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.virtio_block_request_model_probe {
        let probe = probe_virtio_block_request_model();
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.virtio_block_iso_backing_probe {
        let iso = args.iso.clone().ok_or_else(|| {
            anyhow::anyhow!("--iso is required for --virtio-block-iso-backing-probe")
        })?;
        let probe = probe_virtio_block_iso_backing(iso);
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.virtio_block_file_backing_probe {
        let disk = args.disk.clone().ok_or_else(|| {
            anyhow::anyhow!("--disk is required for --virtio-block-file-backing-probe")
        })?;
        let probe = probe_virtio_block_file_backing(disk);
        print!("{}", probe.render_text());
        return Ok(());
    }

    if args.virtio_block_writable_file_backing_probe {
        let disk = args.disk.clone().ok_or_else(|| {
            anyhow::anyhow!("--disk is required for --virtio-block-writable-file-backing-probe")
        })?;
        let probe = probe_virtio_block_writable_file_backing(disk);
        print!("{}", probe.render_text());
        return Ok(());
    }

    println!("hvf-runner ready: metadata-only Windows 11 Arm no-QEMU planning boundary");
    println!("Run: hvf-runner --windows-plan --installer <Win11_Arm64.iso>");
    println!("Run: hvf-runner --machine-plan --installer <Win11_Arm64.iso>");
    println!(
        "Run: hvf-runner --windows-boot-disk-layout-probe --disk <windows.raw> [--size-gib 64] [--create]"
    );
    println!(
        "Run: hvf-runner --windows-firmware-handoff-probe --firmware <AAVMF_CODE.fd> --vars-template <AAVMF_VARS.fd> --vars <vars.fd> [--create-vars]"
    );
    println!(
        "Run: hvf-runner --windows-pflash-map-probe --firmware <AAVMF_CODE.fd> --vars-template <AAVMF_VARS.fd> --vars <vars.fd> [--create-vars]"
    );
    println!(
        "Run: hvf-runner --windows-pflash-hvf-map-probe --firmware <AAVMF_CODE.fd> --vars-template <AAVMF_VARS.fd> --vars <vars.fd> [--create-vars] [--allow-map]"
    );
    println!(
        "Run: hvf-runner --windows-reset-vector-entry-probe --firmware <AAVMF_CODE.fd> --vars-template <AAVMF_VARS.fd> --vars <vars.fd> [--create-vars] [--allow-entry]"
    );
    println!(
        "Run: hvf-runner --windows-firmware-run-loop-probe --firmware <AAVMF_CODE.fd> --vars-template <AAVMF_VARS.fd> --vars <vars.fd> [--create-vars] [--allow-loop] [--max-exits 8] [--guest-ram-mib 64] [--watchdog-ms 100] [--map-low-pflash-alias] [--seed-diagnostic-vector|--seed-guest-ram-diagnostic-vector|--seed-executable-diagnostic-vector] [--try-recommended-vector-base-vbar] [--continue-after-recommended-vector-base-vbar] [--repair-low-vector-diagnostic-page] [--remap-low-vector-to-recommended-vector] [--continue-after-low-vector-repair] [--restore-low-vector-slot-before-eret] [--wire-interrupt-timer] [--iso <Win11_Arm64.iso>] [--writable-disk <windows.raw>]"
    );
    println!(
        "Run: hvf-runner --windows-firmware-device-discovery-probe --firmware <AAVMF_CODE.fd> --vars-template <AAVMF_VARS.fd> --vars <vars.fd> [--create-vars] [--allow-loop] [--max-exits 16] [--guest-ram-mib 64] [--watchdog-ms 100] [--map-low-pflash-alias] [--repair-low-vector-diagnostic-page] [--continue-after-low-vector-repair] [--wire-interrupt-timer] [--iso <Win11_Arm64.iso>] [--writable-disk <windows.raw>]"
    );
    println!("Run: hvf-runner --windows-platform-description-probe [--memory-gib 6] [--vcpus 4]");
    println!("Run: hvf-runner --windows-xhci-hid-boot-key-probe");
    println!("Run: hvf-runner --host-capabilities");
    println!("Run: hvf-runner --vm-probe [--allow-create]");
    println!("Run: hvf-runner --vcpu-probe [--allow-create]");
    println!("Run: hvf-runner --vcpu-run-probe [--allow-run]");
    println!("Run: hvf-runner --interrupt-timer-probe [--allow-interrupt-timer]");
    println!("Run: hvf-runner --vtimer-exit-probe [--allow-vtimer-exit]");
    println!("Run: hvf-runner --memory-map-probe [--allow-map]");
    println!("Run: hvf-runner --guest-entry-probe [--allow-entry]");
    println!("Run: hvf-runner --guest-exit-loop-probe [--allow-loop]");
    println!("Run: hvf-runner --mmio-read-probe [--allow-mmio]");
    println!("Run: hvf-runner --mmio-read-emulation-probe [--allow-emulate]");
    println!("Run: hvf-runner --mmio-write-emulation-probe [--allow-emulate]");
    println!("Run: hvf-runner --mmio-serial-device-probe [--allow-device]");
    println!("Run: hvf-runner --mmio-rtc-device-probe [--allow-device]");
    println!("Run: hvf-runner --mmio-block-device-probe [--allow-device]");
    println!(
        "Run: hvf-runner --mmio-block-queue-probe [--allow-device] [--disk <disk.img>|--iso <installer.iso>|--writable-disk <disk.img>]"
    );
    println!("Run: hvf-runner --virtio-block-request-model-probe");
    println!("Run: hvf-runner --virtio-block-file-backing-probe --disk <disk.img>");
    println!("Run: hvf-runner --virtio-block-writable-file-backing-probe --disk <disk.img>");
    println!("Run: hvf-runner --virtio-block-iso-backing-probe --iso <installer.iso>");
    println!(
        "Run: hvf-runner --launch --target <installed-windows.raw> --vars <vars.fd> --evidence-dir <dir> [--daily|--smp-cpus N|--boot-timer|--print-policy]"
    );
    Ok(())
}

fn launch_installed_windows(args: &Args) -> Result<()> {
    let invocation_dir =
        env::current_dir().context("resolve current directory for hvf-runner --launch")?;
    let repo_root = launch_repo_root(args, &invocation_dir);
    let wrapper = repo_root.join("scripts/run-hvf-windows-installed-boot.sh");
    if !wrapper.is_file() {
        bail!(
            "installed Windows HVF boot wrapper not found at {}; pass --repo-root or run from the repository root",
            wrapper.display()
        );
    }

    let wrapper_args = installed_boot_launch_args(args, &invocation_dir)?;
    let status = Command::new(&wrapper)
        .args(&wrapper_args)
        .current_dir(&repo_root)
        .status()
        .with_context(|| format!("launch installed Windows HVF wrapper {}", wrapper.display()))?;
    if status.success() {
        Ok(())
    } else {
        bail!("installed Windows HVF boot wrapper failed with status {status}")
    }
}

fn launch_repo_root(args: &Args, invocation_dir: &Path) -> PathBuf {
    if let Some(repo_root) = &args.repo_root {
        return resolve_launch_path(repo_root, invocation_dir);
    }
    if let Ok(repo_root) = env::var("BRIDGEVM_REPO_ROOT") {
        if !repo_root.trim().is_empty() {
            return resolve_launch_path(Path::new(&repo_root), invocation_dir);
        }
    }
    invocation_dir.to_path_buf()
}

fn installed_boot_launch_args(args: &Args, invocation_dir: &Path) -> Result<Vec<String>> {
    if args.boot_timer_desktop_agent && args.boot_timer_desktop_checksum64.is_some() {
        bail!("choose exactly one BOOT_TIMER desktop oracle: --boot-timer-desktop-agent or --boot-timer-desktop-checksum64");
    }
    let mut target_candidates = [
        args.target.as_ref(),
        args.disk.as_ref(),
        args.writable_disk.as_ref(),
    ]
    .into_iter()
    .flatten();
    let target = target_candidates
        .next()
        .ok_or_else(|| anyhow::anyhow!("--launch requires --target, --disk, or --writable-disk"))?;
    if target_candidates.next().is_some() {
        bail!("--launch accepts only one of --target, --disk, or --writable-disk");
    }
    let vars = args
        .vars
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("--launch requires --vars"))?;
    let evidence_dir = args
        .evidence_dir
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("--launch requires --evidence-dir"))?;

    let mut out = vec![
        "--target".to_string(),
        path_arg(target, invocation_dir),
        "--vars".to_string(),
        path_arg(vars, invocation_dir),
        "--evidence-dir".to_string(),
        path_arg(evidence_dir, invocation_dir),
    ];

    push_path_arg(
        &mut out,
        "--placeholder-nsid1",
        args.placeholder_nsid1.as_ref(),
        invocation_dir,
    );
    push_num_arg(&mut out, "--watchdog-ms", args.watchdog_ms);
    push_num_arg(&mut out, "--max-reboots", args.max_reboots);
    push_num_arg(&mut out, "--ram-mib", args.ram_mib);
    push_num_arg(&mut out, "--smp-cpus", args.smp_cpus);
    push_flag(&mut out, args.boot_timer, "--boot-timer");
    push_num_arg(&mut out, "--boot-timer-ramfb-ms", args.boot_timer_ramfb_ms);
    push_string_arg(
        &mut out,
        "--boot-timer-desktop-checksum64",
        args.boot_timer_desktop_checksum64.as_deref(),
    );
    push_flag(
        &mut out,
        args.boot_timer_desktop_agent,
        "--boot-timer-desktop-agent",
    );
    push_flag(&mut out, args.enable_xhci, "--enable-xhci");
    push_flag(&mut out, args.virtio_net, "--virtio-net");
    push_flag(&mut out, args.virtio_gpu_3d, "--virtio-gpu-3d");
    push_string_arg(
        &mut out,
        "--virtio-gpu-device-id",
        args.virtio_gpu_device_id.as_deref(),
    );
    push_path_arg(
        &mut out,
        "--gpu-trace",
        args.gpu_trace.as_ref(),
        invocation_dir,
    );
    push_string_arg(
        &mut out,
        "--gpu-trace-protocol",
        args.gpu_trace_protocol.as_deref(),
    );
    push_flag(
        &mut out,
        args.require_gpu_trace_gate,
        "--require-gpu-trace-gate",
    );
    push_path_arg(
        &mut out,
        "--viogpu3d-dir",
        args.viogpu3d_dir.as_ref(),
        invocation_dir,
    );
    push_flag(
        &mut out,
        args.require_viogpu3d_readiness,
        "--require-viogpu3d-readiness",
    );
    push_flag(&mut out, args.daily, "--daily");
    push_flag(&mut out, args.release, "--release");
    push_flag(&mut out, args.skip_build, "--skip-build");
    push_flag(&mut out, args.print_policy, "--print-policy");

    Ok(out)
}

fn resolve_launch_path(path: &Path, invocation_dir: &Path) -> PathBuf {
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        invocation_dir.join(path)
    };
    // Existing media/repository paths are canonicalized so a relative `..`
    // does not become ambiguous after the child changes its working directory.
    // Output paths may not exist yet, so retain their invocation-rooted absolute
    // spelling when canonicalization is unavailable.
    resolved.canonicalize().unwrap_or(resolved)
}

fn path_arg(path: &Path, invocation_dir: &Path) -> String {
    resolve_launch_path(path, invocation_dir)
        .to_string_lossy()
        .into_owned()
}

fn push_path_arg(
    out: &mut Vec<String>,
    flag: &str,
    value: Option<&PathBuf>,
    invocation_dir: &Path,
) {
    if let Some(value) = value {
        out.push(flag.to_string());
        out.push(path_arg(value, invocation_dir));
    }
}

fn push_string_arg(out: &mut Vec<String>, flag: &str, value: Option<&str>) {
    if let Some(value) = value {
        out.push(flag.to_string());
        out.push(value.to_string());
    }
}

fn push_num_arg<T: ToString>(out: &mut Vec<String>, flag: &str, value: Option<T>) {
    if let Some(value) = value {
        out.push(flag.to_string());
        out.push(value.to_string());
    }
}

fn push_flag(out: &mut Vec<String>, enabled: bool, flag: &str) {
    if enabled {
        out.push(flag.to_string());
    }
}

fn env_truthy(name: &str) -> bool {
    match env::var(name) {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let args =
            Args::try_parse_from(["hvf-runner", "--windows-xhci-hid-boot-key-probe"]).unwrap();

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
                "--enable-xhci",
                "--virtio-net",
                "--daily",
                "--release",
                "--print-policy",
            ]
        );
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
}
