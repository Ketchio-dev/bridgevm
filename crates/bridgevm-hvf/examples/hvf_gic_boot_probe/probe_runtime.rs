use super::*;
use crate::boot_media_setup::attach_boot_media;
use crate::final_report::persist_and_report_stop;
use crate::gpu_shm_setup::install_gpu_shm_port;
use crate::hvf_setup::{create_gic, create_vm};
use crate::probe_config::ProbeConfig;
use crate::probe_setup::prepare_platform;
use crate::watchpoint_setup::watchpoint_config;

pub(crate) fn run() -> ExitCode {
    let mut fatal_vcpu_run_error = false;
    let mut fatal_reset_error = false;
    let config = ProbeConfig::from_env();
    let ProbeConfig {
        media,
        smp_cpus,
        swtpm_data_socket,
        swtpm_control_socket,
        platform_cfg,
        ram_size,
        watchdog_ms,
        watchdog_enabled,
        max_exits,
        trace_fwcfg,
        trace_msix,
        trace_spi,
        trace_run_loop,
        trace_xhci_bringup,
        smp_trace_enabled,
        stop_on_linux,
    } = config;

    unsafe {
        create_vm();
        let _vm_guard = HvVmGuard;
        create_gic();

        let (mut platform, vars_data, boot_dtb) = prepare_platform(
            &media,
            platform_cfg,
            swtpm_data_socket.as_deref(),
            swtpm_control_socket.as_deref(),
        );

        let ram_layout = Layout::from_size_align(ram_size, 0x1_0000).unwrap();
        let ram = alloc_zeroed(ram_layout);
        assert!(boot_dtb.len() < ram_size, "DTB must fit in guest RAM");
        assert_eq!(
            hv_vm_map(
                ram as *mut c_void,
                machine::RAM_BASE,
                ram_size,
                HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC
            ),
            0,
            "map ram"
        );

        let mut vcpu: HvVcpuT = 0;
        let mut exit: *mut HvVcpuExit = null_mut();
        assert_eq!(
            hv_vcpu_create(&mut vcpu, &mut exit, null_mut()),
            0,
            "hv_vcpu_create"
        );
        let _vcpu_guard = HvVcpuGuard { vcpu };

        let (watch_addr, watch_target) = watchpoint_config();
        attach_boot_media(&mut platform, &media, platform_cfg, ram_size, &vars_data);
        let mut guest_ram = MappedRam {
            base: machine::RAM_BASE,
            ptr: ram,
            len: ram_size,
        };
        reset_guest_ram_for_boot(&mut guest_ram, &boot_dtb);
        let reboot_plan = RebootPlan::from_env();
        println!("PSCI SYSTEM_RESET max reboots: {}", reboot_plan.max_reboots);
        let watchdog_generation = Arc::new(AtomicU64::new(0));
        let mut reboot_count = 0u64;
        let mut resets_dumped = 0u64;
        reset_vcpu_for_boot(vcpu);
        arm_watchpoint_for_boot(vcpu, watch_addr);
        checkpoint_glue::restore_if_requested(
            &[vcpu],
            std::slice::from_raw_parts_mut(ram, ram_size),
            &mut platform,
        )
        .unwrap_or_else(|error| panic!("restore VM checkpoint: {error}"));
        let hv_gpu_shm_state = install_gpu_shm_port(&mut platform);
        let platform = Arc::new(Mutex::new(platform));
        // Probe-lifetime instance, deliberately OUTSIDE the reboot loop: its
        // ticker thread keeps firing across guest reboots, and the SAME fired
        // flag must be the one each boot generation's exit dispatcher consumes.
        // (A per-boot instance turned the first post-reset EXIT_CANCELED from
        // the previous generation's ticker into a bogus "watchdog (CANCELED)"
        // stop — live-observed as the reboot loop dying at reboot 1/8.)
        let mut agent_service_wake = agent_console::ServiceWake::new();
        // Same probe-lifetime rule as ServiceWake above; the wake-state Arc is
        // shared with the virtio-gpu device and survives platform resets.
        let mut gpu_vblank_wake = vblank_wake::VblankWake::new();
        let gpu_vblank_wake_state = platform
            .lock()
            .expect("platform mutex for vblank wake state")
            .virtio_gpu_vblank_wake();
        // KD (kernel-debug) serial bridge, probe-lifetime like the wakers: it
        // owns the PL011 for the run when BRIDGEVM_KD_SERIAL_SOCKET is set, so
        // a WinDbg peer can attach to the guest's KDCOM stream. None otherwise,
        // leaving the serial to the boot scanner unchanged.
        let mut kd_serial_bridge = kd_serial_bridge::KdSerialBridge::from_env();
        // Keep the file offset and pending queue across guest SYSTEM_RESET.
        // Recreating the controller per boot generation replays every command
        // already consumed from an append-only control file, which can repeat
        // destructive guest actions such as a TPM-clear reboot request.
        let mut live_input = LiveInputController::from_env();

        'reboot: loop {
            // Secondary vCPUs are intentionally scoped to one boot generation in
            // Stage 1. SYSTEM_RESET joins them before resetting CPU0/platform
            // state; the next loop iteration respawns a fresh parked set.
            let drain_trace = DrainTrace {
                msix: trace_msix,
                spi: trace_spi,
            };
            let smp_trace = (smp_cpus > 1 && smp_trace_enabled).then(|| Arc::new(SmpTrace::new()));
            let pre_run_drain_gate = Arc::new(PreRunDrainGate::from_env());
            let secondary_vcpus = (smp_cpus > 1).then(|| {
                SecondaryVcpuSet::spawn(SecondaryVcpuSpawnConfig {
                    cpu_count: smp_cpus,
                    primary_vcpu: vcpu,
                    ram_base: ram as usize,
                    ram_size,
                    platform: Arc::clone(&platform),
                    drain_trace,
                    pre_run_drain_gate: Arc::clone(&pre_run_drain_gate),
                    smp_trace: smp_trace.clone(),
                    max_exits,
                })
            });
            let boot_generation = begin_watchdog_generation(&watchdog_generation);
            let watchdog_fired = Arc::new(AtomicBool::new(false));
            if watchdog_enabled {
                spawn_boot_watchdog(
                    vcpu,
                    watchdog_ms,
                    Arc::clone(&watchdog_generation),
                    boot_generation,
                    Arc::clone(&watchdog_fired),
                );
            }

            {
                let mut platform_guard = lock_platform(
                    &platform,
                    smp_trace.as_deref(),
                    0,
                    "cpu0 UART preload platform mutex",
                );
                let platform = &mut *platform_guard;
                if let Ok(input) = std::env::var("BRIDGEVM_UART_RX") {
                    platform.push_uart_input(input.as_bytes());
                    println!("UART RX preloaded: {} bytes", input.len());
                }
            }
            let mut uart_triggers = Vec::new();
            if let Some(trigger) = SerialTriggeredUartInput::from_env(
                "cd-prompt",
                "BRIDGEVM_UART_RX_ON_CD_PROMPT",
                b"Press any key to boot from CD or DVD",
            ) {
                uart_triggers.push(trigger);
            }
            if let Some(trigger) = SerialTriggeredUartInput::from_env_with_marker_env(
                "serial-marker",
                "BRIDGEVM_UART_RX_ON_SERIAL_MARKER",
                "BRIDGEVM_UART_RX_SERIAL_MARKER",
            ) {
                uart_triggers.push(trigger);
            }
            let mut xhci_hid_boot_key_triggers = Vec::new();
            if let Some(trigger) =
                XhciHidBootKeyTrigger::from_env("cd-prompt", "BRIDGEVM_XHCI_BOOT_KEY_ON_CD_PROMPT")
            {
                xhci_hid_boot_key_triggers.push(trigger);
            }
            if let Some(trigger) = XhciHidBootKeyTrigger::from_env_with_marker_env(
                "serial-marker",
                "BRIDGEVM_XHCI_BOOT_KEY_ON_SERIAL_MARKER",
                "BRIDGEVM_XHCI_BOOT_KEY_SERIAL_MARKER",
            ) {
                xhci_hid_boot_key_triggers.push(trigger);
            }
            let mut xhci_setup_input_triggers = Vec::new();
            if let Some(trigger_result) = XhciSetupInputTrigger::from_env(
                "setup-input",
                "BRIDGEVM_XHCI_SETUP_INPUT_ACTIONS",
                "BRIDGEVM_XHCI_SETUP_INPUT_SERIAL_MARKER",
            ) {
                match trigger_result {
                    Ok(trigger) => xhci_setup_input_triggers.push(trigger),
                    Err(error) => print_setup_input_rejection("setup-input", &error),
                }
            }
            if let Some(trigger_result) = XhciSetupInputTrigger::from_env_with_timing_envs(
                "setup-input-2",
                "BRIDGEVM_XHCI_SETUP_INPUT2_ACTIONS",
                "BRIDGEVM_XHCI_SETUP_INPUT2_SERIAL_MARKER",
                "BRIDGEVM_XHCI_SETUP_INPUT2_FIRE_DELAY_MS",
                "BRIDGEVM_XHCI_SETUP_INPUT2_RAMFB_DELAY_MS",
            ) {
                match trigger_result {
                    Ok(trigger) => xhci_setup_input_triggers.push(trigger),
                    Err(error) => print_setup_input_rejection("setup-input-2", &error),
                }
            }
            if let Some(trigger_result) = XhciSetupInputTrigger::from_env_with_timing_envs(
                "setup-input-3",
                "BRIDGEVM_XHCI_SETUP_INPUT3_ACTIONS",
                "BRIDGEVM_XHCI_SETUP_INPUT3_SERIAL_MARKER",
                "BRIDGEVM_XHCI_SETUP_INPUT3_FIRE_DELAY_MS",
                "BRIDGEVM_XHCI_SETUP_INPUT3_RAMFB_DELAY_MS",
            ) {
                match trigger_result {
                    Ok(trigger) => xhci_setup_input_triggers.push(trigger),
                    Err(error) => print_setup_input_rejection("setup-input-3", &error),
                }
            }
            let mut xhci_pointer_input_triggers = Vec::new();
            if let Some(trigger_result) = XhciPointerInputTrigger::from_env(
                "pointer-input",
                "BRIDGEVM_XHCI_POINTER_INPUT_ACTIONS",
                "BRIDGEVM_XHCI_POINTER_INPUT_SERIAL_MARKER",
            ) {
                match trigger_result {
                    Ok(trigger) => xhci_pointer_input_triggers.push(trigger),
                    Err(error) => print_pointer_input_rejection("pointer-input", &error),
                }
            }
            let mut mmio_traces: BTreeMap<&'static str, MmioTrace> = BTreeMap::new();
            let mut recent_pcie_mmio = RecentMmio::new(
                "pcie-mmio-32",
                usize::try_from(env_u64("BRIDGEVM_RECENT_PCIE_MMIO", 32)).unwrap_or(32),
            );
            let mut recent_pcie_pio = RecentMmio::new(
                "pcie-pio",
                usize::try_from(env_u64("BRIDGEVM_RECENT_PCIE_PIO", 32)).unwrap_or(32),
            );
            let mut recent_pcie_ecam = RecentPcieEcam::new(
                usize::try_from(env_u64("BRIDGEVM_RECENT_PCIE_ECAM", 128)).unwrap_or(128),
            );
            let mut recent_xhci = XhciBringupTrace::new(
                usize::try_from(env_u64("BRIDGEVM_RECENT_XHCI", 160)).unwrap_or(160),
            );
            recent_xhci.print_events_immediately(trace_xhci_bringup);
            let mut unimpl: BTreeMap<&'static str, u64> = BTreeMap::new();
            let mut redist_lo = u64::MAX;
            let mut redist_hi = 0u64;
            let mut exits = 0u64;
            let mut vtimer_exits = 0u64;
            let mut surplus_canceled_exits = 0u64;
            let mut psci_calls = 0u64;
            let mut last_pc = 0u64;
            let mut last_pre_run_pc: u64;
            let mut watch_hits = 0u32;
            let mut last_watch_pc = 0u64;
            let mut last_watch_lr = 0u64;
            let mut fwcfg_trace_count = 0u32;
            let mut drain_stats = RunLoopDrainStats::new(trace_run_loop);
            let mut ramfb_sample_loop = RamfbSampleLoop::from_env();
            let mut live_display_exporter = LiveDisplayExporter::from_env();
            let mut setup_input_host_wake = SetupInputHostWake::new();
            let boot_started = Instant::now();
            let mut boot_timer = BootTimer::from_env();
            let mut agent_console = AgentConsoleHarness::from_env(boot_started);
            let automation_always_check = !uart_triggers.is_empty()
                || !xhci_hid_boot_key_triggers.is_empty()
                || !xhci_setup_input_triggers.is_empty()
                || !xhci_pointer_input_triggers.is_empty()
                || agent_console
                    .as_ref()
                    .is_some_and(AgentConsoleHarness::per_exit_tick_needed);
            let mut automation_gate = AutomationGate::new(automation_always_check);
            // Resident service mode and measurement-safe scripted commands are
            // host-driven: without a steady waker the main loop sleeps in
            // hv_vcpu_run while the desktop idles and their tick starves (see
            // ServiceWake docs). BOOT_TIMER uses a wake no slower than 250 ms
            // and honors a shorter requested RAMFB interval. These sources
            // deliberately do not force the automation block's platform mutex
            // on every CPU0 exit. ensure_started is idempotent, so re-entering
            // here after a guest reboot is fine.
            let service_wake_interval = boot_timer
                .service_wake_interval()
                .or_else(|| {
                    agent_console
                        .as_ref()
                        .is_some_and(|harness| harness.service_wake_needed())
                        .then_some(Duration::from_millis(250))
                })
                // A guest halted at a KD breakpoint generates no vCPU exits, so
                // the pre-run drain (and the KD serial pump with it) would stall
                // until the debugger's next byte happened to arrive. A steady
                // wake keeps the debugger<->guest byte pipe flowing while halted.
                .or_else(|| {
                    kd_serial_bridge
                        .is_some()
                        .then_some(Duration::from_millis(2))
                });
            if let Some(interval) = service_wake_interval {
                agent_service_wake.ensure_started(vcpu, interval);
            }
            if let Some(state) = gpu_vblank_wake_state.as_ref() {
                gpu_vblank_wake.ensure_started(vcpu, Arc::clone(state));
            }
            let mut serial_stop_scans = SerialStopScans::default();
            let mut stop_reason;
            let mut stop_reason_code = None;
            let mut requested_system_reset = false;

            loop {
                let mut drain_pc = 0u64;
                hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut drain_pc);
                last_pre_run_pc = drain_pc;
                let pending = {
                    let mut platform_guard = lock_platform(
                        &platform,
                        smp_trace.as_deref(),
                        0,
                        "cpu0 pre-run platform mutex",
                    );
                    let platform = &mut *platform_guard;
                    if let Some(bridge) = kd_serial_bridge.as_mut() {
                        bridge.pump(platform);
                    }
                    drain_stats.prepare_pending_delivery(
                        platform,
                        &mut guest_ram,
                        drain_trace,
                        DrainContext {
                            location: DrainLocation::PreRun,
                            exit: exits,
                            pc: drain_pc,
                        },
                    )
                };
                drain_stats.complete_pending_delivery(pending, drain_trace);
                let reason = match run_hvf_vcpu_once(vcpu, exit) {
                    Ok(reason) => reason,
                    Err(r) => {
                        hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut last_pc);
                        fatal_vcpu_run_error = true;
                        stop_reason = format!("hv_vcpu_run error {r:#x}");
                        break;
                    }
                };
                exits += 1;
                if let Some(trace) = smp_trace.as_deref() {
                    trace.cpu0_progress(exits);
                }
                stop_reason_code = Some(reason);
                if let Some(action) = secondary_vcpus
                    .as_ref()
                    .and_then(SecondaryVcpuSet::terminal_action)
                {
                    psci_calls += 1;
                    match action {
                        PsciTerminalAction::SystemOff => {
                            stop_reason = format!("PSCI {PSCI_SYSTEM_OFF:#x} (system off)");
                        }
                        PsciTerminalAction::SystemReset => {
                            requested_system_reset = true;
                            stop_reason = format!("PSCI {PSCI_SYSTEM_RESET:#x} (system reset)");
                        }
                    }
                    break;
                }
                let sample_tick_canceled =
                    ramfb_sample_loop.canceled_by_sample_tick(reason, &watchdog_fired);
                let setup_input_wake_canceled =
                    setup_input_host_wake.canceled_by_host_wake(reason, &watchdog_fired);
                let service_wake_canceled =
                    agent_service_wake.canceled_by_service_wake(reason, &watchdog_fired);
                let vblank_wake_canceled =
                    gpu_vblank_wake.canceled_by_vblank_wake(reason, &watchdog_fired);
                let automation_tick_canceled = sample_tick_canceled
                    || setup_input_wake_canceled
                    || service_wake_canceled
                    || vblank_wake_canceled;
                if reason == EXIT_CANCELED && !automation_tick_canceled {
                    // Two automation wakes can merge into ONE canceled exit
                    // (both hv_vcpus_exit calls land while the vCPU is still
                    // in guest mode); that single exit consumes BOTH fired
                    // flags above, and the second, sticky cancel then arrives
                    // with no flag left to claim it. Attributing such surplus
                    // cancels to the watchdog killed live boots (b2 86s,
                    // b5 258s). Only the watchdog's own flag identifies a real
                    // watchdog stop; an unclaimed cancel without it is benign.
                    if watchdog_fired.load(Ordering::SeqCst) {
                        hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut last_pc);
                        stop_reason = "watchdog (CANCELED)".into();
                        break;
                    }
                    surplus_canceled_exits += 1;
                    continue;
                }
                if !automation_tick_canceled && reason == EXIT_VTIMER {
                    hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut last_pc);
                    vtimer_exits += 1;
                    hv_vcpu_set_vtimer_mask(vcpu, true);
                    if exits >= max_exits {
                        stop_reason = format!("exit cap {max_exits}");
                        break;
                    }
                    continue;
                }
                if !automation_tick_canceled && reason != EXIT_EXCEPTION {
                    hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut last_pc);
                    stop_reason = format!("exit reason {reason}");
                    break;
                }
                if !automation_tick_canceled {
                    let esr = (*exit).exception.syndrome;
                    let ec = (esr >> 26) & 0x3f;
                    hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut last_pc);
                    match ec {
                        EC_DATA_ABORT => {
                            let ipa = (*exit).exception.physical_address;
                            let size = 1u8 << ((esr >> 22) & 0x3);
                            let srt = ((esr >> 16) & 0x1f) as u32;
                            let is_write = (esr >> 6) & 1 == 1;
                            trace_isv0_data_abort(esr, last_pc, ipa);
                            // srt=31 is WZR/XZR (stores write zero, loads
                            // discard) — never index the HV register file,
                            // where slot 31 is the PC. Linux emits `str wzr`
                            // for zero MMIO writes; this leaked the guest PC
                            // into device registers (virtio feature_select).
                            let op = if is_write {
                                let mut v = 0u64;
                                if srt != 31 {
                                    hv_vcpu_get_reg(vcpu, HV_REG_X0 + srt, &mut v);
                                }
                                MmioOp::Write { size, value: v }
                            } else {
                                MmioOp::Read { size }
                            };
                            let trace_this_fwcfg = trace_fwcfg
                                && machine::FW_CFG.contains(ipa)
                                && fwcfg_trace_count < 512;
                            if trace_this_fwcfg {
                                fwcfg_trace_count += 1;
                                println!(
                                "FWCFG[{fwcfg_trace_count:03}] pc={last_pc:#x} off={:#x} op={op:?}",
                                ipa - machine::FW_CFG.base
                            );
                            }
                            let (outcome, pending, device, pcie_target, pcie_context) = {
                                let mut platform_guard = lock_platform(
                                    &platform,
                                    smp_trace.as_deref(),
                                    0,
                                    "cpu0 data-abort platform mutex",
                                );
                                let platform = &mut *platform_guard;
                                let device = machine::device_at(ipa).unwrap_or("<unmapped>");
                                let pcie_target = match device {
                                    "pcie-mmio-32" => {
                                        platform.pcie_mmio_target(ipa).map(PcieTraceTarget::Mmio)
                                    }
                                    "pcie-pio" => {
                                        platform.pcie_pio_target(ipa).map(PcieTraceTarget::Pio)
                                    }
                                    _ => None,
                                };
                                let xhci_target = match pcie_target {
                                    Some(PcieTraceTarget::Mmio(target)) => Some(target),
                                    Some(PcieTraceTarget::Pio(_)) | None => None,
                                };
                                recent_xhci.record_mmio(xhci_target, &op, &guest_ram);
                                platform.set_host_now(std::time::Instant::now());
                                let (outcome, post_drain) =
                                    platform.on_mmio_with_post_drain(ipa, op, &mut guest_ram);
                                if device == "pcie-ecam" && matches!(op, MmioOp::Write { .. }) {
                                    let base = platform.virtio_gpu_host_visible_bar_base();
                                    let mut state = hv_gpu_shm_state.lock().unwrap();
                                    state.ecam_writes = state.ecam_writes.saturating_add(1);
                                    if state.bar2_base != base {
                                        state.base_changes = state.base_changes.saturating_add(1);
                                        eprintln!(
                                            "virtio-gpu hv shm BAR2 update: ipa={ipa:#x} old={:?} new={base:?} ecam_writes={} base_changes={}",
                                            state.bar2_base,
                                            state.ecam_writes,
                                            state.base_changes
                                        );
                                    }
                                    state.bar2_base = base;
                                }
                                recent_pcie_ecam.record_after_with_context(
                                    platform,
                                    &mut guest_ram,
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
                                    &mut guest_ram,
                                    device,
                                    ipa,
                                    pcie_target,
                                    &outcome,
                                );
                                let pending = drain_stats.prepare_pending_delivery_after_mmio(
                                    platform,
                                    &mut guest_ram,
                                    drain_trace,
                                    DrainContext {
                                        location: DrainLocation::DataAbort,
                                        exit: exits,
                                        pc: last_pc,
                                    },
                                    post_drain,
                                );
                                (outcome, pending, device, pcie_target, pcie_context)
                            };
                            recent_pcie_mmio.record_with_context(PcieMmioEventInput {
                                device,
                                pc: last_pc,
                                ipa,
                                target: pcie_target,
                                op: &op,
                                outcome: &outcome,
                                context: pcie_context,
                            });
                            recent_pcie_pio.record(
                                device,
                                last_pc,
                                ipa,
                                pcie_target,
                                &op,
                                &outcome,
                            );
                            if device == "uart" && matches!(op, MmioOp::Write { .. }) {
                                automation_gate.mark_serial_output_dirty();
                            }
                            record_mmio_trace(&mut mmio_traces, device, last_pc, ipa, op, &outcome);
                            drain_stats.complete_pending_delivery(pending, drain_trace);
                            if trace_this_fwcfg {
                                println!("FWCFG[{fwcfg_trace_count:03}] -> {outcome:?}");
                            }
                            match outcome {
                                MmioOutcome::ReadValue(v) if !is_write => {
                                    if srt != 31 {
                                        hv_vcpu_set_reg(vcpu, HV_REG_X0 + srt, v);
                                    }
                                }
                                MmioOutcome::ReadValue(_) | MmioOutcome::WriteAck => {}
                                MmioOutcome::KnownUnimplemented(name) => {
                                    *unimpl.entry(name).or_insert(0) += 1;
                                    if name == "gic-redist" {
                                        redist_lo = redist_lo.min(ipa);
                                        redist_hi = redist_hi.max(ipa);
                                    }
                                    if !is_write && srt != 31 {
                                        hv_vcpu_set_reg(vcpu, HV_REG_X0 + srt, 0);
                                    }
                                }
                                MmioOutcome::Unmapped => {
                                    *unimpl.entry("<unmapped>").or_insert(0) += 1;
                                    if !is_write && srt != 31 {
                                        hv_vcpu_set_reg(vcpu, HV_REG_X0 + srt, 0);
                                    }
                                }
                            }
                            hv_vcpu_set_reg(vcpu, HV_REG_PC, last_pc + 4);
                        }
                        EC_HVC => {
                            // SMCCC: PSCI (DTB method = "hvc") + ARM TRNG (RngDxe uses it).
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
                                    let result = match secondary_vcpus.as_ref() {
                                        Some(secondary_vcpus) => psci_cpu_on(
                                            &secondary_vcpus.controls,
                                            target,
                                            entry,
                                            context,
                                            smp_trace.as_deref(),
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
                                    hv_vcpu_get_reg(vcpu, HV_REG_X0 + 1, &mut target);
                                    let result = match secondary_vcpus.as_ref() {
                                        Some(secondary_vcpus) => {
                                            psci_affinity_info(&secondary_vcpus.controls, target)
                                        }
                                        None => 1,
                                    };
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, result);
                                }
                                TRNG_VERSION => {
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, 0x1_0000);
                                } // TRNG_VERSION 1.0
                                TRNG_FEATURES => {
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, 0);
                                } // TRNG_FEATURES: present
                                TRNG_GET_UUID => {
                                    // TRNG_GET_UUID
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, 0x0b0a_0908);
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0 + 1, 0x0f0e_0d0c);
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0 + 2, 0x0302_0100);
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0 + 3, 0x0706_0504);
                                }
                                TRNG_RND_32 | TRNG_RND_64 => {
                                    // TRNG_RND_32 / _64
                                    let r = exits
                                        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                                        .wrapping_add(0xD1B5_4A32);
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, PSCI_SUCCESS); // SUCCESS
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0 + 1, r);
                                    hv_vcpu_set_reg(
                                        vcpu,
                                        HV_REG_X0 + 2,
                                        r.rotate_left(17) ^ 0xA5A5_5A5A,
                                    );
                                    hv_vcpu_set_reg(
                                        vcpu,
                                        HV_REG_X0 + 3,
                                        r.rotate_left(41) ^ 0x1234_5678,
                                    );
                                }
                                value
                                    if psci_terminal_action(value)
                                        == Some(PsciTerminalAction::SystemOff) =>
                                {
                                    psci_calls += 1;
                                    stop_reason = format!("PSCI {PSCI_SYSTEM_OFF:#x} (system off)");
                                    break;
                                }
                                value
                                    if psci_terminal_action(value)
                                        == Some(PsciTerminalAction::SystemReset) =>
                                {
                                    psci_calls += 1;
                                    requested_system_reset = true;
                                    stop_reason =
                                        format!("PSCI {PSCI_SYSTEM_RESET:#x} (system reset)");
                                    break;
                                }
                                _ => {
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, PSCI_NOT_SUPPORTED);
                                } // NOT_SUPPORTED
                            }
                            // HVF reports the HVC exit PC already PAST the `hvc` instruction
                            // (unlike a data abort, where the PC is AT the faulting insn). So
                            // do NOT advance again: +4 would skip the next instruction — e.g.
                            // ArmCallHvc's `ldr x9, [sp], #0x10`, which was the RngDxe crash.
                            hv_vcpu_set_reg(vcpu, HV_REG_PC, last_pc);
                            psci_calls += 1;
                        }
                        EC_SYS_REG_TRAP => {
                            let trap = SysRegTrap::decode(esr);
                            if emulate_debug_os_lock_sysreg(vcpu, trap) {
                                hv_vcpu_set_reg(vcpu, HV_REG_PC, last_pc + 4);
                            } else {
                                stop_reason = format!(
                            "unsupported system register trap {} ESR {esr:#x} @ PC {last_pc:#x}",
                            trap.describe()
                        );
                                break;
                            }
                        }
                        EC_WATCHPOINT_LOWER | EC_WATCHPOINT_SAME => {
                            watch_hits += 1;
                            let mut lr = 0u64;
                            hv_vcpu_get_reg(vcpu, HV_REG_X0 + 30, &mut lr);
                            last_watch_pc = last_pc;
                            last_watch_lr = lr;
                            print!("WATCH #{watch_hits}: store @ PC {last_pc:#x} LR {lr:#x}");
                            // Single-step over the store: disable the watchpoint and arm
                            // PSTATE.SS so the store retires and we can read the new value.
                            hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_DBGWCR0_EL1, 0);
                            let mut md = 0u64;
                            hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_MDSCR_EL1, &mut md);
                            hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_MDSCR_EL1, md | 1); // SS
                            let mut cp = 0u64;
                            hv_vcpu_get_reg(vcpu, HV_REG_CPSR, &mut cp);
                            hv_vcpu_set_reg(vcpu, HV_REG_CPSR, cp | (1 << 21)); // PSTATE.SS
                                                                                // do NOT advance PC: re-execute the store under single-step.
                        }
                        EC_SOFTSTEP_LOWER | EC_SOFTSTEP_SAME => {
                            let mut bytes = [0u8; 8];
                            let cur = if guest_ram.read_into(watch_target, &mut bytes) {
                                u64::from_le_bytes(bytes)
                            } else {
                                0
                            };
                            println!(" -> {watch_target:#x} = {cur:#x}");
                            // Clear single-step; re-arm the watchpoint unless we have enough.
                            let mut md = 0u64;
                            hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_MDSCR_EL1, &mut md);
                            hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_MDSCR_EL1, md & !1);
                            let mut cp = 0u64;
                            hv_vcpu_get_reg(vcpu, HV_REG_CPSR, &mut cp);
                            hv_vcpu_set_reg(vcpu, HV_REG_CPSR, cp & !(1 << 21));
                            if watch_hits < 40 {
                                hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_DBGWCR0_EL1, DBGWCR_STORE_8B);
                            }
                            // do NOT advance PC: the step already retired the instruction.
                        }
                        _ => {
                            stop_reason =
                                format!("exception EC {ec:#x} ESR {esr:#x} @ PC {last_pc:#x}");
                            break;
                        }
                    }
                }
                if exits >= max_exits {
                    stop_reason = format!("exit cap {max_exits}");
                    break;
                }
                let ramfb_checkpoint_due =
                    ramfb_sample_loop.checkpoint_due_at(std::time::Instant::now());
                let live_display_due = live_display_exporter.due(std::time::Instant::now());
                let live_input_due = live_input.poll_due(std::time::Instant::now());
                let automation_stop_reason = if automation_gate.should_check(
                    automation_tick_canceled
                        || ramfb_checkpoint_due
                        || live_display_due
                        || live_input_due,
                ) {
                    let mut platform_guard = lock_platform(
                        &platform,
                        smp_trace.as_deref(),
                        0,
                        "cpu0 automation platform mutex",
                    );
                    let platform = &mut *platform_guard;
                    automation_gate.note_checked();
                    live_input.tick(platform, &mut guest_ram, std::time::Instant::now());
                    if serial_reached_linux_panic(platform.uart_output(), &mut serial_stop_scans) {
                        Some("serial reached Linux kernel panic".into())
                    } else {
                        for trigger in &mut uart_triggers {
                            trigger.maybe_fire(platform);
                        }
                        for trigger in &mut xhci_hid_boot_key_triggers {
                            trigger.maybe_fire(platform);
                        }
                        let now = std::time::Instant::now();
                        let mut checkpoint_committed = false;
                        if let Some(agent_console) = agent_console.as_mut() {
                            agent_console.tick(platform, &mut guest_ram, now);
                            if agent_console.desktop_ready() {
                                boot_timer.observe_agent_ready(now, exits);
                                checkpoint_committed = checkpoint_glue::checkpoint_if_requested(
                                    &[vcpu],
                                    std::slice::from_raw_parts(ram, ram_size),
                                    platform,
                                )
                                .unwrap_or_else(|error| panic!("capture VM checkpoint: {error}"));
                            }
                        }
                        for trigger in &mut xhci_setup_input_triggers {
                            let now = std::time::Instant::now();
                            trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
                                platform,
                                &mut guest_ram,
                                now,
                                |platform, label, mem| {
                                    ramfb_dump::print_checkpoint_for_platform(label, platform, mem);
                                },
                            );
                            if let Some(deadline) =
                                trigger.pending_host_wake_deadline_at(platform, now)
                            {
                                let v = vcpu;
                                let wake_generation = Arc::clone(&watchdog_generation);
                                let wake_boot_generation = boot_generation;
                                if setup_input_host_wake.arm(deadline, move || {
                                    if watchdog_generation_matches(
                                        &wake_generation,
                                        wake_boot_generation,
                                    ) {
                                        hv_vcpus_exit(&v, 1);
                                    }
                                }) {
                                    println!(
                                        "xHCI setup-input host wake armed for delayed trigger"
                                    );
                                }
                            }
                        }
                        for trigger in &mut xhci_pointer_input_triggers {
                            let now = std::time::Instant::now();
                            trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
                                platform,
                                &mut guest_ram,
                                now,
                                |platform, label, mem| {
                                    ramfb_dump::print_checkpoint_for_platform(label, platform, mem);
                                },
                            );
                            if let Some(deadline) =
                                trigger.pending_host_wake_deadline_at(platform, now)
                            {
                                let v = vcpu;
                                let wake_generation = Arc::clone(&watchdog_generation);
                                let wake_boot_generation = boot_generation;
                                if setup_input_host_wake.arm(deadline, move || {
                                    if watchdog_generation_matches(
                                        &wake_generation,
                                        wake_boot_generation,
                                    ) {
                                        hv_vcpus_exit(&v, 1);
                                    }
                                }) {
                                    println!(
                                        "xHCI pointer-input host wake armed for delayed trigger"
                                    );
                                }
                            }
                        }
                        ramfb_sample_loop.emit_due(vcpu, |label| {
                            ramfb_dump::print_checkpoint_for_platform(label, platform, &guest_ram);
                        });
                        live_display_exporter.export_due(platform, std::time::Instant::now());
                        boot_timer.tick(platform, &guest_ram, exits, last_pc);
                        if checkpoint_committed {
                            Some("VM checkpoint committed; suspended process exiting".into())
                        } else if stop_on_linux
                            && serial_reached_linux_early_boot(
                                platform.uart_output(),
                                &mut serial_stop_scans,
                            )
                        {
                            Some("serial reached Linux early boot".into())
                        } else if serial_reached_shell(
                            platform.uart_output(),
                            &mut serial_stop_scans,
                        ) {
                            match ramfb_sample_loop.observe_shell(vcpu) {
                                RamfbSampleShellAction::Continue => None,
                                RamfbSampleShellAction::StopNow { reason } => Some(reason.into()),
                            }
                        } else {
                            None
                        }
                    }
                } else {
                    None
                };
                if let Some(reason) = automation_stop_reason {
                    stop_reason = reason;
                    break;
                }
            }

            // Freeze the measured duration at the VM stop boundary. Media
            // persistence and diagnostic dumps below are evidence work, not
            // guest boot time.
            let boot_timer_elapsed = boot_timer.elapsed();
            invalidate_watchdog_generation(&watchdog_generation);
            let (secondary_exit_counts, secondary_vcpu_run_error) = secondary_vcpus
                .map(SecondaryVcpuSet::shutdown_and_join)
                .unwrap_or_default();
            fatal_vcpu_run_error |= secondary_vcpu_run_error;
            if requested_system_reset {
                // Crash-survivable snapshot: capture regs + full RAM BEFORE the
                // reboot arm below wipes guest RAM / resets the vCPU. Gated on
                // BRIDGEVM_DUMP_ON_RESET; defaults to only the first reset so a
                // gen1 bugcheck (venus StartDevice) is caught, not later gens.
                if let Some(dir) = dump_on_reset_dir() {
                    if resets_dumped < dump_on_reset_max() {
                        println!(
                            "DUMP_ON_RESET: capturing reset #{resets_dumped} (reboot_count={reboot_count})"
                        );
                        dump_guest_state_on_reset(&dir, resets_dumped, vcpu, ram, ram_size);
                        resets_dumped += 1;
                    }
                }
                match decide_system_reset(reboot_count, reboot_plan) {
                    SystemResetDecision::Reboot {
                        next_reboot_count,
                        actions,
                    } => {
                        reboot_count = next_reboot_count;
                        println!(
                            "PSCI SYSTEM_RESET: reboot {reboot_count}/{}",
                            reboot_plan.max_reboots
                        );
                        let gic_reset_status = if actions.reset_gic {
                            // All secondary vCPUs have stopped and joined above, and CPU0
                            // is outside hv_vcpu_run. Apple documents hv_gic_reset as the
                            // VM-reset operation for the distributor, redistributors, and
                            // the GIC device's internal state.
                            let status = hv_gic_reset();
                            println!("hv_gic_reset = {status:#x}");
                            status
                        } else {
                            0
                        };
                        if gic_reset_status != 0 {
                            fatal_reset_error = true;
                            stop_reason = format!(
                                "hv_gic_reset failed during PSCI SYSTEM_RESET: {gic_reset_status:#x}"
                            );
                        } else {
                            if actions.reset_platform {
                                let mut platform_guard = platform.lock().expect("platform mutex");
                                let platform = &mut *platform_guard;
                                platform.reset();
                            }
                            if actions.reset_guest_ram {
                                reset_guest_ram_for_boot(&mut guest_ram, &boot_dtb);
                            }
                            if actions.reset_vcpu {
                                reset_vcpu_for_boot(vcpu);
                                arm_watchpoint_for_boot(vcpu, watch_addr);
                            }
                            if actions.continue_run_loop {
                                continue 'reboot;
                            }
                        }
                    }
                    SystemResetDecision::Stop { reason } => {
                        stop_reason = reason;
                    }
                }
            }

            persist_and_report_stop!(
                platform,
                media,
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
                boot_timer,
                boot_timer_elapsed,
                secondary_exit_counts,
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
            );
            break 'reboot;
        }
    }

    probe_exit_code(fatal_vcpu_run_error, fatal_reset_error)
}
