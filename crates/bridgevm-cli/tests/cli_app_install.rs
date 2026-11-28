#[path = "support/app_cli.rs"]
mod app_fixture;
use app_fixture::Fixture;

#[test]
fn install_help_exposes_saved_request_and_secret_boundaries() {
    let fixture = Fixture::new();
    for (args, says) in [
        (
            &["app", "--help"][..],
            "Passwords and recovery keys are never accepted",
        ),
        (
            &["app", "install", "--help"],
            "no paths or secrets are accepted",
        ),
        (
            &["app", "install-status", "--help"],
            "progress of the VM's Windows installation",
        ),
        (
            &["app", "install-cancel", "--help"],
            "Cancel the VM's Windows installation",
        ),
    ] {
        let output = fixture.invoke(args);
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains(says), "{args:?}: {help}");
    }
}

#[test]
fn invalid_install_arguments_fail_before_app_discovery() {
    let fixture = Fixture::new();
    for args in [
        vec!["app", "install"],
        vec!["app", "install", "vm", "other"],
        vec!["app", "install-status"],
        vec!["app", "install-cancel", "vm", "--iso", "/secret.iso"],
        vec!["app", "install", "vm", "--library", "relative"],
    ] {
        let output = fixture.invoke(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("error:"));
    }
}
