//! Secondary vCPU creation, PSCI handling, and run-loop execution.

use crate::*;

pub(crate) fn secondary_vcpu_thread(
    control: Arc<VcpuControl>,
    context: SecondaryVcpuThreadContext,
) {
    let SecondaryVcpuThreadContext {
        shutdown,
        terminal,
        primary_vcpu,
        ram_base,
        ram_size,
        platform,
        drain_trace,
        pre_run_drain_gate,
        controls,
        smp_trace,
        max_exits,
    } = context;
    let mut vcpu: HvVcpuT = 0;
    let mut exit: *mut HvVcpuExit = null_mut();
    let mut _vcpu_guard: Option<HvVcpuGuard> = None;
    let mut guest_ram = MappedRam {
        base: machine::RAM_BASE,
        ptr: ram_base as *mut u8,
        len: ram_size,
    };
    let mut drain_stats = RunLoopDrainStats::new(false);
    let mut exits = 0u64;

    macro_rules! lock_secondary_state {
        () => {
            lock_vcpu_state(
                &control,
                smp_trace.as_deref(),
                control.index,
                "secondary vCPU state mutex",
            )
        };
    }
    let mut state = lock_secondary_state!();
    loop {
        while *state == PsciState::Off && !shutdown.load(Ordering::SeqCst) {
            if let Some(trace) = smp_trace.as_deref() {
                trace.secondary_waiting_off(control.index);
            }
            state = control
                .condvar
                .wait(state)
                .expect("secondary vCPU state condvar");
            if let Some(trace) = smp_trace.as_deref() {
                trace.secondary_woke(control.index, *state);
            }
        }
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        match *state {
            PsciState::Off => {}
            PsciState::OnPending => {
                let entry = control.entry.load(Ordering::SeqCst);
                let context = control.context.load(Ordering::SeqCst);
                drop(state);
                if vcpu == 0 {
                    let (created_vcpu, created_exit, guard) = create_secondary_hvf_vcpu(&control);
                    vcpu = created_vcpu;
                    exit = created_exit;
                    _vcpu_guard = Some(guard);
                    control.publish_vcpu(vcpu);
                    if let Some(trace) = smp_trace.as_deref() {
                        trace.secondary_vcpu_created(control.index, vcpu, exit);
                    }
                }
                apply_secondary_cpu_on_reset(vcpu, control.mpidr, entry, context);
                if let Some(trace) = smp_trace.as_deref() {
                    trace.state_transition(control.index, PsciState::OnPending, PsciState::On);
                }
                state = lock_secondary_state!();
                *state = PsciState::On;
                drop(state);
                let stop = run_secondary_until_parked(SecondaryRunLoopContext {
                    vcpu,
                    exit,
                    guest_ram: &mut guest_ram,
                    platform: &platform,
                    drain_stats: &mut drain_stats,
                    drain_trace,
                    control: &control,
                    controls: &controls,
                    shutdown: &shutdown,
                    terminal: &terminal,
                    primary_vcpu,
                    exits: &mut exits,
                    pre_run_drain_gate: &pre_run_drain_gate,
                    smp_trace: smp_trace.as_deref(),
                    max_exits,
                });
                state = lock_secondary_state!();
                if stop {
                    break;
                }
            }
            PsciState::On => {
                drop(state);
                let stop = run_secondary_until_parked(SecondaryRunLoopContext {
                    vcpu,
                    exit,
                    guest_ram: &mut guest_ram,
                    platform: &platform,
                    drain_stats: &mut drain_stats,
                    drain_trace,
                    control: &control,
                    controls: &controls,
                    shutdown: &shutdown,
                    terminal: &terminal,
                    primary_vcpu,
                    exits: &mut exits,
                    pre_run_drain_gate: &pre_run_drain_gate,
                    smp_trace: smp_trace.as_deref(),
                    max_exits,
                });
                state = lock_secondary_state!();
                if stop {
                    break;
                }
            }
        }
    }
    drop(state);
    if vcpu != 0 {
        // Withdraw under the same mutex used by shutdown before the guard can
        // destroy the HVF object. This closes both the stale-value and
        // load-versus-destroy races on early secondary-thread exits.
        control.withdraw_vcpu(vcpu);
    }
}

