//! Keep prelude diagnostics ordered, then release the platform before input-file access.

macro_rules! prepare_live_input {
    ($platform:ident, $smp_trace:ident, $automation_gate:ident, $live_input:ident, $input_wake:ident $(,)?) => {{
        {
            let platform_guard =
                lock_platform(&$platform, $smp_trace.as_deref(), 0, HoldSite::Automation);
            $automation_gate.note_checked();
            if let Some(deadline) = platform_guard.xhci_pointer_report_deadline() {
                pointer_deadline_trace::report_overdue(deadline);
            }
        }
        $live_input.prepare_tick(std::time::Instant::now(), $input_wake)
    }};
}
pub(crate) use prepare_live_input;
