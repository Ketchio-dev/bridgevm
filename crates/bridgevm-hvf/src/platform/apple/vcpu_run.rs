//! One-shot vCPU run with a watchdog, and exception-syndrome helpers.
//!
//! Split out of the single 12,111-line apple.rs backend.

use super::*;
use crate::*;

pub(crate) fn run_vcpu_once_with_watchdog(
    vcpu: HvVcpu,
    exit: *mut HvVcpuExit,
) -> VcpuRunObservation {
    run_vcpu_once_with_watchdog_timeout(vcpu, exit, 100)
}

pub(crate) fn run_vcpu_once_with_watchdog_timeout(
    vcpu: HvVcpu,
    exit: *mut HvVcpuExit,
    watchdog_timeout_ms: u64,
) -> VcpuRunObservation {
    let done = Arc::new(AtomicBool::new(false));
    let watchdog_done = Arc::clone(&done);
    let vcpu_for_watchdog = vcpu;
    let watchdog_timeout_ms = watchdog_timeout_ms.max(1);
    let watchdog = thread::spawn(move || {
        for _ in 0..watchdog_timeout_ms {
            if watchdog_done.load(Ordering::SeqCst) {
                return None;
            }
            thread::sleep(Duration::from_millis(1));
        }
        let mut vcpu = vcpu_for_watchdog;
        Some(unsafe { hv_vcpus_exit(&mut vcpu, 1) })
    });

    let run_status = unsafe { hv_vcpu_run(vcpu) };
    done.store(true, Ordering::SeqCst);
    let watchdog_cancel_status = watchdog.join().ok().flatten();

    let mut exit_reason = None;
    let mut exit_syndrome = None;
    let mut exit_virtual_address = None;
    let mut exit_physical_address = None;
    if run_status == HV_SUCCESS && !exit.is_null() {
        let exit_info = unsafe { &*exit };
        exit_reason = Some(exit_info.reason);
        exit_syndrome = Some(exit_info.exception.syndrome);
        exit_virtual_address = Some(exit_info.exception.virtual_address);
        exit_physical_address = Some(exit_info.exception.physical_address);
    }

    VcpuRunObservation {
        run_status,
        exit_reason,
        exit_syndrome,
        exit_virtual_address,
        exit_physical_address,
        watchdog_cancel_status,
    }
}

pub(crate) fn recommended_vector_base_vbar_redirect_target(
    exit: &WindowsArmUefiFirmwareRunLoopExit,
) -> Option<&WindowsArmUefiVectorBaseRecommendation> {
    exit.stage1_executable_candidates_after_exit
        .iter()
        .find_map(|candidate| candidate.recommended_vector_base_candidate.as_ref())
}

pub(crate) fn low_vector_recommended_vector_remap_target(
    exit: &WindowsArmUefiFirmwareRunLoopExit,
) -> Option<&WindowsArmUefiVectorBaseRecommendation> {
    recommended_vector_base_vbar_redirect_target(exit)
        .filter(|recommendation| recommendation.is_populated_low_vector_remap_target())
}

pub(crate) fn exception_class(syndrome: u64) -> u64 {
    syndrome >> 26
}

pub(crate) fn is_data_abort_syndrome(syndrome: u64) -> bool {
    matches!(exception_class(syndrome), 0x24 | 0x25)
}
