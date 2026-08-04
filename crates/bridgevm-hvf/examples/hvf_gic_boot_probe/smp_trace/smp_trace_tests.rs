use super::*;
use super::event_ring::lock_id;

#[test]
fn a_new_trace_has_recorded_nothing() {
    let trace = SmpTrace::new();
    assert_eq!(trace.trace_overflow(), 0);
    assert!(trace.ring.drain().is_empty());
}

#[test]
fn a_state_transition_is_stored_as_numbers_not_text() {
    let trace = SmpTrace::new();
    trace.state_transition(3, PsciState::Off, PsciState::OnPending);
    let events = trace.ring.drain();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, EventKind::StateTransition);
    assert_eq!(events[0].cpu, 3);
    assert_eq!(events[0].a, 0, "Off");
    assert_eq!(events[0].b, 1, "OnPending");
}

#[test]
fn progress_is_recorded_only_at_the_interval() {
    let trace = SmpTrace::new();
    for exits in 1..SMP_TRACE_PROGRESS_INTERVAL {
        trace.cpu0_progress(exits);
    }
    assert!(
        trace.ring.drain().is_empty(),
        "sub-interval progress must not fill the ring"
    );
    trace.cpu0_progress(SMP_TRACE_PROGRESS_INTERVAL);
    assert_eq!(trace.ring.drain().len(), 1);
}

#[test]
fn progress_still_updates_the_counters_between_intervals() {
    // The counters are read by the final report, so they must stay live even
    // when no event is recorded.
    let trace = SmpTrace::new();
    trace.cpu0_progress(7);
    assert_eq!(trace.cpu0_exits.load(Ordering::Relaxed), 7);
    trace.secondary_progress();
    assert_eq!(trace.secondary_exits.load(Ordering::Relaxed), 1);
}

#[test]
fn early_drain_events_are_recorded_and_later_ones_are_not() {
    let trace = SmpTrace::new();
    for exit in 0..12 {
        trace.secondary_pre_run_drain(1, exit, 0x1000 + exit);
    }
    assert_eq!(
        trace.ring.drain().len(),
        10,
        "only the first ten runs are traced"
    );
}

#[test]
fn overflow_is_reported_so_a_truncated_trace_is_never_read_as_complete() {
    let trace = SmpTrace::new();
    for cpu in 0..(event_ring::RING_CAPACITY as u64 + 5) {
        trace.state_transition(cpu, PsciState::Off, PsciState::On);
    }
    assert_eq!(trace.trace_overflow(), 5);
}

#[test]
fn the_record_path_contains_no_formatting_or_stdout_write() {
    // This is the property that matters: the a1-smp investigation measured a
    // severe observer effect from println! inside the vCPU run loops. Guard it
    // by reading the source of the record path rather than trusting review.
    let source = include_str!("../smp_trace.rs");
    let body = source
        .split("pub(crate) fn dump(&self)")
        .nth(1)
        .and_then(|rest| rest.split_once("\n    }"))
        .map(|(dump, after)| {
            // Everything after `dump` is the record path; `dump` itself is
            // allowed to format because it runs after the vCPUs have stopped.
            let _ = dump;
            after.to_string()
        })
        .expect("smp_trace.rs must define dump");

    for forbidden in ["println!", "format!", "eprintln!", "to_string()"] {
        assert!(
            !body.contains(forbidden),
            "{forbidden} must not appear on the SMP trace record path"
        );
    }
}

#[test]
fn lock_identities_round_trip_through_their_numeric_form() {
    for name in ["platform mutex", "VcpuControl.state mutex"] {
        assert_eq!(lock_name(lock_id(name)), name);
    }
    assert_eq!(lock_name(lock_id("something else")), "lock");
}

#[test]
fn an_uncontended_lock_records_nothing() {
    let trace = SmpTrace::new();
    let mutex = Mutex::new(0u8);
    let guard = trace.lock_with_wait_trace(0, "platform mutex", "test", &mutex);
    drop(guard);
    assert!(
        trace.ring.drain().is_empty(),
        "only slow acquisitions are worth recording"
    );
}
