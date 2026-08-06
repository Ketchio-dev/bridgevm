use super::*;

use std::fs;
use std::path::{Path, PathBuf};

fn scratch(tag: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let disk = dir.join(format!("bv-vmb-{tag}-{pid}-disk.raw"));
    let vars = dir.join(format!("bv-vmb-{tag}-{pid}-vars.fd"));
    // Same-pid rerun can leave stale files AND stale sidecar locks.
    for path in [&disk, &vars] {
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(bridgevm_hvf::media::lock::lock_path_for(path));
        fs::write(path, b"image").unwrap();
    }
    (disk, vars)
}

fn manifest(disk: &Path, vars: &Path) -> LaunchManifest {
    LaunchManifest::parse(
        &format!(
            "{{\"version\": 1, \"disk\": \"{}\", \"uefi_vars\": \"{}\", \
             \"ram_mib\": 4096, \"vcpus\": 4}}",
            disk.display(),
            vars.display()
        ),
        false,
    )
    .unwrap()
}

#[test]
fn prepare_leases_both_images_and_drop_releases_them() {
    let (disk, vars) = scratch("lease");
    let prepared = prepare(manifest(&disk, &vars), "test-holder").unwrap();
    assert_eq!(prepared.manifest().ram_mib(), 4096);
    drop(prepared);
    // Released: a second prepare succeeds.
    prepare(manifest(&disk, &vars), "test-holder").unwrap();
}

#[test]
fn second_writer_fails_before_any_vm_exists_and_names_the_holder() {
    let (disk, vars) = scratch("second");
    let _first = prepare(manifest(&disk, &vars), "first-holder").unwrap();
    let err = prepare(manifest(&disk, &vars), "second-holder").unwrap_err();
    let message = err.to_string();
    assert!(message.contains("first-holder"), "{message}");
}

#[test]
fn prepared_vm_starts_at_a_fresh_generation_with_an_empty_queue() {
    let (disk, vars) = scratch("gen");
    let prepared = prepare(manifest(&disk, &vars), "test-holder").unwrap();
    let tag = prepared.generation().stamp();
    assert!(prepared.generation().is_current(tag));
    let drained = prepared.events().drain(prepared.generation());
    assert!(drained.events.is_empty());
    assert_eq!(drained.stale_discarded, 0);
}
