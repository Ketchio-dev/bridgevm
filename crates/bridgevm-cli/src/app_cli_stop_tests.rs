use super::*;

#[test]
fn status_and_stop_accept_namespace_options_before_or_after_the_exact_id() {
    for verb in ["status", "stop"] {
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
fn stop_forwards_one_exact_id_and_preserves_an_unusual_library_path() {
    for library in [None, Some(PathBuf::from("/owned library/with 'quotes'"))] {
        let mut expected = vec![OsString::from("--cli"), "stop".into(), "개발-vm".into()];
        if let Some(path) = &library {
            expected.extend(["--library".into(), path.as_os_str().to_owned()]);
        }
        assert_eq!(
            native_arguments(AppArgs {
                command: AppCommand::Stop {
                    id: "개발-vm".into()
                },
                library,
                json: false
            }),
            expected
        );
    }
}
