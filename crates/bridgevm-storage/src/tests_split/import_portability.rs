//! Portable imports own every selected/backing path and publish only on success.

use super::helpers::{manifest, temp_store};
use super::import_images::{chain, create_overlay, create_primary, image};
use super::snapshot_names::files_under;
use crate::*;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Barrier};

fn relative_files(root: &Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    files_under(root)
        .into_iter()
        .map(|(path, bytes)| (path.strip_prefix(root).unwrap().to_path_buf(), bytes))
        .collect()
}

fn populated() -> VmStore {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    create_primary(&store, "dev");
    for name in ["one", "two"] {
        store
            .create_snapshot("dev", name, SnapshotKind::Disk)
            .unwrap();
        create_overlay(&store, "dev", name);
    }
    store
}

fn assert_no_staging(store: &VmStore) {
    assert!(fs::read_dir(store.vms_dir()).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".bridgevm-import")));
}

#[test]
fn directory_and_tar_imports_relocate_real_backing_chains_after_sources_disappear() {
    for extension in ["vmbridge", "tar"] {
        let source = populated();
        let bundle = source.bundle_path("dev");
        let original = relative_files(&bundle);
        let token = source.guest_tools_token("dev").unwrap();
        let exported = source.root().join(format!("export.{extension}"));
        source.export_vm("dev", &exported).unwrap();
        let target = temp_store();
        let first = fs::canonicalize(target.import_vm(&exported, None).unwrap().output).unwrap();
        let second =
            fs::canonicalize(target.import_vm(&exported, Some("copy")).unwrap().output).unwrap();
        assert_eq!(relative_files(&bundle), original);
        let withdrawn = source.root().with_extension("unavailable");
        fs::rename(source.root(), &withdrawn).unwrap();
        for (name, imported) in [("dev", &first), ("copy", &second)] {
            assert_eq!(target.guest_tools_token(name).unwrap(), token);
            let active = target.active_disk(name).unwrap();
            assert_eq!(active.path, imported.join("disks/snapshots/two.qcow2"));
            assert!(active.exists);
            let actual = chain(&active.path);
            assert_eq!(actual.as_array().unwrap().len(), 3);
            assert!(actual
                .as_array()
                .unwrap()
                .iter()
                .all(|entry| Path::new(entry["filename"].as_str().unwrap()).starts_with(imported)));
            let restored = target
                .restore_snapshot(name, "two")
                .unwrap()
                .active_disk
                .unwrap();
            assert_eq!(restored.path, imported.join("disks/snapshots/one.qcow2"));
            assert!(restored.exists);
        }
        assert!(
            relative_files(&withdrawn.join("vms/dev.vmbridge")) == original,
            "source contents changed"
        );
        assert_no_staging(&target);
        fs::remove_dir_all(withdrawn).unwrap();
        fs::remove_dir_all(target.root()).unwrap();
    }
}

#[test]
fn unresolved_backing_refuses_import_cleans_staging_and_allows_corrected_retry() {
    let source = populated();
    let exported = source.root().join("export.vmbridge");
    source.export_vm("dev", &exported).unwrap();
    let overlay = exported.join("disks/snapshots/two.qcow2");
    image(&[
        "rebase",
        "-u",
        "-f",
        "qcow2",
        "-F",
        "qcow2",
        "-b",
        "/unrelated/not-copied.qcow2",
        overlay.to_str().unwrap(),
    ]);
    let before = relative_files(&exported);
    let target = temp_store();
    let error = target.import_vm(&exported, Some("copy")).unwrap_err();
    assert!(error.to_string().contains("unresolved"));
    assert_eq!(relative_files(&exported), before);
    assert!(!target.bundle_path("copy").exists());
    assert_no_staging(&target);
    let backing = source.bundle_path("dev").join("disks/snapshots/one.qcow2");
    image(&[
        "rebase",
        "-u",
        "-f",
        "qcow2",
        "-F",
        "qcow2",
        "-b",
        backing.to_str().unwrap(),
        overlay.to_str().unwrap(),
    ]);
    target.import_vm(&exported, Some("copy")).unwrap();
    assert_no_staging(&target);
}

