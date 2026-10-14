#![cfg(unix)]

use bridgevm_config::{Guest, VmManifest, VmMode};
use bridgevm_storage::VmStore;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, String> {
    fn visit(root: &Path, current: &Path, result: &mut BTreeMap<PathBuf, String>) {
        for entry in fs::read_dir(current).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, result);
            } else {
                result.insert(
                    path.strip_prefix(root).unwrap().into(),
                    hex::encode(Sha256::digest(fs::read(path).unwrap())),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

fn cli(store: &VmStore, temporary: &Path, args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bridgevm"))
        .env("TMPDIR", temporary)
        .arg("--store")
        .arg(store.root())
        .args(args)
        .output()
        .unwrap()
}

fn check(tar: bool, deleted: bool) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "bridgevm-import-name-{}-{now}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    let _fixture = Fixture(root.clone());
    let temporary = root.join("temporary");
    fs::create_dir(&temporary).unwrap();
    let source_store = VmStore::new(root.join("source-store"));
    let mut manifest = VmManifest::new(
        "original",
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
    let source = source_store.create_vm(&manifest).unwrap();
    fs::write(source.join("disks/root.raw"), b"synthetic raw bytes").unwrap();
    let input = if tar {
        let archive = root.join("backup.tar");
        source_store.export_vm("original", &archive).unwrap();
        archive
    } else {
        source.clone()
    };
    if deleted {
        assert!(cli(
            &source_store,
            &temporary,
            &[
                "delete".as_ref(),
                "original".as_ref(),
                "--metadata-only".as_ref()
            ]
        )
        .status
        .success());
        if tar {
            assert!(Command::new("/usr/bin/tar")
                .arg("-rf")
                .arg(&input)
                .arg("-C")
                .arg(&source)
                .args(["metadata/deletion.json", "metadata/deleted-manifest.yaml"])
                .status()
                .unwrap()
                .success());
        }
    }
    let source_before = snapshot(&source);
    let archive_before = tar.then(|| Sha256::digest(fs::read(&input).unwrap()));
    let target = VmStore::new(root.join("target-store"));
    target.ensure().unwrap();
    let args = ["import".as_ref(), input.as_os_str()];
    let first = cli(&target, &temporary, &args);
    assert_eq!(first.status.success(), !deleted);
    if deleted {
        assert!(String::from_utf8(first.stderr)
            .unwrap()
            .contains("metadata/deletion.json"));
        assert!(!target.bundle_path("original").exists());
        assert!(target.list_vms().unwrap().is_empty());
    } else {
        assert_eq!(target.get_vm("original").unwrap().1.name, "original");
        assert_eq!(target.list_vms().unwrap().len(), 1);
        let status = cli(
            &target,
            &temporary,
            &["status".as_ref(), "original".as_ref()],
        );
        assert!(status.status.success());
        assert_eq!(
            fs::read(target.active_disk("original").unwrap().path).unwrap(),
            b"synthetic raw bytes"
        );
    }
    let target_before_repeat = snapshot(target.root());
    assert!(!cli(&target, &temporary, &args).status.success());
    assert_eq!(snapshot(target.root()), target_before_repeat);
    assert_eq!(snapshot(&source), source_before);
    if let Some(before) = archive_before {
        assert_eq!(Sha256::digest(fs::read(input).unwrap()), before);
    }
    assert_eq!(fs::read_dir(&temporary).unwrap().count(), 0);
    assert!(fs::read_dir(target.vms_dir()).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".bridgevm-import")));
}

#[test]
fn healthy_directory_import_keeps_original_name() {
    check(false, false);
}
#[test]
fn healthy_tar_import_keeps_original_name() {
    check(true, false);
}
#[test]
fn deleted_directory_without_rename_refuses() {
    check(false, true);
}
#[test]
fn deleted_tar_without_rename_refuses() {
    check(true, true);
}
