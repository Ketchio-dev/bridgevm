//! Clone identity and manifest controls use valid tiny raw media.

use super::helpers::*;
use crate::*;
use std::fs;

#[test]
fn full_clone_copies_disk_and_manifest_with_new_name_and_hostname() {
    let store = temp_store();
    store
        .create_vm(&super::clone_fixture::raw_manifest("dev"))
        .unwrap();
    let primary = store.prepare_primary_disk("dev").unwrap();
    fs::write(&primary.path, b"fake disk contents").unwrap();

    let clone = store.clone_vm("dev", "dev-copy", false).unwrap();
    assert!(!clone.linked);

    let (clone_bundle, clone_manifest) = store.get_vm("dev-copy").unwrap();
    assert_eq!(clone_manifest.name, "dev-copy");
    assert_eq!(clone_manifest.network.hostname, "dev-copy.bridgevm.local");

    // Primary disk file was copied into the clone bundle (independent file).
    let clone_disk = resolve_bundle_path(&clone_bundle, &clone_manifest.storage.primary.path);
    assert!(clone_disk.starts_with(&clone_bundle));
    assert_eq!(fs::read(&clone_disk).unwrap(), b"fake disk contents");
    assert_ne!(clone_disk, primary.path);

    // Source is untouched.
    let (_, source_manifest) = store.get_vm("dev").unwrap();
    assert_eq!(source_manifest.name, "dev");
    assert_eq!(source_manifest.network.hostname, "dev.bridgevm.local");
}

#[test]
fn full_clone_resets_per_vm_identity_and_runtime_state() {
    let store = temp_store();
    let source_bundle = store
        .create_vm(&super::clone_fixture::raw_manifest("dev"))
        .unwrap();
    let primary = store.prepare_primary_disk("dev").unwrap();
    fs::write(&primary.path, b"fake disk").unwrap();

    // Persisted per-VM identity written by the Apple VZ runner.
    let source_metadata = source_bundle.join("metadata");
    fs::write(source_metadata.join("machine-identifier.bin"), b"source-id").unwrap();
    fs::write(
        source_metadata.join("network-mac-address.txt"),
        b"52:54:00:aa:bb:cc",
    )
    .unwrap();
    // Transient runtime state that must not be inherited.
    fs::write(source_metadata.join("runner.json"), b"{}").unwrap();
    let fast_suspend = source_metadata.join("suspend-images");
    fs::create_dir_all(&fast_suspend).unwrap();
    fs::write(fast_suspend.join("dev.bin"), b"saved-state").unwrap();
    store
        .write_state_at(&source_bundle, VmRuntimeState::Suspended)
        .unwrap();
    let source_token = store.guest_tools_token("dev").unwrap();

    let clone_bundle = store.clone_vm("dev", "dev-copy", false).unwrap().output;
    let clone_metadata = clone_bundle.join("metadata");

    // Identity dropped: regenerated fresh on next launch, not the source's.
    assert!(!clone_metadata.join("machine-identifier.bin").exists());
    assert!(!clone_metadata.join("network-mac-address.txt").exists());
    // Transient runtime state excluded.
    assert!(!clone_metadata.join("runner.json").exists());
    assert!(!clone_metadata.join("suspend-images").exists());

    // Guest-tools token regenerated (distinct credential).
    let clone_token = store.guest_tools_token("dev-copy").unwrap();
    assert_ne!(clone_token.token, source_token.token);

    // Clone starts stopped/clean even though the source was suspended.
    let clone_state = store.state("dev-copy").unwrap();
    assert_eq!(clone_state.state, VmRuntimeState::Stopped);

    // Source identity is preserved.
    assert!(source_metadata.join("machine-identifier.bin").exists());
    assert!(source_metadata.join("network-mac-address.txt").exists());
}

#[test]
fn full_clone_is_independent_of_source() {
    let store = temp_store();
    store
        .create_vm(&super::clone_fixture::raw_manifest("dev"))
        .unwrap();
    let primary = store.prepare_primary_disk("dev").unwrap();
    fs::write(&primary.path, b"fake disk").unwrap();

    let clone_bundle = store.clone_vm("dev", "dev-copy", false).unwrap().output;

    // Mutate the clone's manifest and confirm the source is unaffected.
    let (_, mut clone_manifest) = store.get_vm("dev-copy").unwrap();
    clone_manifest.guest.os = "windows".to_string();
    clone_manifest
        .write(&clone_bundle.join("manifest.yaml"))
        .unwrap();

    let (_, source_manifest) = store.get_vm("dev").unwrap();
    assert_eq!(source_manifest.guest.os, "ubuntu");
    let (_, reread_clone) = store.get_vm("dev-copy").unwrap();
    assert_eq!(reread_clone.guest.os, "windows");
}

#[test]
fn full_clone_rejects_existing_destination() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    store.create_vm(&manifest("taken")).unwrap();

    let error = store.clone_vm("dev", "taken", false).unwrap_err();
    assert!(matches!(error, StorageError::AlreadyExists(name) if name == "taken"));
    // Existing destination bundle is left intact.
    assert!(store.get_vm("taken").is_ok());
}

#[test]
fn full_clone_rejects_missing_source() {
    let store = temp_store();
    store.ensure().unwrap();

    let error = store.clone_vm("ghost", "dev-copy", false).unwrap_err();
    assert!(matches!(error, StorageError::NotFound(name) if name == "ghost"));
    assert!(!store.bundle_path("dev-copy").exists());
}
