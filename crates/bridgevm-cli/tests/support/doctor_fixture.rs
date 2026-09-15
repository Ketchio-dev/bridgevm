use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_FIXTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub(super) struct Fixture(pub(super) PathBuf);

impl Fixture {
    pub(super) fn new() -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self::with_timestamp(suffix)
    }

    fn with_timestamp(suffix: u128) -> Self {
        let sequence = NEXT_FIXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = PathBuf::from(format!(
            "/tmp/bv-doctor-{}-{suffix}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }

    pub(super) fn invoke(&self, args: &[&str]) -> Output {
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

#[test]
fn equal_timestamp_fixtures_keep_their_files_independent() {
    let first = Fixture::with_timestamp(0);
    let second = Fixture::with_timestamp(0);
    std::fs::write(first.0.join("owned"), b"first").unwrap();
    assert_ne!(first.0, second.0);
    assert!(!second.0.join("owned").exists());
    drop(first);
    assert!(second.0.is_dir());
}
