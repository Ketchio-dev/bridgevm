use super::*;

fn reason(watchdog: bool, stall: bool, diagnostic: bool) -> Option<&'static str> {
    reason_with_term(watchdog, stall, diagnostic, false)
}

fn reason_with_term(
    watchdog: bool,
    stall: bool,
    diagnostic: bool,
    term: bool,
) -> Option<&'static str> {
    let stall = Arc::new(AtomicBool::new(stall));
    let diagnostic = Arc::new(AtomicBool::new(diagnostic));
    let term = AtomicBool::new(term);
    cancel_stop_reason(
        &AtomicBool::new(watchdog),
        Some(&stall),
        Some(&diagnostic),
        &term,
    )
}

#[test]
fn a_cancel_with_no_flag_set_is_benign() {
    assert_eq!(reason(false, false, false), None);
}

#[test]
fn each_flag_names_itself() {
    assert_eq!(reason(true, false, false), Some("watchdog (CANCELED)"));
    assert_eq!(
        reason(false, true, false),
        Some("boot-progress stall (kill mode)")
    );
    assert_eq!(
        reason(false, false, true),
        Some("host diagnostic stop requested")
    );
    assert_eq!(
        reason_with_term(false, false, false, true),
        Some("host stop requested (SIGTERM)")
    );
}
