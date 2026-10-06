//! Failure propagation between a secondary owner and the primary run loop.

use crate::*;

#[path = "run_failure_error.rs"]
mod error;
pub(crate) use error::PrimaryRunError;

impl VcpuControl {
    pub(crate) fn record_run_error(&self, primary_vcpu: HvVcpuT) {
        // CPU0 owns this handle until all secondary threads are joined. Publish
        // first so a concurrent primary exit observes the error, even if this
        // wake request fails. Do not report a wake status as hv_vcpu_run's status.
        let status = self.record_run_error_with(|| unsafe { hv_vcpus_exit(&primary_vcpu, 1) });
        if status != 0 {
            println!(
                "secondary vCPU{} fatal-error wake failed {status:#x}",
                self.index
            );
        }
    }

    pub(super) fn record_run_error_with(
        &self,
        wake_primary: impl FnOnce() -> HvReturn,
    ) -> HvReturn {
        self.run_error.store(true, Ordering::SeqCst);
        wake_primary()
    }
}

impl SecondaryVcpuSet {
    fn failed_cpu(&self) -> Option<u64> {
        self.controls
            .iter()
            .find(|control| control.run_error.load(Ordering::SeqCst))
            .map(|control| control.index)
    }
}

pub(crate) fn run_hvf_vcpu_once(
    vcpu: HvVcpuT,
    exit: *mut HvVcpuExit,
    secondaries: Option<&SecondaryVcpuSet>,
) -> Result<u32, PrimaryRunError> {
    run_primary_with_secondary_check(secondaries, || {
        let status = unsafe { hv_vcpu_run(vcpu) };
        if status != 0 {
            return Err(status);
        }
        Ok(unsafe { (*exit).reason })
    })
}

pub(super) fn run_primary_with_secondary_check(
    secondaries: Option<&SecondaryVcpuSet>,
    run: impl FnOnce() -> Result<u32, HvReturn>,
) -> Result<u32, PrimaryRunError> {
    if let Some(index) = secondaries.and_then(SecondaryVcpuSet::failed_cpu) {
        return Err(PrimaryRunError::Secondary(index));
    }
    let reason = run().map_err(PrimaryRunError::Hypervisor)?;
    if let Some(index) = secondaries.and_then(SecondaryVcpuSet::failed_cpu) {
        return Err(PrimaryRunError::Secondary(index));
    }
    Ok(reason)
}

#[cfg(test)]
#[path = "run_failure_tests.rs"]
mod tests;
