#[path = "support/help_fixture.rs"]
mod help_fixture;
use help_fixture::Fixture;

#[test]
fn top_level_help_lists_app_and_own_hvf_routes() {
    let fixture = Fixture::new();
    for flag in ["--help", "-h"] {
        let help = fixture.help(&[flag]);
        for expected in ["bridgevm app list", "bridgevm hvf virtio-gpu-trace-report"] {
            assert!(help.contains(expected), "Missing {expected}: {help}");
        }
        assert!(
            !help.contains("--store"),
            "Legacy store flag in help: {help}"
        );
    }
}

#[test]
fn hvf_help_lists_the_reports_and_rejects_unknown_options() {
    let fixture = Fixture::new();
    let help = fixture.help(&["hvf", "--help"]);
    assert!(help.contains("local-only"));
    assert!(help.contains("virtio-gpu-trace-report") && help.contains("title-gate-report"));
    for command in ["virtio-gpu-trace-report", "title-gate-report"] {
        assert!(fixture
            .help(&["hvf", command, "--help"])
            .contains("--trace"));
    }
    for args in [
        &["start"][..],
        &["list"][..],
        &["--store", "/nonexistent", "hvf", "virtio-gpu-trace-report"][..],
        &["hvf", "vm-probe"][..],
        &["hvf", "virtio-gpu-trace-report", "--not-a-real-option"][..],
    ] {
        let output = fixture.invoke(args);
        assert_eq!(output.status.code(), Some(2), "Parser contract: {args:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("error:"));
    }
}
