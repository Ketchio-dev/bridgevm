//! Owned root fixture for the existing destination-failure budget boundary.

use super::*;
use crate::agent_console::agent_console_tests::tests::harness;

pub(super) struct OwnedRoot(PathBuf);

impl OwnedRoot {
    fn new() -> Self {
        for nonce in 0..128 {
            let path = std::env::temp_dir().join(format!(
                "bvagent-share-failure-{}-{nonce}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create owned share root: {error}"),
            }
        }
        panic!("no fresh owned share root available");
    }
}

impl Drop for OwnedRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(super) fn nearly_exhausted_share() -> (AgentConsoleHarness, OwnedRoot) {
    let root = OwnedRoot::new();
    let mut h = harness();
    let mut engine = ShareSync::new(1024);
    for i in 0..4095 {
        let name = format!("blocked/{i}");
        engine.on_guest_file(name.clone(), vec![1], Some("g1"));
        assert!(!engine.on_host_write_failed(&name));
    }
    h.share = Some(ShareState::new(
        engine,
        root.0.clone(),
        "C:\\share".into(),
        Duration::from_secs(1),
    ));
    (h, root)
}
