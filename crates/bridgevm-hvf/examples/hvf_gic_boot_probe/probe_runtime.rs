//! The shipped VMM's run: create the VM, boot the guest (rebooting in process
//! on a guest SYSTEM_RESET), run vCPU0's exit loop on this thread, and end
//! with the stop report.
//!
//! The layers, outermost first: `run` owns the media lease; `run_vm` owns the
//! VM, vCPU0, guest RAM and vCPU0's NVMe worker; `run_boots` owns the
//! platform, the probe-lifetime services and the 'reboot loop; a
//! `BootGeneration` is one guest boot, and its methods are vCPU0's exit loop.
#[path = "probe_runtime/automation_tick.rs"]
mod automation_tick;
#[path = "probe_runtime/automation_stop.rs"]
mod automation_stop;
#[path = "probe_runtime/boot_env.rs"]
mod boot_env;
#[path = "probe_runtime/cpu0_debug_exits.rs"]
mod cpu0_debug_exits;
#[path = "probe_runtime/input_prepare.rs"]
mod input_prepare;
#[path = "probe_runtime/generation_tpm.rs"]
mod generation_tpm;
#[path = "hda_service_tick.rs"]
mod hda_service_tick;
#[path = "../hvf_gic_boot_probe/input_control_wake.rs"]
mod input_control_wake;
#[path = "probe_runtime/service_wake_interval.rs"]
mod service_wake_interval;
#[path = "probe_runtime/system_reset_failure.rs"]
mod system_reset_failure;
#[path = "probe_runtime/vtimer_recovery.rs"]
pub(crate) mod vtimer_recovery;
#[cfg(feature = "venus")]
#[path = "probe_runtime/venus_work_wake.rs"]
mod venus_work_wake;
use super::*;
use crate::agent_console::ServiceThread;
use crate::boot_media_setup::attach_boot_media;
use crate::final_report::persist_and_report_stop;
use crate::gpu_unlocked_copy::configure_gpu_unlocked_copy;
use crate::guest_ram_backing::GuestRamBacking;
use crate::hvf_setup::{create_gic, create_vm};
use crate::probe_config::ProbeConfig;
use crate::probe_setup::prepare_platform;
use crate::vars_autosave::{stop_before_final_save, VarsAutosaveThread};
use crate::wake_coordinator::ClaimTally;
use boot_env::{BootTriggers, ExitTraces};
use bridgevm_hvf::nvme::{NvmeIo, NvmeIoWorker};
use bridgevm_hvf::snapshot_pair::managed::runtime::RuntimeLease;
use bridgevm_hvf::virtio_gpu::{GpuCopy, VblankWakeState};
use generation_tpm::generation_tpm;
use hda_service_tick::HdaServiceTick;
use service_wake_interval::service_wake_interval;
use system_reset_failure::system_reset_failure;

mod media_lease {
    use super::*;
    use bridgevm_hvf::snapshot_pair::managed::runtime::acquire;

    type Prepared = (ProbeConfig, RuntimeLease, Option<Box<dyn Tpm2Backend>>);

    /// The run's media and this generation's TPM, both owned before the VM exists.
    pub(super) fn prepare() -> Option<Prepared> {
        let mut config = ProbeConfig::from_env();
        if let Err(error) = checkpoint_glue::CheckpointEnv::from_env().validate_cpu_count(config.smp_cpus) {
            eprintln!("{error}");
            return None;
        }
        let lease = match acquire(&mut config.media) {
            Ok(lease) => lease,
            Err(error) => {
                eprintln!("media ownership/selection refused before VM creation: {error}");
                return None;
            }
        };
        let restoring = checkpoint_glue::CheckpointEnv::from_env().restore.is_some();
        let data = config.swtpm_data_socket.as_deref();
        let tpm = generation_tpm(data, config.swtpm_control_socket.as_deref(), restoring)
            .map_err(|stop| println!("{stop}"))
            .ok()?;
        Some((config, lease, tpm))
    }
}

/// How the probe process ends (`probe_exit_code`).
#[derive(Default)]
struct ProbeExit {
    fatal_vcpu_run_error: bool,
    fatal_reset_error: bool,
    exit_for_recreate: bool,
}

pub(crate) fn run() -> ExitCode {
    // Before any media is leased: from here TERM ends the run through persistence.
    install_host_term_handler();
    // run.log keeps room for the disk-full pause's lines (pause_gate.rs).
    crate::pause_gate::keep_log_reserve();
    let mut probe_exit = ProbeExit::default();
    let Some((config, media_lease, tpm_backend)) = media_lease::prepare() else {
        return ExitCode::FAILURE;
    };
    // Shared with the UEFI vars saver until the stop-time save (vars_autosave.rs).
    let media_lease = Arc::new(Mutex::new(media_lease));
    // SAFETY: this thread becomes vCPU0; run_vm owns the VM, vCPU0 and the
    // guest RAM they use until it returns. The lease outlives all of them.
    unsafe { run_vm(config, &media_lease, tpm_backend, &mut probe_exit) };
    let ProbeExit {
        fatal_vcpu_run_error,
        fatal_reset_error,
        exit_for_recreate,
    } = probe_exit;
    probe_exit_code(fatal_vcpu_run_error, fatal_reset_error, exit_for_recreate)
}

/// Creates the VM, its GIC and platform, guest RAM and vCPU0 (this thread),
/// readies the first boot, then runs the boots. The locals drop in reverse:
/// vCPU0's NVMe worker, vCPU0, the guest RAM backing, then the VM; `config`
/// last of all.
unsafe fn run_vm(
    config: ProbeConfig,
    media_lease: &Arc<Mutex<RuntimeLease>>,
    tpm_backend: Option<Box<dyn Tpm2Backend>>,
    probe_exit: &mut ProbeExit,
) {
    let (media, platform_cfg, ram_size) = (&config.media, config.platform_cfg, config.ram_size);
    #[cfg(feature = "venus")]
    let use_4k_ipa = platform_cfg.devices.virtio_gpu_present
        && platform_cfg.devices.virtio_gpu_pci_device_id
            == bridgevm_hvf::pcie::VIRTIO_GPU_DEVICE_ID
        && env_flag("BRIDGEVM_VIRTIO_GPU_3D")
        && matches!(
            bridgevm_hvf::venus_backend::VirtioGpuRendererProtocol::from_env(),
            Ok(bridgevm_hvf::venus_backend::VirtioGpuRendererProtocol::Venus)
        );
    #[cfg(not(feature = "venus"))]
    let use_4k_ipa = false;
    let shm_page_size = create_vm(use_4k_ipa);
    let _vm_guard = HvVmGuard;
    create_gic();
    let (mut platform, boot_dtb) = prepare_platform(media, platform_cfg, tpm_backend);

    let ram_backing = GuestRamBacking::allocate_and_map(machine::RAM_BASE, ram_size);
    let ram = ram_backing.pointer();
    assert!(boot_dtb.len() < ram_size, "DTB must fit in guest RAM");

    let mut vcpu: HvVcpuT = 0;
    let mut exit: *mut HvVcpuExit = null_mut();
    assert_eq!(
        hv_vcpu_create(&mut vcpu, &mut exit, null_mut()),
        0,
        "hv_vcpu_create"
    );
    let _vcpu_guard = HvVcpuGuard { vcpu };
    // vCPU0 runs on this thread.
    raise_vcpu_qos();

    let (watch_addr, watch_target) = watchpoint_config();
    attach_boot_media(&mut platform, media, platform_cfg, ram_size);
    let mut guest_ram = MappedRam {
        base: machine::RAM_BASE,
        ptr: ram,
        len: ram_size,
    };
    // vCPU0's NVMe transfer thread; dropped, and joined, before the RAM backing.
    let mut nvme_io_worker = vcpu_nvme_io_worker(0, ram as usize, ram_size);
    reset_guest_ram_for_boot(&mut guest_ram, &boot_dtb);
    let supervision = BootSupervision::start(vcpu, config.watchdog_enabled);
    reset_vcpu_for_boot(vcpu);
    arm_watchpoint_for_boot(vcpu, watch_addr);
    checkpoint_glue::restore_if_requested(
        &[vcpu],
        std::slice::from_raw_parts_mut(ram, ram_size),
        &mut platform,
    )
    .unwrap_or_else(|error| panic!("restore VM checkpoint: {error}"));
    install_gpu_shm_port(&mut platform, shm_page_size);
    configure_gpu_unlocked_copy(&mut platform);
    #[cfg(feature = "venus")]
    let venus_work_wake = (matches!(shm_page_size, GpuShmPageSize::KiB4)
        && env_flag("BRIDGEVM_VIRTIO_GPU_DIRECT_RENDERER"))
    .then(|| venus_work_wake::Worker::start(vcpu, &mut platform))
    .flatten();
    let vm = Vm {
        #[cfg(feature = "venus")]
        venus_work_wake,
        vcpu,
        exit,
        ram,
        platform: Arc::new(Mutex::new(platform)),
        guest_ram,
        boot_dtb,
        watch_addr,
        watch_target,
    };
    run_boots(vm, &config, &mut nvme_io_worker, &supervision, media_lease, probe_exit);
}

