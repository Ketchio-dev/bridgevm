//! Harmless local subprocess fixtures; never run the HVF probe or real swtpm.

use crate::{HelperLaunch, LaunchManifest};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
#[path = "owned_process_readiness.rs"]
mod readiness;

pub(crate) struct Fixture {
    pub root: PathBuf,
}
impl Fixture {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "bv-owned-runtime-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        for name in ["disk.raw", "vars.fd"] {
            std::fs::write(root.join(name), b"synthetic").unwrap();
        }
        Self { root }
    }
    pub(crate) fn script(&self, name: &str, body: &str) -> PathBuf {
        let path = self.root.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }
    pub(crate) fn manifest(&self) -> LaunchManifest {
        LaunchManifest::parse(
            &format!(
            "{{\"version\":1,\"disk\":\"{}\",\"uefi_vars\":\"{}\",\"ram_mib\":1024,\"vcpus\":1}}",
            self.root.join("disk.raw").display(), self.root.join("vars.fd").display()),
            false,
        )
        .unwrap()
    }
    pub(crate) fn launch(&self, script: PathBuf) -> HelperLaunch {
        HelperLaunch {
            helper: script,
            firmware_code: self.root.join("unused.fd"),
            watchdog_ms: None,
            agent_control: None,
            surfaces: None,
            swtpm_sockets: None,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
