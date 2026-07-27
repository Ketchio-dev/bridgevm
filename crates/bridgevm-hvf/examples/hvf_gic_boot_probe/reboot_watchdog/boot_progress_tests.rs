//! Progress-rate watchdog tests.

use crate::*;

#[test]
fn a_guest_making_no_exits_is_stalled() {
    assert!(is_stalled(
        0,
        Duration::from_secs(1),
        DEFAULT_MIN_EXITS_PER_SEC
    ));
}

#[test]
fn the_healthy_live_rate_is_not_stalled() {
    // 77 exits/s, measured from p1-smoke-100443.
    assert!(!is_stalled(
        77,
        Duration::from_secs(1),
        DEFAULT_MIN_EXITS_PER_SEC
    ));
}

#[test]
fn even_the_stalled_runs_average_rate_is_above_the_floor() {
    // 29 exits/s, measured from p1-smoke2-103715. The floor deliberately
    // sits below this: the average over a whole run is not what trips the
    // watchdog, a sustained window of near-zero progress is. Encoding this
    // keeps a later "tuning" change from quietly turning slow into failed.
    assert!(!is_stalled(
        29,
        Duration::from_secs(1),
        DEFAULT_MIN_EXITS_PER_SEC
    ));
}

#[test]
fn a_zero_length_window_is_never_stalled() {
    assert!(!is_stalled(0, Duration::ZERO, DEFAULT_MIN_EXITS_PER_SEC));
}

#[test]
fn counters_start_at_zero_and_advance() {
    let w = BootProgressWatchdog::new();
    assert_eq!(w.exits(), 0);
    assert_eq!(w.reboots(), 0);
    w.record_exit();
    w.record_exit();
    w.record_reboot();
    assert_eq!(w.exits(), 2);
    assert_eq!(w.reboots(), 1);
}

#[test]
fn disarming_stops_the_sampler() {
    let w = BootProgressWatchdog::new();
    assert!(w.is_armed());
    w.disarm();
    assert!(!w.is_armed());
}

#[test]
fn a_persistent_stall_reports_only_once() {
    let w = BootProgressWatchdog::new();
    assert!(w.mark_fired());
    assert!(!w.mark_fired());
}

#[test]
fn suspect_names_when_the_guest_stopped() {
    let never_ran = BootProgressRecord {
        stalled_for: Duration::from_secs(120),
        exits_in_window: 0,
        total_exits: 0,
        reboots: 0,
    };
    assert!(never_ran
        .format("probe")
        .contains("suspect=guest-not-running"));

    let before_reboot = BootProgressRecord {
        stalled_for: Duration::from_secs(120),
        exits_in_window: 2,
        total_exits: 500,
        reboots: 0,
    };
    assert!(before_reboot
        .format("probe")
        .contains("suspect=stalled-before-first-reboot"));

    // The shape actually observed in p1-smoke2-103715: one reboot, then no
    // further progress for the rest of the budget.
    let between_stages = BootProgressRecord {
        stalled_for: Duration::from_secs(120),
        exits_in_window: 1,
        total_exits: 69429,
        reboots: 1,
    };
    assert!(between_stages
        .format("probe")
        .contains("suspect=stalled-between-boot-stages"));
}

#[test]
fn the_record_line_carries_the_reboot_count() {
    let record = BootProgressRecord {
        stalled_for: Duration::from_secs(120),
        exits_in_window: 1,
        total_exits: 69429,
        reboots: 1,
    };
    let line = record.format("probe");
    assert!(line.contains("stalled_for_ms=120000"));
    assert!(line.contains("total_exits=69429"));
    assert!(line.contains("reboots=1"));
}
