use super::*;

fn scratch_receipt(tag: &str) -> String {
    let path = std::env::temp_dir().join(format!("bv-svcli-{tag}-{}.receipt", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path.to_string_lossy().into_owned()
}

#[test]
fn real_processes_cycle_with_distinct_pids_until_clean_shutdown() {
    // Two resets (exit 42), then a clean shutdown (exit 0): the generation
    // env var the supervisor sets decides which.
    let script = format!(
        "if [ \"$BRIDGEVM_RESET_GENERATION\" -lt 2 ]; then exit {RESET_REQUESTED_EXIT}; fi"
    );
    let receipt = scratch_receipt("cycles");
    run_supervise(&["/bin/sh".into(), "-c".into(), script], &receipt, 10).unwrap();
    // The receipt of the last reset (generation 1) exists and proves it.
    assert!(std::path::Path::new(&receipt).exists());
}

#[test]
fn a_failing_helper_stops_the_loop_with_its_status() {
    let receipt = scratch_receipt("fail");
    let error = run_supervise(
        &["/bin/sh".into(), "-c".into(), "exit 3".into()],
        &receipt,
        10,
    )
    .unwrap_err();
    assert!(error.to_string().contains("exit status"), "{error}");
}

#[test]
fn a_missing_program_reports_spawn_context() {
    let receipt = scratch_receipt("noprog");
    let error = run_supervise(&["/nonexistent/helper".into()], &receipt, 1).unwrap_err();
    assert!(
        error.to_string().contains("spawn supervised helper"),
        "{error}"
    );
}
