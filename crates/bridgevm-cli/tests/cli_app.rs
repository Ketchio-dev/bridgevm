#[path = "support/app_cli.rs"]
mod app_fixture;
use app_fixture::Fixture;

#[test]
fn actual_help_exposes_native_commands_without_discovery() {
    let fixture = Fixture::new();
    for args in [
        &["app", "--help"][..],
        &["app", "list", "--help"],
        &["app", "inspect", "--help"],
        &["app", "readiness", "--help"],
        &["app", "status", "--help"],
    ] {
        let output = fixture.invoke(args);
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("--library") && help.contains("--json"));
    }
    let help = String::from_utf8(fixture.invoke(&["app", "--help"]).stdout).unwrap();
    assert!(help.contains("vm.json") && help.contains("Inventory runtime remains unobserved"));
    assert!(help.contains("readiness") && help.contains("  start "));
    assert!(help.contains("status") && help.contains("already-running app"));
}

#[test]
fn store_and_socket_are_rejected_before_store_or_connection_work() {
    let fixture = Fixture::new();
    let store = fixture.0.join("must-not-exist");
    let socket = fixture.0.join("absent.sock");
    for args in [
        vec!["--store", store.to_str().unwrap(), "app", "list"],
        vec!["app", "list", "--store", store.to_str().unwrap()],
        vec!["--store", store.to_str().unwrap(), "app", "status", "vm"],
        vec!["app", "status", "vm", "--store", store.to_str().unwrap()],
        vec!["--socket", socket.to_str().unwrap(), "app", "status", "vm"],
        vec!["app", "status", "vm", "--socket", socket.to_str().unwrap()],
        vec![
            "--socket",
            socket.to_str().unwrap(),
            "app",
            "inspect",
            "개발-vm",
        ],
        vec![
            "app",
            "readiness",
            "개발-vm",
            "--socket",
            socket.to_str().unwrap(),
        ],
    ] {
        let output = fixture.invoke(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("omit --store and --socket") && error.contains("--library"));
    }
}

#[test]
fn invalid_app_arguments_fail_in_the_real_parser() {
    let fixture = Fixture::new();
    for args in [
        &["app"][..],
        &["app", "unsupported", "vm"],
        &["app", "inspect"],
        &["app", "list", "--library", "relative"],
        &["app", "list", "--library", "/tmp/../x"],
        &["app", "list", "--json", "--json"],
        &["app", "readiness"],
        &["app", "status"],
        &["app", "status", "vm", "other"],
        &["app", "status", "vm", "--library", "relative"],
        &["app", "status", "vm", "--library", "/tmp/../x"],
        &["app", "status", "vm", "--json", "--json"],
        &["app", "list", "--library", "/a", "--library", "/b"],
    ] {
        let output = fixture.invoke(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("error:") || args == ["app"]);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn paired_fixture_preserves_unicode_streams_exit_status_and_process_identity() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let fixture = Fixture::new();
    let contents = fixture.0.join("Owned Fixture.app/Contents");
    let cli = contents.join("Resources/target/release/bridgevm");
    let helper = contents.join("MacOS/BridgeVMControl");
    std::fs::create_dir_all(cli.parent().unwrap()).unwrap();
    std::fs::create_dir_all(helper.parent().unwrap()).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_bridgevm"), &cli).unwrap();
    // This is a harmless shell fixture, never a native app or a guest launcher.
    let script = b"#!/bin/sh\n# bridgevm-native-cli-v1\nprintf 'pid=%s\\n' \"$$\"\nprintf 'arg=<%s>\\n' \"$@\"\nIFS= read -r line\nprintf 'stdin=<%s>\\n' \"$line\"\nprintf 'fixture-stderr\\n' >&2\nexit 23\n";
    std::fs::write(&helper, script).unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let library = fixture.0.join("library with 'quotes'");
    for verb in ["inspect", "status", "stop"] {
        let mut child = Command::new(&cli)
            .env_clear()
            .env("PATH", &fixture.0)
            .args([
                "app",
                verb,
                "개발-vm",
                "--library",
                library.to_str().unwrap(),
                "--json",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        let write = child.stdin.take().unwrap().write_all(b"owned stdin\n");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut timed_out = false;
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                timed_out = true;
                child.kill().unwrap();
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            !timed_out,
            "only the owned fixture child was killed on timeout"
        );
        write.unwrap();
        assert_eq!(output.status.code(), Some(23));
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            "fixture-stderr\n"
        );
        let expected = format!("pid={pid}\narg=<--cli>\narg=<{verb}>\narg=<개발-vm>\narg=<--library>\narg=<{}>\narg=<--json>\nstdin=<owned stdin>\n", library.display());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
        assert!(!library.exists());
        assert_eq!(std::fs::read(&helper).unwrap(), script);
    }
}
