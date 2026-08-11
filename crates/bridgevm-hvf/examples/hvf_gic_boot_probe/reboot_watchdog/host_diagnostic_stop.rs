//! Opt-in host request that stops the probe through its normal final report.
//!
//! The request is observed only from a host filesystem path. It does not send
//! guest input or touch a device, and it is useful only after a gate has
//! already failed and needs the stopped-vCPU diagnostics before cleanup.

use super::boot_progress::{BootProgressWatchdog, PROGRESS_SAMPLE_INTERVAL};
use crate::{hv_vcpus_exit, HvVcpuT};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct HostDiagnosticStop {
    vcpu: HvVcpuT,
    request_path: PathBuf,
    pub(crate) fired: Arc<AtomicBool>,
}

impl HostDiagnosticStop {
    fn claim_request(&self) -> bool {
        request_is_regular_file(&self.request_path)
            && !self.fired.swap(true, Ordering::SeqCst)
    }

    fn fire_if_requested(&self) -> bool {
        if !self.claim_request() {
            return false;
        }
        println!(
            "HOST-DIAGNOSTIC-STOP: request observed; ending run through final report"
        );
        let vcpu = self.vcpu;
        // SAFETY: Category 8 - `vcpu` remains a live probe-owned HVF handle;
        // this call only asks its owning run thread to leave `hv_vcpu_run`.
        unsafe { hv_vcpus_exit(&vcpu, 1) };
        true
    }
}

fn request_is_regular_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
}

fn configured_path(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    value.map(PathBuf::from).filter(|path| path.is_absolute())
}

pub(crate) fn start_host_diagnostic_stop_watcher(
    vcpu: HvVcpuT,
    run: Arc<BootProgressWatchdog>,
) -> Option<Arc<AtomicBool>> {
    let request_path = configured_path(std::env::var_os(
        "BRIDGEVM_HOST_DIAGNOSTIC_STOP_REQUEST",
    ))?;
    println!("Host diagnostic stop request: {}", request_path.display());
    let stop = HostDiagnosticStop {
        vcpu,
        request_path,
        fired: Arc::new(AtomicBool::new(false)),
    };
    let fired = Arc::clone(&stop.fired);
    std::thread::spawn(move || {
        while run.is_armed() {
            std::thread::sleep(PROGRESS_SAMPLE_INTERVAL);
            if !run.is_armed() || stop.fire_if_requested() {
                return;
            }
        }
    });
    Some(fired)
}

#[cfg(test)]
#[path = "host_diagnostic_stop_tests.rs"]
mod tests;
