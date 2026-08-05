//! Deciding whether a canceled vCPU exit ends the run.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Why a canceled exit should end the run, or None to keep going.
///
/// Two flags, not one: the deadline watchdog is rebuilt per reboot generation,
/// while the boot-progress stall kill lives for the whole run. A canceled exit
/// with neither set is an automation wake and is benign -- which is correct,
/// and was also why a confirmed stall did not stop anything.
pub(crate) fn cancel_stop_reason(
    watchdog_fired: &AtomicBool,
    stall_kill_fired: Option<&Arc<AtomicBool>>,
) -> Option<&'static str> {
    if watchdog_fired.load(Ordering::SeqCst) {
        return Some("watchdog (CANCELED)");
    }
    if stall_kill_fired.is_some_and(|fired| fired.load(Ordering::SeqCst)) {
        return Some("boot-progress stall (kill mode)");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancel_with_no_flag_set_is_benign() {
        assert_eq!(cancel_stop_reason(&AtomicBool::new(false), None), None);
        let quiet = Arc::new(AtomicBool::new(false));
        assert_eq!(
            cancel_stop_reason(&AtomicBool::new(false), Some(&quiet)),
            None
        );
    }

    #[test]
    fn each_flag_names_itself() {
        assert_eq!(
            cancel_stop_reason(&AtomicBool::new(true), None),
            Some("watchdog (CANCELED)")
        );
        let fired = Arc::new(AtomicBool::new(true));
        assert_eq!(
            cancel_stop_reason(&AtomicBool::new(false), Some(&fired)),
            Some("boot-progress stall (kill mode)")
        );
    }
}

/// Two GIC snapshots 50ms apart, rendered, for a stall that was just confirmed.
///
/// The question a single sample cannot answer is whether the vCPU is parked or
/// running without exiting. A PC that moves between the two says the guest is
/// executing; one that does not, with the vtimer masked, says it is waiting for
/// an interrupt that will not arrive.
pub(crate) fn stall_gic_report(vcpu: crate::HvVcpuT, reboots: u64) -> Vec<String> {
    // HVF only lets the owning thread read a vCPU's registers. This runs on the
    // watchdog thread, so every read fails and capture() -- which ignores the
    // return codes -- reports zeros. The first version of this printed
    // "PC 0x0 -> 0x0, CNTV_CTL=0x0, verdict=parked" for a guest whose real PC
    // was 0x1bf33ba04, which is worse than printing nothing: it looks like
    // evidence.
    let mut probe = 0u64;
    // SAFETY: Category 8 - FFI boundary. `vcpu` is a live HVF handle owned by
    // the probe for the whole run; this reads one register and checks the
    // status rather than trusting the out-parameter.
    let status = unsafe { crate::hv_vcpu_get_reg(vcpu, crate::HV_REG_PC, &mut probe) };
    if status != 0 {
        return vec![format!(
            "GIC SNAPSHOT: unavailable off the vCPU thread (hv_vcpu_get_reg={status:#x}); \
             see the REGS line in the final report"
        )];
    }
    // SAFETY: as above; the status check proves this thread may read.
    let (before, after) = unsafe {
        let before = crate::gic_snapshot::capture(vcpu, 0, reboots);
        std::thread::sleep(std::time::Duration::from_millis(50));
        (before, crate::gic_snapshot::capture(vcpu, 0, reboots))
    };
    crate::gic_snapshot::render(&before, &after)
}
