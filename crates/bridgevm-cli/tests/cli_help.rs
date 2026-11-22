#[path = "support/help_fixture.rs"]
mod help_fixture;
use help_fixture::Fixture;

#[test]
fn top_level_help_identifies_store_and_own_hvf_route() {
    let fixture = Fixture::new();
    for flag in ["--help", "-h"] {
        let help = fixture.help(&[flag]);
        for expected in [
            "manifest.yaml",
            "vm.json",
            "--store",
            "--socket",
            "bridgevm hvf virtio-gpu-trace-report",
        ] {
            assert!(help.contains(expected), "Missing {expected}: {help}");
        }
    }
}

#[test]
fn lifecycle_help_distinguishes_recording_from_execution() {
    let fixture = Fixture::new();
    let start = fixture.help(&["start", "--help"]);
    assert!(start.contains("metadata only") && start.contains("does not launch"));
    let restart = fixture.help(&["restart", "--help"]);
    assert!(restart.contains("does not relaunch"));
    let run = fixture.help(&["run", "--help"]);
    assert!(run.contains("dry run by default") && run.contains("--spawn"));
    let explicit = fixture.help(&["run", "example", "--spawn", "--help"]);
    assert_eq!(run, explicit);
    for command in ["stop", "suspend", "resume"] {
        assert!(fixture.help(&[command, "--help"]).contains("backend"));
    }
}

#[test]
fn hvf_help_lists_the_reports_and_rejects_unknown_options() {
    let fixture = Fixture::new();
    let help = fixture.help(&["hvf", "--help"]);
    assert!(help.contains("local-only"));
    assert!(help.contains("virtio-gpu-trace-report") && help.contains("title-gate-report"));
    for args in [
        &["start"][..],
        &["run", "--spawn"][..],
        &["hvf", "virtio-gpu-trace-report", "--not-a-real-option"][..],
    ] {
        let output = fixture.invoke(args);
        assert_eq!(output.status.code(), Some(2), "Parser contract: {args:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("error:"));
    }
}

#[test]
fn hvf_socket_refusal_names_the_local_only_route() {
    let fixture = Fixture::new();
    let socket = fixture.0.join("absent.sock");
    let trace = fixture.0.join("absent-trace.jsonl");
    let output = fixture.invoke(&[
        "--socket",
        socket.to_str().unwrap(),
        "hvf",
        "virtio-gpu-trace-report",
        "--trace",
        trace.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("local-only") && error.contains("omit --socket"));
    assert!(!error.contains("metadata-only"));
}
