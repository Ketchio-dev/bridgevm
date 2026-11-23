//! Vtimer state reporting only; the host does not "recover" the vtimer.
//!
//! HVF auto-masks the vtimer when it fires, which suggests unmasking, pulsing
//! the mask and rewriting CNTV_CVAL on canceled exits. Under the in-kernel GIC
//! that stops the guest: the GIC owns the timer and delivers PPI 27 without any
//! userspace exit, and rewriting CVAL and toggling the mask keeps the PPI from
//! ever going pending. EDK2's ArmTimerDxe drives the DXE tick from the virtual
//! timer alone, so the tick dies, BdsWait never returns, and UEFI parks in the
//! boot countdown before it can start the boot option.

use crate::hvf_abi::{hv_vcpu_get_vtimer_mask, HvVcpuT};

/// Final-report line: the mask state that diagnosed the swallowed-fire stall.
pub(crate) fn report_vtimer_mask(vcpu: HvVcpuT) {
    let mut vtimer_masked = false;
    let status = unsafe { hv_vcpu_get_vtimer_mask(vcpu, &mut vtimer_masked) };
    println!("VTIMER MASK: masked={vtimer_masked} (status={status:#x})");
}
