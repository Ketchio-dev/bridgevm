use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("bridgevm-cli-help-{}-{suffix}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }

    fn invoke(&self, args: &[&str]) -> Output {
        let output = Command::new(env!("CARGO_BIN_EXE_bridgevm"))
            .env_clear()
            .env("PATH", &self.0)
            .args(["--store", self.0.join("unused-store").to_str().unwrap()])
            .args(args)
            .output()
            .unwrap();
        assert_eq!(
            std::fs::read_dir(&self.0).unwrap().count(),
            0,
            "Help/refusal must not write to the owned fixture"
        );
        output
    }

    fn help(&self, args: &[&str]) -> String {
        let output = self.invoke(args);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        String::from_utf8(output.stdout).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn top_level_help_identifies_store_and_own_hvf_route() {
    let fixture = Fixture::new();
    for flag in ["--help", "-h"] {
        let help = fixture.help(&[flag]);
        for expected in [
            "manifest.yaml",
            "vm.json",
            "--store",
            "--socket",
            "bridgevm hvf host-capabilities",
            "bridgevm hvf machine-plan",
        ] {
            assert!(help.contains(expected), "Missing {expected}: {help}");
        }
    }
}

#[test]
fn lifecycle_help_distinguishes_recording_from_execution() {
    let fixture = Fixture::new();
    let start = fixture.help(&["start", "--help"]);
    assert!(start.contains("metadata only") && start.contains("does not launch"));
    let restart = fixture.help(&["restart", "--help"]);
    assert!(restart.contains("does not relaunch"));
    let run = fixture.help(&["run", "--help"]);
    assert!(run.contains("dry run by default") && run.contains("--spawn"));
    let explicit = fixture.help(&["run", "example", "--spawn", "--help"]);
    assert_eq!(run, explicit);
    for command in ["stop", "suspend", "resume"] {
        assert!(fixture.help(&[command, "--help"]).contains("backend"));
    }
}

#[test]
fn hvf_help_and_parser_preserve_explicit_opt_ins() {
    let fixture = Fixture::new();
    let help = fixture.help(&["hvf", "--help"]);
    assert!(help.contains("local-only queries and opt-in probes"));
    assert!(help.contains("host-capabilities") && help.contains("vm-probe"));
    let probe = fixture.help(&["hvf", "vm-probe", "--allow-create", "--help"]);
    assert!(probe.contains("--allow-create"));
    for args in [
        &["start"][..],
        &["run", "--spawn"][..],
        &["hvf", "vm-probe", "--not-a-real-option"][..],
    ] {
        let output = fixture.invoke(args);
        assert_eq!(output.status.code(), Some(2), "Parser contract: {args:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("error:"));
    }
}

#[test]
fn hvf_socket_refusal_does_not_mislabel_effectful_probes() {
    let fixture = Fixture::new();
    let socket = fixture.0.join("absent.sock");
    let output = fixture.invoke(&[
        "--socket",
        socket.to_str().unwrap(),
        "hvf",
        "host-capabilities",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("local-only") && error.contains("omit --socket"));
    assert!(!error.contains("metadata-only"));
}