/// vCPU0, guest RAM and the platform as `run_vm` made them, moved into the
/// boot loop (`run_vm` keeps the HVF guards and the RAM backing).
struct Vm {
    #[cfg(feature = "venus")]
    venus_work_wake: Option<venus_work_wake::Worker>,
    vcpu: HvVcpuT,
    exit: *mut HvVcpuExit,
    ram: *mut u8,
    platform: Arc<Mutex<VirtPlatform>>,
    guest_ram: MappedRam,
    boot_dtb: Vec<u8>,
    watch_addr: Option<u64>,
    watch_target: u64,
}

/// The run's boot-progress watchdog and host stop watchers (reboot_watchdog).
struct BootSupervision {
    stall_kill_fired: Option<Arc<AtomicBool>>,
    reboot_plan: RebootPlan,
    watchdog_generation: Arc<AtomicU64>,
    boot_progress: Arc<BootProgressWatchdog>,
    diagnostic_stop_fired: Option<Arc<AtomicBool>>,
}

impl BootSupervision {
    fn start(vcpu: HvVcpuT, watchdog_enabled: bool) -> Self {
        // Read in the run loop: waking the vCPU is not the same as stopping it.
        let boot_progress_kill = boot_progress_kill_for(vcpu);
        let stall_kill_fired = boot_progress_kill
            .as_ref()
            .map(|kill| Arc::clone(&kill.fired));
        let (reboot_plan, watchdog_generation, boot_progress) =
            setup_boot_supervision(watchdog_enabled, boot_progress_kill);
        let diagnostic_stop_fired =
            start_host_diagnostic_stop_watcher(vcpu, Arc::clone(&boot_progress));
        start_host_term_watcher(vcpu, Arc::clone(&boot_progress));
        Self {
            stall_kill_fired,
            reboot_plan,
            watchdog_generation,
            boot_progress,
            diagnostic_stop_fired,
        }
    }

    /// The stop flags a canceled exit checks: this boot's watchdog and the run's.
    #[inline]
    fn host_stop<'a>(&'a self, watchdog_fired: &'a AtomicBool) -> HostStop<'a> {
        HostStop {
            watchdog_fired,
            stall_kill_fired: self.stall_kill_fired.as_ref(),
            diagnostic_stop_fired: self.diagnostic_stop_fired.as_ref(),
        }
    }
}

/// A host stop (TERM, a diagnostic stop, the watchdog or a stall kill) ends
/// a pause and the run (wake_coordinator/cancel_stop.rs).
#[derive(Clone, Copy)]
struct HostStop<'a> {
    watchdog_fired: &'a AtomicBool,
    stall_kill_fired: Option<&'a Arc<AtomicBool>>,
    diagnostic_stop_fired: Option<&'a Arc<AtomicBool>>,
}

impl HostStop<'_> {
    #[inline]
    fn reason(self) -> Option<&'static str> {
        cancel_stop_reason(
            self.watchdog_fired,
            self.stall_kill_fired,
            self.diagnostic_stop_fired,
            &HOST_TERM_REQUESTED,
        )
    }
}

/// vCPU0's probe-lifetime wakes, host input and KD serial bridge: each one
/// lives across guest reboots. The fields drop in declaration order, the
/// reverse of `start`'s, and before the platform they serve.
struct ProbeServices {
    live_input: LiveInputController,
    kd_serial_bridge: Option<kd_serial_bridge::KdSerialBridge>,
    nvme_storage_full: Arc<AtomicBool>,
    gpu_vblank_wake_state: Option<Arc<VblankWakeState>>,
    gpu_vblank_wake: vblank_wake::VblankWake,
    input_control_wake: input_control_wake::InputControlWake,
    agent_service_wake: agent_console::ServiceWake,
    agent_survives_battery: Arc<AtomicBool>,
}

impl ProbeServices {
    fn start(config: &ProbeConfig, platform: &Arc<Mutex<VirtPlatform>>, vcpu: HvVcpuT) -> Self {
        // Probe-lifetime: follows the Mac's battery off the vCPUs and ends
        // with the platform (host_power.rs). Each boot's agent console says
        // whether the running agent's task survives battery.
        let agent_survives_battery = Arc::new(AtomicBool::new(false));
        if let Some(shown) = config.platform_cfg.devices.host_power {
            crate::host_power::start_monitor(
                config.power_source.clone(),
                shown,
                Arc::clone(&agent_survives_battery),
                Arc::downgrade(platform),
                vcpu,
            );
        }
        // Probe-lifetime instance, deliberately OUTSIDE the reboot loop: its
        // ticker thread keeps firing across guest reboots, and the SAME fired
        // flag must be the one each boot generation's exit dispatcher consumes.
        // (A per-boot instance turned the first post-reset EXIT_CANCELED from
        // the previous generation's ticker into a bogus "watchdog (CANCELED)"
        // stop — live-observed as the reboot loop dying at reboot 1/8.)
        let agent_service_wake = agent_console::ServiceWake::new();
        let input_control_wake = input_control_wake::InputControlWake::new();
        // Same probe-lifetime rule as ServiceWake above; the wake-state Arc is
        // shared with the virtio-gpu device and survives platform resets. So
        // does the flag the NVMe controller raises while it holds guest writes
        // because the Mac's disk is full: vCPU0 then pauses the VM.
        let gpu_vblank_wake = vblank_wake::VblankWake::new();
        let (gpu_vblank_wake_state, nvme_storage_full) = {
            let platform = platform.lock().expect("platform mutex for vblank wake state");
            (
                platform.virtio_gpu_vblank_wake(),
                platform.nvme_storage_full_flag(),
            )
        };
        // KD (kernel-debug) serial bridge, probe-lifetime like the wakers: it
        // owns the PL011 for the run when BRIDGEVM_KD_SERIAL_SOCKET is set, so
        // a WinDbg peer can attach to the guest's KDCOM stream. None otherwise,
        // leaving the serial to the boot scanner unchanged.
        let kd_serial_bridge = kd_serial_bridge::KdSerialBridge::from_env();
        // Keep the file offset and pending queue across guest SYSTEM_RESET.
        // Recreating the controller per boot generation replays every command
        // already consumed from an append-only control file, which can repeat
        // destructive guest actions such as a TPM-clear reboot request.
        let live_input = LiveInputController::from_env();
        Self {
            live_input,
            kd_serial_bridge,
            nvme_storage_full,
            gpu_vblank_wake_state,
            gpu_vblank_wake,
            input_control_wake,
            agent_service_wake,
            agent_survives_battery,
        }
    }
}

