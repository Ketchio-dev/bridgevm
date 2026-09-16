#[path = "support/app_cli.rs"]
mod app_fixture;
use app_fixture::Fixture;

#[test]
fn create_help_exposes_owned_pending_registration_boundary() {
    let fixture = Fixture::new();
    for args in [&["app", "--help"][..], &["app", "create-windows", "--help"]] {
        let output = fixture.invoke(args);
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("installation-pending"));
        assert!(help.contains("does not install or boot"));
    }
}

#[test]
fn invalid_create_arguments_fail_before_app_discovery() {
    let fixture = Fixture::new();
    for args in [
        vec!["app", "create-windows"],
        vec!["app", "create-windows", "VM", "--iso", "relative.iso"],
        vec![
            "app",
            "create-windows",
            "VM",
            "--iso",
            "/tmp/a.iso",
            "--disk-gib",
            "63",
        ],
        vec![
            "app",
            "create-windows",
            "VM",
            "--iso",
            "/tmp/a.iso",
            "--memory-mib",
            "1234",
        ],
        vec![
            "app",
            "create-windows",
            "VM",
            "--iso",
            "/tmp/a.iso",
            "--resolution",
            "800x600",
        ],
        vec![
            "app",
            "create-windows",
            "VM",
            "--iso",
            "/tmp/a.iso",
            "extra",
        ],
    ] {
        let output = fixture.invoke(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("error:"));
    }
}
