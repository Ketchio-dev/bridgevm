use super::*;

fn event(kind: EventKind, cpu: u64) -> SmpEvent {
    SmpEvent {
        kind,
        cpu,
        a: 0,
        b: 0,
    }
}

#[test]
fn an_empty_ring_reports_nothing_and_no_overflow() {
    let ring = EventRing::new();
    assert!(ring.drain().is_empty());
    assert_eq!(ring.overflow(), 0);
}

#[test]
fn events_come_back_in_the_order_they_happened() {
    let ring = EventRing::new();
    for cpu in 0..5 {
        ring.record(event(EventKind::Progress, cpu));
    }
    let drained = ring.drain();
    assert_eq!(drained.len(), 5);
    assert_eq!(
        drained.iter().map(|e| e.cpu).collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4]
    );
    assert_eq!(ring.overflow(), 0);
}

#[test]
fn the_ring_holds_exactly_its_capacity_without_overflow() {
    let ring = EventRing::new();
    for cpu in 0..RING_CAPACITY as u64 {
        ring.record(event(EventKind::Progress, cpu));
    }
    assert_eq!(ring.drain().len(), RING_CAPACITY);
    assert_eq!(
        ring.overflow(),
        0,
        "filling to capacity is not yet an overflow"
    );
}

#[test]
fn overwritten_events_are_counted_not_silently_dropped() {
    // A trace that quietly loses events invites conclusions drawn from an
    // incomplete record. Overflow must be visible in the receipt.
    let ring = EventRing::new();
    for cpu in 0..(RING_CAPACITY as u64 + 10) {
        ring.record(event(EventKind::Progress, cpu));
    }
    assert_eq!(ring.overflow(), 10);
    assert_eq!(ring.drain().len(), RING_CAPACITY);
}

#[test]
fn after_wrapping_the_oldest_retained_event_is_returned_first() {
    let ring = EventRing::new();
    for cpu in 0..(RING_CAPACITY as u64 + 3) {
        ring.record(event(EventKind::Progress, cpu));
    }
    let drained = ring.drain();
    assert_eq!(drained.first().unwrap().cpu, 3, "0..2 were overwritten");
    assert_eq!(
        drained.last().unwrap().cpu,
        RING_CAPACITY as u64 + 2,
        "the newest event is retained"
    );
}

#[test]
fn recording_is_safe_from_several_threads_and_loses_no_event() {
    let ring = std::sync::Arc::new(EventRing::new());
    let mut handles = Vec::new();
    for cpu in 0..4u64 {
        let ring = std::sync::Arc::clone(&ring);
        handles.push(std::thread::spawn(move || {
            for _ in 0..100 {
                ring.record(event(EventKind::Progress, cpu));
            }
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(ring.drain().len() as u64 + ring.overflow(), 400);
}

#[test]
fn every_event_kind_renders_without_panicking() {
    let kinds = [
        EventKind::StateTransition,
        EventKind::SecondaryCreated,
        EventKind::SecondaryWaitingOff,
        EventKind::SecondaryWoke,
        EventKind::RunLoopEntered,
        EventKind::PreRunDrain,
        EventKind::PostRunDrain,
        EventKind::RunResult,
        EventKind::Progress,
        EventKind::LockWait,
        EventKind::LockAcquired,
    ];
    for kind in kinds {
        let rendered = SmpEvent {
            kind,
            cpu: 1,
            a: 2,
            b: 3,
        }
        .render();
        assert!(!rendered.is_empty(), "{kind:?} rendered empty");
        assert!(!kind.as_str().is_empty());
    }
}

#[test]
fn state_transitions_render_psci_names_rather_than_raw_numbers() {
    let rendered = SmpEvent {
        kind: EventKind::StateTransition,
        cpu: 2,
        a: 0,
        b: 1,
    }
    .render();
    assert_eq!(rendered, "vCPU2 Off -> OnPending");
}
