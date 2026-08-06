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

use crate::host_support::host_cntvct;
use crate::hvf_abi::{
    hv_vcpu_get_sys_reg, hv_vcpu_get_vtimer_mask, hv_vcpu_get_vtimer_offset, hv_vcpu_set_sys_reg,
    hv_vcpu_set_vtimer_mask, HvVcpuT, HV_SYS_REG_CNTV_CTL_EL0, HV_SYS_REG_CNTV_CVAL_EL0,
};

/// Call after a surplus (unclaimed) canceled exit. A spurious unmask when no
/// fire was swallowed is harmless; the timer condition is level-evaluated.
pub(crate) fn recover_swallowed_vtimer_fire(vcpu: HvVcpuT) {
    unsafe {
        hv_vcpu_set_vtimer_mask(vcpu, false);
        let mut cntv_ctl = 0u64;
        let mut cntv_cval = 0u64;
        hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_CNTV_CTL_EL0, &mut cntv_ctl);
        hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_CNTV_CVAL_EL0, &mut cntv_cval);
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
            }
        }
    }
}

/// 10us at the 24MHz architectural counter.
const REARM_FUTURE_TICKS: u64 = 240;

/// Final-report line: the mask state that diagnosed the swallowed-fire stall.
pub(crate) fn report_vtimer_mask(vcpu: HvVcpuT) {
    let mut vtimer_masked = false;
    let status = unsafe { hv_vcpu_get_vtimer_mask(vcpu, &mut vtimer_masked) };
    println!("VTIMER MASK: masked={vtimer_masked} (status={status:#x})");
}
