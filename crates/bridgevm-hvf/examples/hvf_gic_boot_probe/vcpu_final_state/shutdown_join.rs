//! Stop every secondary owner before reporting or tearing down guest RAM.

use crate::*;

impl SecondaryVcpuSet {
    fn join_owners(&mut self) -> thread::Result<()> {
        if self.handles.is_empty() {
            return Ok(());
        }
        self.shutdown.store(true, Ordering::SeqCst);
        for control in &self.controls {
            control.notify_shutdown();
        }
        for control in &self.controls {
            control.request_exit_if_published();
        }
        // Interpret failures only after every owner is joined. An early panic
        // must not detach later owners while the caller tears down guest RAM.
        let mut outcome = Ok(());
        for handle in std::mem::take(&mut self.handles) {
            let result = handle.join();
            if outcome.is_ok() {
                outcome = result;
            }
        }
        outcome
    }

    /// A panicked owner is a fatal run error like any other, not a panic of
    /// the stop path: the stop save (UEFI vars, final F_FULLFSYNC) still runs.
    pub(crate) fn shutdown_and_join(mut self) -> SecondaryVcpuStopResult {
        let panicked = self.join_owners().is_err();
        if panicked {
            // The panic hook already logged the message.
            println!("secondary vCPU thread panicked; stopping the VM");
        }
        let run_error = panicked
            || self
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

impl Drop for SecondaryVcpuSet {
    fn drop(&mut self) {
        // A primary panic may already be unwinding. Join every owner, but leave
        // panic reporting to the explicit stop path rather than double-panicking.
        let _ = self.join_owners();
    }
}

#[cfg(test)]
#[path = "shutdown_join_tests.rs"]
mod tests;
