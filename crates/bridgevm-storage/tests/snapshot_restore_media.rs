#![cfg(unix)]

use bridgevm_config::{Guest, VmManifest, VmMode};
use bridgevm_storage::{SnapshotKind, StorageError, VmStore};
use std::ffi::CString;
use std::fs;
use std::os::unix::{ffi::OsStrExt, fs::symlink};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[derive(Clone, Copy, Debug)]
enum Media {
    Directory,
    DirectoryAlias,
    Fifo,
    Missing,
    File,
    FileAlias,
}

fn restore(kind: SnapshotKind, media: Media, accepted: bool) {
    let fixture = Fixture(std::env::temp_dir().join(format!(
        "bridgevm-restore-media-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    fs::create_dir(&fixture.0).unwrap();
    let store = VmStore::new(fixture.0.join("store"));
    let mut manifest = VmManifest::new(
        "synthetic",
        VmMode::Fast,
        Guest {
            os: "ubuntu".into(),
            version: None,
            arch: "arm64".into(),
        },
        "4KiB",
    );
    manifest.storage.primary.path = "disks/root.raw".into();
    manifest.storage.primary.format = "raw".into();
    let bundle = store.create_vm(&manifest).unwrap();
    let primary = bundle.join("disks/root.raw");
    fs::write(&primary, b"synthetic raw media").unwrap();
    store
        .create_snapshot("synthetic", "baseline", SnapshotKind::Disk)
        .unwrap();
    store.restore_snapshot("synthetic", "baseline").unwrap();
    store.create_snapshot("synthetic", "target", kind).unwrap();
    let path = match kind {
        SnapshotKind::Disk => primary,
        SnapshotKind::Suspend => {
            store
                .snapshot_suspend_image_metadata("synthetic", "target")
                .unwrap()
                .unwrap()
                .image_path
        }
        SnapshotKind::ApplicationConsistent => unreachable!(),
    };
    if path.exists() {
        fs::remove_file(&path).unwrap();
    }
    let target = fixture.0.join("alias-target");
    match media {
        Media::Directory => {
            fs::create_dir(&path).unwrap();
        }
        Media::DirectoryAlias => {
            fs::create_dir(&target).unwrap();
            symlink(&target, &path).unwrap();
        }
        Media::Fifo => {
            let name = CString::new(path.as_os_str().as_bytes()).unwrap();
            // SAFETY: name is a valid NUL-terminated path; no file is opened.
            assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        }
        Media::Missing => {}
        Media::File => {
            fs::write(&path, b"synthetic saved media").unwrap();
        }
        Media::FileAlias => {
            fs::write(&target, b"synthetic saved media").unwrap();
            symlink(&target, &path).unwrap();
        }
    };
    let metadata_paths = ["active-disk.json", "state.json", "last-restore.json"];
    let before = metadata_paths.map(|name| fs::read(bundle.join("metadata").join(name)).unwrap());
    let result = store.restore_snapshot("synthetic", "target");
    let after = metadata_paths.map(|name| fs::read(bundle.join("metadata").join(name)).unwrap());
    println!(
        "kind={kind:?} media={media:?} result={result:?} selected_state_and_receipt_unchanged={}",
        before == after
    );
    assert_eq!(
        result.is_ok(),
        accepted,
        "restore admission for {kind:?}/{media:?}"
    );
    if accepted {
        assert_eq!(
            store.last_restore("synthetic").unwrap().unwrap().snapshot,
            "target"
        );
        assert!(path.is_file());
        assert_eq!(fs::read(&path).unwrap(), b"synthetic saved media");
    } else {
        match result.unwrap_err() {
            StorageError::SnapshotDiskBackingMissing(actual) if kind == SnapshotKind::Disk => {
                assert_eq!(actual, path)
            }
            StorageError::SnapshotSuspendImageMissing(actual) if kind == SnapshotKind::Suspend => {
                assert_eq!(actual, path)
            }
            other => panic!("unexpected refusal: {other:?}"),
        }
        assert_eq!(
            before, after,
            "refusal must preserve selection, runtime state and prior receipt"
        );
        if kind == SnapshotKind::Suspend {
            assert!(
                !store
                    .snapshot_suspend_image_metadata("synthetic", "target")
                    .unwrap()
                    .unwrap()
                    .image_exists
            );
        }
    }
}

macro_rules! case {
    ($name:ident, $kind:ident, $media:ident, $accepted:expr) => {
        #[test]
        fn $name() {
            restore(SnapshotKind::$kind, Media::$media, $accepted);
        }
    };
}

case!(disk_directory_is_refused, Disk, Directory, false);
case!(disk_directory_alias_is_refused, Disk, DirectoryAlias, false);
case!(disk_fifo_is_refused, Disk, Fifo, false);
case!(disk_missing_is_refused, Disk, Missing, false);
case!(disk_regular_file_is_accepted, Disk, File, true);
case!(disk_regular_file_alias_is_accepted, Disk, FileAlias, true);
case!(suspend_directory_is_refused, Suspend, Directory, false);
case!(
    suspend_directory_alias_is_refused,
    Suspend,
    DirectoryAlias,
    false
);
case!(suspend_fifo_is_refused, Suspend, Fifo, false);
case!(suspend_missing_is_refused, Suspend, Missing, false);
case!(suspend_regular_file_is_accepted, Suspend, File, true);
case!(
    suspend_regular_file_alias_is_accepted,
    Suspend,
    FileAlias,
    true
);