/// What each boot generation borrows from the probe for vCPU0's exit loop.
struct Probe<'a> {
    config: &'a ProbeConfig,
    vcpu: HvVcpuT,
    exit: *mut HvVcpuExit,
    ram: *mut u8,
    watch_target: u64,
    platform: &'a Arc<Mutex<VirtPlatform>>,
    guest_ram: &'a mut MappedRam,
    nvme_io_worker: &'a mut NvmeIoWorker,
    supervision: &'a BootSupervision,
    services: &'a mut ProbeServices,
    hda_service_tick: &'a HdaServiceTick,
    fatal_vcpu_run_error: &'a mut bool,
}

/// BRIDGEVM_UART_RX: serial input queued before the boot's first instruction.
fn preload_uart(platform: &Arc<Mutex<VirtPlatform>>, smp_trace: Option<&SmpTrace>) {
    let mut platform_guard = lock_platform(platform, smp_trace, 0, HoldSite::UartPreload);
    let platform = &mut *platform_guard;
    if let Ok(input) = std::env::var("BRIDGEVM_UART_RX") {
        platform.push_uart_input(input.as_bytes());
        println!("UART RX preloaded: {} bytes", input.len());
    }
}

/// Crash-survivable snapshot: capture regs + full RAM BEFORE the reboot arm
/// wipes guest RAM / resets the vCPU. Gated on BRIDGEVM_DUMP_ON_RESET;
/// defaults to only the first reset so a gen1 bugcheck (venus StartDevice)
/// is caught, not later gens.
fn dump_on_reset(
    resets_dumped: &mut u64,
    reboot_count: u64,
    vcpu: HvVcpuT,
    ram: *mut u8,
    ram_size: usize,
) {
    if let Some(dir) = dump_on_reset_dir() {
        if *resets_dumped < dump_on_reset_max() {
            println!(
                "DUMP_ON_RESET: capturing reset #{resets_dumped} (reboot_count={reboot_count})"
            );
            dump_guest_state_on_reset(&dir, *resets_dumped, vcpu, ram, ram_size);
            *resets_dumped += 1;
        }
    }
}

/// One guest boot: its secondary vCPUs, watchdog and automation, and what
/// vCPU0's exits counted and traced for the stop report. Rebuilt for every
/// in-process reboot.
struct BootGeneration {
    drain_trace: DrainTrace,
    smp_trace: Option<Arc<SmpTrace>>,
    secondary_vcpus: Option<SecondaryVcpuSet>,
    watchdog_fired: Arc<AtomicBool>,
    wake_coordinator: Arc<WakeCoordinator>,
    wake_cancel_claims: ClaimTally,
    automation: BootAutomation,
    traces: ExitTraces,
    drain_stats: RunLoopDrainStats,
    exits: u64,
    vtimer_exits: u64,
    surplus_canceled_exits: u64,
    psci_calls: u64,
    last_pc: u64,
    last_pre_run_pc: u64,
    watch_hits: u32,
    last_watch_pc: u64,
    last_watch_lr: u64,
    stop_reason_code: Option<u32>,
    requested_system_reset: bool,
    pause_requested: bool,
    cpu0_offset_version: u64,
}

/// This boot's automation: scripted input, framebuffer samples and exports,
/// BOOT_TIMER telemetry and the agent console's service thread.
struct BootAutomation {
    triggers: BootTriggers,
    pointer_deadline_wake: input_control_wake::PointerDeadlineWake,
    gate: AutomationGate,
    serial_stop_scans: SerialStopScans,
    no_bootable_disk: NoBootableDisk,
    ramfb_sample_loop: RamfbSampleLoop,
    live_display_exporter: LiveDisplayExporter,
    setup_input_host_wake: SetupInputHostWake,
    boot_timer: BootTimer,
    /// Joined on drop, at `continue 'reboot` or after the stop report:
    /// always before the next generation's from_env.
    agent_console: Option<ServiceThread>,
}

impl BootGeneration {
    /// This boot's vCPUs, watchdog and automation, in the probe's established
    /// order: secondaries and the watchdog first, vCPU0's wakes last.
    unsafe fn start(probe: &mut Probe<'_>, diagnostic_generation: u64) -> Self {
        let (config, vcpu, supervision) = (probe.config, probe.vcpu, probe.supervision);
        let drain_trace = DrainTrace {
            msix: config.trace_msix,
            spi: config.trace_spi,
        };
        let smp_trace =
            (config.smp_cpus > 1 && config.smp_trace_enabled).then(|| Arc::new(SmpTrace::new()));
        let pre_run_drain_gate = Arc::new(PreRunDrainGate::from_env());
        let secondary_vcpus = (config.smp_cpus > 1).then(|| {
            SecondaryVcpuSet::spawn(SecondaryVcpuSpawnConfig {
                cpu_count: config.smp_cpus,
                primary_vcpu: vcpu,
                ram_base: probe.ram as usize,
                ram_size: config.ram_size,
                platform: Arc::clone(probe.platform),
                drain_trace,
                pre_run_drain_gate: Arc::clone(&pre_run_drain_gate),
                smp_trace: smp_trace.clone(),
                max_exits: config.max_exits,
                generation: diagnostic_generation,
            })
        });
        let watchdog_generation = &supervision.watchdog_generation;
        let boot_generation = begin_watchdog_generation(watchdog_generation);
        let pointer_deadline_wake = input_control_wake::PointerDeadlineWake::new(
            vcpu,
            (watchdog_generation, boot_generation),
        );
        let watchdog_fired = Arc::new(AtomicBool::new(false));
        // One cancellation ledger per vCPU generation; see wake_coordinator.
        let wake_coordinator = Arc::new(WakeCoordinator::new());
        wake_coordinator.begin_generation();
        if config.watchdog_enabled {
            spawn_boot_watchdog(
                vcpu,
                config.watchdog_ms,
                Arc::clone(watchdog_generation),
                boot_generation,
                Arc::clone(&watchdog_fired),
            );
        }
        preload_uart(probe.platform, smp_trace.as_deref());
        let triggers = BootTriggers::from_env();
        let traces = ExitTraces::from_env(config.trace_xhci_bringup);
        let drain_stats = RunLoopDrainStats::new(config.trace_run_loop);
        let automation = BootAutomation::start(probe, triggers, pointer_deadline_wake);
        Self {
            drain_trace,
            smp_trace,
            secondary_vcpus,
            watchdog_fired,
            wake_coordinator,
            wake_cancel_claims: ClaimTally::default(),
            automation,
            traces,
            drain_stats,
            exits: 0,
            vtimer_exits: 0,
            surplus_canceled_exits: 0,
            psci_calls: 0,
            last_pc: 0,
            last_pre_run_pc: 0,
            watch_hits: 0,
            last_watch_pc: 0,
            last_watch_lr: 0,
            stop_reason_code: None,
            requested_system_reset: false,
            pause_requested: false,
            cpu0_offset_version: 0,
        }
    }

