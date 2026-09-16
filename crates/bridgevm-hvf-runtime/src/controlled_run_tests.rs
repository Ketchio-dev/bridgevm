use super::*;
use crate::{owned_control::test_support::Fixture, prepare};
use std::cell::Cell;

#[test]
fn pulse_cancellation_during_first_generation_never_launches_another_generation() {
    let fixture = Fixture::new();
    let marker = fixture.root.join("generations");
    let helper = fixture.script(
        "reset.sh",
        &format!(
            "printf '%s\\n' \"$BRIDGEVM_RESET_GENERATION\" >> '{}'; exit 42",
            marker.display()
        ),
    );
    let prepared = prepare(fixture.manifest(), "cancel reset fixture").unwrap();
    let pulsed = Cell::new(false);
    let requested =
        || std::fs::read_to_string(&marker).ok().as_deref() == Some("0\n") && !pulsed.replace(true);
    let control = RuntimeControl::new(&requested, &|_| panic!("unexpected uncertainty"));
    let outcome = run_prepared_vm(
        &prepared,
        &fixture.launch(helper),
        &fixture.root.join("receipt"),
        4,
        &control,
    )
    .unwrap();
    assert_eq!(outcome.termination, RunTermination::Cancelled);
    assert!(control.is_cancelled());
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "0\n");
    assert!(!fixture.root.join("receipt").exists());
    assert_eq!(prepared.generation().stamp().value(), 0);
    assert!(prepare(fixture.manifest(), "competitor").is_err());
    drop(prepared);
    assert!(prepare(fixture.manifest(), "after cleanup").is_ok());
}

#[test]
fn cancellation_after_flushed_reset_blocks_the_next_generation() {
    let fixture = Fixture::new();
    let marker = fixture.root.join("generations");
    let helper = fixture.script(
        "reset.sh",
        &format!(
            "printf '%s\\n' \"$BRIDGEVM_RESET_GENERATION\" >> '{}'; exit 42",
            marker.display()
        ),
    );
    let prepared = prepare(fixture.manifest(), "post-reset fixture").unwrap();
    let pulsed = Cell::new(false);
    let requested = || prepared.generation().stamp().value() > 0 && !pulsed.replace(true);
    let control = RuntimeControl::new(&requested, &|_| panic!("unexpected uncertainty"));
    let receipt = fixture.root.join("receipt");
    let outcome =
        run_prepared_vm(&prepared, &fixture.launch(helper), &receipt, 4, &control).unwrap();
    assert_eq!(outcome.termination, RunTermination::Cancelled);
    assert!(control.is_cancelled());
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "0\n");
    assert!(std::fs::read_to_string(receipt)
        .unwrap()
        .contains("generation: 0"));
    assert_eq!(outcome.cycles.len(), 1);
    assert_eq!(prepared.generation().stamp().value(), 1);
    assert!(prepare(fixture.manifest(), "competitor").is_err());
}

#[test]
fn cancellation_before_first_spawn_preserves_media_and_has_no_helper_effect() {
    let fixture = Fixture::new();
    let marker = fixture.root.join("unexpected-child");
    let helper = fixture.script("never.sh", &format!("touch '{}'", marker.display()));
    let prepared = prepare(fixture.manifest(), "pre-cancel fixture").unwrap();
    let outcome = run_prepared_vm(
        &prepared,
        &fixture.launch(helper),
        &fixture.root.join("receipt"),
        4,
        &RuntimeControl::new(&|| true, &|_| panic!("unexpected uncertainty")),
    )
    .unwrap();
    assert_eq!(outcome.termination, RunTermination::Cancelled);
    assert!(outcome.cycles.is_empty());
    assert!(!marker.exists());
    assert!(prepare(fixture.manifest(), "competitor").is_err());
}
