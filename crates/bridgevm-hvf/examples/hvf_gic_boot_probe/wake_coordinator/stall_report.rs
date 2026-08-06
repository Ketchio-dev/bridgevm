//! The stop-path stall report: wake attribution, timer snapshot, GIC state.
//!
//! Split from wake_coordinator.rs: the coordinator owns claims and
//! generations; this renders them, once, from the owning thread.

use crate::{report_wake_attribution, CancelClaim, WakeCoordinator};
use crate::{HvVcpuT, SmpTrace};

/// Everything the stop path reports about wakes, timer state and the trace.
///
/// Called once from the owning thread after the run has stopped: the GIC
/// snapshot is only valid there, and the SMP trace must not be formatted while
/// vCPUs are still running.
///
/// # Safety
/// `vcpu` must be live and owned by the calling thread.
pub(crate) unsafe fn report_stall_diagnostics(
    vcpu: HvVcpuT,
    coordinator: &WakeCoordinator,
    claims: &[CancelClaim],
    smp_trace: Option<&SmpTrace>,
) {
    report_wake_attribution(coordinator, claims);
    // Two captures, so a vCPU that is still executing is not called parked.
    let before = crate::gic_snapshot::capture(vcpu, 2, coordinator.generation());
    std::thread::sleep(std::time::Duration::from_millis(50));
    let after = crate::gic_snapshot::capture(vcpu, 2, coordinator.generation());
    for line in crate::gic_snapshot::render(&before, &after) {
        println!("{line}");
    }
    // The layer below: is the GIC itself refusing, holding, or empty?
    for line in crate::gic_irq_state::render(&crate::gic_irq_state::capture(vcpu)) {
        println!("{line}");
    }
    if let Some(trace) = smp_trace {
        println!("SMP trace: overflow={}", trace.trace_overflow());
        trace.dump();
    }
}
