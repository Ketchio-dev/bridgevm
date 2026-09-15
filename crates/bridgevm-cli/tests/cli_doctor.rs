use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = PathBuf::from(format!("/tmp/bv-doctor-{}-{suffix}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }

    fn invoke(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_bridgevm"))
            .env_clear()
            .env("PATH", self.0.join("empty-path"))
            .args(args)
            .output()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn output_text(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout).unwrap()
}

fn assert_missing_summary_matches_checks(text: &str) {
    let count = |prefix: &str| text.lines().filter(|line| line.starts_with(prefix)).count();
    let (ok, warn, missing) = (count("[OK] "), count("[WARN] "), count("[MISSING] "));
    assert_eq!(
        ok + warn + missing,
        10,
        "All existing checks must remain: {text}"
    );
    assert!(
        missing >= 2,
        "Empty PATH must expose both QEMU checks: {text}"
    );
    assert!(text.contains(&format!(
        "Client environment summary: MISSING (OK: {ok}, WARN: {warn}, MISSING: {missing})"
    )));
    assert!(!text.lines().any(|line| line == "Status: OK"));
    assert!(text.contains("Client environment checks (paths and tools on this client):"));
    assert!(text.contains("own-HVF runtime readiness is not assessed"));
    assert!(text.contains("Engine lanes:") && text.contains("Parallels-class progress:"));
}

#[test]
fn local_doctor_keeps_store_compatibility_but_reports_missing_tools() {
    let fixture = Fixture::new();
    let store = fixture.0.join("store");
    assert!(!store.exists());
    let text = output_text(fixture.invoke(&["--store", store.to_str().unwrap(), "doctor"]));
    assert!(store.is_dir() && store.join("vms").is_dir());
    assert!(text.contains("Store status (local): OK"));
    assert!(text.contains("[MISSING] qemu-img:"));
    assert!(text.contains("[MISSING] QEMU system binary:"));
    assert_missing_summary_matches_checks(&text);
}

#[test]
fn daemon_doctor_distinguishes_server_status_from_client_environment() {
    let fixture = Fixture::new();
    let socket = fixture.0.join("daemon.sock");
    let store = fixture.0.join("server-only-store");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let response = serde_json::json!({
        "type": "doctor", "store_root": store, "vms_dir": store.join("vms"),
        "status": "DEGRADED (fixture)"
    });
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "CLI never connected to its fixture daemon"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("fixture accept failed: {error}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut request)
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&request).unwrap(),
            serde_json::json!({"type": "doctor"})
        );
        serde_json::to_writer(&mut stream, &response).unwrap();
        stream.write_all(b"\n").unwrap();
    });
    let output = fixture.invoke(&["--socket", socket.to_str().unwrap(), "doctor"]);
    server.join().unwrap();
    let text = output_text(output);
    assert!(
        !store.exists(),
        "Client doctor must not create the daemon-reported store"
    );
    assert!(text.contains("Store status (daemon-reported): DEGRADED (fixture)"));
    assert!(text.contains("BridgeVM store (daemon-reported):"));
    assert!(!text.contains("Store status (local):"));
    assert_missing_summary_matches_checks(&text);
}