pub(crate) fn create_secondary_hvf_vcpu(
    control: &VcpuControl,
) -> (HvVcpuT, *mut HvVcpuExit, HvVcpuGuard) {
    let mut vcpu: HvVcpuT = 0;
    let mut exit: *mut HvVcpuExit = null_mut();
    unsafe {
        assert_eq!(
            hv_vcpu_create(&mut vcpu, &mut exit, null_mut()),
            0,
            "hv_vcpu_create secondary vCPU{}",
            control.index
        );
    }
    let guard = HvVcpuGuard { vcpu };
    unsafe {
        assert_eq!(
            hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_MPIDR_EL1, control.mpidr),
            0,
            "set secondary MPIDR_EL1"
        );
    }
    crate::usgic_bridge::register_vcpu(control.index as usize, vcpu);
    (vcpu, exit, guard)
}

pub(crate) fn apply_secondary_cpu_on_reset(vcpu: HvVcpuT, mpidr: u64, entry: u64, context: u64) {
    unsafe {
        for reg in HV_REG_X0..=HV_REG_LR {
            hv_vcpu_set_reg(vcpu, reg, 0);
        }
        for reg in [
            HV_SYS_REG_SCTLR_EL1,
            HV_SYS_REG_TTBR0_EL1,
            HV_SYS_REG_TTBR1_EL1,
            HV_SYS_REG_TCR_EL1,
            HV_SYS_REG_SPSR_EL1,
            HV_SYS_REG_ELR_EL1,
            HV_SYS_REG_ESR_EL1,
            HV_SYS_REG_FAR_EL1,
            HV_SYS_REG_MAIR_EL1,
            HV_SYS_REG_VBAR_EL1,
            HV_SYS_REG_SP_EL0,
            HV_SYS_REG_SP_EL1,
            HV_SYS_REG_CNTP_CTL_EL0,
            HV_SYS_REG_CNTP_CVAL_EL0,
            HV_SYS_REG_CNTV_CTL_EL0,
            HV_SYS_REG_CNTV_CVAL_EL0,
        ] {
            hv_vcpu_set_sys_reg(vcpu, reg, 0);
        }
        hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_MPIDR_EL1, mpidr);
        let mut dfr0 = 0u64;
        if hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_ID_AA64DFR0_EL1, &mut dfr0) == 0 {
            let dfr0 = (dfr0 & !(0xf << 8)) | (0x1 << 8);
            hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_ID_AA64DFR0_EL1, dfr0);
        }
        hv_vcpu_set_reg(vcpu, HV_REG_PC, entry);
        hv_vcpu_set_reg(vcpu, HV_REG_X0, context);
        hv_vcpu_set_reg(vcpu, HV_REG_CPSR, crate::boot_cpsr());
        hv_vcpu_set_vtimer_mask(vcpu, false);
    }
}

pub(crate) fn run_hvf_vcpu_once(vcpu: HvVcpuT, exit: *mut HvVcpuExit) -> Result<u32, HvReturn> {
    let r = unsafe { hv_vcpu_run(vcpu) };
    if r != 0 {
        return Err(r);
    }
    Ok(unsafe { (*exit).reason })
}

