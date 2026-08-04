//! Owning-thread snapshot of a vCPU's timer/PSCI state, and the diff between
//! two of them.
//!
//! Apple's vCPU and GIC accessors belong to the thread that owns the vCPU;
//! reading them from a watchdog thread is not sound. So the capture is a plain
//! struct filled in by the owning thread, and everything interesting -- the
//! comparison, the verdict, the rendering -- is pure and lives here where it
//! can be tested without Hypervisor.framework.
//!
//! The verdict this exists to produce is the one the T1 microprobe established
//! (see docs/windows-arm/evidence/t1-vtimer-cancel-microprobe-20260804.md):
//! a vCPU parked at `WFI` with `CNTV_CTL` reporting ENABLE=1, ISTATUS=1 and the
//! vtimer masked has a pending interrupt that will never be delivered. That is
//! a different failure from a vCPU that is still executing, and the two used to
//! be indistinguishable in the final report.

use crate::*;

/// `CNTV_CTL_EL0` bit 0: the timer is enabled.
const CNTV_CTL_ENABLE: u64 = 1 << 0;
/// `CNTV_CTL_EL0` bit 1: the interrupt is masked by the guest.
const CNTV_CTL_IMASK: u64 = 1 << 1;
/// `CNTV_CTL_EL0` bit 2: the timer condition is met.
const CNTV_CTL_ISTATUS: u64 = 1 << 2;

/// One capture of a vCPU's architectural timer and PSCI state.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GicSnapshot {
    pub(crate) mpidr: u64,
    pub(crate) pc: u64,
    pub(crate) cntv_ctl: u64,
    pub(crate) cntv_cval: u64,
    /// Guest view of the counter at capture time.
    pub(crate) guest_now: u64,
    /// HVF's own vtimer mask, which is separate from the guest's `IMASK`.
    pub(crate) vtimer_masked: bool,
    /// PSCI state as the probe tracks it: 0 Off, 1 OnPending, 2 On.
    pub(crate) psci_state: u64,
    /// Reboot generation this snapshot belongs to.
    pub(crate) generation: u64,
}

impl GicSnapshot {
    pub(crate) fn timer_enabled(&self) -> bool {
        self.cntv_ctl & CNTV_CTL_ENABLE != 0
    }

    pub(crate) fn guest_masked(&self) -> bool {
        self.cntv_ctl & CNTV_CTL_IMASK != 0
    }

    /// The timer condition is met, i.e. an interrupt is pending.
    pub(crate) fn condition_met(&self) -> bool {
        self.cntv_ctl & CNTV_CTL_ISTATUS != 0
    }

    /// Ticks by which the armed deadline has already passed. Zero when the
    /// deadline is still in the future.
    pub(crate) fn overdue_ticks(&self) -> u64 {
        self.guest_now.saturating_sub(self.cntv_cval)
    }

    /// True when the deadline has passed but the wake cannot arrive: either
    /// HVF has the vtimer masked, or the condition is met and the guest has
    /// masked it. This is the state the T1 probe showed is permanent without
    /// host intervention.
    pub(crate) fn wake_is_unreachable(&self) -> bool {
        if !self.timer_enabled() {
            return false;
        }
        let overdue = self.overdue_ticks() > 0 || self.condition_met();
        overdue && (self.vtimer_masked || self.guest_masked())
    }
}

/// What comparing two snapshots says about the vCPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgressVerdict {
    /// The PC moved: the vCPU is executing.
    Running,
    /// The PC is unchanged but the deadline still lies ahead, so the vCPU is
    /// legitimately waiting.
    WaitingForFutureDeadline,
    /// The PC is unchanged, the deadline has passed, and the wake cannot be
    /// delivered. This is the stall fingerprint.
    ParkedWithUnreachableWake,
    /// The PC is unchanged and the deadline has passed, yet the timer is not
    /// masked. Something other than the vtimer mask is holding the vCPU.
    ParkedWithDeliverableWake,
    /// The two snapshots are from different reboot generations and cannot be
    /// compared.
    GenerationChanged,
}

