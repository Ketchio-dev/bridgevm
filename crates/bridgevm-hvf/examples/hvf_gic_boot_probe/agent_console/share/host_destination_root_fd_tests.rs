//! A verified root descriptor stays bound across later selected-path replacement.

use super::*;
use crate::agent_console::{ShareState, ShareSync};
use std::time::Duration;

#[test]
fn verified_root_fd_cannot_redirect_write_or_delete_after_path_replacement() {
    let f = Fixture::new();
    std::fs::create_dir(f.root().join("sub")).unwrap();
    std::fs::write(f.root().join("sub/a"), b"old").unwrap();
    let selected = f.path.join("selected-root");
    symlink(f.root(), &selected).unwrap();
    let mut share = ShareState::new(
        ShareSync::new(1024),
        selected,
        "C:\\share".into(),
        Duration::from_secs(1),
    );
    let checked = share.checked_root().unwrap();
    let held = f.path.join("held-root");
    std::fs::rename(f.root(), &held).unwrap();
    std::fs::create_dir(f.root()).unwrap();
    std::fs::create_dir(f.root().join("sub")).unwrap();
    std::fs::write(f.root().join("sub/a"), b"replacement sentinel").unwrap();

    let destination = Destination::from_root(checked, "sub/a", true).unwrap();
    destination.write(b"guest").unwrap();
    assert_eq!(std::fs::read(held.join("sub/a")).unwrap(), b"guest");
    assert_eq!(
        std::fs::read(f.root().join("sub/a")).unwrap(),
        b"replacement sentinel"
    );
    destination.delete().unwrap();
    assert!(!held.join("sub/a").exists());
    assert_eq!(
        std::fs::read(f.root().join("sub/a")).unwrap(),
        b"replacement sentinel"
    );
}
