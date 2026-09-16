#[path = "support/app_cli.rs"]
mod app_fixture;
use app_fixture::Fixture;

#[test]
fn stop_help_does_not_discover_or_start_the_app() {
    let fixture = Fixture::new();
    for args in [&["app", "--help"][..], &["app", "stop", "--help"]] {
        let output = fixture.invoke(args);
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("--library") && help.contains("--json"));
        assert!(help.contains("confirmed") && help.contains("cleanup"));
    }
}

#[test]
fn invalid_stop_and_legacy_store_options_fail_before_discovery() {
    let fixture = Fixture::new();
    let store = fixture.0.join("must-not-exist");
    let socket = fixture.0.join("absent.sock");
    for args in [
        vec!["app", "stop"],
        vec!["app", "stop", "vm", "other"],
        vec!["app", "stop", "vm", "--library", "relative"],
        vec!["app", "stop", "vm", "--library", "/tmp/../x"],
        vec!["app", "stop", "vm", "--json", "--json"],
        vec!["--store", store.to_str().unwrap(), "app", "stop", "vm"],
        vec!["app", "stop", "vm", "--store", store.to_str().unwrap()],
        vec!["--socket", socket.to_str().unwrap(), "app", "stop", "vm"],
        vec!["app", "stop", "vm", "--socket", socket.to_str().unwrap()],
    ] {
        let output = fixture.invoke(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("error:"));
    }
}
