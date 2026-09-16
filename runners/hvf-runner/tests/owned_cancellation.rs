//! Harmless full typed-entry subprocess regressions, including actual signals.

use std::process::Command;

fn run(scenario: &str) {
    let output = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/owned_cancellation_fixture.py"
        ))
        .args([
            "--runner",
            env!("CARGO_BIN_EXE_hvf-runner"),
            "--scenario",
            scenario,
        ])
        .output()
        .expect("spawn private Python fixture");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn term_keeps_both_leases_through_helper_and_swtpm_teardown() {
    run("held");
}

#[test]
fn interrupt_escalates_ignored_term_then_reaps_exact_children() {
    run("ignore");
}

#[test]
fn cancellation_interrupts_swtpm_control_readiness_before_helper_spawn() {
    run("readiness");
}
