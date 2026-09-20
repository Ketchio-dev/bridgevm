use std::process::Command;

fn bridgevm() -> Command {
    Command::new(env!("CARGO_BIN_EXE_bridgevm"))
}

#[test]
fn snapshot_export_help_is_available_without_launching_the_app() {
    let output = bridgevm()
        .args(["app", "snapshot-export", "--help"])
        .output()
        .expect("run bridgevm");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("<ID>"));
    assert!(stdout.contains("<OUTPUT>"));
    assert!(stdout.contains("currently selected powered-off"));
}

#[test]
fn snapshot_export_rejects_missing_relative_and_traversal_outputs() {
    for arguments in [
        vec!["app", "snapshot-export", "vm"],
        vec!["app", "snapshot-export", "vm", "relative.snapshot"],
        vec!["app", "snapshot-export", "vm", "/tmp/../escape.snapshot"],
    ] {
        let output = bridgevm().args(arguments).output().expect("run bridgevm");
        assert_eq!(output.status.code(), Some(2));
    }
}
