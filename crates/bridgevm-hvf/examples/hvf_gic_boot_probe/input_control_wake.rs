//! Wake the vCPU only when the host appends to the live-input control file.

use std::path::PathBuf;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
use std::time::{Duration, Instant};
use crate::xhci_hid_input::SetupInputHostWake;
use crate::{hv_vcpus_exit, watchdog_generation_matches, HvVcpuT, EXIT_CANCELED};
use bridgevm_hvf::platform_virt::VirtPlatform;
use std::sync::atomic::AtomicU64;

const POLL: Duration = Duration::from_millis(2);

pub struct InputControlWake {
    fired: Arc<AtomicBool>,
    started: bool,
}

impl InputControlWake {
    pub fn new() -> Self { Self { fired: Arc::new(AtomicBool::new(false)), started: false } }

    pub fn ensure_started(&mut self, vcpu: HvVcpuT) {
        if self.started { return; }
        let Some(path) = std::env::var_os("BRIDGEVM_INPUT_CONTROL").filter(|p| !p.is_empty()).map(PathBuf::from) else { return; };
        self.started = true;
        let fired = Arc::clone(&self.fired);
        std::thread::spawn(move || {
            let mut length = file_length(&path).unwrap_or(0);
            loop {
                if !fired.load(Ordering::SeqCst)
                    && input_length_changed(&mut length, file_length(&path))
                {
                    fired.store(true, Ordering::SeqCst);
                    exit_vcpu(vcpu);
                }
                std::thread::sleep(POLL);
            }
        });
    }

    pub fn canceled(&self, reason: u32, watchdog: &AtomicBool) -> bool {
        reason == EXIT_CANCELED && self.fired.swap(false, Ordering::SeqCst)
            && !watchdog.load(Ordering::SeqCst)
    }
}

/// Arm the shared one-shot host wake for the next queued DCI5 report so a
/// paced release lands on time instead of waiting for an unrelated exit.
pub struct PointerDeadlineWake {
    vcpu: HvVcpuT,
    generation: Arc<AtomicU64>,
    boot_generation: u64,
}

impl PointerDeadlineWake {
    #[rustfmt::skip]
    pub fn new(vcpu: HvVcpuT, generation: (&Arc<AtomicU64>, u64)) -> Self { Self { vcpu, generation: Arc::clone(generation.0), boot_generation: generation.1 } }

    pub fn arm(&self, platform: &VirtPlatform, wake: &mut SetupInputHostWake) {
        let Some(deadline) = platform.xhci_pointer_report_deadline() else { return; };
        super::pointer_deadline_trace::report_overdue(deadline);
        self.arm_at(deadline, wake);
    }

    /// Arm `deadline` and report whether this call installed the timer, so a
    /// caller can log its own trigger label.
    pub fn arm_at(&self, deadline: Instant, wake: &mut SetupInputHostWake) -> bool {
        let (generation, boot, vcpu) = (Arc::clone(&self.generation), self.boot_generation, self.vcpu);
        wake.arm(deadline, move || {
            if watchdog_generation_matches(&generation, boot) { exit_vcpu(vcpu); }
        })
    }
}
fn exit_vcpu(vcpu: HvVcpuT) { unsafe { hv_vcpus_exit(&vcpu, 1) }; }
fn file_length(path: &PathBuf) -> Option<u64> { std::fs::metadata(path).ok().map(|m| m.len()) }
fn input_length_changed(previous: &mut u64, observed: Option<u64>) -> bool {
    let Some(observed) = observed else { return false; };
    if observed == *previous { return false; }
    *previous = observed; true
}

#[cfg(test)]
#[test]
fn only_real_control_file_length_changes_request_a_wake() {
    let mut length = 0;
    assert!(!input_length_changed(&mut length, None));
    assert!(!input_length_changed(&mut length, Some(0)));
    assert!(input_length_changed(&mut length, Some(31)));
    assert!(!input_length_changed(&mut length, Some(31)));
    assert!(input_length_changed(&mut length, Some(63)));
}