#[test]
fn corrupt_media_and_operative_metadata_never_publish_a_success_receipt() {
    for relative in ["metadata/active-disk.json", "disks/root.qcow2"] {
        let source = populated();
        let exported = source.root().join("export.vmbridge");
        source.export_vm("dev", &exported).unwrap();
        fs::write(exported.join(relative), b"invalid").unwrap();
        let before = relative_files(&exported);
        let target = temp_store();
        assert!(target.import_vm(&exported, Some("copy")).is_err());
        assert!(!target.bundle_path("copy").exists());
        assert_no_staging(&target);
        assert_eq!(relative_files(&exported), before);
    }
}

#[test]
fn unsafe_manifest_path_is_refused_without_touching_an_external_file() {
    let source = temp_store();
    let bundle = source.create_vm(&manifest("dev")).unwrap();
    let external = source.root().join("outside.raw");
    fs::write(&external, b"untouched").unwrap();
    let path = bundle.join("manifest.yaml");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("disks/root.qcow2", "../../outside.raw");
    fs::write(path, text).unwrap();
    let target = temp_store();
    assert!(target.import_vm(&bundle, None).is_err());
    assert_eq!(fs::read(external).unwrap(), b"untouched");
    assert_no_staging(&target);
}

#[test]
fn publication_never_replaces_an_unrelated_destination_or_a_concurrent_winner() {
    let store = temp_store();
    store.ensure().unwrap();
    let destination = store.root().join("published");
    fs::create_dir(&destination).unwrap();
    let staged = store.root().join("staged");
    fs::create_dir(&staged).unwrap();
    fs::write(staged.join("identity"), b"import").unwrap();
    assert!(crate::import_publish::publish(&staged, &destination).is_err());
    assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
    fs::write(destination.join("identity"), b"unrelated").unwrap();
    assert!(crate::import_publish::publish(&staged, &destination).is_err());
    assert_eq!(
        fs::read(destination.join("identity")).unwrap(),
        b"unrelated"
    );
    assert_eq!(fs::read(staged.join("identity")).unwrap(), b"import");
    fs::remove_dir_all(&destination).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let handles = (0..2)
        .map(|number| {
            let staging = store.root().join(format!("candidate-{number}"));
            fs::create_dir(&staging).unwrap();
            fs::write(staging.join("identity"), number.to_string()).unwrap();
            let destination = destination.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                (
                    number,
                    crate::import_publish::publish(&staging, &destination).is_ok(),
                    staging,
                )
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|(_, ok, _)| *ok).count(), 1);
    let winner = results.iter().find(|(_, ok, _)| *ok).unwrap().0;
    assert_eq!(
        fs::read_to_string(destination.join("identity")).unwrap(),
        winner.to_string()
    );
    let loser = results.iter().find(|(_, ok, _)| !*ok).unwrap();
    assert_eq!(
        fs::read_to_string(loser.2.join("identity")).unwrap(),
        loser.0.to_string()
    );
}

#[test]
fn copied_backing_cycle_is_refused_before_any_header_mutation() {
    let source = temp_store();
    let bundle = source.create_vm(&manifest("dev")).unwrap();
    create_primary(&source, "dev");
    let first = bundle.join("disks/root.qcow2");
    let second = bundle.join("disks/second.qcow2");
    image(&[
        "create",
        "-f",
        "qcow2",
        "-F",
        "qcow2",
        "-b",
        first.to_str().unwrap(),
        second.to_str().unwrap(),
    ]);
    image(&[
        "rebase",
        "-u",
        "-f",
        "qcow2",
        "-F",
        "qcow2",
        "-b",
        second.to_str().unwrap(),
        first.to_str().unwrap(),
    ]);
    let exported = source.root().join("export.vmbridge");
    source.export_vm("dev", &exported).unwrap();
    let before = relative_files(&exported);
    let target = temp_store();
    let error = target.import_vm(&exported, None).unwrap_err();
    assert!(error.to_string().contains("cyclic"), "{error}");
    assert_eq!(relative_files(&exported), before);
    assert!(!target.bundle_path("dev").exists());
    assert_no_staging(&target);
}

