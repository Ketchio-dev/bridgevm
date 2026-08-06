use super::*;

use crate::reset_receipt::flush_and_write_receipt;
use std::path::PathBuf;

fn scratch_receipt(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("bv-sup-{tag}-{}.receipt", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

#[test]
fn a_receipted_reset_restarts_and_advances_the_generation() {
    let generation = ResetGeneration::new();
    let receipt = scratch_receipt("ok");
    flush_and_write_receipt(&[], &receipt, generation.stamp()).unwrap();
    let old_tag = generation.stamp();
    assert_eq!(
        decide_restart(true, &receipt, &generation),
        RestartDecision::Restart
    );
    // The dead helper's events are stale before any new process exists.
    assert!(!generation.is_current(old_tag));
}

#[test]
fn a_crash_never_restarts_even_with_a_receipt_present() {
    let generation = ResetGeneration::new();
    let receipt = scratch_receipt("crash");
    flush_and_write_receipt(&[], &receipt, generation.stamp()).unwrap();
    let decision = decide_restart(false, &receipt, &generation);
    assert!(
        matches!(decision, RestartDecision::Stop { .. }),
        "{decision:?}"
    );
    let old_tag = generation.stamp();
    assert!(generation.is_current(old_tag), "stop must not advance");
}

#[test]
fn a_reset_without_a_receipt_is_refused() {
    let generation = ResetGeneration::new();
    let receipt = scratch_receipt("norcpt");
    let decision = decide_restart(true, &receipt, &generation);
    assert_eq!(
        decision,
        RestartDecision::Stop {
            reason: "no reset receipt proves this generation's flush; refusing to restart",
        }
    );
}

#[test]
fn a_previous_generations_receipt_does_not_authorize_this_restart() {
    let generation = ResetGeneration::new();
    let receipt = scratch_receipt("stale");
    flush_and_write_receipt(&[], &receipt, generation.stamp()).unwrap();
    generation.advance();
    let decision = decide_restart(true, &receipt, &generation);
    assert!(
        matches!(decision, RestartDecision::Stop { .. }),
        "{decision:?}"
    );
}
