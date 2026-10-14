//! Real copied backing chains must remain usable without their original bundle.

use super::clone_fixture::source;
use super::import_images::chain;
use super::snapshot_names::files_under;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn independent(relative: &str, levels: usize, remove_active: bool) {
    let store = source(relative, levels);
    let source = store.bundle_path("source");
    if remove_active {
        fs::remove_file(source.join("metadata/active-disk.json")).unwrap();
    }
    let before = files_under(&source);
    let copied = store.clone_vm("source", "copy", false).unwrap().output;
    let active = store.active_disk("copy").unwrap();
    let copied_root = fs::canonicalize(&copied).unwrap();
    let actual_chain = chain(&active.path);
    assert_eq!(actual_chain.as_array().unwrap().len(), levels + 1);
    for member in actual_chain.as_array().unwrap() {
        assert!(
            fs::canonicalize(Path::new(member["filename"].as_str().unwrap()))
                .unwrap()
                .starts_with(&copied_root)
        );
    }
    let hidden = store.root().join("hidden-source");
    fs::rename(&source, &hidden).unwrap();
    let readback = store.root().join("readback.raw");
    let conversion = Command::new("qemu-img")
        .args(["convert", "-f", &active.format, "-O", "raw"])
        .arg(&active.path)
        .arg(&readback)
        .output();
    fs::rename(&hidden, &source).unwrap();
    let conversion = conversion.unwrap();
    assert!(
        conversion.status.success(),
        "{}",
        String::from_utf8_lossy(&conversion.stderr)
    );
    assert_eq!(fs::read(readback).unwrap(), vec![0x6du8; 4096]);
    assert_eq!(files_under(&source), before);
    fs::remove_dir_all(store.root()).unwrap();
}

#[test]
fn full_clone_owns_multilevel_qcow_backing_headers() {
    independent("disks/root.raw", 2, false);
}

#[test]
fn full_clone_preserves_literal_backslash_backing_names() {
    independent(r"disks/base\name.raw", 1, false);
}

#[test]
fn full_clone_raw_primary_without_active_metadata_stays_independent() {
    independent("disks/nested/raw.disk", 0, true);
}

#[test]
fn full_clone_refuses_uncopied_linked_backing_without_changing_sources() {
    let store = source("disks/root.raw", 0);
    let source = store.bundle_path("source");
    let before = files_under(&source);
    let linked = store.clone_vm("source", "linked", true).unwrap();
    assert!(linked.linked);
    assert_eq!(linked.backing_path, Some(source.join("disks/root.raw")));
    assert_eq!(
        chain(&store.active_disk("linked").unwrap().path)
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let linked_before = files_under(&linked.output);
    let error = store.clone_vm("linked", "copy", false).unwrap_err();
    assert!(error.to_string().contains("unresolved"), "{error}");
    assert!(!store.bundle_path("copy").exists());
    assert_eq!(files_under(&source), before);
    assert_eq!(files_under(&linked.output), linked_before);
    fs::remove_dir_all(store.root()).unwrap();
}

#[test]
fn full_clone_qcow_helper_failure_removes_only_owned_destination() {
    let store = source("disks/root.raw", 1);
    let source = store.bundle_path("source");
    let overlay = store.active_disk("source").unwrap().path;
    fs::set_permissions(&overlay, fs::Permissions::from_mode(0o444)).unwrap();
    let before = files_under(&source);
    let error = store.clone_vm("source", "copy", false).unwrap_err();
    assert!(
        error.to_string().contains("qemu-img rebase failed"),
        "{error}"
    );
    assert!(!store.bundle_path("copy").exists());
    assert_eq!(files_under(&source), before);
    assert_eq!(
        fs::metadata(&overlay).unwrap().permissions().mode() & 0o777,
        0o444
    );
    fs::remove_dir_all(store.root()).unwrap();
}
