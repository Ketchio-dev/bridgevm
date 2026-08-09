//! Recovery for vtimer fires swallowed by hv_vcpus_exit cancellation.
//!
//! HVF auto-masks the vtimer when it fires. If a cancellation wins the race
//! against an in-flight fire, the exit surfaces as EXIT_CANCELED, the
//! EXIT_VTIMER handler -- the only unmask site on the run path -- never runs,
//! and nothing ever unmasks again: the guest parks in WFI with the deadline
//! in the past and never wakes (measured 21/21 correlation between surplus
//! cancels and the A1 boot stall).
//!
//! Unmasking alone is not enough: the swallowed fire is edge-latched inside
//! HVF (measured post-unmask stalls with ENABLE=1, IMASK=0, ISTATUS=0 and
//! CVAL in the past), so no new EXIT_VTIMER will ever come for the old
//! deadline. If the guest's deadline has already passed, rewrite CVAL to
//! "now": the deadline is unchanged in guest semantics (it was already due)
//! and HVF sees a fresh expiry to deliver through its own PPI path.

use crate::gic_irq_state::{
    GICR_ISACTIVER0, ICC_AP0R0_EL1, ICC_AP1R0_EL1, ICC_RPR_EL1,
};
use crate::host_support::host_cntvct;
use crate::hvf_abi::{
    hv_gic_get_icc_reg, hv_gic_get_redistributor_reg, hv_gic_set_icc_reg, hv_vcpu_get_sys_reg,
    hv_vcpu_get_vtimer_mask, hv_vcpu_get_vtimer_offset, hv_vcpu_set_sys_reg,
    hv_vcpu_set_vtimer_mask, HvVcpuT, HV_SYS_REG_CNTV_CTL_EL0, HV_SYS_REG_CNTV_CVAL_EL0,
};

/// Call after a surplus (unclaimed) canceled exit. A spurious unmask when no
/// fire was swallowed is harmless; the timer condition is level-evaluated.
pub(crate) fn recover_swallowed_vtimer_fire(vcpu: HvVcpuT) {
    // In-kernel GIC only. Under the userspace GIC the bridge synthesizes
    // expired fires in pre_run from the guest's own CNTV state, and this
    // recovery's re-arm (CVAL = now+240 ticks, 31k times in one parked
    // boot) kept pushing the deadline into the future faster than the
    // synthesizer could observe it expired -- the cure blocked the cure.
    // With synthesis in place the recovery has nothing to add there.
    if crate::usgic_bridge::usgic().is_some() {
        return;
    }
    unsafe {
        hv_vcpu_set_vtimer_mask(vcpu, false);
        let mut cntv_ctl = 0u64;
        let mut cntv_cval = 0u64;
        hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_CNTV_CTL_EL0, &mut cntv_ctl);
        hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_CNTV_CVAL_EL0, &mut cntv_cval);
        // ENABLE=1, IMASK=0, ISTATUS=1: the UEFI-shape stall. The timer
        // fired, the PPI sits pending at the redistributor (2026-08-06 soak:
        // ISPENDR0 bit 27 set, group on, PMR open, HVF mask false), and the
        // CPU interface never signals it into hv_vcpu_run -- re-entering
        // guest mode 69k times did not deliver it, so re-entry alone does
        // not re-evaluate. Pulse the HVF vtimer mask: the mask edge is the
        // one lever that forces HVF's own fire path to re-assert, and a
        // pulse on an already-pending level is harmless by the same
        // level-evaluation argument as the unmask above.
        // Forced pending-latch injection was tried here and REVERTED. Soak
        // 20260806-085841 (1/5, worst measured): hand-set/pulsed latches DID
        // deliver -- boot-2 showed ISACTIVER0 bit 27, first active state
        // ever captured, guest inside the handler -- and the boot still
        // died, spinning at EL1 with PSTATE.I=1. Delivery is not the cure;
        // spurious timer PPIs (ISTATUS=0) hand a Windows ISR an interrupt
        // its timer says did not happen. Keep the mask pulse (measured
        // baseline-neutral, mechanism-justified); do not forge interrupts.
        // Order is load-bearing: clear the stale priority BEFORE pulsing.
        // Soak 20260806-142939 boot-4 ran these the other way around: the
        // pulse's fresh fire edge arrived while RPR=0x10 still gated the
        // PPI, the clear then opened the gate with no edge left to take,
        // and the terminal capture read "pending and deliverable" on a
        // fully idle CPU interface -- open gate, no edge, dead boot.
        clear_stale_running_priority(vcpu);
        RECOVERY_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if cntv_ctl & 0b111 == 0b101 {
            hv_vcpu_set_vtimer_mask(vcpu, true);
            hv_vcpu_set_vtimer_mask(vcpu, false);
            RECOVERY_PULSES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        // ENABLE=1 and IMASK=0: the guest is waiting on this timer.
        if cntv_ctl & 0b11 == 0b01 {
            let mut voff = 0u64;
            hv_vcpu_get_vtimer_offset(vcpu, &mut voff);
            let guest_now = host_cntvct().wrapping_sub(voff);
            if cntv_cval <= guest_now {
                // Slightly in the FUTURE, not exactly now. The 2026-08-06
                // soak captured stalls persisting through these recoveries
                // with ENABLE=1, ISTATUS=0 and CVAL in the past -- the
                // architectural contradiction ISTATUS cannot show if the
                // comparator were level-evaluating. A CVAL written as "now"
                // is already past by the time HVF programs its host timer;
                // a small future deadline forces a fresh arm-and-expire
                // edge. 240 ticks is 10us at 24MHz: unmeasurable to the
                // guest against a deadline that was already 8.2M ticks
                // overdue, but unambiguous to the emulation.
                hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_CNTV_CVAL_EL0, guest_now + REARM_FUTURE_TICKS);
                RECOVERY_REARMS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }
}

