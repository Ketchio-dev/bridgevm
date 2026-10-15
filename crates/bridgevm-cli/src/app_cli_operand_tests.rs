use super::*;
use clap::error::ErrorKind;

const ID_VERBS: [&str; 10] = [
    "inspect",
    "readiness",
    "status",
    "start",
    "stop",
    "install",
    "install-status",
    "install-cancel",
    "snapshot-create",
    "snapshot-restore",
];
const CREATE: [&str; 5] = ["bridgevm", "app", "create-windows", "--iso", "/abs.iso"];

fn forwarded(arguments: &[&str]) -> std::result::Result<Vec<OsString>, clap::Error> {
    let cli = Cli::try_parse_from(arguments)?;
    let Command::App(args) = cli.command else {
        panic!("expected native app command")
    };
    Ok(native_arguments(args))
}

fn assert_rejected(arguments: &[&str]) {
    let error = match forwarded(arguments) {
        Ok(native) => panic!("{arguments:?} was accepted and forwarded as {native:?}"),
        Err(error) => error,
    };
    let message = error.to_string();
    assert_eq!(error.kind(), ErrorKind::ValueValidation, "{message}");
    assert_eq!(error.exit_code(), 2, "{message}");
    let expected = "VM IDs and names starting with '-' are not accepted by the CLI";
    assert!(message.contains(expected), "{arguments:?}: {message}");
}

#[test]
fn stop_after_separator_rejects_help_instead_of_forwarding_it() {
    assert_rejected(&["bridgevm", "app", "stop", "--", "--help"]);
}

#[test]
fn every_forwarded_id_and_name_rejects_dash_leading_values() {
    for value in ["--help", "-h", "--json", "-vm", "-"] {
        for verb in ID_VERBS {
            assert_rejected(&["bridgevm", "app", verb, "--", value]);
        }
        let output = "/external/current.snapshot";
        assert_rejected(&["bridgevm", "app", "snapshot-export", "--", value, output]);
        assert_rejected(&[&CREATE[..], &["--", value]].concat());
    }
}

#[test]
fn valid_ids_and_names_forward_unchanged_with_or_without_separator() {
    for verb in ID_VERBS {
        let expected = ["--cli", verb, "개발-vm"].map(OsString::from);
        for arguments in [[verb, "개발-vm"].as_slice(), &[verb, "--", "개발-vm"]] {
            let arguments = [&["bridgevm", "app"][..], arguments].concat();
            assert_eq!(forwarded(&arguments).unwrap(), expected);
        }
    }
    let native = forwarded(&[&CREATE[..], &["create-windows"]].concat()).unwrap();
    let expected = ["--cli", "create-windows", "create-windows", "--iso"];
    assert_eq!(native[..4], expected.map(OsString::from));
}
