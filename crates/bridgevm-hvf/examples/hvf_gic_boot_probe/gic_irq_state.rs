//! Owning-thread capture of the GIC's own interrupt state for one vCPU.
//!
//! `gic_snapshot.rs` answers "is the vCPU parked and is its timer pending".
//! This answers the next question, the one t4-soak left open: a failing boot
//! receives zero interrupts of any kind, so *which layer* is refusing them?
//! The redistributor enable/pending bits and the CPU-interface PMR/IGRPEN
//! registers split that into distinct verdicts: not-enabled, pending-but-not-
//! taken, or nothing-pending-at-all.
//!
//! Every read carries its status. The first GIC capture in this probe
//! reported zeros from a thread HVF refuses, and the zeros looked exactly
//! like evidence; a failed read here renders as `?`, never as a number.

use crate::{hv_gic_get_icc_reg, hv_gic_get_redistributor_reg, HvVcpuT};

/// The vtimer's PPI INTID on the virt platform: bit 27 of the GICR banked
/// enable/pending/active words covers it.
pub(crate) const VTIMER_PPI_BIT: u32 = 1 << 27;

pub(crate) const GICR_IGROUPR0: u32 = 0x10080;
pub(crate) const GICR_ISENABLER0: u32 = 0x10100;
pub(crate) const GICR_ISPENDR0: u32 = 0x10200;
pub(crate) const GICR_ISACTIVER0: u32 = 0x10300;
pub(crate) const ICC_PMR_EL1: u16 = 0xc230;
pub(crate) const ICC_IGRPEN0_EL1: u16 = 0xc666;
pub(crate) const ICC_IGRPEN1_EL1: u16 = 0xc667;
pub(crate) const ICC_CTLR_EL1: u16 = 0xc664;
/// Running priority: 0xff (idle) unless the CPU interface believes an
/// interrupt is currently being handled. A non-idle value at a stall with
/// ISACTIVER0=0 means a stale active priority is gating delivery -- the one
/// state the first capture set left unread.
pub(crate) const ICC_RPR_EL1: u16 = 0xc65b;
pub(crate) const ICC_AP0R0_EL1: u16 = 0xc644;
pub(crate) const ICC_AP1R0_EL1: u16 = 0xc648;

/// One capture. `None` means the read failed, and is rendered as `?`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GicIrqState {
    pub(crate) igroupr0: Option<u64>,
    pub(crate) isenabler0: Option<u64>,
    pub(crate) ispendr0: Option<u64>,
    pub(crate) isactiver0: Option<u64>,
    pub(crate) pmr: Option<u64>,
    pub(crate) igrpen0: Option<u64>,
    pub(crate) igrpen1: Option<u64>,
    pub(crate) ctlr: Option<u64>,
    pub(crate) rpr: Option<u64>,
    pub(crate) ap0r0: Option<u64>,
    pub(crate) ap1r0: Option<u64>,
}


fn reg(status: i32, value: u64) -> Option<u64> {
    (status == 0).then_some(value)
}

fn fmt(v: Option<u64>) -> String {
    v.map_or_else(|| "?".into(), |v| format!("{v:#x}"))
}

/// Capture from the owning thread. Statuses are checked per register; a
/// refused read becomes `None` rather than a zero that looks measured.
///
/// # Safety
/// `vcpu` must be a live vCPU owned by the calling thread.
pub(crate) unsafe fn capture(vcpu: HvVcpuT) -> GicIrqState {
    let read_r = |offset: u32| {
        let mut v = 0u64;
        reg(hv_gic_get_redistributor_reg(vcpu, offset, &mut v), v)
    };
    let (igroupr0, isenabler0, ispendr0, isactiver0) = (
        read_r(GICR_IGROUPR0),
        read_r(GICR_ISENABLER0),
        read_r(GICR_ISPENDR0),
        read_r(GICR_ISACTIVER0),
    );
    let read_c = |sysreg: u16| {
        let mut v = 0u64;
        reg(hv_gic_get_icc_reg(vcpu, sysreg, &mut v), v)
    };
    GicIrqState {
        igroupr0,
        isenabler0,
        ispendr0,
        isactiver0,
        pmr: read_c(ICC_PMR_EL1),
        igrpen0: read_c(ICC_IGRPEN0_EL1),
        igrpen1: read_c(ICC_IGRPEN1_EL1),
        ctlr: read_c(ICC_CTLR_EL1),
        rpr: read_c(ICC_RPR_EL1),
        ap0r0: read_c(ICC_AP0R0_EL1),
        ap1r0: read_c(ICC_AP1R0_EL1),
    }
}

/// Bounded final-report lines.
pub(crate) fn render(state: &GicIrqState) -> Vec<String> {
    vec![
        format!(
            "GIC IRQ STATE: GICR IGROUPR0={} ISENABLER0={} ISPENDR0={} ISACTIVER0={}",
            fmt(state.igroupr0),
            fmt(state.isenabler0),
            fmt(state.ispendr0),
            fmt(state.isactiver0)
        ),
        format!(
            "GIC IRQ STATE: ICC PMR={} IGRPEN0={} IGRPEN1={} CTLR={} RPR={} AP0R0={} AP1R0={} vtimer_verdict={}",
            fmt(state.pmr),
            fmt(state.igrpen0),
            fmt(state.igrpen1),
            fmt(state.ctlr),
            fmt(state.rpr),
            fmt(state.ap0r0),
            fmt(state.ap1r0),
            state.vtimer_verdict().unwrap_or("unavailable (a read failed)")
        ),
    ]
}

#[path = "gic_irq_state/verdict.rs"]
mod verdict;

#[cfg(test)]
#[path = "gic_irq_state_tests.rs"]
mod tests;
