#[test]
fn install_commands_forward_exact_ids_and_namespace_options() {
    for verb in ["install", "install-status", "install-cancel"] {
        for arguments in [
            vec!["bridgevm", "app", verb, "개발-vm", "--library", "/owned library", "--json"],
            vec!["bridgevm", "app", "--json", "--library", "/owned library", verb, "개발-vm"],
        ] {
            let cli = Cli::try_parse_from(arguments).unwrap();
            let Command::App(args) = cli.command else { panic!("expected native app command") };
            assert_eq!(native_arguments(args), ["--cli", verb, "개발-vm", "--library",
                "/owned library", "--json"].map(OsString::from));
        }
    }
}

#[test]
fn install_variants_add_no_secret_or_media_arguments() {
    for (command, verb) in [
        (AppCommand::Install { id: "vm".into() }, "install"),
        (AppCommand::InstallStatus { id: "vm".into() }, "install-status"),
        (AppCommand::InstallCancel { id: "vm".into() }, "install-cancel"),
    ] {
        assert_eq!(native_arguments(AppArgs { command, library: None, json: false }),
            ["--cli", verb, "vm"].map(OsString::from));
    }
}
