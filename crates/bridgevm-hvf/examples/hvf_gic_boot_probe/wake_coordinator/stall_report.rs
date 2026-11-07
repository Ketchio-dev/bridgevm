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
    crate::usgic_bridge::stall_report(vcpu);
    // One capture: vCPU0 is this thread's and out of hv_vcpu_run, and the
    // secondaries are joined, so a second one later cannot see it move (it
    // only made every stop, a guest shutdown too, 50 ms slower).
    let snapshot = crate::gic_snapshot::capture(vcpu, 2, coordinator.generation());
    for line in crate::gic_snapshot::render(&snapshot, &snapshot) {
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
