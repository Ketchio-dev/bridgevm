use super::*;
#[test]
fn forwards_unicode_and_spaces_as_single_arguments() {
    for (command, verb) in [
        (
            AppQueryCommand::Inspect {
                id: "개발-vm".into(),
            },
            "inspect",
        ),
        (
            AppQueryCommand::Readiness {
                id: "개발-vm".into(),
            },
            "readiness",
        ),
        (
            AppQueryCommand::Status {
                id: "개발-vm".into(),
            },
            "status",
        ),
    ] {
        let args = AppArgs {
            command: AppCommand::Query(command),
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
    let removed = AppQueryCommand::Status {
        id: "removed-vm".into(),
    };
    for (command, expected) in [
        (AppQueryCommand::List, vec!["--cli", "list"]),
        (
            AppQueryCommand::Inspect { id: "vm".into() },
            vec!["--cli", "inspect", "vm"],
        ),
        (
            AppQueryCommand::Readiness { id: "vm".into() },
            vec!["--cli", "readiness", "vm"],
        ),
        (removed, vec!["--cli", "status", "removed-vm"]),
    ] {
        assert_eq!(
            native_arguments(AppArgs {
                command: AppCommand::Query(command),
                library: None,
                json: false
            }),
            expected.into_iter().map(OsString::from).collect::<Vec<_>>()
        );
    }
}

#[test]
fn install_commands_forward_exact_ids_and_namespace_options() {
    for verb in ["install", "install-status", "install-cancel"] {
        for arguments in [
            vec![
                "bridgevm",
                "app",
                verb,
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
                verb,
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
                    verb,
                    "개발-vm",
                    "--library",
                    "/owned library",
                    "--json"
                ]
                .map(OsString::from)
            );
        }
    }
}

#[test]
fn install_variants_add_no_secret_or_media_arguments() {
    for (command, verb) in [
        (AppCommand::Install { id: "vm".into() }, "install"),
        (
            AppCommand::InstallStatus { id: "vm".into() },
            "install-status",
        ),
        (
            AppCommand::InstallCancel { id: "vm".into() },
            "install-cancel",
        ),
    ] {
        assert_eq!(
            native_arguments(AppArgs {
                command,
                library: None,
                json: false
            }),
            ["--cli", verb, "vm"].map(OsString::from)
        );
    }
}
#[test]
fn snapshot_commands_forward_exact_ids_and_namespace_options() {
    for verb in ["snapshot-create", "snapshot-restore"] {
        for arguments in [
            vec![
                "bridgevm",
                "app",
                verb,
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
                verb,
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
                    verb,
                    "개발-vm",
                    "--library",
                    "/owned library",
                    "--json"
                ]
                .map(OsString::from)
            );
        }
    }
}

#[test]
fn snapshot_variants_add_no_media_or_secret_arguments() {
    for (command, verb) in [
        (
            AppCommand::SnapshotCreate { id: "vm".into() },
            "snapshot-create",
        ),
        (
            AppCommand::SnapshotRestore { id: "vm".into() },
            "snapshot-restore",
        ),
    ] {
        assert_eq!(
            native_arguments(AppArgs {
                command,
                library: None,
                json: false
            }),
            ["--cli", verb, "vm"].map(OsString::from)
        );
    }
}
#[test]
fn snapshot_export_forwards_exact_id_output_and_namespace_options() {
    for arguments in [
        vec![
            "bridgevm",
            "app",
            "snapshot-export",
            "개발-vm",
            "/external/current.snapshot",
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
            "snapshot-export",
            "개발-vm",
            "/external/current.snapshot",
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
                "snapshot-export",
                "개발-vm",
                "/external/current.snapshot",
                "--library",
                "/owned library",
                "--json"
            ]
            .map(OsString::from)
        );
    }
}

#[test]
fn snapshot_export_rejects_relative_and_parent_traversal_outputs() {
    for output in ["relative.snapshot", "/tmp/../escape.snapshot"] {
        assert!(Cli::try_parse_from(["bridgevm", "app", "snapshot-export", "vm", output]).is_err());
    }
}
