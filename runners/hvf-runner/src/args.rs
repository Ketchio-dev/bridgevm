//! Split out of main.rs to keep files under 800 lines.

use anyhow::bail;
use anyhow::Result;
use bridgevm_hvf::plan_windows_11_arm_hvf_machine;
use bridgevm_hvf::plan_windows_11_arm_no_qemu;
use bridgevm_hvf::probe_hvf_guest_entry;
use bridgevm_hvf::probe_hvf_guest_exit_loop;
use bridgevm_hvf::probe_hvf_interrupt_timer;
use bridgevm_hvf::probe_hvf_memory_map;
use bridgevm_hvf::probe_hvf_mmio_block_device;
use bridgevm_hvf::probe_hvf_mmio_block_queue;
use bridgevm_hvf::probe_hvf_mmio_read_emulation;
use bridgevm_hvf::probe_hvf_mmio_read_exit;
use bridgevm_hvf::probe_hvf_mmio_rtc_device;
use bridgevm_hvf::probe_hvf_mmio_serial_device;
use bridgevm_hvf::probe_hvf_mmio_write_emulation;
use bridgevm_hvf::probe_hvf_vcpu_create;
use bridgevm_hvf::probe_hvf_vcpu_run;
use bridgevm_hvf::probe_hvf_vm_create;
use bridgevm_hvf::probe_hvf_vtimer_exit;
use bridgevm_hvf::probe_virtio_block_file_backing;
use bridgevm_hvf::probe_virtio_block_iso_backing;
use bridgevm_hvf::probe_virtio_block_request_model;
use bridgevm_hvf::probe_virtio_block_writable_file_backing;
use bridgevm_hvf::probe_windows_11_arm_boot_disk_layout;
use bridgevm_hvf::probe_windows_11_arm_platform_description;
use bridgevm_hvf::probe_windows_11_arm_uefi_firmware_device_discovery;
use bridgevm_hvf::probe_windows_11_arm_uefi_firmware_handoff;
use bridgevm_hvf::probe_windows_11_arm_uefi_firmware_run_loop;
use bridgevm_hvf::probe_windows_11_arm_uefi_pflash_hvf_map;
use bridgevm_hvf::probe_windows_11_arm_uefi_pflash_map;
use bridgevm_hvf::probe_windows_11_arm_uefi_reset_vector_entry;
use bridgevm_hvf::probe_windows_11_arm_xhci_hid_boot_key_report;
use bridgevm_hvf::query_hvf_host_capabilities;
use bridgevm_hvf::HvfMachinePlanOptions;
use bridgevm_hvf::WindowsArmBootDiskLayoutOptions;
use bridgevm_hvf::WindowsArmPlatformDescriptionOptions;
use bridgevm_hvf::WindowsArmUefiFirmwareHandoffOptions;
use bridgevm_hvf::WindowsArmUefiFirmwareRunLoopExecutionOptions;
use bridgevm_hvf::WindowsArmUefiFirmwareRunLoopOptions;
use bridgevm_hvf::WindowsArmUefiPflashMapOptions;
use bridgevm_hvf::WINDOWS_ARM_BOOT_DISK_DEFAULT_SIZE_GIB;
use clap::Parser;
use std::env;
use std::path::PathBuf;

#[cfg(debug_assertions)]
#[path = "legacy_launch.rs"]
mod legacy_launch;
#[cfg(debug_assertions)]
pub(crate) use legacy_launch::*;

