use super::*;
use std::io::BufRead;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

pub(super) fn absent_fixture() -> Fixture {
    let fixture = Fixture::new();
    fs::remove_file(fixture.disk()).unwrap();
    fs::remove_file(fixture.vars()).unwrap();
    fixture
}

pub(super) fn assert_empty(fixture: &Fixture) {
    assert_eq!(
        fs::read_dir(fixture.disk().parent().unwrap())
            .unwrap()
            .count(),
        0
    );
}

fn command(mode: &str, disk: &Path, vars: &Path) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "media_lease_session::tests::pending_delete::subprocess",
            "--ignored",
            "--nocapture",
        ])
        .env("BV_TEST_SESSION_MODE", mode)
        .env("BV_TEST_SESSION_DISK", disk)
        .env("BV_TEST_SESSION_VARS", vars)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

pub(super) fn assert_refused(mode: &str, disk: &Path, vars: &Path) {
    let output = command(mode, disk, vars)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!output
        .stdout
        .windows(READY.len())
        .any(|bytes| bytes == READY));
    assert!(String::from_utf8_lossy(&output.stderr).contains("media lease session:"));
}

pub(super) struct Session(Child);
impl Session {
    pub(super) fn start(mode: &str, disk: &Path, vars: &Path) -> Self {
        let mut session = Self(
            command(mode, disk, vars)
                .stdin(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let stdout = session.0.stdout.take().unwrap();
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = io::BufReader::new(stdout);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap() != 0 {
                // The child is a Rust test executable; ignore its harness header.
                if line.as_bytes() == READY {
                    let _ = send.send(());
                    return;
                }
                line.clear();
            }
        });
        receive
            .recv_timeout(Duration::from_secs(10))
            .expect("child READY");
        session
    }

    pub(super) fn finish(mut self, release: &[u8], success: bool) {
        self.0.stdin.as_mut().unwrap().write_all(release).unwrap();
        drop(self.0.stdin.take());
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                assert_eq!(status.success(), success);
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "lease child did not exit"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        drop(self.0.stdin.take());
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub(super) fn assert_blocked(path: &Path) {
    assert_eq!(
        MediaLease::acquire([path]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
}
