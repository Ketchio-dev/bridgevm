//! Descriptor-bound reads keep regular hardlinks, enforce size, and reject changes.

use super::*;
use crate::agent_console::share::ShareFileReadError;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::unix::fs::MetadataExt;
use std::time::UNIX_EPOCH;

#[test]
fn regular_hardlink_snapshots_accept_the_exact_cap_and_refuse_one_more_before_read() {
    let f = Fixture::new();
    let source = f.root().join("source");
    std::fs::write(&source, b"data").unwrap();
    std::fs::hard_link(&source, f.root().join("linked")).unwrap();
    assert_eq!(std::fs::metadata(&source).unwrap().nlink(), 2);
    let destination = Destination::open(&f.root(), "linked", false).unwrap();
    let snapshot = destination.snapshot(4).unwrap();
    assert_eq!(snapshot.bytes, b"data");
    let mtime = std::fs::metadata(&source)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    assert_eq!(snapshot.mtime_ms, mtime);

    std::fs::write(&source, b"data+").unwrap();
    let refused = destination.snapshot_with(4, |_, _, _| {
        panic!("an over-cap file must be refused before reading");
    });
    assert!(matches!(refused, Err(ShareFileReadError::TooLarge(5))));
}

#[test]
fn snapshots_refuse_leaf_and_intermediate_symlinks() {
    let f = Fixture::new();
    let outside = f.path.join("outside/source");
    std::fs::write(&outside, b"sentinel").unwrap();
    symlink(&outside, f.root().join("leaf-link")).unwrap();
    let leaf = Destination::open(&f.root(), "leaf-link", false).unwrap();
    assert!(matches!(leaf.snapshot(64), Err(ShareFileReadError::Io)));
    symlink(f.path.join("outside"), f.root().join("dir-link")).unwrap();
    assert!(Destination::open(&f.root(), "dir-link/source", false).is_err());
    assert_eq!(std::fs::read(&outside).unwrap(), b"sentinel");
}

#[test]
fn a_file_changed_in_place_after_the_real_read_is_not_a_snapshot() {
    let f = Fixture::new();
    let path = f.root().join("source");
    std::fs::write(&path, b"data").unwrap();
    let destination = Destination::open(&f.root(), "source", false).unwrap();
    let changed = destination.snapshot_with(16, |file, bytes, limit| {
        (&mut *file).take(limit).read_to_end(bytes)?;
        assert_eq!(bytes.as_slice(), b"data");
        OpenOptions::new()
            .append(true)
            .open(&path)?
            .write_all(b"+")?;
        Ok(())
    });
    assert!(matches!(changed, Err(ShareFileReadError::Io)));
    assert_eq!(std::fs::read(path).unwrap(), b"data+");
}
