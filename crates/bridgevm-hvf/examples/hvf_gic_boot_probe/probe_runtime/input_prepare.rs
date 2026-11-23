//! Keep prelude diagnostics ordered, then release the platform before input-file access.
//! The prelude prints how late a queued DCI5 report is against its own deadline.

use std::time::Instant;

macro_rules! prepare_live_input {
    ($platform:ident, $smp_trace:ident, $automation_gate:ident, $live_input:ident, $input_wake:ident $(,)?) => {{
        {
            let platform_guard =
                lock_platform(&$platform, $smp_trace.as_deref(), 0, HoldSite::Automation);
            $automation_gate.note_checked();
            if let Some(deadline) = platform_guard.xhci_pointer_report_deadline() {
                input_prepare::report_overdue(deadline);
            }
        }
        $live_input.prepare_tick(std::time::Instant::now(), $input_wake)
    }};
}
pub(crate) use prepare_live_input;

/// A report still queued past its deadline means the automation pass that
/// would emit it ran late, which separates a missed host wake from a guest TD
/// that was not ready. Emission timestamps alone cannot tell those apart.
pub(crate) fn report_overdue(deadline: Instant) {
    let late = Instant::now().saturating_duration_since(deadline);
    if late.is_zero() || !enabled() {
        return;
    }
    println!(
        "xHCI dci5 pointer report overdue late_us={}",
        late.as_micros()
    );
}

fn enabled() -> bool {
    matches!(
        std::env::var("BRIDGEVM_TRACE_DCI5_EMISSION").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    )
}
