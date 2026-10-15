//! Powered-off snapshots against media an owned runtime still holds.
//!
//! The owned runtime keeps its writer leases from `prepare` until it exits,
//! but the cooperative pair lease is held only while a guest generation runs.
//! Before the first generation and between reboots the writer leases are the
//! only evidence that a runtime owns the media, so these tests hold them with
//! the real `prepare` and no guest generation.

use super::manifest;
use crate::vm_builder::prepare;
use bridgevm_hvf::snapshot_pair::{create_snapshot, managed::LockedPair, restore_snapshot};
use std::fs;
use std::path::{Path, PathBuf};

const RUNTIME: &str = "hvf-runner --owned-runtime-stdio";
const IN_USE: &str = "media is in use by a running VM";

fn private_pair(tag: &str) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bv-vmb-snap-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let dir = fs::canonicalize(dir).unwrap();
    let (disk, vars) = (dir.join("disk.raw"), dir.join("vars.fd"));
    fs::write(&disk, [7u8; 64]).unwrap();
    fs::write(&vars, [9u8; 4]).unwrap();
    let dest = dir.join("latest.snapshot");
    (dir, disk, vars, dest)
}

/// What a guest generation would boot: read through the pair owner, as the
/// probe does while the runtime holds its writer leases.
fn selected(disk: &Path, vars: &Path) -> (Vec<u8>, Vec<u8>) {
    let pair = LockedPair::open(disk, vars).expect("generation path opens the pair");
    let (disk, vars) = pair.paths().unwrap();
    (fs::read(disk).unwrap(), fs::read(vars).unwrap())
}

#[test]
fn restore_is_refused_while_an_owned_runtime_holds_the_media() {
    let (dir, disk, vars, dest) = private_pair("restore");
    create_snapshot(&disk, &vars, &dest, "vm", false, 1 << 20).unwrap();
    fs::write(&disk, [1u8; 64]).unwrap();
    fs::write(&vars, [2u8; 4]).unwrap();

    let runtime = prepare(manifest(&disk, &vars), RUNTIME).unwrap();
    let result = restore_snapshot(&dest, &disk, &vars, false);
    let message = result
        .map(|_| String::new())
        .unwrap_or_else(|e| e.to_string());
    assert!(message.contains(IN_USE), "restore admitted: {message:?}");
    assert!(message.contains(RUNTIME), "{message}");
    assert_eq!(selected(&disk, &vars), (vec![1u8; 64], vec![2u8; 4]));

    drop(runtime);
    restore_snapshot(&dest, &disk, &vars, false).unwrap();
    assert_eq!(selected(&disk, &vars), (vec![7u8; 64], vec![9u8; 4]));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn create_and_export_are_refused_while_an_owned_runtime_holds_the_media() {
    let (dir, disk, vars, dest) = private_pair("create");
    let runtime = prepare(manifest(&disk, &vars), RUNTIME).unwrap();
    let result = create_snapshot(&disk, &vars, &dest, "vm", false, 1 << 20);
    let message = result
        .map(|_| String::new())
        .unwrap_or_else(|e| e.to_string());
    assert!(message.contains(IN_USE), "create admitted: {message:?}");
    assert!(!dest.exists(), "refused create published a snapshot");
    assert!(!dir.join(".latest.snapshot.staging").exists());

    drop(runtime);
    create_snapshot(&disk, &vars, &dest, "vm", false, 1 << 20).unwrap();
    fs::remove_dir_all(dir).unwrap();
}
