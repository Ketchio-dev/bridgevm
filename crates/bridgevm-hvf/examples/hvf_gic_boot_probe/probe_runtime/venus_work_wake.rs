//! Give direct Venus feedback a short vCPU0 drain window after real GPU work.

use super::{hv_vcpus_exit, HvVcpuT, VirtPlatform};
use crate::pause_gate::GATE;
use bridgevm_hvf::virtio_gpu_3d::VenusWorkWake;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const KICK_INTERVAL: Duration = Duration::from_millis(2);

pub(super) struct Worker {
    activity: Arc<VenusWorkWake>,
    stopped: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    pub(super) fn start(vcpu: HvVcpuT, platform: &mut VirtPlatform) -> Option<Self> {
        let activity = Arc::new(VenusWorkWake::default());
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_activity = Arc::clone(&activity);
        let worker_stopped = Arc::clone(&stopped);
        let thread = thread::Builder::new()
            .name("venus-work-wake".into())
            .spawn(move || run(vcpu, &worker_activity, &worker_stopped));
        match thread {
            Ok(thread) => {
                platform.install_venus_work_wake(Arc::clone(&activity));
                Some(Self {
                    activity,
                    stopped,
                    thread: Some(thread),
                })
            }
            Err(error) => {
                eprintln!("virtio-gpu: could not start the Venus feedback worker: {error}");
                None
            }
        }
    }

    pub(super) fn clear(&self) {
        self.activity.clear();
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
            let _ = thread.join();
        }
    }
}

fn run(vcpu: HvVcpuT, activity: &VenusWorkWake, stopped: &AtomicBool) {
    activity.register_waiter();
    let mut next_kick = None;
    while !stopped.load(Ordering::Acquire) {
        let now = Instant::now();
        let Some(remaining) = activity.remaining(now) else {
            next_kick = None;
            thread::park();
            continue;
        };
        let due = *next_kick.get_or_insert(now + KICK_INTERVAL);
        if now < due {
            thread::park_timeout((due - now).min(remaining));
            continue;
        }
        if !GATE.paused() {
            // SAFETY: the owner joins this worker before destroying vCPU0.
            // Apple permits requesting a vCPU exit from another thread.
            unsafe { hv_vcpus_exit(&vcpu, 1) };
        }
        next_kick = Some(now + KICK_INTERVAL);
    }
}
