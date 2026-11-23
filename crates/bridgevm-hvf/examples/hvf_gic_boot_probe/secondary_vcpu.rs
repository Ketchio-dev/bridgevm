//! Secondary vCPU creation, PSCI handling, and run-loop execution.

use crate::*;
use bridgevm_hvf::nvme::{NvmeIo, NvmeIoWorker};
use bridgevm_hvf::virtio_gpu::GpuCopy;

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
        generation,
    } = context;
    let mut vcpu: HvVcpuT = 0;
    let mut exit: *mut HvVcpuExit = null_mut();
    let mut _vcpu_guard: Option<HvVcpuGuard> = None;
    let mut guest_ram = MappedRam {
        base: machine::RAM_BASE,
        ptr: ram_base as *mut u8,
        len: ram_size,
    };
    // This vCPU's NVMe transfer thread; joined when this thread ends, which
    // the primary waits for before the RAM backing goes.
    let mut nvme_io_worker = vcpu_nvme_io_worker(control.index, ram_base, ram_size);
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
            PsciState::Off => continue,
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
            }
            PsciState::On => drop(state),
        }
        let stop = run_secondary_until_parked(SecondaryRunLoopContext {
            vcpu,
            exit,
            guest_ram: &mut guest_ram,
            nvme_io_worker: &mut nvme_io_worker,
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
    let final_psci_state = *state;
    drop(state);
    if vcpu != 0 {
        // This thread owns `vcpu`. Capture after the terminal stop and before
        // withdrawal/destruction; the primary reads only the published value.
        let final_state = VcpuFinalState::capture_on_owner_thread(
            control.index,
            generation,
            final_psci_state,
            control.mpidr,
            exits,
            vcpu,
        );
        if !control.publish_final_state(final_state) {
            println!(
                "secondary vCPU{} final-state publication rejected: already published",
                control.index
            );
        }
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

pub(crate) struct SecondaryRunLoopContext<'a> {
    pub(crate) vcpu: HvVcpuT,
    pub(crate) exit: *mut HvVcpuExit,
    pub(crate) guest_ram: &'a mut MappedRam,
    pub(crate) nvme_io_worker: &'a mut NvmeIoWorker,
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

/// Runs this secondary until it parks (PSCI CPU_OFF: `false`) or must stop
/// (shutdown, a terminal PSCI call, an error: `true`).
pub(crate) fn run_secondary_until_parked(mut context: SecondaryRunLoopContext<'_>) -> bool {
    if let Some(trace) = context.smp_trace {
        trace.secondary_run_loop_entered(context.control.index, *context.exits);
    }
    // A pause waits for every vCPU in this loop to park (pause_gate.rs).
    let _in_run_loop = crate::pause_gate::GATE.enter_run_loop();
    let mut offset_version = 0u64;
    loop {
        if let Err(stop) = context.pass(&mut offset_version) {
            return stop;
        }
    }
}

/// Where a secondary's run loop goes after an exit it handled; a step's `Err`
/// leaves the loop with `run_secondary_until_parked`'s result.
enum Next {
    /// Straight back into hv_vcpu_run.
    Run,
    /// Through the exit cap first.
    CheckExitCap,
}

impl SecondaryRunLoopContext<'_> {
    /// Pre-run work, one hv_vcpu_run and its exit, then the exit cap.
    #[inline]
    fn pass(&mut self, offset_version: &mut u64) -> Result<(), bool> {
        self.before_run(offset_version)?;
        let reason = self.run_once()?;
        if let Next::Run = self.on_exit(reason)? {
            return Ok(());
        }
        if *self.exits >= self.max_exits {
            return Err(self.control.stop_unexpected_secondary(
                self.primary_vcpu,
                SecondaryUnexpectedStop::ExitCap(self.max_exits),
            ));
        }
        Ok(())
    }

    /// Shutdown and the pause gate, then the pre-run drain when one is due.
    fn before_run(&mut self, offset_version: &mut u64) -> Result<(), bool> {
        let (vcpu, index) = (self.vcpu, self.control.index);
        if self.shutdown.load(Ordering::SeqCst) {
            return Err(true);
        }
        // SAFETY: this thread owns `vcpu` and is outside hv_vcpu_run.
        crate::pause_gate::GATE.before_run(offset_version, |offset| unsafe {
            crate::pause_gate::set_vtimer_offset(vcpu, offset)
        });
        if self.shutdown.load(Ordering::SeqCst) {
            return Err(true);
        }
        let mut drain_pc = 0u64;
        unsafe {
            hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut drain_pc);
        }
        if let Some(trace) = self.smp_trace {
            if *self.exits == 0 {
                let mut cpsr = 0u64;
                let mut x0 = 0u64;
                let mut mpidr = 0u64;
                unsafe {
                    hv_vcpu_get_reg(vcpu, HV_REG_CPSR, &mut cpsr);
                    hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut x0);
                    hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_MPIDR_EL1, &mut mpidr);
                }
                trace.secondary_before_first_run(index, drain_pc, cpsr, x0, mpidr);
            }
        }
        if self.pre_run_drain_gate.should_drain_secondary_pre_run() {
            if let Some(trace) = self.smp_trace {
                trace.secondary_pre_run_drain(index, *self.exits, drain_pc);
            }
            let pending = self.drain_stats.prepare_secondary_pre_run(
                self.platform,
                self.guest_ram,
                self.smp_trace,
                index,
                self.drain_trace,
                DrainContext::pre_run(*self.exits, drain_pc),
            );
            self.deliver(pending)?;
            if let Some(trace) = self.smp_trace {
                trace.secondary_post_run_drain(index, *self.exits);
            }
        } else {
            self.drain_stats.record_pre_run_skip();
        }
        Ok(())
    }

    /// One hv_vcpu_run, counted; a failed run ends the thread.
    #[inline]
    fn run_once(&mut self) -> Result<u32, bool> {
        let (vcpu, exit, index) = (self.vcpu, self.exit, self.control.index);
        let run_index = *self.exits + 1;
        let run_status = unsafe { hv_vcpu_run(vcpu) };
        if let Some(trace) = self.smp_trace {
            trace.secondary_run_result(index, run_index, run_status, exit);
        }
        let reason = if run_status == 0 {
            unsafe { (*exit).reason }
        } else {
            let r = run_status;
            println!("secondary vCPU{} hv_vcpu_run error {r:#x}", index);
            self.control.record_run_error(self.primary_vcpu);
            return Err(true);
        };
        *self.exits += 1;
        self.control.exits.store(*self.exits, Ordering::Relaxed);
        if let Some(trace) = self.smp_trace {
            trace.secondary_progress();
        }
        Ok(reason)
    }

    /// One exit by class.
    #[inline]
    fn on_exit(&mut self, reason: u32) -> Result<Next, bool> {
        let vcpu = self.vcpu;
        match reason {
            EXIT_CANCELED if self.shutdown.load(Ordering::SeqCst) => return Err(true),
            EXIT_CANCELED => return Ok(Next::Run),
            EXIT_VTIMER => {
                unsafe {
                    hv_vcpu_set_vtimer_mask(vcpu, true);
                }
                return Ok(Next::Run);
            }
            EXIT_EXCEPTION => {}
            _ => {
                return Err(self.control.stop_unexpected_secondary(
                    self.primary_vcpu,
                    SecondaryUnexpectedStop::ExitReason(reason),
                ));
            }
        }
        let esr = unsafe { (*self.exit).exception.syndrome };
        let ec = (esr >> 26) & 0x3f;
        let mut pc = 0u64;
        unsafe {
            hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc);
        }
        match ec {
            EC_DATA_ABORT => self.on_data_abort(esr, pc),
            EC_HVC => self.on_hvc(pc),
            EC_SYS_REG_TRAP => {
                let trap = SysRegTrap::decode(esr);
                if unsafe { emulate_debug_os_lock_sysreg(vcpu, trap) } {
                    unsafe {
                        hv_vcpu_set_reg(vcpu, HV_REG_PC, pc + 4);
                    }
                    Ok(Next::CheckExitCap)
                } else {
                    Err(self.control.stop_unexpected_secondary(
                        self.primary_vcpu,
                        SecondaryUnexpectedStop::SysReg(trap, esr, pc),
                    ))
                }
            }
            _ => Err(self.control.stop_unexpected_secondary(
                self.primary_vcpu,
                SecondaryUnexpectedStop::Exception(ec, esr, pc),
            )),
        }
    }

    /// Completes a prepared interrupt delivery; a failed one ends the thread.
    #[inline]
    fn deliver(
        &mut self,
        pending: Result<PendingDrainDelivery, SpiDeliveryError>,
    ) -> Result<(), bool> {
        let delivered = self
            .drain_stats
            .finish_prepared_interrupt_delivery(pending, self.drain_trace);
        delivered.map_err(|error| {
            error.stop_secondary(|| self.control.record_run_error(self.primary_vcpu));
            true
        })
    }

    /// A data abort: a framebuffer page's first write, or MMIO to a device.
    fn on_data_abort(&mut self, esr: u64, pc: u64) -> Result<Next, bool> {
        let (vcpu, index) = (self.vcpu, self.control.index);
        let ipa = unsafe { (*self.exit).exception.physical_address };
        if self.guest_ram.host_ptr(ipa, 1).is_some()
            && crate::ramfb_dirty_watch::FB_WATCH.take_write_fault(ipa, esr)
        {
            return Ok(Next::Run);
        }
        trace_isv0_data_abort(esr, pc, ipa);
        let access = match DataAbort::decode(esr) {
            Ok(access) => access,
            Err(error) => {
                self.control.record_run_error(self.primary_vcpu);
                println!("secondary vCPU{} {error} @ PC {pc:#x}", index);
                return Err(true);
            }
        };
        let (size, is_write) = (access.size, access.write);
        // Register 31 is WZR/XZR: stores write zero, loads discard. It must
        // never index the HV register file, where slot 31 is the PC —
        // Linux emits `str wzr` for zero MMIO writes (e.g. virtio
        // device_feature_select=0) and the store leaked the guest PC
        // into the device register.
        let op = if is_write {
            MmioOp::Write {
                size,
                value: unsafe { mmio_store_value(vcpu, access) },
            }
        } else {
            MmioOp::Read { size }
        };
        let (outcome, pending, nvme_io, gpu_copy) = {
            let mut platform_guard =
                lock_platform(self.platform, self.smp_trace, index, HoldSite::Mmio(ipa));
            platform_guard.set_host_now(std::time::Instant::now());
            let (outcome, post_drain) =
                platform_guard.on_mmio_with_post_drain(ipa, op, self.guest_ram);
            refresh_on_ecam_write(&platform_guard, ipa, is_write);
            let pending = self.drain_stats.prepare_pending_delivery_after_mmio(
                &mut platform_guard,
                self.guest_ram,
                self.drain_trace,
                DrainContext::data_abort(*self.exits, pc),
                post_drain,
            );
            // SAFETY: the transfers run only on nvme_io_worker (finish_mmio_work)
            // or, if it cannot run them, in guest_ram. Both views map the
            // guest RAM backing, which the primary unmaps only after it
            // has joined this thread (shutdown_and_join, or the reset
            // join), and so dropped and joined this vCPU's worker.
            let nvme_io = unsafe { platform_guard.take_nvme_io() };
            let gpu_copy = platform_guard.take_virtio_gpu_copy();
            (outcome, pending, nvme_io, gpu_copy)
        };
        self.finish_mmio_work(ipa, pc, pending, nvme_io, gpu_copy)?;
        self.pre_run_drain_gate.mark_secondary_pending();
        match outcome {
            MmioOutcome::ReadValue(v) if !is_write => {
                unsafe { write_mmio_read(vcpu, access, v) };
            }
            MmioOutcome::ReadValue(_) | MmioOutcome::WriteAck => {}
            MmioOutcome::KnownUnimplemented(_) | MmioOutcome::Unmapped => {
                unsafe { write_mmio_read(vcpu, access, 0) };
            }
        }
        unsafe {
            hv_vcpu_set_reg(vcpu, HV_REG_PC, pc + 4);
        }
        Ok(Next::CheckExitCap)
    }

    /// After the MMIO hold: its interrupt delivery, then the NVMe transfers
    /// and virtio-gpu copies it queued, each run unlocked and posted under a
    /// second hold.
    fn finish_mmio_work(
        &mut self,
        ipa: u64,
        pc: u64,
        pending: Result<PendingDrainDelivery, SpiDeliveryError>,
        nvme_io: Option<NvmeIo>,
        gpu_copy: Option<GpuCopy>,
    ) -> Result<(), bool> {
        let (platform, smp_trace, index) = (self.platform, self.smp_trace, self.control.index);
        self.deliver(pending)?;
        // NVMe reads, writes and TRIM run on this vCPU's worker, unlocked.
        // Finished within NVME_IO_WAIT, this exit's second hold posts
        // them; otherwise the drain FLUSH_DONE_HOOK asks for does.
        let nvme_io_worker = &mut *self.nvme_io_worker;
        if nvme_io.is_some_and(|io| {
            nvme_io_worker.run(io, &*self.guest_ram, bridgevm_hvf::nvme::NVME_IO_WAIT)
        }) {
            let pending = {
                let mut platform_guard =
                    lock_platform(platform, smp_trace, index, HoldSite::Mmio(ipa));
                self.drain_stats.prepare_pending_delivery(
                    &mut platform_guard,
                    self.guest_ram,
                    self.drain_trace,
                    DrainContext::data_abort(*self.exits, pc),
                )
            };
            self.deliver(pending)?;
        }
        // A virtio-gpu 2D copy runs here, unlocked; the second hold
        // posts it (gpu_unlocked_copy.rs).
        if let Err(error) = self.drain_stats.run_gpu_copies(
            gpu_copy,
            || lock_platform(platform, smp_trace, index, HoldSite::Mmio(ipa)),
            self.guest_ram,
            self.drain_trace,
            DrainContext::data_abort(*self.exits, pc),
        ) {
            error.stop_secondary(|| self.control.record_run_error(self.primary_vcpu));
            return Err(true);
        }
        Ok(())
    }

    /// SMCCC through HVC: this vCPU's PSCI calls and ARM TRNG; a terminal
    /// PSCI call is forwarded to the primary run loop.
    fn on_hvc(&mut self, pc: u64) -> Result<Next, bool> {
        let (vcpu, control, smp_trace) = (self.vcpu, self.control, self.smp_trace);
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
                return Err(false);
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
                        psci_cpu_on(self.controls, target, entry, context, smp_trace),
                    );
                }
            }
            PSCI_AFFINITY_INFO_32 | PSCI_AFFINITY_INFO_64 => {
                let mut target = 0u64;
                let mut level = 0u64;
                unsafe {
                    hv_vcpu_get_reg(vcpu, HV_REG_X0 + 1, &mut target);
                    hv_vcpu_get_reg(vcpu, HV_REG_X0 + 2, &mut level);
                    let result = psci_affinity_info(self.controls, target, level);
                    hv_vcpu_set_reg(vcpu, HV_REG_X0, result);
                }
            }
            value if psci_terminal_action(value).is_some() => {
                if self.terminal.record(value) {
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
                    hv_vcpus_exit(&self.primary_vcpu, 1);
                }
                return Err(true);
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
        Ok(Next::CheckExitCap)
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