#[cfg(unix)]
#[test]
fn restricted_copied_directories_remain_private_and_cleanup_on_failure() {
    use std::os::unix::fs::PermissionsExt;
    let source = temp_store();
    let bundle = source.create_vm(&manifest("dev")).unwrap();
    let metadata = bundle.join("metadata");
    fs::set_permissions(&metadata, fs::Permissions::from_mode(0o500)).unwrap();
    let target = temp_store();
    let imported = target.import_vm(&bundle, Some("copy")).unwrap().output;
    assert_eq!(
        fs::metadata(imported.join("metadata"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o500
    );
    fs::set_permissions(&metadata, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(metadata.join("active-disk.json"), b"invalid").unwrap();
    fs::set_permissions(&metadata, fs::Permissions::from_mode(0o500)).unwrap();
    assert!(target.import_vm(&bundle, Some("failed")).is_err());
    assert!(!target.bundle_path("failed").exists());
    assert_no_staging(&target);
    assert_eq!(
        fs::metadata(&metadata).unwrap().permissions().mode() & 0o777,
        0o500
    );
    fs::set_permissions(&metadata, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn renamed_import_rekeys_fast_saved_state_and_relocates_named_suspend_images() {
    let source = temp_store();
    let bundle = source.create_vm(&manifest("dev")).unwrap();
    let image = bundle.join("metadata/saved-state.bin");
    fs::write(&image, b"synthetic saved state").unwrap();
    source
        .mark_fast_suspend_image_exists("dev", &image)
        .unwrap();
    source
        .create_snapshot("dev", "paused", SnapshotKind::Suspend)
        .unwrap();
    let named = source
        .snapshot_suspend_image_metadata("dev", "paused")
        .unwrap()
        .unwrap();
    fs::write(&named.image_path, b"named suspend bytes").unwrap();
    let before = relative_files(&bundle);
    let target = temp_store();
    let imported =
        fs::canonicalize(target.import_vm(&bundle, Some("copy")).unwrap().output).unwrap();
    let fast = target
        .fast_suspend_image_metadata("copy")
        .unwrap()
        .expect("renamed import must retain operative saved-state lookup");
    assert_eq!(fast.vm, "copy");
    assert_eq!(fast.image_path, imported.join("metadata/saved-state.bin"));
    assert_eq!(fs::read(fast.image_path).unwrap(), b"synthetic saved state");
    let named = target
        .snapshot_suspend_image_metadata("copy", "paused")
        .unwrap()
        .unwrap();
    assert!(named.image_exists);
    assert_eq!(fs::read(named.image_path).unwrap(), b"named suspend bytes");
    assert!(relative_files(&bundle) == before, "source changed");
}

#[test]
fn unpublished_copy_is_invisible_to_vm_listing_until_atomic_publication() {
    let store = temp_store();
    store.ensure().unwrap();
    let staged = crate::import_staging::Staging::create(&store.vms_dir()).unwrap();
    fs::create_dir(&staged.bundle).unwrap();
    super::helpers::manifest("hidden")
        .write(&staged.bundle.join("manifest.yaml"))
        .unwrap();
    assert!(store.list_vms().unwrap().is_empty());
    crate::import_publish::publish(&staged.bundle, &store.bundle_path("hidden")).unwrap();
    assert_eq!(store.list_vms().unwrap().len(), 1);
    drop(staged);
    assert_no_staging(&store);
}

#[test]
fn missing_recorded_materialized_primary_refuses_import_while_blank_template_remains_valid() {
    let source = temp_store();
    let bundle = source.create_vm(&manifest("dev")).unwrap();
    create_primary(&source, "dev");
    source.prepare_active_disk("dev").unwrap();
    let exported = source.root().join("export.vmbridge");
    source.export_vm("dev", &exported).unwrap();
    fs::remove_file(exported.join("disks/root.qcow2")).unwrap();
    let before = relative_files(&exported);
    let target = temp_store();
    assert!(target.import_vm(&exported, Some("lost")).is_err());
    assert!(!target.bundle_path("lost").exists());
    assert_no_staging(&target);
    assert!(relative_files(&exported) == before, "input changed");
    source.create_vm(&manifest("blank")).unwrap();
    target
        .import_vm(source.bundle_path("blank"), Some("blank-copy"))
        .unwrap();
    assert!(!target.active_disk("blank-copy").unwrap().exists);
    assert!(bundle.exists());
}
