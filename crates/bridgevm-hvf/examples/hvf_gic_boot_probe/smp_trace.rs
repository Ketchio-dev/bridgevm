//! SMP lock and vCPU progress tracing.

use crate::*;

pub(crate) const SMP_TRACE_PROGRESS_INTERVAL: u64 = 10_000;

pub(crate) const SMP_TRACE_LOCK_WARN_AFTER: Duration = Duration::from_millis(250);

pub(crate) struct SmpTrace {
    pub(crate) cpu0_exits: AtomicU64,
    pub(crate) secondary_exits: AtomicU64,
}

impl SmpTrace {
    pub(crate) fn new() -> Self {
        Self {
            cpu0_exits: AtomicU64::new(0),
            secondary_exits: AtomicU64::new(0),
        }
    }
    pub(crate) fn state_transition(&self, cpu: u64, from: PsciState, to: PsciState) {
        println!("SMP trace: vCPU{cpu} {from:?} -> {to:?}");
    }
    pub(crate) fn secondary_vcpu_created(&self, cpu: u64, vcpu: HvVcpuT, exit: *mut HvVcpuExit) {
        println!("SMP trace: vCPU{cpu} created HVF vCPU {vcpu} exit={exit:p}");
    }
    pub(crate) fn secondary_waiting_off(&self, cpu: u64) {
        println!("SMP trace: vCPU{cpu} blocking while Off");
    }
    pub(crate) fn secondary_woke(&self, cpu: u64, state: PsciState) {
        println!("SMP trace: vCPU{cpu} woke with state {state:?}");
    }
    pub(crate) fn secondary_run_loop_entered(&self, cpu: u64, exits: u64) {
        println!("SMP trace: vCPU{cpu} entering run loop after {exits} exits");
    }
    pub(crate) fn secondary_before_first_run(
        &self,
        cpu: u64,
        pc: u64,
        cpsr: u64,
        x0: u64,
        mpidr: u64,
    ) {
        println!(
            "SMP trace: vCPU{cpu} before first hv_vcpu_run PC={pc:#x} CPSR={cpsr:#x} X0={x0:#x} MPIDR_EL1={mpidr:#x}"
        );
    }
    pub(crate) fn secondary_pre_run_drain(&self, cpu: u64, exit: u64, pc: u64) {
        if exit < 10 {
            println!(
                "SMP trace: vCPU{cpu} pre-run drain before run {} PC={pc:#x}",
                exit + 1
            );
        }
    }
    pub(crate) fn secondary_post_run_drain(&self, cpu: u64, exit: u64) {
        if exit < 10 {
            println!(
                "SMP trace: vCPU{cpu} pre-run drain complete before run {}",
                exit + 1
            );
        }
    }
    pub(crate) fn secondary_run_result(
        &self,
        cpu: u64,
        run: u64,
        status: HvReturn,
        exit: *mut HvVcpuExit,
    ) {
        if run > 10 {
            return;
        }
        if status != 0 {
            println!("SMP trace: vCPU{cpu} hv_vcpu_run #{run} returned {status:#x}");
            return;
        }
        let reason = unsafe { (*exit).reason };
        if reason == EXIT_EXCEPTION {
            let esr = unsafe { (*exit).exception.syndrome };
            let ec = (esr >> 26) & 0x3f;
            println!(
                "SMP trace: vCPU{cpu} hv_vcpu_run #{run} returned {status:#x} reason={reason} EC={ec:#x} ESR={esr:#x}"
            );
        } else {
            println!(
                "SMP trace: vCPU{cpu} hv_vcpu_run #{run} returned {status:#x} reason={reason}"
            );
        }
    }
    pub(crate) fn cpu0_progress(&self, exits: u64) {
        self.cpu0_exits.store(exits, Ordering::Relaxed);
        if exits != 0 && exits % SMP_TRACE_PROGRESS_INTERVAL == 0 {
            println!(
                "SMP trace: progress cpu0_exits={} secondary_exits={}",
                exits,
                self.secondary_exits.load(Ordering::Relaxed)
            );
        }
    }
    pub(crate) fn secondary_progress(&self) {
        let exits = self.secondary_exits.fetch_add(1, Ordering::Relaxed) + 1;
        if exits % SMP_TRACE_PROGRESS_INTERVAL == 0 {
            println!(
                "SMP trace: progress cpu0_exits={} secondary_exits={}",
                self.cpu0_exits.load(Ordering::Relaxed),
                exits
            );
        }
    }
    pub(crate) fn lock_with_wait_trace<'a, T>(
        &self,
        cpu: u64,
        lock_name: &'static str,
        context: &'static str,
        mutex: &'a Mutex<T>,
    ) -> MutexGuard<'a, T> {
        let started = Instant::now();
        let mut last_report = Duration::ZERO;
        loop {
            match mutex.try_lock() {
                Ok(guard) => {
                    let elapsed = started.elapsed();
                    if elapsed >= SMP_TRACE_LOCK_WARN_AFTER {
                        println!(
                            "SMP trace: vCPU{cpu} acquired {lock_name} after {} ms ({context})",
                            elapsed.as_millis()
                        );
                    }
                    return guard;
                }
                Err(TryLockError::WouldBlock) => {
                    let elapsed = started.elapsed();
                    if elapsed >= SMP_TRACE_LOCK_WARN_AFTER
                        && elapsed.saturating_sub(last_report) >= SMP_TRACE_LOCK_WARN_AFTER
                    {
                        println!(
                            "SMP trace: vCPU{cpu} waiting {} ms for {lock_name} ({context})",
                            elapsed.as_millis()
                        );
                        last_report = elapsed;
                    }
                    thread::sleep(Duration::from_millis(1));
                }
                Err(TryLockError::Poisoned(_)) => panic!("{context}"),
            }
        }
    }
}

pub(crate) fn lock_with_optional_trace<'a, T>(
    mutex: &'a Mutex<T>,
    smp_trace: Option<&SmpTrace>,
    cpu: u64,
    lock_name: &'static str,
    context: &'static str,
) -> MutexGuard<'a, T> {
    match smp_trace {
        Some(trace) => trace.lock_with_wait_trace(cpu, lock_name, context, mutex),
        None => mutex.lock().expect(context),
    }
}

pub(crate) fn lock_platform<'a>(
    platform: &'a Arc<Mutex<VirtPlatform>>,
    smp_trace: Option<&SmpTrace>,
    cpu: u64,
    context: &'static str,
) -> MutexGuard<'a, VirtPlatform> {
    lock_with_optional_trace(platform, smp_trace, cpu, "platform mutex", context)
}

pub(crate) fn lock_vcpu_state<'a>(
    control: &'a VcpuControl,
    smp_trace: Option<&SmpTrace>,
    cpu: u64,
    context: &'static str,
) -> MutexGuard<'a, PsciState> {
    lock_with_optional_trace(
        &control.state,
        smp_trace,
        cpu,
        "VcpuControl.state mutex",
        context,
    )
}
