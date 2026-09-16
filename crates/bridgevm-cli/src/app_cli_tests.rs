use super::*;

#[test]
fn forwards_unicode_and_spaces_as_single_arguments() {
    for (command, verb) in [
        (
            AppCommand::Inspect {
                id: "개발-vm".into(),
            },
            "inspect",
        ),
        (
            AppCommand::Readiness {
                id: "개발-vm".into(),
            },
            "readiness",
        ),
        (
            AppCommand::Status {
                id: "개발-vm".into(),
            },
            "status",
        ),
    ] {
        let args = AppArgs {
            command,
            library: Some(PathBuf::from("/owned library/with 'quotes'")),
            json: true,
        };
        assert_eq!(
            native_arguments(args),
            [
                "--cli",
                verb,
                "개발-vm",
                "--library",
                "/owned library/with 'quotes'",
                "--json"
            ]
            .map(OsString::from)
        );
    }
}

#[test]
fn default_library_and_text_queries_forward_no_extra_arguments() {
    for (command, expected) in [
        (AppCommand::List, vec!["--cli", "list"]),
        (
            AppCommand::Inspect { id: "vm".into() },
            vec!["--cli", "inspect", "vm"],
        ),
        (
            AppCommand::Readiness { id: "vm".into() },
            vec!["--cli", "readiness", "vm"],
        ),
        (
            AppCommand::Status {
                id: "removed-vm".into(),
            },
            vec!["--cli", "status", "removed-vm"],
        ),
    ] {
        assert_eq!(
            native_arguments(AppArgs {
                command,
                library: None,
                json: false
            }),
            expected.into_iter().map(OsString::from).collect::<Vec<_>>()
        );
    }
}

#[test]
fn status_accepts_namespace_options_before_or_after_its_exact_id() {
    for arguments in [
        vec![
            "bridgevm",
            "app",
            "status",
            "개발-vm",
            "--library",
            "/owned library",
            "--json",
        ],
        vec![
            "bridgevm",
            "app",
            "--json",
            "--library",
            "/owned library",
            "status",
            "개발-vm",
        ],
    ] {
        let cli = Cli::try_parse_from(arguments).unwrap();
        let Command::App(args) = cli.command else {
            panic!("expected native app command")
        };
        assert_eq!(
            native_arguments(args),
            [
                "--cli",
                "status",
                "개발-vm",
                "--library",
                "/owned library",
                "--json"
            ]
            .map(OsString::from)
        );
    }
}
