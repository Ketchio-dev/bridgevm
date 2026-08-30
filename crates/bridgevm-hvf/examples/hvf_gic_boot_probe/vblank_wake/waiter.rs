//! Deadline-aware parked host thread for vblank completion wakes.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bridgevm_hvf::virtio_gpu::VblankWakeState;

use crate::{hv_vcpus_exit, HvVcpuT};

const ACTIVE_POLL: Duration = Duration::from_millis(2);

pub(super) fn run(vcpu: HvVcpuT, state: Arc<VblankWakeState>, fired: Arc<AtomicBool>) -> ! {
    state.register_current_thread();
    loop {
        // Never fire while a previous fire is unconsumed: each forced exit
        // must map 1:1 onto one fired claim in the exit dispatcher.
        if fired.load(Ordering::SeqCst) {
            std::thread::park();
            continue;
        }
        let Some(remaining) = state.time_to_deadline(Instant::now()) else {
            std::thread::park();
            continue;
        };
        if !remaining.is_zero() {
            std::thread::sleep(remaining.min(ACTIVE_POLL));
            continue;
        }
        fired.store(true, Ordering::SeqCst);
        // SAFETY: Category 8 - `vcpu` is the live HVF vCPU handle owned by the
        // probe loop and the pointer is valid for this synchronous call.
        unsafe { hv_vcpus_exit(&vcpu, 1) };
    }
}
