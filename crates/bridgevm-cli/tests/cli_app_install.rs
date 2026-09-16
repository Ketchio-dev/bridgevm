#[path = "support/app_cli.rs"]
mod app_fixture;
use app_fixture::Fixture;

#[test]
fn install_help_exposes_saved_request_and_secret_boundaries() {
    let fixture = Fixture::new();
    for args in [
        &["app", "--help"][..],
        &["app", "install", "--help"],
        &["app", "install-status", "--help"],
        &["app", "install-cancel", "--help"],
    ] {
        let output = fixture.invoke(args);
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("not accepted on argv"));
        assert!(help.contains("pending request") || help.contains("retained"));
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
