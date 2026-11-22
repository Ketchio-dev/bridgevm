#[path = "support/help_fixture.rs"]
mod help_fixture;
use help_fixture::Fixture;

#[test]
fn top_level_help_lists_app_and_own_hvf_routes() {
    let fixture = Fixture::new();
    for flag in ["--help", "-h"] {
        let help = fixture.help(&[flag]);
        for expected in [
            "bridgevm app list",
            "bridgevm hvf host-capabilities",
            "bridgevm hvf machine-plan",
        ] {
            assert!(help.contains(expected), "Missing {expected}: {help}");
        }
        assert!(
            !help.contains("--store"),
            "Legacy store flag in help: {help}"
        );
    }
}

#[test]
fn hvf_help_and_parser_preserve_explicit_opt_ins() {
    let fixture = Fixture::new();
    let help = fixture.help(&["hvf", "--help"]);
    assert!(help.contains("local-only queries and opt-in probes"));
    assert!(help.contains("host-capabilities") && help.contains("vm-probe"));
    let probe = fixture.help(&["hvf", "vm-probe", "--allow-create", "--help"]);
    assert!(probe.contains("--allow-create"));
    for command in ["virtio-gpu-trace-report", "title-gate-report"] {
        assert!(fixture
            .help(&["hvf", command, "--help"])
            .contains("--trace"));
    }
    for args in [
        &["start"][..],
        &["list"][..],
        &["--store", "/nonexistent", "hvf", "host-capabilities"][..],
        &["hvf", "vm-probe", "--not-a-real-option"][..],
    ] {
        let output = fixture.invoke(args);
        assert_eq!(output.status.code(), Some(2), "Parser contract: {args:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("error:"));
    }
}