#[derive(Debug, Parser)]
#[command(
    name = "hvf-runner",
    about = "BridgeVM Apple Hypervisor.framework runner R&D boundary"
)]
pub(crate) struct Args {
    #[arg(long)]
    pub(crate) windows_plan: bool,
    #[arg(long)]
    pub(crate) machine_plan: bool,
    #[arg(long)]
    pub(crate) windows_boot_disk_layout_probe: bool,
    #[arg(long)]
    pub(crate) windows_firmware_handoff_probe: bool,
    #[arg(long)]
    pub(crate) windows_pflash_map_probe: bool,
    #[arg(long)]
    pub(crate) windows_pflash_hvf_map_probe: bool,
    #[arg(long)]
    pub(crate) windows_reset_vector_entry_probe: bool,
    #[arg(long)]
    pub(crate) windows_firmware_run_loop_probe: bool,
    #[arg(long)]
    pub(crate) windows_firmware_device_discovery_probe: bool,
    #[arg(long)]
    pub(crate) windows_platform_description_probe: bool,
    #[arg(long)]
    pub(crate) windows_xhci_hid_boot_key_probe: bool,
    #[arg(long)]
    pub(crate) host_capabilities: bool,
    #[arg(long)]
    pub(crate) vm_probe: bool,
    #[arg(long)]
    pub(crate) vcpu_probe: bool,
    #[arg(long)]
    pub(crate) allow_create: bool,
    #[arg(long)]
    pub(crate) vcpu_run_probe: bool,
    #[arg(long)]
    pub(crate) allow_run: bool,
    #[arg(long)]
    pub(crate) interrupt_timer_probe: bool,
    #[arg(long)]
    pub(crate) allow_interrupt_timer: bool,
    #[arg(long)]
    pub(crate) vtimer_exit_probe: bool,
    #[arg(long)]
    pub(crate) allow_vtimer_exit: bool,
    #[arg(long)]
    pub(crate) memory_map_probe: bool,
    #[arg(long)]
    pub(crate) allow_map: bool,
    #[arg(long)]
    pub(crate) guest_entry_probe: bool,
    #[arg(long)]
    pub(crate) allow_entry: bool,
    #[arg(long)]
    pub(crate) guest_exit_loop_probe: bool,
    #[arg(long)]
    pub(crate) allow_loop: bool,
    #[arg(long)]
    pub(crate) mmio_read_probe: bool,
    #[arg(long)]
    pub(crate) allow_mmio: bool,
    #[arg(long)]
    pub(crate) mmio_read_emulation_probe: bool,
    #[arg(long)]
    pub(crate) mmio_write_emulation_probe: bool,
    #[arg(long)]
    pub(crate) mmio_serial_device_probe: bool,
    #[arg(long)]
    pub(crate) mmio_rtc_device_probe: bool,
    #[arg(long)]
    pub(crate) mmio_block_device_probe: bool,
    #[arg(long)]
    pub(crate) mmio_block_queue_probe: bool,
    #[arg(long)]
    pub(crate) virtio_block_request_model_probe: bool,
    #[arg(long)]
    pub(crate) virtio_block_file_backing_probe: bool,
    #[arg(long)]
    pub(crate) virtio_block_writable_file_backing_probe: bool,
    #[arg(long)]
    pub(crate) virtio_block_iso_backing_probe: bool,
    #[arg(long)]
    pub(crate) allow_emulate: bool,
    #[arg(long)]
    pub(crate) allow_device: bool,
    #[arg(long, value_name = "PATH")]
    pub(crate) installer: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) disk: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) target: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) placeholder_nsid1: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) iso: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) writable_disk: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) firmware: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) vars_template: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) vars: Option<PathBuf>,
    #[arg(long, value_name = "DIR")]
    pub(crate) evidence_dir: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) repo_root: Option<PathBuf>,
    #[arg(long)]
    pub(crate) create: bool,
    #[arg(long)]
    pub(crate) create_vars: bool,
    #[arg(long, default_value_t = 6)]
    pub(crate) memory_gib: u32,
    #[arg(long, default_value_t = WINDOWS_ARM_BOOT_DISK_DEFAULT_SIZE_GIB)]
    pub(crate) size_gib: u32,
    #[arg(long, default_value_t = 4)]
    pub(crate) vcpus: u8,
    #[arg(long, default_value_t = 8)]
    pub(crate) max_exits: u32,
    #[arg(long, default_value_t = 64)]
    pub(crate) guest_ram_mib: u32,
    #[arg(long)]
    pub(crate) watchdog_ms: Option<u64>,
    #[arg(long, requires = "launch", conflicts_with = "watchdog_ms")]
    pub(crate) no_watchdog: bool,
    #[arg(long)]
    pub(crate) max_reboots: Option<u32>,
    #[arg(long)]
    pub(crate) ram_mib: Option<u32>,
    #[arg(long)]
    pub(crate) smp_cpus: Option<u8>,
    #[arg(long)]
    pub(crate) boot_timer: bool,
    #[arg(long)]
    pub(crate) boot_timer_ramfb_ms: Option<u32>,
    #[arg(long)]
    pub(crate) boot_timer_desktop_checksum64: Option<String>,
    #[arg(long)]
    pub(crate) boot_timer_desktop_agent: bool,
    #[arg(long)]
    pub(crate) shutdown_after_agent_ready: bool,
    #[arg(long)]
    pub(crate) host_pause_resume_proof_ms: Option<u32>,
    #[arg(long, value_name = "PATH")]
    pub(crate) agent_service_control: Option<PathBuf>,
    #[arg(long, value_name = "COMMAND")]
    pub(crate) agent_service_command: Option<String>,
    #[arg(long)]
    pub(crate) agent_clipboard_sync: bool,
    #[arg(long, value_name = "DIR")]
    pub(crate) agent_share_host: Option<PathBuf>,
    #[arg(long, value_name = "DIR")]
    pub(crate) agent_share_guest: Option<String>,
    #[arg(long)]
    pub(crate) agent_share_ms: Option<u64>,
    #[arg(long)]
    pub(crate) agent_share_max_kb: Option<u64>,
    #[arg(long)]
    pub(crate) enable_xhci: bool,
    #[arg(long)]
    pub(crate) virtio_net: bool,
    #[arg(long)]
    pub(crate) nvme_buffered_io: bool,
    #[arg(long)]
    pub(crate) virtio_gpu_3d: bool,
    #[arg(long)]
    pub(crate) virtio_gpu_device_id: Option<String>,
    #[arg(long, value_name = "PATH")]
    pub(crate) gpu_trace: Option<PathBuf>,
    #[arg(long)]
    pub(crate) gpu_trace_protocol: Option<String>,
    #[arg(long)]
    pub(crate) require_gpu_trace_gate: bool,
    #[arg(long, value_name = "DIR")]
    pub(crate) viogpu3d_dir: Option<PathBuf>,
    #[arg(long)]
    pub(crate) require_viogpu3d_readiness: bool,
    #[arg(long)]
    pub(crate) daily: bool,
    #[arg(long)]
    pub(crate) release: bool,
    #[arg(long)]
    pub(crate) skip_build: bool,
    #[arg(long)]
    pub(crate) print_policy: bool,
    #[arg(long)]
    pub(crate) map_low_pflash_alias: bool,
    #[arg(long)]
    pub(crate) seed_diagnostic_vector: bool,
    #[arg(long)]
    pub(crate) seed_guest_ram_diagnostic_vector: bool,
    #[arg(long)]
    pub(crate) seed_executable_diagnostic_vector: bool,
    #[arg(long)]
    pub(crate) try_recommended_vector_base_vbar: bool,
    #[arg(long)]
    pub(crate) continue_after_recommended_vector_base_vbar: bool,
    #[arg(long)]
    pub(crate) repair_low_vector_diagnostic_page: bool,
    #[arg(long)]
    pub(crate) remap_low_vector_to_recommended_vector: bool,
    #[arg(long)]
    pub(crate) continue_after_low_vector_repair: bool,
    #[arg(long)]
    pub(crate) restore_low_vector_slot_before_eret: bool,
    #[arg(long)]
    pub(crate) wire_interrupt_timer: bool,
    #[arg(long)]
    pub(crate) launch: bool,
    #[command(flatten)]
    pub(crate) typed: crate::launch_spec::LaunchSpecArgs,
    /// Run CMD (argv, no shell) under the reset-cycle supervisor: exit 42
    /// means the guest requested SYSTEM_RESET and a fresh process follows
    /// after the flush receipt; any other success ends the loop.
    #[arg(long, value_name = "CMD", num_args = 1.., allow_hyphen_values = true)]
    pub(crate) supervise: Option<Vec<String>>,
    /// Reset receipt path for --supervise (default: beside the temp dir).
    #[arg(long, value_name = "PATH")]
    pub(crate) supervise_receipt: Option<String>,
    /// Cycle budget for --supervise.
    #[arg(long, value_name = "N", default_value_t = 100)]
    pub(crate) supervise_max_cycles: u32,
}