/// 10us at the 24MHz architectural counter.
const REARM_FUTURE_TICKS: u64 = 240;

/// Diagnostics: how often the recovery ran / pulsed / re-armed. Read by the
/// usgic stall report to tell "re-arm loop starves the fire" from "fire died
/// with no recovery in sight".
pub(crate) static RECOVERY_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub(crate) static RECOVERY_PULSES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub(crate) static RECOVERY_REARMS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Clear a running priority that has no active interrupt behind it.
///
/// First seen live 2026-08-06 (job 20260806-141247 boot-1, UEFI shape):
/// RPR=0x10 and AP1R0=0x4 while ISACTIVER0=0. An active priority with no
/// active interrupt cannot arise architecturally -- it means an activation
/// was recorded whose EOI/priority-drop never landed (the same cancel race
/// that swallows vtimer fires can swallow the trapped EOI write). While it
/// stands, the CPU interface gates every interrupt at or below that
/// priority, including the pending vtimer PPI: the UEFI-shape stall.
///
/// Unlike the reverted pending-latch forge, nothing guest-visible is
/// invented here: no interrupt is injected; HVF's own bookkeeping is
/// corrected to match the (empty) active set it itself reports.
///
/// # Safety
/// `vcpu` must be live and owned by the calling thread.
unsafe fn clear_stale_running_priority(vcpu: HvVcpuT) {
    let mut rpr = 0u64;
    let mut active = 0u64;
    if hv_gic_get_icc_reg(vcpu, ICC_RPR_EL1, &mut rpr) != 0
        || hv_gic_get_redistributor_reg(vcpu, GICR_ISACTIVER0, &mut active) != 0
    {
        return; // refuse to act on unread state
    }
    if rpr == 0xff || active != 0 {
        return; // idle, or genuinely in service: nothing stale
    }
    let ap0 = hv_gic_set_icc_reg(vcpu, ICC_AP0R0_EL1, 0);
    let ap1 = hv_gic_set_icc_reg(vcpu, ICC_AP1R0_EL1, 0);
    println!(
        "VTIMER RECOVERY: cleared stale running priority rpr={rpr:#x} \
         (ISACTIVER0=0) ap0_status={ap0:#x} ap1_status={ap1:#x}"
    );
}

/// Final-report line: the mask state that diagnosed the swallowed-fire stall.
pub(crate) fn report_vtimer_mask(vcpu: HvVcpuT) {
    let mut vtimer_masked = false;
    let status = unsafe { hv_vcpu_get_vtimer_mask(vcpu, &mut vtimer_masked) };
    println!("VTIMER MASK: masked={vtimer_masked} (status={status:#x})");
}
