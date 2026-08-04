//! SMP lock and vCPU progress tracing.
//!
//! Events are recorded into a fixed-size numeric ring (see `event_ring`) and
//! rendered only by `dump`, after the run has stopped. Nothing on the record
//! path allocates, formats or writes to stdout, because doing so from inside
//! the vCPU run loops perturbed the very timing this trace exists to observe.

#[path = "smp_trace/event_ring.rs"]
pub(crate) mod event_ring;

use crate::*;
use event_ring::{EventKind, EventRing, SmpEvent};

pub(crate) const SMP_TRACE_PROGRESS_INTERVAL: u64 = 10_000;

pub(crate) const SMP_TRACE_LOCK_WARN_AFTER: Duration = Duration::from_millis(250);

/// Lock identities, stored as a number so the ring holds no strings.
const LOCK_PLATFORM: u64 = 0;
const LOCK_VCPU_STATE: u64 = 1;
const LOCK_OTHER: u64 = 2;

fn lock_id(lock_name: &str) -> u64 {
    match lock_name {
        "platform mutex" => LOCK_PLATFORM,
        "VcpuControl.state mutex" => LOCK_VCPU_STATE,
        _ => LOCK_OTHER,
    }
}

fn lock_name(id: u64) -> &'static str {
    match id {
        LOCK_PLATFORM => "platform mutex",
        LOCK_VCPU_STATE => "VcpuControl.state mutex",
        _ => "lock",
    }
}

fn psci_state_code(state: PsciState) -> u64 {
    match state {
        PsciState::Off => 0,
        PsciState::OnPending => 1,
        PsciState::On => 2,
    }
}

pub(crate) struct SmpTrace {
    pub(crate) cpu0_exits: AtomicU64,
    pub(crate) secondary_exits: AtomicU64,
    ring: EventRing,
}

impl SmpTrace {
    pub(crate) fn new() -> Self {
        Self {
            cpu0_exits: AtomicU64::new(0),
            secondary_exits: AtomicU64::new(0),
            ring: EventRing::new(),
        }
    }

    fn record(&self, kind: EventKind, cpu: u64, a: u64, b: u64) {
        self.ring.record(SmpEvent { kind, cpu, a, b });
    }

    /// Events lost to ring wrap. Reported so a conclusion is never drawn from
    /// a silently truncated record.
    pub(crate) fn trace_overflow(&self) -> u64 {
        self.ring.overflow()
    }

    /// Render the retained events. Call after the run has stopped.
    pub(crate) fn dump(&self) {
        self.ring.dump(lock_name);
    }

    pub(crate) fn state_transition(&self, cpu: u64, from: PsciState, to: PsciState) {
        self.record(
            EventKind::StateTransition,
            cpu,
            psci_state_code(from),
            psci_state_code(to),
        );
    }
    pub(crate) fn secondary_vcpu_created(&self, cpu: u64, vcpu: HvVcpuT, exit: *mut HvVcpuExit) {
        self.record(EventKind::SecondaryCreated, cpu, vcpu, exit as u64);
    }
    pub(crate) fn secondary_waiting_off(&self, cpu: u64) {
        self.record(EventKind::SecondaryWaitingOff, cpu, 0, 0);
    }
    pub(crate) fn secondary_woke(&self, cpu: u64, state: PsciState) {
        self.record(EventKind::SecondaryWoke, cpu, psci_state_code(state), 0);
    }
    pub(crate) fn secondary_run_loop_entered(&self, cpu: u64, exits: u64) {
        self.record(EventKind::RunLoopEntered, cpu, exits, 0);
    }
    pub(crate) fn secondary_before_first_run(
        &self,
        cpu: u64,
        pc: u64,
        cpsr: u64,
        x0: u64,
        mpidr: u64,
    ) {
        // Two events rather than a wider struct: the ring stays four words.
        self.record(EventKind::RunLoopEntered, cpu, pc, cpsr);
        self.record(EventKind::RunLoopEntered, cpu, x0, mpidr);
    }
    pub(crate) fn secondary_pre_run_drain(&self, cpu: u64, exit: u64, pc: u64) {
        if exit < 10 {
            self.record(EventKind::PreRunDrain, cpu, exit + 1, pc);
        }
    }
    pub(crate) fn secondary_post_run_drain(&self, cpu: u64, exit: u64) {
        if exit < 10 {
            self.record(EventKind::PostRunDrain, cpu, exit + 1, 0);
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
            self.record(EventKind::RunResult, cpu, run, status as u32 as u64);
            return;
        }
        // SAFETY: caller passes the exit structure belonging to this vCPU.
        let reason = unsafe { (*exit).reason } as u64;
        self.record(EventKind::RunResult, cpu, run, reason);
    }
    pub(crate) fn cpu0_progress(&self, exits: u64) {
        self.cpu0_exits.store(exits, Ordering::Relaxed);
        if exits != 0 && exits % SMP_TRACE_PROGRESS_INTERVAL == 0 {
            self.record(
                EventKind::Progress,
                0,
                exits,
                self.secondary_exits.load(Ordering::Relaxed),
            );
        }
    }
    pub(crate) fn secondary_progress(&self) {
        let exits = self.secondary_exits.fetch_add(1, Ordering::Relaxed) + 1;
        if exits % SMP_TRACE_PROGRESS_INTERVAL == 0 {
            self.record(
                EventKind::Progress,
                1,
                self.cpu0_exits.load(Ordering::Relaxed),
                exits,
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
        let id = lock_id(lock_name);
        let started = Instant::now();
        let mut last_report = Duration::ZERO;
        loop {
            match mutex.try_lock() {
                Ok(guard) => {
                    let elapsed = started.elapsed();
                    if elapsed >= SMP_TRACE_LOCK_WARN_AFTER {
                        self.record(EventKind::LockAcquired, cpu, elapsed.as_millis() as u64, id);
                    }
                    return guard;
                }
                Err(TryLockError::WouldBlock) => {
                    let elapsed = started.elapsed();
                    if elapsed >= SMP_TRACE_LOCK_WARN_AFTER
                        && elapsed.saturating_sub(last_report) >= SMP_TRACE_LOCK_WARN_AFTER
                    {
                        self.record(EventKind::LockWait, cpu, elapsed.as_millis() as u64, id);
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

#[cfg(test)]
#[path = "smp_trace/smp_trace_tests.rs"]
mod tests;
