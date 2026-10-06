//! Stop every secondary owner before propagating an owner panic.

use crate::*;

impl SecondaryVcpuSet {
    pub(crate) fn shutdown_and_join(self) -> SecondaryVcpuStopResult {
        self.shutdown.store(true, Ordering::SeqCst);
        for control in &self.controls {
            control.notify_shutdown();
        }
        for control in &self.controls {
            control.request_exit_if_published();
        }
        // Interpret failures only after every owner is joined. An early panic
        // must not detach later owners while the caller tears down guest RAM.
        let joined: Vec<_> = self.handles.into_iter().map(JoinHandle::join).collect();
        for result in joined {
            result.expect("join secondary vCPU thread");
        }
        let run_error = self
            .controls
            .iter()
            .any(|control| control.run_error.load(Ordering::SeqCst));
        let exit_counts = self
            .controls
            .iter()
            .map(|control| (control.index, control.exits.load(Ordering::SeqCst)))
            .collect();
        let mut final_states = Vec::new();
        let mut missing_final_states = Vec::new();
        for control in &self.controls {
            if let Some(state) = control.take_final_state() {
                final_states.push(state);
            } else if control.created.load(Ordering::Acquire) {
                missing_final_states.push(control.index);
            }
        }
        SecondaryVcpuStopResult {
            exit_counts,
            run_error,
            final_states,
            missing_final_states,
        }
    }
}

#[cfg(test)]
#[path = "shutdown_join_tests.rs"]
mod tests;