pub(crate) struct SecondaryRunLoopContext<'a> {
    pub(crate) vcpu: HvVcpuT,
    pub(crate) exit: *mut HvVcpuExit,
    pub(crate) guest_ram: &'a mut MappedRam,
    pub(crate) platform: &'a Arc<Mutex<VirtPlatform>>,
    pub(crate) drain_stats: &'a mut RunLoopDrainStats,
    pub(crate) drain_trace: DrainTrace,
    pub(crate) control: &'a Arc<VcpuControl>,
    pub(crate) controls: &'a [Arc<VcpuControl>],
    pub(crate) shutdown: &'a AtomicBool,
    pub(crate) terminal: &'a SecondaryTerminalSignal,
    pub(crate) primary_vcpu: HvVcpuT,
    pub(crate) exits: &'a mut u64,
    pub(crate) pre_run_drain_gate: &'a PreRunDrainGate,
    pub(crate) smp_trace: Option<&'a SmpTrace>,
    pub(crate) max_exits: u64,
}

pub(crate) fn run_secondary_until_parked(context: SecondaryRunLoopContext<'_>) -> bool {
    let SecondaryRunLoopContext {
        vcpu,
        exit,
        guest_ram,
        platform,
        drain_stats,
        drain_trace,
        control,
        controls,
        shutdown,
        terminal,
        primary_vcpu,
        exits,
        pre_run_drain_gate,
        smp_trace,
        max_exits,
    } = context;
    if let Some(trace) = smp_trace {
        trace.secondary_run_loop_entered(control.index, *exits);
    }
    loop {
        if shutdown.load(Ordering::SeqCst) {
            return true;
        }
        let mut drain_pc = 0u64;
        unsafe {
            hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut drain_pc);
        }
        if let Some(trace) = smp_trace {
            if *exits == 0 {
                let mut cpsr = 0u64;
                let mut x0 = 0u64;
                let mut mpidr = 0u64;
                unsafe {
                    hv_vcpu_get_reg(vcpu, HV_REG_CPSR, &mut cpsr);
                    hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut x0);
                    hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_MPIDR_EL1, &mut mpidr);
                }
                trace.secondary_before_first_run(control.index, drain_pc, cpsr, x0, mpidr);
            }
        }
        if pre_run_drain_gate.should_drain_secondary_pre_run() {
            if let Some(trace) = smp_trace {
                trace.secondary_pre_run_drain(control.index, *exits, drain_pc);
            }
            let pending = {
                let mut platform_guard = lock_platform(
                    platform,
                    smp_trace,
                    control.index,
                    "secondary pre-run platform mutex",
                );
                drain_stats.prepare_pending_delivery(
                    &mut platform_guard,
                    guest_ram,
                    drain_trace,
                    DrainContext {
                        location: DrainLocation::PreRun,
                        exit: *exits,
                        pc: drain_pc,
                    },
                )
            };
            drain_stats.complete_pending_delivery(pending, drain_trace);
            if let Some(trace) = smp_trace {
                trace.secondary_post_run_drain(control.index, *exits);
            }
        } else {
            drain_stats.record_pre_run_skip();
        }
        let run_index = *exits + 1;
        crate::usgic_bridge::pre_run(control.index as usize, vcpu);
        let run_status = unsafe { hv_vcpu_run(vcpu) };
        if let Some(trace) = smp_trace {
            trace.secondary_run_result(control.index, run_index, run_status, exit);
        }
        let reason = if run_status == 0 {
            unsafe { (*exit).reason }
        } else {
            {
                let r = run_status;
                println!("secondary vCPU{} hv_vcpu_run error {r:#x}", control.index);
                control.run_error.store(true, Ordering::SeqCst);
                return true;
            }
        };
        *exits += 1;
        control.exits.store(*exits, Ordering::Relaxed);
        if let Some(trace) = smp_trace {
            trace.secondary_progress();
        }
        if reason == EXIT_CANCELED {
            if shutdown.load(Ordering::SeqCst) {
                return true;
            }
            crate::probe_runtime::vtimer_recovery::recover_swallowed_vtimer_fire(vcpu);
            continue;
        }
        if reason == EXIT_VTIMER {
            crate::usgic_bridge::vtimer_exit_mask(control.index as usize, vcpu);
            continue;
        }
        if reason != EXIT_EXCEPTION {
            println!(
                "secondary vCPU{} stopped on exit reason {reason}",
                control.index
            );
            return true;
        }
        let esr = unsafe { (*exit).exception.syndrome };
        let ec = (esr >> 26) & 0x3f;
        let mut pc = 0u64;
        unsafe {
            hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc);
        }
        match ec {
            EC_DATA_ABORT => {
                let ipa = unsafe { (*exit).exception.physical_address };
                let size = 1u8 << ((esr >> 22) & 0x3);
                let srt = ((esr >> 16) & 0x1f) as u32;
                let is_write = (esr >> 6) & 1 == 1;
                trace_isv0_data_abort(esr, pc, ipa);
                // srt=31 is WZR/XZR: stores write zero, loads discard. It must
                // never index the HV register file, where slot 31 is the PC —
                // Linux emits `str wzr` for zero MMIO writes (e.g. virtio
                // device_feature_select=0) and the store leaked the guest PC
                // into the device register.
                let op = if is_write {
                    let mut v = 0u64;
                    if srt != 31 {
                        unsafe { hv_vcpu_get_reg(vcpu, HV_REG_X0 + srt, &mut v) };
                    }
                    MmioOp::Write { size, value: v }
                } else {
                    MmioOp::Read { size }
                };
                if crate::usgic_bridge::try_data_abort(
                    control.index as usize,
                    vcpu,
                    ipa,
                    &op,
                    srt,
                    pc,
                ) {
                    continue;
                }
                let (outcome, pending) = {
                    let mut platform_guard = lock_platform(
                        platform,
                        smp_trace,
                        control.index,
                        "secondary data-abort platform mutex",
                    );
                    platform_guard.set_host_now(std::time::Instant::now());
                    let (outcome, post_drain) =
                        platform_guard.on_mmio_with_post_drain(ipa, op, guest_ram);
                    gpu_shm_bar2::refresh_on_ecam_write(&platform_guard, ipa, is_write);
                    let pending = drain_stats.prepare_pending_delivery_after_mmio(
                        &mut platform_guard,
                        guest_ram,
                        drain_trace,
                        DrainContext {
                            location: DrainLocation::DataAbort,
                            exit: *exits,
                            pc,
                        },
                        post_drain,
                    );
                    (outcome, pending)
                };
                drain_stats.complete_pending_delivery(pending, drain_trace);
                pre_run_drain_gate.mark_secondary_pending();
                match outcome {
                    MmioOutcome::ReadValue(v) if !is_write => {
                        if srt != 31 {
                            unsafe {
                                hv_vcpu_set_reg(vcpu, HV_REG_X0 + srt, v);
                            }
                        }
                    }
                    MmioOutcome::ReadValue(_) | MmioOutcome::WriteAck => {}
                    MmioOutcome::KnownUnimplemented(_) | MmioOutcome::Unmapped => {
                        if !is_write && srt != 31 {
                            unsafe {
                                hv_vcpu_set_reg(vcpu, HV_REG_X0 + srt, 0);
                            }
                        }
                    }
                }
                unsafe {
                    hv_vcpu_set_reg(vcpu, HV_REG_PC, pc + 4);
                }
            }
            EC_HVC => {
                let mut func = 0u64;
                unsafe {
                    hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut func);
                }
                match func {
                    SMCCC_VERSION => unsafe {
                        hv_vcpu_set_reg(vcpu, HV_REG_X0, 0x1_0001);
                    },
                    PSCI_VERSION => unsafe {
                        hv_vcpu_set_reg(vcpu, HV_REG_X0, 0x0001_0001);
                    },
                    PSCI_FEATURES => {
                        let mut queried = 0u64;
                        unsafe {
                            hv_vcpu_get_reg(vcpu, HV_REG_X0 + 1, &mut queried);
                            hv_vcpu_set_reg(vcpu, HV_REG_X0, psci_features(queried));
                        }
                    }
                    PSCI_CPU_OFF => {
                        let mut state = lock_vcpu_state(
                            control,
                            smp_trace,
                            control.index,
                            "secondary PSCI state mutex",
                        );
                        if let Some(trace) = smp_trace {
                            trace.state_transition(control.index, *state, PsciState::Off);
                        }
                        *state = PsciState::Off;
                        return false;
                    }
                    PSCI_CPU_ON_32 | PSCI_CPU_ON_64 => {
                        let mut target = 0u64;
                        let mut entry = 0u64;
                        let mut context = 0u64;
                        unsafe {
                            hv_vcpu_get_reg(vcpu, HV_REG_X0 + 1, &mut target);
                            hv_vcpu_get_reg(vcpu, HV_REG_X0 + 2, &mut entry);
                            hv_vcpu_get_reg(vcpu, HV_REG_X0 + 3, &mut context);
                            hv_vcpu_set_reg(
                                vcpu,
                                HV_REG_X0,
                                psci_cpu_on(controls, target, entry, context, smp_trace),
                            );
                        }
                    }
                    PSCI_AFFINITY_INFO_32 | PSCI_AFFINITY_INFO_64 => {
                        let mut target = 0u64;
                        let mut level = 0u64;
                        unsafe {
                            hv_vcpu_get_reg(vcpu, HV_REG_X0 + 1, &mut target);
                            hv_vcpu_get_reg(vcpu, HV_REG_X0 + 2, &mut level);
                            let result = psci_affinity_info(controls, target, level);
                            hv_vcpu_set_reg(vcpu, HV_REG_X0, result);
                        }
                    }
                    value if psci_terminal_action(value).is_some() => {
                        if terminal.record(value) {
                            println!(
                                "secondary vCPU{} forwarded PSCI {:#x} to the primary run loop",
                                control.index,
                                value & 0xffff_ffff
                            );
                        }
                        // SAFETY: `primary_vcpu` remains owned by the probe
                        // until all secondary threads are joined. Waking it is
                        // required so CPU0 can observe the terminal request.
                        unsafe {
                            hv_vcpus_exit(&primary_vcpu, 1);
                        }
                        return true;
                    }
                    // SAFETY: this thread owns `vcpu` for the whole run loop.
                    id if crate::trng_dispatch::is_trng_function(id) => unsafe {
                        crate::trng_dispatch::handle_trng_hvc(vcpu, id);
                    },
                    _ => unsafe {
                        hv_vcpu_set_reg(vcpu, HV_REG_X0, PSCI_NOT_SUPPORTED);
                    },
                }
                unsafe {
                    hv_vcpu_set_reg(vcpu, HV_REG_PC, pc);
                }
            }
            EC_SYS_REG_TRAP => {
                let trap = SysRegTrap::decode(esr);
                if crate::usgic_bridge::try_sysreg(control.index as usize, vcpu, esr, pc) {
                    // Trapped ICC_* consumed by the userspace GIC.
                } else if unsafe { emulate_debug_os_lock_sysreg(vcpu, trap) } {
                    unsafe {
                        hv_vcpu_set_reg(vcpu, HV_REG_PC, pc + 4);
                    }
                } else {
                    println!(
                        "secondary vCPU{} unsupported system register trap {} ESR {esr:#x} @ PC {pc:#x}",
                        control.index,
                        trap.describe()
                    );
                    return true;
                }
            }
            EC_WFX if crate::usgic_bridge::usgic().is_some() => {
                crate::usgic_bridge::wfx_trap(control.index as usize, vcpu, esr, pc);
            }
            _ => {
                println!(
                    "secondary vCPU{} exception EC {ec:#x} ESR {esr:#x} @ PC {pc:#x}",
                    control.index
                );
                return true;
            }
        }
        if *exits >= max_exits {
            println!("secondary vCPU{} exit cap {max_exits}", control.index);
            return true;
        }
    }
}

