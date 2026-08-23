//! Wake the vCPU only when the host appends to the live-input control file.

use std::path::PathBuf;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
use std::time::Duration;
use crate::{hv_vcpus_exit, HvVcpuT, EXIT_CANCELED};

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
            let mut schedule = None;
            loop {
                let now = std::time::Instant::now();
                if input_length_changed(&mut length, file_length(&path)) {
                    schedule = Some((now, now + super::input_control_schedule::BURST));
                }
                if super::input_control_schedule::wake_due(fired.load(Ordering::SeqCst), now, schedule) {
                    fired.store(true, Ordering::SeqCst);
                    schedule = schedule.map(|(_, until)| (now + super::input_control_schedule::CADENCE, until));
                    unsafe { hv_vcpus_exit(&vcpu, 1) };
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

fn file_length(path: &PathBuf) -> Option<u64> { std::fs::metadata(path).ok().map(|m| m.len()) }
fn input_length_changed(previous: &mut u64, observed: Option<u64>) -> bool {
    let Some(observed) = observed else { return false; };
    if observed == *previous { return false; }
    *previous = observed; true
}

#[cfg(test)]
#[test]
fn only_real_control_file_length_changes_start_a_burst() {
    let mut length = 0;
    assert!(!input_length_changed(&mut length, None));
    assert!(!input_length_changed(&mut length, Some(0)));
    assert!(input_length_changed(&mut length, Some(31)));
    assert!(!input_length_changed(&mut length, Some(31)));
    assert!(input_length_changed(&mut length, Some(63)));
}
