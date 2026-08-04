//! Mutex acquisition with wait tracing.
//!
//! Split from `smp_trace.rs` so the tracer holds the event ring and these hold
//! the locking policy. Only slow acquisitions are recorded: an uncontended
//! lock must cost nothing beyond `try_lock`, since these wrap the platform and
//! vCPU-state mutexes taken on every exit.

use crate::*;
use super::event_ring::{lock_id, EventKind};
use super::SmpTrace;

impl SmpTrace {
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

