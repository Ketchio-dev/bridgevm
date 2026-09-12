#![cfg(unix)]

use super::helpers::{manifest, temp_store};
use crate::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}
fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn fixture() -> (VmStore, std::path::PathBuf) {
    let store = temp_store();
    let bundle = store.create_vm(&manifest("permissions")).unwrap();
    let current = bundle.join("disks/.bridgevm-pair-v2-fixture/current");
    fs::create_dir_all(&current).unwrap();
    set_mode(&bundle, 0o700);
    set_mode(current.parent().unwrap(), 0o700);
    set_mode(&current, 0o700);
    fs::write(current.join("disk.raw"), b"selected disk").unwrap();
    fs::write(current.join("vars.fd"), b"selected vars").unwrap();
    (store, bundle)
}

fn assert_private_pair(bundle: &Path) {
    let current = bundle.join("disks/.bridgevm-pair-v2-fixture/current");
    assert_eq!(mode(bundle), 0o700);
    assert_eq!(mode(current.parent().unwrap()), 0o700);
    assert_eq!(mode(&current), 0o700);
    assert_eq!(
        fs::read(current.join("disk.raw")).unwrap(),
        b"selected disk"
    );
    assert_eq!(fs::read(current.join("vars.fd")).unwrap(), b"selected vars");
}

#[test]
fn public_directory_export_import_preserves_private_generations() {
    let (source, bundle) = fixture();
    let destination = temp_store();
    let exported = source.root().join("export.vmbridge");
    source.export_vm("permissions", &exported).unwrap();
    assert_private_pair(&exported);
    let imported = destination.import_vm(&exported, Some("imported")).unwrap();
    assert_private_pair(&imported.output);
    assert_private_pair(&bundle);
    fs::remove_dir_all(source.root()).unwrap();
    fs::remove_dir_all(destination.root()).unwrap();
}

#[test]
fn public_tar_export_import_preserves_private_generations() {
    let (source, bundle) = fixture();
    let destination = temp_store();
    let archive = source.root().join("export.tar");
    source.export_vm("permissions", &archive).unwrap();
    let imported = destination.import_vm(&archive, Some("imported")).unwrap();
    assert_private_pair(&imported.output);
    assert_private_pair(&bundle);
    fs::remove_dir_all(source.root()).unwrap();
    fs::remove_dir_all(destination.root()).unwrap();
}

#[test]
fn restrictive_source_directories_are_applied_after_copying_children() {
    let (source, bundle) = fixture();
    let restricted = bundle.join("readonly");
    fs::create_dir(&restricted).unwrap();
    fs::write(restricted.join("child"), b"retained").unwrap();
    set_mode(&restricted, 0o500);
    let output = source.root().join("copy");
    source.export_vm("permissions", &output).unwrap();
    assert_eq!(mode(&output.join("readonly")), 0o500);
    assert_eq!(
        fs::read(output.join("readonly/child")).unwrap(),
        b"retained"
    );
    set_mode(&restricted, 0o700);
    set_mode(&output.join("readonly"), 0o700);
    fs::remove_dir_all(source.root()).unwrap();
}

#[test]
fn deferred_archive_modes_strip_special_bits_and_allow_child_creation() {
    let store = temp_store();
    let root = store.root().join("extract");
    let mut modes = bundle_directory_permissions::DirectoryModes::default();
    modes.record(root.clone(), 0o7500).unwrap();
    let child = root.join("nested");
    modes.record(child.clone(), 0o700).unwrap();
    fs::write(child.join("file"), b"complete").unwrap();
    modes.apply().unwrap();
    assert_eq!(mode(&root), 0o500);
    assert_eq!(mode(&child), 0o700);
    assert_eq!(fs::read(child.join("file")).unwrap(), b"complete");
    set_mode(&root, 0o700);
    fs::remove_dir_all(store.root()).unwrap();
}