    /// After vCPU0's loop: freeze BOOT_TIMER, mark a guest power-off, end the
    /// watchdog generation and join the secondaries, whose fatal run error
    /// becomes the stop.
    fn stop_vcpus(
        &mut self,
        supervision: &BootSupervision,
        stop_reason: &mut String,
        fatal_vcpu_run_error: &mut bool,
    ) -> (Duration, SecondaryVcpuStopResult) {
        // Freeze the measured duration at the VM stop boundary. Media
        // persistence and diagnostic dumps after this are evidence work, not
        // guest boot time.
        let boot_timer_elapsed = self.automation.boot_timer.elapsed();
        if *stop_reason == format!("PSCI {PSCI_SYSTEM_OFF:#x} (system off)") {
            // The guest's shutdown ends here; vm_bench times it to this line.
            println!("guest powered off; host teardown follows");
            if let Some(agent) = self.automation.agent_console.as_mut() {
                agent.guest_powered_off();
            }
        }
        invalidate_watchdog_generation(&supervision.watchdog_generation);
        let secondary_stop = self
            .secondary_vcpus
            .take()
            .map(SecondaryVcpuSet::shutdown_and_join)
            .unwrap_or_default();
        secondary_stop.merge_run_error(
            fatal_vcpu_run_error,
            &mut self.requested_system_reset,
            stop_reason,
        );
        (boot_timer_elapsed, secondary_stop)
    }
}

impl BootAutomation {
    /// The rest of the boot's setup, vCPU0's wakes last.
    unsafe fn start(
        probe: &mut Probe<'_>,
        triggers: BootTriggers,
        pointer_deadline_wake: input_control_wake::PointerDeadlineWake,
    ) -> Self {
        let vcpu = probe.vcpu;
        let ramfb_sample_loop = RamfbSampleLoop::from_env();
        let live_display_exporter = LiveDisplayExporter::from_env();
        let setup_input_host_wake = SetupInputHostWake::new();
        // The agent console runs on VM time, from this boot's start (boot_clocks).
        let boot_started = crate::boot_clocks::agent_origin(&crate::boot_clocks::Live);
        let boot_timer = BootTimer::from_env();
        let services = &mut *probe.services;
        let agent_harness = AgentConsoleHarness::from_env(boot_started).map(|mut harness| {
            harness.share_agent_survives_battery(&services.agent_survives_battery);
            harness
        });
        let automation_always_check = triggers.any()
            || agent_harness
                .as_ref()
                .is_some_and(AgentConsoleHarness::per_exit_tick_needed);
        let gate = AutomationGate::new(automation_always_check);
        // Host-driven vCPU0 work with no wake of its own: without a steady
        // waker the main loop sleeps in hv_vcpu_run while the desktop idles
        // and its tick starves (see ServiceWake docs). BOOT_TIMER uses a
        // wake no slower than 250 ms and honors a shorter requested RAMFB
        // interval. These sources deliberately do not force the automation
        // block's platform mutex on every CPU0 exit. ensure_started is
        // idempotent, so re-entering here after a guest reboot is fine.
        // The agent service ticks on its own thread and needs none.
        if let Some(interval) = service_wake_interval(
            &boot_timer,
            live_display_exporter.enabled(),
            services.kd_serial_bridge.is_some(),
        ) {
            services.agent_service_wake.ensure_started(vcpu, interval);
        }
        let agent_console = agent_harness.map(|harness| {
            services
                .agent_service_wake
                .spawn_service(vcpu, harness, probe.platform)
        });
        services.input_control_wake.ensure_started(vcpu);
        // A finished NVMe flush completes at the next pre-run drain: run one now.
        // (vCPU0's live handle; the call only asks it to leave hv_vcpu_run.)
        let _ = bridgevm_hvf::nvme::FLUSH_DONE_HOOK.set(Box::new(move || {
            hv_vcpus_exit(&vcpu, 1);
        }));
        // A NAT host socket has data for the guest: vCPU0's drain polls it.
        let _ = bridgevm_hvf::net_nat::NET_RX_HOOK.set(Box::new(move || {
            hv_vcpus_exit(&vcpu, 1);
        }));
        if let Some(state) = services.gpu_vblank_wake_state.as_ref() {
            services.gpu_vblank_wake.ensure_started(vcpu, Arc::clone(state));
        }
        Self {
            triggers,
            pointer_deadline_wake,
            gate,
            serial_stop_scans: SerialStopScans::default(),
            no_bootable_disk: NoBootableDisk::default(),
            ramfb_sample_loop,
            live_display_exporter,
            setup_input_host_wake,
            boot_timer,
            agent_console,
        }
    }
}