impl ProgressVerdict {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ProgressVerdict::Running => "running",
            ProgressVerdict::WaitingForFutureDeadline => "waiting (deadline ahead)",
            ProgressVerdict::ParkedWithUnreachableWake => {
                "parked (deadline passed, wake unreachable)"
            }
            ProgressVerdict::ParkedWithDeliverableWake => {
                "parked (deadline passed, wake still deliverable)"
            }
            ProgressVerdict::GenerationChanged => "generation changed",
        }
    }

    /// Whether this verdict should fail a gate. Only the unreachable-wake case
    /// is a defect; a vCPU waiting on a future deadline is healthy.
    pub(crate) fn is_stall(self) -> bool {
        matches!(self, ProgressVerdict::ParkedWithUnreachableWake)
    }
}

/// Compare two snapshots of the same vCPU taken a short interval apart.
pub(crate) fn compare(before: &GicSnapshot, after: &GicSnapshot) -> ProgressVerdict {
    if before.generation != after.generation {
        return ProgressVerdict::GenerationChanged;
    }
    // A moving PC or a re-armed deadline both mean the guest ran.
    if before.pc != after.pc || before.cntv_cval != after.cntv_cval {
        return ProgressVerdict::Running;
    }
    if after.overdue_ticks() == 0 && !after.condition_met() {
        return ProgressVerdict::WaitingForFutureDeadline;
    }
    if after.wake_is_unreachable() {
        ProgressVerdict::ParkedWithUnreachableWake
    } else {
        ProgressVerdict::ParkedWithDeliverableWake
    }
}

/// Bounded final-report lines for one vCPU.
pub(crate) fn render(before: &GicSnapshot, after: &GicSnapshot) -> Vec<String> {
    let verdict = compare(before, after);
    vec![
        format!(
            "GIC SNAPSHOT: MPIDR={:#x} generation={} verdict={} stall={}",
            after.mpidr,
            after.generation,
            verdict.as_str(),
            verdict.is_stall()
        ),
        format!(
            "GIC SNAPSHOT: PC {:#x} -> {:#x} psci_state={} vtimer_masked={}",
            before.pc, after.pc, after.psci_state, after.vtimer_masked
        ),
        format!(
            "GIC SNAPSHOT: CNTV_CTL={:#x} (enable={} imask={} istatus={}) CVAL={:#x} \
             guest_now={:#x} overdue_ticks={}",
            after.cntv_ctl,
            after.timer_enabled(),
            after.guest_masked(),
            after.condition_met(),
            after.cntv_cval,
            after.guest_now,
            after.overdue_ticks()
        ),
    ]
}

#[cfg(test)]
#[path = "gic_snapshot_tests.rs"]
mod tests;

/// Capture the current state. Must be called from the thread that owns
/// `vcpu`: Apple's vCPU accessors are not valid from another thread.
///
/// # Safety
/// `vcpu` must be a live vCPU owned by the calling thread.
pub(crate) unsafe fn capture(vcpu: HvVcpuT, psci_state: u64, generation: u64) -> GicSnapshot {
    let mut mpidr = 0u64;
    let mut pc = 0u64;
    let mut cntv_ctl = 0u64;
    let mut cntv_cval = 0u64;
    let mut vtimer_offset = 0u64;
    let mut vtimer_masked = false;
    hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_MPIDR_EL1, &mut mpidr);
    hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc);
    hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_CNTV_CTL_EL0, &mut cntv_ctl);
    hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_CNTV_CVAL_EL0, &mut cntv_cval);
    hv_vcpu_get_vtimer_offset(vcpu, &mut vtimer_offset);
    hv_vcpu_get_vtimer_mask(vcpu, &mut vtimer_masked);
    GicSnapshot {
        mpidr,
        pc,
        cntv_ctl,
        cntv_cval,
        guest_now: host_cntvct().wrapping_sub(vtimer_offset),
        vtimer_masked,
        psci_state,
        generation,
    }
}
