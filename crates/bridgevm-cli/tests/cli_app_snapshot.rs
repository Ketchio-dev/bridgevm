use std::process::Command;

fn bridgevm() -> Command {
    Command::new(env!("CARGO_BIN_EXE_bridgevm"))
}

#[test]
fn snapshot_help_is_available_without_launching_the_app() {
    for verb in ["snapshot-create", "snapshot-restore"] {
        let output = bridgevm()
            .args(["app", verb, "--help"])
            .output()
            .expect("run bridgevm");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("powered-off"));
        assert!(stdout.contains("<ID>"));
    }
}

#[test]
fn snapshot_commands_require_one_id_and_reject_media_arguments() {
    for arguments in [
        vec!["app", "snapshot-create"],
        vec!["app", "snapshot-restore"],
        vec!["app", "snapshot-create", "vm", "extra"],
        vec!["app", "snapshot-restore", "vm", "--disk", "/secret.raw"],
    ] {
        let output = bridgevm().args(arguments).output().expect("run bridgevm");
        assert_eq!(output.status.code(), Some(2));
    }
}