/// Starts the probe's services and its display and HDA threads, then boots
/// the guest until a boot stops the probe. A SYSTEM_RESET reboots in process
/// (fresh secondaries, the same vCPU0 and platform); every other stop ends in
/// the stop report, which takes `media_lease` back from the UEFI vars saver.
unsafe fn run_boots(
    vm: Vm,
    config: &ProbeConfig,
    nvme_io_worker: &mut NvmeIoWorker,
    supervision: &BootSupervision,
    media_lease: &Arc<Mutex<RuntimeLease>>,
    probe_exit: &mut ProbeExit,
) {
    let Vm {
        #[cfg(feature = "venus")]
        venus_work_wake,
        vcpu,
        exit,
        ram,
        platform,
        mut guest_ram,
        boot_dtb,
        watch_addr,
        watch_target,
    } = vm;
    let (media, ram_size) = (&config.media, config.ram_size);
    let mut services = ProbeServices::start(config, &platform, vcpu);
    // Off vCPU0; joined on drop, before the guest RAM backing is unmapped.
    let ramfb_display = RamfbDisplayThread::start(&platform, ram, ram_size);
    // Polls HDA DMA every 5 ms while the guest's stream runs; same lifetime.
    let hda_service_tick = HdaServiceTick::start(&platform, ram, ram_size);
    // Saves the guest's UEFI variable changes during the run, across reboots.
    let vars_autosave = media
        .flash_vars
        .write_back
        .then(|| VarsAutosaveThread::start(&platform, media_lease))
        .flatten();
    let mut reboot_count = 0u64;
    let mut resets_dumped = 0u64;

    'reboot: loop {
        // Secondary vCPUs are intentionally scoped to one boot generation in
        // Stage 1. SYSTEM_RESET joins them before resetting CPU0/platform
        // state; the next loop iteration respawns a fresh parked set.
        let diagnostic_generation = reboot_count.wrapping_add(1);
        let mut probe = Probe {
            config,
            vcpu,
            exit,
            ram,
            watch_target,
            platform: &platform,
            guest_ram: &mut guest_ram,
            nvme_io_worker: &mut *nvme_io_worker,
            supervision,
            services: &mut services,
            hda_service_tick: &hda_service_tick,
            fatal_vcpu_run_error: &mut probe_exit.fatal_vcpu_run_error,
        };
        let mut generation = BootGeneration::start(&mut probe, diagnostic_generation);
        let mut stop_reason = generation.run_cpu0(&mut probe);
        let (boot_timer_elapsed, secondary_stop) = generation.stop_vcpus(
            supervision,
            &mut stop_reason,
            &mut probe_exit.fatal_vcpu_run_error,
        );
        if generation.requested_system_reset {
            #[cfg(feature = "venus")]
            if let Some(wake) = &venus_work_wake {
                wake.clear();
            }
            dump_on_reset(&mut resets_dumped, reboot_count, vcpu, ram, ram_size);
            match decide_system_reset(reboot_count, supervision.reboot_plan) {
                SystemResetDecision::Reboot { next_reboot_count } => {
                    // No HDA tick polls or sends an MSI across these resets.
                    let _hda_quiesced = hda_service_tick.quiesce();
                    reboot_count = next_reboot_count;
                    supervision.boot_progress.record_reboot();
                    println!(
                        "PSCI SYSTEM_RESET: reboot {reboot_count}/{}",
                        supervision.reboot_plan.max_reboots
                    );
                    // All secondary vCPUs have stopped and joined above, and CPU0
                    // is outside hv_vcpu_run. Apple documents hv_gic_reset as the
                    // VM-reset operation for the distributor, redistributors, and
                    // the GIC device's internal state.
                    let gic_reset_status = hv_gic_reset();
                    println!("hv_gic_reset = {gic_reset_status:#x}");
                    if let Some(reason) = system_reset_failure(gic_reset_status, &platform) {
                        probe_exit.fatal_reset_error = true;
                        stop_reason = reason;
                    } else {
                        reset_guest_ram_for_boot(&mut guest_ram, &boot_dtb);
                        reset_vcpu_for_boot(vcpu);
                        arm_watchpoint_for_boot(vcpu, watch_addr);
                        continue 'reboot;
                    }
                }
                SystemResetDecision::Stop { reason } => {
                    supervision.boot_progress.disarm();
                    stop_reason = reason;
                }
                SystemResetDecision::ExitForRecreate => {
                    supervision.boot_progress.disarm();
                    probe_exit.exit_for_recreate = true;
                    stop_reason = format!(
                        "PSCI {PSCI_SYSTEM_RESET:#x} exiting for process recreation (exit {RESET_EXIT_CODE})"
                    );
                }
            }
        }

        // Terminal runs only; the export's stop record must precede the report footer.
        supervision.boot_progress.disarm();
        #[cfg(feature = "venus")]
        drop(venus_work_wake);
        drop(ramfb_display);
        drop(hda_service_tick);
        // No background vars save overlaps the stop-time one, which always runs.
        let mut media_lease = stop_before_final_save(vars_autosave, media_lease);
        // The report's names; the agent console stays in `generation` and is
        // joined when this iteration ends, after the report.
        let BootGeneration {
            smp_trace,
            wake_coordinator,
            wake_cancel_claims,
            automation: BootAutomation {
                triggers, boot_timer, ..
            },
            traces,
            drain_stats,
            exits,
            vtimer_exits,
            surplus_canceled_exits,
            psci_calls,
            last_pc,
            last_pre_run_pc,
            last_watch_pc,
            last_watch_lr,
            stop_reason_code,
            ..
        } = generation;
        let ExitTraces {
            mmio_traces,
            recent_pcie_mmio,
            recent_pcie_pio,
            recent_pcie_ecam,
            recent_xhci,
            unimpl,
            redist_lo,
            redist_hi,
            ..
        } = traces;
        let BootTriggers {
            uart: uart_triggers,
            xhci_hid_boot_key: xhci_hid_boot_key_triggers,
            xhci_setup_input: xhci_setup_input_triggers,
            xhci_pointer_input: xhci_pointer_input_triggers,
        } = triggers;
        persist_and_report_stop!(
            platform,
            (media, media_lease),
            vcpu,
            guest_ram,
            last_pc,
            last_pre_run_pc,
            last_watch_pc,
            last_watch_lr,
            stop_reason,
            stop_reason_code,
            exits,
            vtimer_exits,
            psci_calls,
            surplus_canceled_exits,
            wake_coordinator,
            wake_cancel_claims,
            boot_timer,
            boot_timer_elapsed,
            diagnostic_generation,
            secondary_stop,
            drain_stats,
            unimpl,
            mmio_traces,
            recent_pcie_ecam,
            recent_pcie_mmio,
            recent_pcie_pio,
            recent_xhci,
            uart_triggers,
            xhci_hid_boot_key_triggers,
            xhci_setup_input_triggers,
            xhci_pointer_input_triggers,
            redist_lo,
            redist_hi,
            smp_trace,
        );
        break 'reboot;
    }
}

/// An interrupt delivery prepared under the platform lock.
type PreparedDelivery = Result<PendingDrainDelivery, SpiDeliveryError>;

/// Where vCPU0's exit loop goes after an exit it handled; the `Err` of a
/// handler's result is the reason this boot stops.
enum AfterExit {
    /// Straight back into hv_vcpu_run.
    RunAgain,
    /// On through the exit cap and the automation pass.
    Automation,
}

/// vCPU0's exit loop. Each pass runs the pre-run work, one hv_vcpu_run and
/// the handler for its exit class, then the exit cap and the automation pass.
impl BootGeneration {
    /// Runs the guest on vCPU0 until this boot stops, and says why.
    unsafe fn run_cpu0(&mut self, probe: &mut Probe<'_>) -> String {
        loop {
            if let Err(stop_reason) = self.cpu0_pass(probe) {
                return stop_reason;
            }
        }
    }

    #[inline]
    unsafe fn cpu0_pass(&mut self, probe: &mut Probe<'_>) -> Result<(), String> {
        self.before_run(probe)?;
        let reason = self.run_vcpu0(probe)?;
        let (automation_tick_canceled, input_wake) = self.wake_cancels(probe.services, reason);
        if let AfterExit::RunAgain = self.on_exit(probe, reason, automation_tick_canceled)? {
            return Ok(());
        }
        let max_exits = probe.config.max_exits;
        if self.exits >= max_exits {
            return Err(format!("exit cap {max_exits}"));
        }
        self.after_exit(probe, automation_tick_canceled, input_wake)
    }

