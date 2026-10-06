//! Tiny real images for portable-import tests; no guest is started.

use crate::*;
use std::path::Path;
use std::process::Command;

pub(super) fn image(args: &[&str]) {
    let output = Command::new("qemu-img")
        .args(args)
        .output()
        .expect("qemu-img is required for import image contracts");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub(super) fn create_primary(store: &VmStore, name: &str) {
    let primary = store.prepare_primary_disk(name).unwrap();
    image(&[
        "create",
        "-f",
        "qcow2",
        primary.path.to_str().unwrap(),
        "1M",
    ]);
}

pub(super) fn create_overlay(store: &VmStore, vm: &str, name: &str) {
    let disk = store.snapshot_disk_metadata(vm, name).unwrap().unwrap();
    image(&[
        "create",
        "-f",
        "qcow2",
        "-F",
        &disk.backing_format,
        "-b",
        disk.backing_path.to_str().unwrap(),
        disk.overlay_path.to_str().unwrap(),
    ]);
    store.create_snapshot_disk(vm, name).unwrap();
}

pub(super) fn chain(path: &Path) -> serde_json::Value {
    let output = Command::new("qemu-img")
        .args([
            "info",
            "--output=json",
            "--backing-chain",
            path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
