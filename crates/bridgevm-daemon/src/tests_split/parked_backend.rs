//! Stable fake backend lifetime and observable command completion for tests.

use super::wait::wait_up_to_ten_seconds;
use crate::{DaemonState, SupervisedBackend};
use std::process::{Command, Stdio};

pub(super) fn parked_test_backend() -> SupervisedBackend {
    let child = Command::new("cat").stdin(Stdio::piped()).spawn().unwrap();
    SupervisedBackend::new(child)
}

pub(super) fn wait_for_guest_tools_commands(state: &mut DaemonState, name: &str) -> bool {
    wait_up_to_ten_seconds(|| {
        state.reconcile_children().unwrap();
        state
            .children
            .get(name)
            .is_some_and(|backend| backend.guest_tools_commands.pending_count() == 0)
    })
}
