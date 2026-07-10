use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use super::{
    env_flag, hv_vcpus_exit,
    ramfb_dump::{self, RamfbSampleSchedule, RamfbShellObservation},
    HvVcpuT, EXIT_CANCELED,
};

pub struct RamfbSampleLoop {
    schedule: Option<RamfbSampleSchedule>,
    sample_until_complete: bool,
    start: Instant,
    tick_fired: Arc<AtomicBool>,
    tick_armed_for: Option<Duration>,
    shell_observed: bool,
}

pub enum RamfbSampleShellAction {
    Continue,
    StopNow { reason: &'static str },
}

impl RamfbSampleLoop {
    pub fn from_env() -> Self {
        let schedule = match RamfbSampleSchedule::from_env("BRIDGEVM_RAMFB_SAMPLE_MS") {
            Ok(schedule) => Some(schedule),
            Err(error) => {
                ramfb_dump::print_sample_rejection(&error);
                None
            }
        };
        Self {
            schedule,
            sample_until_complete: env_flag("BRIDGEVM_RAMFB_SAMPLE_UNTIL_COMPLETE"),
            start: Instant::now(),
            tick_fired: Arc::new(AtomicBool::new(false)),
            tick_armed_for: None,
            shell_observed: false,
        }
    }

    pub fn canceled_by_sample_tick(&self, exit_reason: u32, watchdog_fired: &AtomicBool) -> bool {
        exit_reason == EXIT_CANCELED
            && self.tick_fired.swap(false, Ordering::SeqCst)
            && self.sample_until_complete
            && self.shell_observed
            && !watchdog_fired.load(Ordering::SeqCst)
    }

    pub fn emit_due<F>(&mut self, vcpu: HvVcpuT, emit_checkpoint: F)
    where
        F: FnMut(&str),
    {
        let Some(elapsed) = Instant::now().checked_duration_since(self.start) else {
            return;
        };
        let Some(schedule) = &mut self.schedule else {
            return;
        };
        schedule.emit_due(elapsed, emit_checkpoint);
        if self.sample_until_complete && self.shell_observed && !schedule.is_complete() {
            if let Some(deadline) = schedule.next_deadline_after(elapsed) {
                self.arm_sample_tick(elapsed, deadline, vcpu);
            }
        }
    }

    /// Cheap pre-lock gate for scheduled framebuffer checkpoints. The caller
    /// can inspect this on every exit and acquire the platform mutex only when
    /// a checkpoint has actually become due.
    pub fn checkpoint_due_at(&self, now: Instant) -> bool {
        let Some(elapsed) = now.checked_duration_since(self.start) else {
            return false;
        };
        self.schedule
            .as_ref()
            .is_some_and(|schedule| schedule.has_due_checkpoint(elapsed))
    }

    pub fn observe_shell(&mut self, vcpu: HvVcpuT) -> RamfbSampleShellAction {
        let Some(schedule) = &self.schedule else {
            return RamfbSampleShellAction::StopNow {
                reason: "serial reached UEFI shell",
            };
        };
        match schedule.uefi_shell_observation(self.sample_until_complete, self.shell_observed) {
            RamfbShellObservation::ContinueSampling { message } => {
                if !self.shell_observed {
                    println!("{message}");
                    self.shell_observed = true;
                    if let Some(elapsed) = Instant::now().checked_duration_since(self.start) {
                        if let Some(deadline) = schedule.next_deadline_after(elapsed) {
                            self.arm_sample_tick(elapsed, deadline, vcpu);
                        }
                    }
                }
                RamfbSampleShellAction::Continue
            }
            RamfbShellObservation::StopNow { reason } => RamfbSampleShellAction::StopNow { reason },
        }
    }

    fn arm_sample_tick(&mut self, elapsed: Duration, deadline: Duration, vcpu: HvVcpuT) {
        if self.tick_armed_for == Some(deadline) {
            return;
        }
        self.tick_armed_for = Some(deadline);
        let delay = deadline.saturating_sub(elapsed);
        let tick_fired = Arc::clone(&self.tick_fired);
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            tick_fired.store(true, Ordering::SeqCst);
            let v = vcpu;
            // SAFETY: Category 8 - `v` is the live HVF vCPU handle owned by
            // the probe loop, and the pointer is valid for this synchronous
            // call that requests one vCPU to leave `hv_vcpu_run`.
            unsafe { hv_vcpus_exit(&v, 1) };
        });
    }
}
