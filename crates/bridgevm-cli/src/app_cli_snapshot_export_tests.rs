#[test]
fn snapshot_export_forwards_exact_id_output_and_namespace_options() {
    for arguments in [
        vec!["bridgevm", "app", "snapshot-export", "개발-vm", "/external/current.snapshot", "--library", "/owned library", "--json"],
        vec!["bridgevm", "app", "--json", "--library", "/owned library", "snapshot-export", "개발-vm", "/external/current.snapshot"],
    ] {
        let cli = Cli::try_parse_from(arguments).unwrap();
        let Command::App(args) = cli.command else { panic!("expected native app command") };
        assert_eq!(
            native_arguments(args),
            ["--cli", "snapshot-export", "개발-vm", "/external/current.snapshot", "--library", "/owned library", "--json"].map(OsString::from)
        );
    }
}

#[test]
fn snapshot_export_rejects_relative_and_parent_traversal_outputs() {
    for output in ["relative.snapshot", "/tmp/../escape.snapshot"] {
        assert!(Cli::try_parse_from(["bridgevm", "app", "snapshot-export", "vm", output]).is_err());
    }
}