pub(crate) fn run() -> Result<()> {
    let args = Args::parse();

    #[cfg(not(debug_assertions))]
    {
        if args.launch || args.repo_root.is_some() || args.supervise.is_some() {
            bail!("--launch, --repo-root, and --supervise are unavailable in release builds");
        }
        if args
            .typed
            .helper
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
        {
            bail!("release helper executable path must be absolute: --helper");
        }
        if args
            .typed
            .helper_swtpm_bin
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
        {
            bail!("release helper executable path must be absolute: --helper-swtpm-bin");
        }
    }

    if let Some(spec) = &args.typed.launch_spec {
        return crate::launch_spec::run_launch_spec(spec, &args);
    }

    #[cfg(debug_assertions)]
    if let Some(argv) = &args.supervise {
        let default_receipt = std::env::temp_dir().join("bridgevm-reset.receipt");
        let receipt = args
            .supervise_receipt
            .clone()
            .unwrap_or_else(|| default_receipt.to_string_lossy().into_owned());
        return crate::supervise_cli::run_supervise(argv, &receipt, args.supervise_max_cycles);
    }

    #[cfg(debug_assertions)]
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
    #[cfg(debug_assertions)]
    println!(
        "Run: hvf-runner --launch --target <installed-windows.raw> --vars <vars.fd> --evidence-dir <dir> [--daily|--smp-cpus N|--boot-timer|--print-policy]"
    );
    Ok(())
}

pub(crate) fn env_truthy(name: &str) -> bool {
    match env::var(name) {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}