#[cfg(test)]
mod vcpu_control_tests {
    use super::*;

    #[test]
    fn vcpu_control_starts_off_with_linear_mpidr() {
        let control = VcpuControl::new(17);

        assert_eq!(*control.state.lock().unwrap(), PsciState::Off);
        assert_eq!(control.entry.load(Ordering::SeqCst), 0);
        assert_eq!(control.context.load(Ordering::SeqCst), 0);
        assert_eq!(*control.vcpu.lock().unwrap(), None);
        assert_eq!(control.index, 17);
        assert_eq!(control.mpidr, 0x8000_0000 | machine::cpu_mpidr(17));
    }

    #[test]
    fn psci_state_has_parked_secondary_transitions_reserved() {
        let control = VcpuControl::new(1);
        {
            let mut state = control.state.lock().unwrap();
            *state = PsciState::OnPending;
            assert_eq!(*state, PsciState::OnPending);
            *state = PsciState::On;
            assert_eq!(*state, PsciState::On);
            *state = PsciState::Off;
        }
        assert_eq!(*control.state.lock().unwrap(), PsciState::Off);
    }

    #[test]
    fn secondary_vcpu_handle_cannot_withdraw_during_shutdown_action() {
        let control = Arc::new(VcpuControl::new(1));
        let fake_handle = 0x1234;
        let (action_entered_tx, action_entered_rx) = std::sync::mpsc::channel();
        let (release_action_tx, release_action_rx) = std::sync::mpsc::channel();
        let (withdraw_started_tx, withdraw_started_rx) = std::sync::mpsc::channel();
        let (withdraw_done_tx, withdraw_done_rx) = std::sync::mpsc::channel();

        control.publish_vcpu(fake_handle);
        let action_control = Arc::clone(&control);
        let action_thread = thread::spawn(move || {
            let _ = action_control.with_published_vcpu(|vcpu| {
                assert_eq!(vcpu, fake_handle);
                action_entered_tx.send(()).unwrap();
                release_action_rx.recv().unwrap();
            });
        });
        action_entered_rx.recv().unwrap();

        let withdraw_control = Arc::clone(&control);
        let withdraw_thread = thread::spawn(move || {
            withdraw_started_tx.send(()).unwrap();
            withdraw_control.withdraw_vcpu(fake_handle);
            withdraw_done_tx.send(()).unwrap();
        });
        withdraw_started_rx.recv().unwrap();
        let withdrawal_while_locked = withdraw_done_rx.recv_timeout(Duration::from_millis(25));

        release_action_tx.send(()).unwrap();
        action_thread.join().unwrap();
        if withdrawal_while_locked.is_err() {
            withdraw_done_rx.recv().unwrap();
        }
        withdraw_thread.join().unwrap();
        assert!(
            matches!(
                withdrawal_while_locked,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            ),
            "withdrawal completed while a shutdown action held the handle"
        );
        assert_eq!(*control.vcpu.lock().unwrap(), None);
    }

    #[test]
    fn secondary_terminal_signal_preserves_the_first_system_request() {
        let signal = SecondaryTerminalSignal::new();

        assert_eq!(signal.action(), None);
        assert!(signal.record(PSCI_SYSTEM_OFF));
        assert!(!signal.record(PSCI_SYSTEM_RESET));
        assert_eq!(signal.action(), Some(PsciTerminalAction::SystemOff));
    }

    #[test]
    fn secondary_terminal_signal_accepts_a_system_reset_request() {
        let signal = SecondaryTerminalSignal::new();

        assert!(signal.record(PSCI_SYSTEM_RESET));
        assert_eq!(signal.action(), Some(PsciTerminalAction::SystemReset));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SysRegTrap {
    pub(crate) op0: u8,
    pub(crate) op1: u8,
    pub(crate) crn: u8,
    pub(crate) crm: u8,
    pub(crate) op2: u8,
    pub(crate) rt: u32,
    pub(crate) is_read: bool,
}
