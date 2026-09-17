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

include!("app_cli_install_tests.rs");
include!("app_cli_snapshot_tests.rs");
