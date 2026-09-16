//! Exercise the same framed entry used by the native app, without a guest.
use std::process::Command;
fn run(scenario: &str) {
    let output = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/owned_protocol_fixture.py"
        ))
        .args([
            "--runner",
            env!("CARGO_BIN_EXE_hvf-runner"),
            "--scenario",
            scenario,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn stop_ack_precedes_both_reaps_and_media_release() {
    run("stop");
}
#[test]
fn owner_eof_cancels_with_output_pipe_still_open() {
    run("eof");
}
#[test]
fn every_reset_generation_has_a_matching_child_receipt() {
    run("reset");
}
#[test]
fn cancellation_during_readiness_preserves_actual_tpm_cleanup() {
    run("readiness");
}
#[test]
fn invalid_hello_cannot_create_children_leases_or_surfaces() {
    run("invalid");
}

#[test]
fn broken_output_pipe_does_not_sigpipe_runner_or_strand_children() {
    run("output");
}
#[test]
fn partial_control_frame_deadline_cancels_and_reaps_without_false_completion() {
    run("partial");
}

#[test]
fn partial_media_admission_produces_no_ready_and_releases_its_first_lease() {
    run("admission");
}
#[test]
fn partial_initial_handshake_expires_before_any_runtime_effect() {
    run("handshake");
}
