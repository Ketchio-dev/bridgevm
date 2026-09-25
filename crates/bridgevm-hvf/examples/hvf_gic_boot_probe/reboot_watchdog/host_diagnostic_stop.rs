//! Opt-in host request that stops the probe through its normal final report.
//!
//! The request is observed only from a host filesystem path. It does not send
//! guest input or touch a device, and it is useful only after a gate has
//! already failed and needs the stopped-vCPU diagnostics before cleanup.

use super::boot_progress::{BootProgressWatchdog, PROGRESS_SAMPLE_INTERVAL};
use crate::{hv_vcpus_exit, HvVcpuT};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
#[path = "host_diagnostic_stop_request.rs"]
mod request;
#[derive(Clone)]
pub(crate) struct HostDiagnosticStop {
    vcpu: HvVcpuT,
    request_path: PathBuf,
    pub(crate) fired: Arc<AtomicBool>,
}
impl HostDiagnosticStop {
    fn claim_request(&self) -> Option<request::StopRequest> {
        if self.fired.load(Ordering::SeqCst) { return None; }
        let consumed = request::consume(&self.request_path)?;
        (!self.fired.swap(true, Ordering::SeqCst)).then_some(consumed)
    }

    fn fire_if_requested(&self) -> bool {
        let Some(consumed) = self.claim_request() else { return false; };
        let generation = std::env::var("BRIDGEVM_RESET_GENERATION").ok().and_then(|s| s.parse::<u64>().ok())
            .map(|n| n.to_string()).unwrap_or_else(|| "unavailable".into());
        let nonce = consumed.nonce().map_or_else(String::new, |value| format!(" nonce={value}"));
        println!("HOST-DIAGNOSTIC-STOP: generation={generation}{nonce} request consumed; ending run through final report");
        let vcpu = self.vcpu;
        // SAFETY: Category 8 - `vcpu` remains a live probe-owned HVF handle;
        // this call only asks its owning run thread to leave `hv_vcpu_run`.
        unsafe { hv_vcpus_exit(&vcpu, 1) };
        true
    }
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
