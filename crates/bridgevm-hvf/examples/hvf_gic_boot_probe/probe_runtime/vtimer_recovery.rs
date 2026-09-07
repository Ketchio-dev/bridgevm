//! Vtimer state reporting. The host-side "recovery" that used to live here is
//! retracted.
//!
//! It existed because HVF auto-masks the vtimer when it fires: if a
//! cancellation won the race against an in-flight fire, the EXIT_VTIMER
//! handler -- the only unmask site on the run path -- never ran, and a
//! 2026-08-06 soak measured a 21/21 correlation between surplus cancels and
//! the A1 boot stall. On every canceled exit it unmasked, pulsed the mask and
//! rewrote CNTV_CVAL to just past "now" so HVF would see a fresh expiry.
//!
//! Under the in-kernel GIC that is what stops the guest. The GIC owns the
//! timer there and delivers PPI 27 without any userspace exit; writing CVAL
//! and toggling the mask roughly every 250 ms keeps the PPI from ever going
//! pending. EDK2's ArmTimerDxe drives the DXE tick from the virtual timer
//! alone, so the tick dies, BdsWait never returns, and UEFI parks in the boot
//! countdown before it can start the boot option. Interleaved A/B on one host
//! with identical media, vars and firmware, 2026-09-07: with the recovery the
//! boot failed 0/5, without it it completed 2/2, reaching the desktop in
//! 21382 ms and 30300 ms. Its own header already recorded the same dynamic
//! under the userspace GIC, where it was disabled for the same reason -- the
//! cure blocked the cure. No configuration is left in which it is measured to
//! help.
//!
//! If the A1 stall returns, derive the fix against current HVF behaviour
//! rather than restoring this one.

use crate::hvf_abi::{hv_vcpu_get_vtimer_mask, HvVcpuT};

/// Final-report line: the mask state that diagnosed the swallowed-fire stall.
pub(crate) fn report_vtimer_mask(vcpu: HvVcpuT) {
    let mut vtimer_masked = false;
    let status = unsafe { hv_vcpu_get_vtimer_mask(vcpu, &mut vtimer_masked) };
    println!("VTIMER MASK: masked={vtimer_masked} (status={status:#x})");
}