    /// Before each hv_vcpu_run: a requested pause, the vTimer offset a
    /// resume left, and the pre-run interrupt drain.
    #[inline]
    unsafe fn before_run(&mut self, probe: &mut Probe<'_>) -> Result<(), String> {
        let vcpu = probe.vcpu;
        let mut drain_pc = 0u64;
        hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut drain_pc);
        self.last_pre_run_pc = drain_pc;
        self.pause_if_requested(probe)?;
        // After a resume vCPU0 takes the shifted vTimer offset on its
        // own thread, outside hv_vcpu_run (pause_gate.rs).
        crate::pause_gate::GATE.take_offset(&mut self.cpu0_offset_version, |offset| {
            crate::pause_gate::set_vtimer_offset(vcpu, offset)
        });
        // CPU0 always drains here. A renderer worker can finish an
        // asynchronous present after the post-MMIO drain but before
        // this pre-run site; treating the two drains as duplicates can
        // strand the newest frame until the guest exits again.
        let pending = self.drain_stats.prepare_primary_pre_run(
            probe.platform,
            probe.guest_ram,
            self.smp_trace.as_deref(),
            probe.services.kd_serial_bridge.as_mut(),
            self.drain_trace,
            DrainContext::pre_run(self.exits, drain_pc),
        );
        self.deliver(probe.fatal_vcpu_run_error, pending)
    }

    /// An input.ctl PAUSE read at the end of the last pass, or guest disk
    /// writes held for a full Mac disk (nvme disk_full.rs), pause the VM here
    /// until it resumes; an input.ctl RESET while paused ends the boot.
    unsafe fn pause_if_requested(&mut self, probe: &mut Probe<'_>) -> Result<(), String> {
        let host_stop = probe.supervision.host_stop(&self.watchdog_fired);
        // Not while a host stop is pending: the held writes stay held,
        // and vCPU0 runs on to the cancel exit that ends the run.
        let disk_full = (probe.services.nvme_storage_full.load(Ordering::Acquire)
            && host_stop.reason().is_none())
        .then(|| {
            lock_platform(probe.platform, self.smp_trace.as_deref(), 0, HoldSite::Automation)
                .nvme_disk_full()
        })
        .flatten();
        if !(std::mem::take(&mut self.pause_requested) || disk_full.is_some()) {
            return Ok(());
        }
        if let Some(full) = &disk_full {
            crate::pause_gate::report_storage_full(full);
        }
        // No HDA DMA or MSI while paused; with no PCM written, the
        // CoreAudio supervisor pauses the AudioQueue (IDLE_PAUSE).
        let hda_quiesced = probe.hda_service_tick.quiesce();
        let paused = crate::pause_gate::pause_and_wait(
            &mut probe.services.live_input,
            self.secondary_vcpus.as_ref(),
            || host_stop.reason().is_some(),
            // The held writes complete before any vCPU runs.
            || {
                lock_platform(probe.platform, self.smp_trace.as_deref(), 0, HoldSite::Automation)
                    .retry_nvme_held(probe.guest_ram)
            },
        );
        drop(hda_quiesced);
        match paused {
            crate::pause_gate::PausedExit::Reset => Err(self.host_reset()),
            // Windows' wall clock fell behind by the pause, as after
            // a host sleep: the agent puts it back on host time.
            crate::pause_gate::PausedExit::Resumed(paused) => {
                if let Some(agent) = self.automation.agent_console.as_mut() {
                    agent.note_vm_pause(paused);
                }
                Ok(())
            }
        }
    }

    /// The app's Reset: end this boot exactly as the guest's PSCI
    /// SYSTEM_RESET does, so the runtime recreates the helper.
    fn host_reset(&mut self) -> String {
        self.requested_system_reset = true;
        "host reset (input.ctl RESET)".into()
    }

    /// Completes a prepared interrupt delivery; a failed one stops the run.
    #[inline]
    fn deliver(&mut self, fatal: &mut bool, pending: PreparedDelivery) -> Result<(), String> {
        self.drain_stats
            .finish_prepared_interrupt_delivery(pending, self.drain_trace)
            .map_err(|error| error.stop_primary(fatal, &mut self.last_pc))
    }

    /// One hv_vcpu_run, counted; a terminal PSCI call a secondary forwarded
    /// stops the boot here.
    #[inline]
    unsafe fn run_vcpu0(&mut self, probe: &mut Probe<'_>) -> Result<u32, String> {
        let reason = match run_hvf_vcpu_once(probe.vcpu, probe.exit, self.secondary_vcpus.as_ref())
        {
            Ok(reason) => reason,
            Err(r) => {
                hv_vcpu_get_reg(probe.vcpu, HV_REG_PC, &mut self.last_pc);
                *probe.fatal_vcpu_run_error = true;
                return Err(r.to_string());
            }
        };
        self.exits = probe.supervision.boot_progress.count_exit(self.exits);
        if let Some(trace) = self.smp_trace.as_deref() {
            trace.cpu0_progress(self.exits);
        }
        self.stop_reason_code = Some(reason);
        if let Some((reason, reset)) = self
            .secondary_vcpus
            .as_ref()
            .and_then(SecondaryVcpuSet::psci_stop_reason)
        {
            self.psci_calls += 1;
            self.requested_system_reset = reset;
            return Err(reason);
        }
        Ok(reason)
    }

    /// Which host wakes this exit answers, each wake's fired flag consumed
    /// in this order, and a cancel's attribution (wake_coordinator). Returns
    /// whether an automation wake merged into the exit, and whether the
    /// input-control wake was one of them.
    #[inline]
    fn wake_cancels(&mut self, services: &ProbeServices, reason: u32) -> (bool, bool) {
        let watchdog_fired = &self.watchdog_fired;
        let automation = &self.automation;
        let sample_tick_canceled = automation
            .ramfb_sample_loop
            .canceled_by_sample_tick(reason, watchdog_fired);
        let setup_input_wake_canceled = automation
            .setup_input_host_wake
            .canceled_by_host_wake(reason, watchdog_fired);
        let service_wake_canceled = services
            .agent_service_wake
            .canceled_by_service_wake(reason, watchdog_fired);
        let input_wake = services.input_control_wake.canceled(reason, watchdog_fired);
        let setup_input_wake_canceled = setup_input_wake_canceled || input_wake;
        let vblank_wake_canceled = services
            .gpu_vblank_wake
            .canceled_by_vblank_wake(reason, watchdog_fired);
        let automation_tick_canceled = sample_tick_canceled
            || setup_input_wake_canceled
            || service_wake_canceled
            || vblank_wake_canceled;
        if reason == EXIT_CANCELED {
            self.wake_cancel_claims
                .add(self.wake_coordinator.attribute_cancel(&[
                    (sample_tick_canceled, WakeReason::RamfbSample),
                    (setup_input_wake_canceled, WakeReason::SetupInput),
                    (service_wake_canceled, WakeReason::AgentConsole),
                    (vblank_wake_canceled, WakeReason::Vblank),
                    (
                        watchdog_fired.load(Ordering::SeqCst),
                        WakeReason::RebootWatchdog,
                    ),
                ]));
        }
        (automation_tick_canceled, input_wake)
    }

    /// One exit by class, unless an automation wake merged into it.
    #[inline]
    unsafe fn on_exit(
        &mut self,
        probe: &mut Probe<'_>,
        reason: u32,
        automation_tick_canceled: bool,
    ) -> Result<AfterExit, String> {
        let vcpu = probe.vcpu;
        if reason == EXIT_CANCELED {
            // Check terminal flags even when an automation wake merged
            // into this cancel: a host diagnostic request must not be
            // mistaken for a service tick.
            if let Some(reason) = probe.supervision.host_stop(&self.watchdog_fired).reason() {
                hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut self.last_pc);
                return Err(reason.into());
            }
            if !automation_tick_canceled {
                self.surplus_canceled_exits += 1;
                return Ok(AfterExit::RunAgain);
            }
        }
        if automation_tick_canceled {
            return Ok(AfterExit::Automation);
        }
        match reason {
            EXIT_VTIMER => {
                hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut self.last_pc);
                self.vtimer_exits += 1;
                hv_vcpu_set_vtimer_mask(vcpu, true);
                let max_exits = probe.config.max_exits;
                if self.exits >= max_exits {
                    return Err(format!("exit cap {max_exits}"));
                }
                Ok(AfterExit::RunAgain)
            }
            EXIT_EXCEPTION => self.on_exception(probe),
            _ => {
                hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut self.last_pc);
                Err(format!("exit reason {reason}"))
            }
        }
    }

    /// A guest exception: its class picks the handler.
    unsafe fn on_exception(&mut self, probe: &mut Probe<'_>) -> Result<AfterExit, String> {
        let vcpu = probe.vcpu;
        let esr = (*probe.exit).exception.syndrome;
        let ec = (esr >> 26) & 0x3f;
        hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut self.last_pc);
        let last_pc = self.last_pc;
        match ec {
            EC_DATA_ABORT => return self.on_data_abort(probe, esr, ec),
            EC_HVC => self.on_hvc(vcpu)?,
            EC_SYS_REG_TRAP => self.on_sys_reg_trap(vcpu, esr)?,
            EC_WATCHPOINT_LOWER | EC_WATCHPOINT_SAME => self.on_watchpoint(vcpu),
            EC_SOFTSTEP_LOWER | EC_SOFTSTEP_SAME => self.on_softstep(probe),
            _ => return Err(format!("exception EC {ec:#x} ESR {esr:#x} @ PC {last_pc:#x}")),
        }
        Ok(AfterExit::Automation)
    }

    /// A data abort: a framebuffer page's first write, or MMIO to a device.
    unsafe fn on_data_abort(
        &mut self,
        probe: &mut Probe<'_>,
        esr: u64,
        ec: u64,
    ) -> Result<AfterExit, String> {
        let (vcpu, last_pc) = (probe.vcpu, self.last_pc);
        let ipa = (*probe.exit).exception.physical_address;
        if probe.guest_ram.host_ptr(ipa, 1).is_some()
            && crate::ramfb_dirty_watch::FB_WATCH.take_write_fault(ipa, esr)
        {
            return Ok(AfterExit::RunAgain);
        }
        let access = match DataAbort::decode(esr) {
            Ok(access) => access,
            Err(error) => {
                let ignored = ignore_reset_gpu_bar2_store(
                    vcpu,
                    last_pc,
                    &(*probe.exit).exception,
                    probe.guest_ram,
                    probe.platform,
                )
                .is_some();
                trace_isv0_data_abort(esr, last_pc, ipa, ignored);
                if ignored {
                    return Ok(AfterExit::Automation);
                }
                *probe.fatal_vcpu_run_error = true;
                return Err(error);
            }
        };
        let (size, srt, is_write) = (access.size, access.register, access.write);
        // srt=31 is WZR/XZR; never leak register-file slot 31 (PC) into MMIO.
        let op = if is_write {
            MmioOp::Write {
                size,
                value: mmio_store_value(vcpu, access),
            }
        } else {
            MmioOp::Read { size }
        };
        let trace_this_fwcfg = probe.config.trace_fwcfg
            && machine::FW_CFG.contains(ipa)
            && self.traces.fwcfg_trace_count < 512;
        if trace_this_fwcfg {
            self.traces.fwcfg_trace_count += 1;
            let fwcfg_trace_count = self.traces.fwcfg_trace_count;
            println!(
                "FWCFG[{fwcfg_trace_count:03}] pc={last_pc:#x} off={:#x} op={op:?}",
                ipa - machine::FW_CFG.base
            );
        }
        let (outcome, pending, nvme_io, gpu_copy) = self.dispatch_mmio(probe, ipa, esr, ec, srt, op);
        self.finish_mmio_work(probe, ipa, pending, nvme_io, gpu_copy)?;
        if trace_this_fwcfg {
            let fwcfg_trace_count = self.traces.fwcfg_trace_count;
            println!("FWCFG[{fwcfg_trace_count:03}] -> {outcome:?}");
        }
        match outcome {
            MmioOutcome::ReadValue(v) if !is_write => {
                write_mmio_read(vcpu, access, v);
            }
            MmioOutcome::ReadValue(_) | MmioOutcome::WriteAck => {}
            MmioOutcome::KnownUnimplemented(name) => {
                let traces = &mut self.traces;
                *traces.unimpl.entry(name).or_insert(0) += 1;
                if name == "gic-redist" {
                    traces.redist_lo = traces.redist_lo.min(ipa);
                    traces.redist_hi = traces.redist_hi.max(ipa);
                }
                write_mmio_read(vcpu, access, 0);
            }
            MmioOutcome::Unmapped => {
                *self.traces.unimpl.entry("<unmapped>").or_insert(0) += 1;
                write_mmio_read(vcpu, access, 0);
            }
        }
        hv_vcpu_set_reg(vcpu, HV_REG_PC, last_pc.wrapping_add(4));
        Ok(AfterExit::Automation)
    }

    /// The MMIO access under one platform hold: the device's outcome, its
    /// interrupt drain and any NVMe transfer or virtio-gpu copy it queued;
    /// then, unlocked, the PCIe, serial and per-device traces.
    unsafe fn dispatch_mmio(
        &mut self,
        probe: &mut Probe<'_>,
        ipa: u64,
        esr: u64,
        ec: u64,
        srt: u32,
        op: MmioOp,
    ) -> (MmioOutcome, PreparedDelivery, Option<NvmeIo>, Option<GpuCopy>) {
        let (last_pc, exits) = (self.last_pc, self.exits);
        let guest_ram = &mut *probe.guest_ram;
        let traces = &mut self.traces;
        let (outcome, pending, device, pcie_target, pcie_context, nvme_io, gpu_copy) = {
            let mut platform_guard =
                lock_platform(probe.platform, self.smp_trace.as_deref(), 0, HoldSite::Mmio(ipa));
            let platform = &mut *platform_guard;
            let device = machine::device_at(ipa).unwrap_or("<unmapped>");
            let pcie_target = match device {
                "pcie-mmio-32" => platform.pcie_mmio_target(ipa).map(PcieTraceTarget::Mmio),
                "pcie-pio" => platform.pcie_pio_target(ipa).map(PcieTraceTarget::Pio),
                _ => None,
            };
            let xhci_target = match pcie_target {
                Some(PcieTraceTarget::Mmio(target)) => Some(target),
                Some(PcieTraceTarget::Pio(_)) | None => None,
            };
            traces.recent_xhci.record_mmio(xhci_target, &op, guest_ram);
            platform.set_host_now(std::time::Instant::now());
            let (outcome, post_drain) = platform.on_mmio_with_post_drain(ipa, op, guest_ram);
            refresh_on_ecam_write(platform, ipa, matches!(op, MmioOp::Write { .. }));
            traces.recent_pcie_ecam.record_after_with_context(
                platform,
                guest_ram,
                PcieEcamAccess {
                    pc: last_pc,
                    ipa,
                    exit: exits,
                    esr,
                    ec,
                    srt,
                    op: &op,
                    outcome: &outcome,
                    #[cfg(test)]
                    owner_context: None,
                },
            );
            let pcie_context = targetless_xhci_trace_context(
                platform,
                guest_ram,
                device,
                ipa,
                pcie_target,
                &outcome,
            );
            let pending = self.drain_stats.prepare_pending_delivery_after_mmio(
                platform,
                guest_ram,
                self.drain_trace,
                DrainContext::data_abort(exits, last_pc),
                post_drain,
            );
            // SAFETY: the transfers run only on vCPU0's nvme_io_worker
            // (finish_mmio_work) or, if it cannot run them, in guest_ram.
            // Both views map ram_backing, which outlives the worker
            // (run_vm declares it after the backing) and is unmapped only
            // after every secondary vCPU thread, and so its worker, is
            // joined (shutdown_and_join).
            let nvme_io = platform.take_nvme_io();
            let gpu_copy = platform.take_virtio_gpu_copy();
            (
                outcome,
                pending,
                device,
                pcie_target,
                pcie_context,
                nvme_io,
                gpu_copy,
            )
        };
        traces.recent_pcie_mmio.record_with_context(PcieMmioEventInput {
            device,
            pc: last_pc,
            ipa,
            target: pcie_target,
            op: &op,
            outcome: &outcome,
            context: pcie_context,
        });
        traces
            .recent_pcie_pio
            .record(device, last_pc, ipa, pcie_target, &op, &outcome);
        if device == "uart" && matches!(op, MmioOp::Write { .. }) {
            self.automation.gate.mark_serial_output_dirty();
        }
        record_mmio_trace(&mut traces.mmio_traces, device, last_pc, ipa, op, &outcome);
        (outcome, pending, nvme_io, gpu_copy)
    }

    /// After the MMIO hold: its interrupt delivery, then the NVMe transfers
    /// and virtio-gpu copies it queued, each run unlocked and posted under a
    /// second hold.
    unsafe fn finish_mmio_work(
        &mut self,
        probe: &mut Probe<'_>,
        ipa: u64,
        pending: PreparedDelivery,
        nvme_io: Option<NvmeIo>,
        gpu_copy: Option<GpuCopy>,
    ) -> Result<(), String> {
        let platform = probe.platform;
        self.deliver(probe.fatal_vcpu_run_error, pending)?;
        // NVMe reads, writes and TRIM run on vCPU0's worker,
        // unlocked. Finished within NVME_IO_WAIT, this
        // exit's second hold posts them; otherwise the
        // drain FLUSH_DONE_HOOK asks for does.
        let nvme_io_worker = &mut *probe.nvme_io_worker;
        if nvme_io.is_some_and(|io| {
            nvme_io_worker.run(io, &*probe.guest_ram, bridgevm_hvf::nvme::NVME_IO_WAIT)
        }) {
            let pending = {
                let mut platform_guard =
                    lock_platform(platform, self.smp_trace.as_deref(), 0, HoldSite::Mmio(ipa));
                self.drain_stats.prepare_pending_delivery(
                    &mut platform_guard,
                    probe.guest_ram,
                    self.drain_trace,
                    DrainContext::data_abort(self.exits, self.last_pc),
                )
            };
            self.deliver(probe.fatal_vcpu_run_error, pending)?;
        }
        // A virtio-gpu 2D copy runs here, unlocked; the
        // second hold posts it (gpu_unlocked_copy.rs).
        let smp_trace = self.smp_trace.as_deref();
        if let Err(error) = self.drain_stats.run_gpu_copies(
            gpu_copy,
            || lock_platform(platform, smp_trace, 0, HoldSite::Mmio(ipa)),
            probe.guest_ram,
            self.drain_trace,
            DrainContext::data_abort(self.exits, self.last_pc),
        ) {
            return Err(error.stop_primary(probe.fatal_vcpu_run_error, &mut self.last_pc));
        }
        Ok(())
    }

    /// SMCCC through HVC: PSCI (DTB method = "hvc") + ARM TRNG (RngDxe uses it).
    unsafe fn on_hvc(&mut self, vcpu: HvVcpuT) -> Result<(), String> {
        let mut func = 0u64;
        hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut func);
        match func & 0xffff_ffff {
            SMCCC_VERSION => {
                hv_vcpu_set_reg(vcpu, HV_REG_X0, 0x1_0001);
            } // SMCCC_VERSION 1.1
            PSCI_VERSION => {
                hv_vcpu_set_reg(vcpu, HV_REG_X0, 0x0001_0001);
            } // PSCI_VERSION 1.1
            PSCI_FEATURES => {
                let mut queried = 0u64;
                hv_vcpu_get_reg(vcpu, HV_REG_X0 + 1, &mut queried);
                hv_vcpu_set_reg(vcpu, HV_REG_X0, psci_features(queried));
            } // PSCI_FEATURES
            PSCI_CPU_ON_32 | PSCI_CPU_ON_64 => {
                let mut target = 0u64;
                let mut entry = 0u64;
                let mut context = 0u64;
                hv_vcpu_get_reg(vcpu, HV_REG_X0 + 1, &mut target);
                hv_vcpu_get_reg(vcpu, HV_REG_X0 + 2, &mut entry);
                hv_vcpu_get_reg(vcpu, HV_REG_X0 + 3, &mut context);
                let result = match self.secondary_vcpus.as_ref() {
                    Some(secondary_vcpus) => psci_cpu_on(
                        &secondary_vcpus.controls,
                        target,
                        entry,
                        context,
                        self.smp_trace.as_deref(),
                    ),
                    None => PSCI_NOT_SUPPORTED,
                };
                hv_vcpu_set_reg(vcpu, HV_REG_X0, result);
            }
            PSCI_CPU_OFF => {
                hv_vcpu_set_reg(vcpu, HV_REG_X0, PSCI_NOT_SUPPORTED);
            }
            PSCI_AFFINITY_INFO_32 | PSCI_AFFINITY_INFO_64 => {
                let mut target = 0u64;
                let mut level = 0u64;
                hv_vcpu_get_reg(vcpu, HV_REG_X0 + 1, &mut target);
                // X2 carries lowest_affinity_level; ignoring
                // it answered cluster queries as if they
                // named a single CPU.
                hv_vcpu_get_reg(vcpu, HV_REG_X0 + 2, &mut level);
                let controls = self
                    .secondary_vcpus
                    .as_ref()
                    .map(|vcpus| vcpus.controls.as_slice())
                    .unwrap_or(&[]);
                let result = psci_affinity_info(controls, target, level);
                hv_vcpu_set_reg(vcpu, HV_REG_X0, result);
            }
            id if crate::trng_dispatch::is_trng_function(id) => {
                crate::trng_dispatch::handle_trng_hvc(vcpu, id);
            }
            value if psci_terminal_action(value) == Some(PsciTerminalAction::SystemOff) => {
                self.psci_calls += 1;
                return Err(format!("PSCI {PSCI_SYSTEM_OFF:#x} (system off)"));
            }
            value if psci_terminal_action(value) == Some(PsciTerminalAction::SystemReset) => {
                self.psci_calls += 1;
                self.requested_system_reset = true;
                return Err(format!("PSCI {PSCI_SYSTEM_RESET:#x} (system reset)"));
            }
            _ => {
                hv_vcpu_set_reg(vcpu, HV_REG_X0, PSCI_NOT_SUPPORTED);
            } // NOT_SUPPORTED
        }
        // HVF reports the HVC exit PC already PAST the `hvc` instruction
        // (unlike a data abort, where the PC is AT the faulting insn). So
        // do NOT advance again: +4 would skip the next instruction — e.g.
        // ArmCallHvc's `ldr x9, [sp], #0x10`, which was the RngDxe crash.
        hv_vcpu_set_reg(vcpu, HV_REG_PC, self.last_pc);
        self.psci_calls += 1;
        Ok(())
    }

    /// The automation pass when one is due, then the app's Reset and Pause.
    unsafe fn after_exit(
        &mut self,
        probe: &mut Probe<'_>,
        automation_tick_canceled: bool,
        input_wake: bool,
    ) -> Result<(), String> {
        let tick = Instant::now();
        let mut host_reset = false;
        let mut host_pause = None;
        let automation_stop_reason = if self.automation.gate.should_check(
            automation_tick_canceled
                || self.automation.ramfb_sample_loop.checkpoint_due_at(tick)
                || self.automation.live_display_exporter.due(tick)
                || probe.services.live_input.poll_due(tick),
        ) {
            let reason = self.automation_tick(probe, input_wake);
            host_reset = probe.services.live_input.take_host_reset();
            host_pause = probe.services.live_input.take_host_pause();
            reason
        } else {
            None
        };
        if let Some(reason) = automation_stop_reason {
            return Err(reason);
        }
        if host_reset {
            return Err(self.host_reset());
        }
        if crate::pause_gate::request(host_pause, false)
            == crate::pause_gate::Request::AlreadyRunning
        {
            crate::pause_gate::answer_running();
        }
        // Paused at the top of the next pass, before vCPU0 runs again.
        self.pause_requested |=
            crate::pause_gate::request(host_pause, false) == crate::pause_gate::Request::Pause;
        Ok(())
    }
}
