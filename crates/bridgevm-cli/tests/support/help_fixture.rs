use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_FIXTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub(super) struct Fixture(pub(super) PathBuf);

impl Fixture {
    pub(super) fn new() -> Self {
        let sequence = NEXT_FIXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "bridgevm-cli-help-{}-{suffix}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }

    pub(super) fn invoke(&self, args: &[&str]) -> Output {
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

    pub(super) fn help(&self, args: &[&str]) -> String {
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
