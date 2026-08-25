//! Collision-free temporary stores for daemon tests.

use bridgevm_storage::VmStore;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_ID: AtomicU64 = AtomicU64::new(0);

fn reserve_root(base: &Path, pid: u32, next: &AtomicU64) -> PathBuf {
    loop {
        let id = next.fetch_add(1, Ordering::Relaxed);
        let path = base.join(format!("bvmd-{pid}-{id}"));
        match fs::create_dir(&path) {
            Ok(()) => return path,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("reserve daemon test root {}: {error}", path.display()),
        }
    }
}

pub(super) fn temp_store() -> VmStore {
    VmStore::new(reserve_root(
        Path::new("/tmp"),
        std::process::id(),
        &TEST_ID,
    ))
}

#[test]
fn temp_store_reservation_skips_a_namespace_left_by_a_reused_pid() {
    let base = std::env::temp_dir().join(format!("bvmd-reserve-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir(&base).unwrap();
    fs::create_dir(base.join("bvmd-77-0")).unwrap();
    let next = AtomicU64::new(0);

    let reserved = reserve_root(&base, 77, &next);

    assert_eq!(reserved, base.join("bvmd-77-1"));
    fs::remove_dir_all(base).unwrap();
}
