//! Runtime source slices included by the existing acquisition-coverage regression.

pub(super) fn runtime() -> String {
    [
        include_str!("probe_runtime.rs"),
        include_str!("probe_runtime/automation_tick.rs"),
        include_str!("probe_runtime/input_prepare.rs"),
        include_str!("probe_runtime/automation_stop.rs"),
    ]
    .concat()
}
