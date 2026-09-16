use super::*;
use std::sync::mpsc::sync_channel;
fn stop(id: &str) -> Stop {
    Stop {
        schema_version: 1,
        kind: "stop".into(),
        run_token: "run".into(),
        sequence: 1,
        operation_id: id.into(),
    }
}
#[test]
fn repeats_keep_original_operation_and_ack_and_late_stop_cannot_follow_complete() {
    let (sender, receiver) = sync_channel(31);
    let mut state = State::default();
    state.stop(stop("one"), &sender);
    state.stop(stop("one"), &sender);
    assert_eq!(state.cause, Some("stopRequested"));
    assert_eq!(receiver.try_iter().count(), 1);
    state.stop(stop("two"), &sender);
    assert!(state.broken);
    assert_eq!(state.operation.as_deref(), Some("one"));
    let mut finished = State {
        finishing: Some(Instant::now()),
        ..State::default()
    };
    finished.stop(stop("late"), &sender);
    assert!(finished.operation.is_none());
    assert!(receiver.try_recv().is_err());
}
#[test]
fn saturated_output_cannot_prevent_cancellation_admission() {
    let (sender, _receiver) = sync_channel(1);
    sender.try_send(Event::new("ready")).unwrap();
    let mut state = State::default();
    state.stop(stop("one"), &sender);
    assert!(state.broken);
    assert_eq!(state.cause, Some("stopRequested"));
    assert_eq!(state.operation.as_deref(), Some("one"));
}
