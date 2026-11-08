//! An idle share plans from the last walk while FSEvents reports nothing
//! (share_host_watch.rs); a reported change, the service's own write or
//! delete, and a minute's age each bring a new walk.

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

fn watched(label: &str) -> (Rig, Arc<AtomicU64>) {
    let mut rig = Rig::new(label);
    let changes = Arc::new(AtomicU64::new(0));
    rig.h.share.as_mut().unwrap().host_watch = HostWatch::manual(changes.clone());
    (rig, changes)
}

fn walks(rig: &Rig) -> usize {
    rig.h.share.as_ref().unwrap().host_scans
}

#[test]
fn a_quiet_watch_spares_the_walk_and_a_reported_change_lands() {
    let (mut rig, changes) = watched("watch-quiet");
    std::fs::write(rig.dir.join("a.txt"), b"one").unwrap();
    rig.run(10);
    assert_eq!(rig.guest.content("a.txt"), Some(&b"one"[..]));
    let walked = walks(&rig);
    rig.run(20);
    assert_eq!(walks(&rig), walked, "an idle share walked the Mac folder");
    std::fs::write(rig.dir.join("a.txt"), b"two!").unwrap();
    changes.fetch_add(1, Ordering::Release);
    rig.run(3);
    assert_eq!(rig.guest.content("a.txt"), Some(&b"two!"[..]));
    assert_eq!(walks(&rig), walked + 1);
}

#[test]
fn a_minute_old_walk_is_redone_even_when_the_watch_is_quiet() {
    let (mut rig, _changes) = watched("watch-age");
    std::fs::write(rig.dir.join("a.txt"), b"one").unwrap();
    rig.run(10);
    // An edit FSEvents does not report, as one made through a hard link elsewhere.
    std::fs::write(rig.dir.join("a.txt"), b"two!").unwrap();
    rig.run(40);
    assert_eq!(rig.guest.content("a.txt"), Some(&b"one"[..]));
    rig.run(30);
    assert_eq!(rig.guest.content("a.txt"), Some(&b"two!"[..]));
}

#[test]
fn the_services_own_write_and_delete_on_the_mac_bring_a_new_walk() {
    let (mut rig, _changes) = watched("watch-own-write");
    rig.run(5);
    let walked = walks(&rig);
    rig.guest.write("from-windows.txt", b"guest data");
    rig.run(5);
    assert_eq!(rig.host("from-windows.txt"), Some(b"guest data".to_vec()));
    assert_eq!(walks(&rig), walked + 1, "the published file was planned from the old list");
    rig.guest.files.clear();
    rig.run(5);
    assert_eq!(rig.host("from-windows.txt"), None);
    assert_eq!(walks(&rig), walked + 2, "the deleted file was planned from the old list");
    rig.run(10);
    assert_eq!(walks(&rig), walked + 2);
}
