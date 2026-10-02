use super::super::snapshot_pair_tests::{Scratch, QUOTA};
use super::super::{create_snapshot, restore_snapshot};
use super::*;
use std::fs;

#[test]
fn after_a_restore_the_selected_pair_matches_the_snapshot_and_the_originals_do_not() {
    let s = Scratch::new("selected");
    let disk = s.write("disk.raw", b"disk before");
    let vars = s.write("vars.fd", b"vars before");
    let snap = s.path("snap");
    let m = create_snapshot(&disk, &vars, &snap, "vm-1", false, QUOTA).expect("create");
    fs::write(&disk, b"disk mutated!").expect("mutate disk");
    fs::write(&vars, b"vars mutated!").expect("mutate vars");
    restore_snapshot(&snap, &disk, &vars, false).expect("restore");

    let selected = selected_digest(&disk, &vars).expect("digest");
    assert_eq!(
        (selected.disk_sha256.as_str(), selected.disk_bytes),
        (m.disk_sha256.as_str(), m.disk_bytes)
    );
    assert_eq!(
        (selected.vars_sha256.as_str(), selected.vars_bytes),
        (m.vars_sha256.as_str(), m.vars_bytes)
    );
    assert_eq!(
        fs::read(&disk).expect("original disk"),
        b"disk mutated!",
        "originals keep their bytes"
    );
}

#[test]
fn without_a_managed_generation_the_originals_are_the_selected_pair() {
    let s = Scratch::new("selected-plain");
    let disk = s.write("disk.raw", b"d");
    let vars = s.write("vars.fd", b"v");
    let selected = selected_digest(&disk, &vars).expect("digest");
    assert_eq!((selected.disk_bytes, selected.vars_bytes), (1, 1));
    assert_ne!(selected.disk_sha256, selected.vars_sha256);
}
