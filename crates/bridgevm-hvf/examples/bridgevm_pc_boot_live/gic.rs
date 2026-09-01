//! The board's interrupt controller: a userspace GICv3.
//!
//! No in-kernel `hv_gic` is created, so the guest's GIC MMIO and `ICC_*`
//! system-register accesses trap to the host and are served by `UsGic`, and
//! interrupts are injected with `hv_vcpu_set_pending_interrupt`. This is the
//! model the shipping HVF engine uses to boot Windows.

use super::us_gic::UsGic;

pub(super) fn create() -> Result<UsGic, String> {
    Ok(UsGic::new())
}
