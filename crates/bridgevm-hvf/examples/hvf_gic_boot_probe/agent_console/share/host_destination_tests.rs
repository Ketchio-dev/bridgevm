//! Real filesystem publication and symlink-swap regressions.

use super::*;
use std::os::unix::fs::symlink;

static FIXTURE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
struct Fixture { path: std::path::PathBuf, _lock: std::sync::MutexGuard<'static, ()> }
impl Fixture {
    fn new() -> Self {
        let lock = FIXTURE_LOCK.lock().unwrap();
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("bridgevm-share-{}-{id}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        std::fs::create_dir(path.join("root")).unwrap();
        std::fs::create_dir(path.join("outside")).unwrap();
        Self { path, _lock: lock }
    }
    fn root(&self) -> std::path::PathBuf { self.path.join("root") }
}
impl Drop for Fixture {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.path); }
}

#[test]
fn temporary_name_equal_to_destination_does_not_remove_published_file() {
    let f = Fixture::new();
    let id = NEXT_TEMP.load(Ordering::Relaxed);
    let leaf = format!(".bridgevm-sync-{}-{id}", std::process::id());
    Destination::open(&f.root(), &leaf, true).unwrap().write(b"guest").unwrap();
    assert_eq!(std::fs::read(f.root().join(leaf)).unwrap(), b"guest");
    assert_eq!(std::fs::read_dir(f.root()).unwrap().count(), 1);
}

#[test]
fn unreadable_delete_does_not_need_content_read_access() {
    let f = Fixture::new();
    std::fs::write(f.root().join("a"), b"old").unwrap();
    let d = Destination::open(&f.root(), "a", true).unwrap();
    std::fs::set_permissions(f.root().join("a"), std::fs::Permissions::from_mode(0)).unwrap();
    d.delete().unwrap();
    assert!(!f.root().join("a").exists());
}

#[test]
fn readonly_existing_file_is_not_replaced() {
    let f = Fixture::new();
    std::fs::write(f.root().join("a"), b"old").unwrap();
    std::fs::set_permissions(f.root().join("a"), std::fs::Permissions::from_mode(0o400)).unwrap();
    let d = Destination::open(&f.root(), "a", true).unwrap();
    assert!(d.write(b"new").is_err());
    assert_eq!(std::fs::read(f.root().join("a")).unwrap(), b"old");
}

#[test]
fn publication_creates_parents_and_preserves_existing_mode() {
    let f = Fixture::new();
    let d = Destination::open(&f.root(), "sub/a", true).unwrap();
    assert!(d.write(b"one").unwrap().is_some());
    std::fs::set_permissions(f.root().join("sub/a"), std::fs::Permissions::from_mode(0o640)).unwrap();
    d.write(b"two").unwrap();
    assert_eq!(std::fs::read(f.root().join("sub/a")).unwrap(), b"two");
    assert_eq!(std::fs::metadata(f.root().join("sub/a")).unwrap().permissions().mode() & 0o777, 0o640);
    d.delete().unwrap();
    assert!(Destination::absent(&f.root(), "sub/a"));
}

#[test]
fn partial_write_failure_keeps_old_destination_and_removes_temporary() {
    let f = Fixture::new();
    std::fs::write(f.root().join("a"), b"old").unwrap();
    let d = Destination::open(&f.root(), "a", true).unwrap();
    assert!(d.write_with(|file| {
        file.write_all(b"partial")?;
        Err(io::ErrorKind::WriteZero.into())
    }).is_err());
    assert_eq!(std::fs::read(f.root().join("a")).unwrap(), b"old");
    assert_eq!(std::fs::read_dir(f.root()).unwrap().count(), 1);
}

#[test]
fn stable_symlinks_and_traversal_are_refused_but_selected_root_may_be_link() {
    let f = Fixture::new();
    symlink(f.path.join("outside"), f.root().join("dir-link")).unwrap();
    symlink(f.path.join("outside/missing"), f.root().join("file-link")).unwrap();
    assert!(Destination::open(&f.root(), "dir-link/a", true).is_err());
    let leaf = Destination::open(&f.root(), "file-link", true).unwrap();
    assert!(leaf.write(b"bad").is_err());
    assert!(leaf.delete().is_err());
    assert!(!Destination::absent(&f.root(), "file-link"));
    for key in ["../outside/a", "/absolute", "", "sub/../a"] {
        assert!(Destination::open(&f.root(), key, true).is_err());
    }
    symlink(f.root(), f.path.join("selected-root")).unwrap();
    Destination::open(&f.path.join("selected-root"), "ok", true).unwrap().write(b"ok").unwrap();
}

#[test]
fn ancestor_swap_cannot_redirect_write_or_delete_to_outside_target() {
    let f = Fixture::new();
    std::fs::create_dir(f.root().join("sub")).unwrap();
    std::fs::write(f.path.join("outside/a"), b"sentinel").unwrap();
    let d = Destination::open(&f.root(), "sub/a", true).unwrap();
    std::fs::rename(f.root().join("sub"), f.root().join("held")).unwrap();
    symlink(f.path.join("outside"), f.root().join("sub")).unwrap();
    d.write(b"guest").unwrap();
    assert_eq!(std::fs::read(f.root().join("held/a")).unwrap(), b"guest");
    d.delete().unwrap();
    assert_eq!(std::fs::read(f.path.join("outside/a")).unwrap(), b"sentinel");
}

#[test]
fn leaf_swap_during_write_replaces_link_without_writing_its_target() {
    let f = Fixture::new();
    std::fs::write(f.root().join("a"), b"old").unwrap();
    std::fs::write(f.path.join("outside/a"), b"sentinel").unwrap();
    let d = Destination::open(&f.root(), "a", true).unwrap();
    d.write_with(|file| {
        std::fs::remove_file(f.root().join("a"))?;
        symlink(f.path.join("outside/a"), f.root().join("a"))?;
        file.write_all(b"guest")
    }).unwrap();
    assert_eq!(std::fs::read(f.root().join("a")).unwrap(), b"guest");
    assert_eq!(std::fs::read(f.path.join("outside/a")).unwrap(), b"sentinel");
}
