//! Print-only lateness at the common queued-pointer drain boundary.

use std::time::Instant;

pub(super) fn observe(deadline: Option<Instant>) {
    let Some(deadline) = deadline else {
        super::xhci_pointer_overdue_state::first(None);
        return;
    };
    let Some(late_us) = lateness_us(Instant::now(), deadline) else {
        return;
    };
    if enabled() && super::xhci_pointer_overdue_state::first(Some(deadline)).is_some() {
        println!("xHCI dci5 pointer report overdue late_us={late_us}");
    }
}

pub(super) fn lateness_us(now: Instant, deadline: Instant) -> Option<u128> {
    let late = now.saturating_duration_since(deadline);
    (!late.is_zero()).then_some(late.as_micros())
}

fn enabled() -> bool {
    matches!(
        std::env::var("BRIDGEVM_TRACE_DCI5_EMISSION")
            .ok()
            .as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    )
}
