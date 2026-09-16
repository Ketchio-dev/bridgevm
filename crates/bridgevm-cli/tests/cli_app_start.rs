#[path = "support/app_cli.rs"]
mod app_fixture;
use app_fixture::Fixture;

#[test]
fn start_help_does_not_discover_or_start_the_app() {
    let fixture = Fixture::new();
    for args in [&["app", "--help"][..], &["app", "start", "--help"]] {
        let output = fixture.invoke(args);
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("--library") && help.contains("--json"));
        assert!(help.contains("confirmed") && help.contains("startup"));
    }
}

#[test]
fn invalid_start_and_legacy_store_options_fail_before_discovery() {
    let fixture = Fixture::new();
    let store = fixture.0.join("must-not-exist");
    let socket = fixture.0.join("absent.sock");
    for args in [
        vec!["app", "start"],
        vec!["app", "start", "vm", "other"],
        vec!["app", "start", "vm", "--library", "relative"],
        vec!["app", "start", "vm", "--library", "/tmp/../x"],
        vec!["app", "start", "vm", "--json", "--json"],
        vec!["--store", store.to_str().unwrap(), "app", "start", "vm"],
        vec!["app", "start", "vm", "--store", store.to_str().unwrap()],
        vec!["--socket", socket.to_str().unwrap(), "app", "start", "vm"],
        vec!["app", "start", "vm", "--socket", socket.to_str().unwrap()],
    ] {
        let output = fixture.invoke(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("error:"));
    }
}
