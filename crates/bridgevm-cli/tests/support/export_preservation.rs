use bridgevm_config::{Guest, VmManifest, VmMode};
use bridgevm_storage::VmStore;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{symlink, MetadataExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT: AtomicU64 = AtomicU64::new(0);
pub enum Invocation {
    Cli,
    Storage,
}
pub enum Destination {
    DanglingTar,
    DanglingDirectory,
    ExistingTar,
    AbsentTar,
}
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn files(root: &Path) -> BTreeMap<PathBuf, [u8; 32]> {
    fn visit(root: &Path, current: &Path, files: &mut BTreeMap<PathBuf, [u8; 32]>) {
        for entry in fs::read_dir(current).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root).unwrap().into(),
                    Sha256::digest(fs::read(path).unwrap()).into(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

pub fn check(invocation: Invocation, destination: Destination) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "bridgevm-export-preservation-{}-{now}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    let _fixture = Fixture(root.clone());
    let store = VmStore::new(root.join("store"));
    let mut manifest = VmManifest::new(
        "source",
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
    let source = store.create_vm(&manifest).unwrap();
    fs::write(source.join("disks/root.raw"), b"synthetic export source").unwrap();
    let before = files(&source);
    let output = root.join(if matches!(destination, Destination::DanglingDirectory) {
        "copy.vmbridge"
    } else {
        "copy.tar"
    });
    let link_target = source.join("must-not-be-created");
    let identity = match destination {
        Destination::DanglingTar | Destination::DanglingDirectory => {
            symlink(&link_target, &output).unwrap();
            let metadata = fs::symlink_metadata(&output).unwrap();
            Some((metadata.dev(), metadata.ino()))
        }
        Destination::ExistingTar => {
            fs::write(&output, b"preexisting caller archive").unwrap();
            None
        }
        Destination::AbsentTar => None,
    };
    let success = match invocation {
        Invocation::Cli => Command::new(env!("CARGO_BIN_EXE_bridgevm"))
            .arg("--store")
            .arg(store.root())
            .args(["export", "source", "--output"])
            .arg(&output)
            .output()
            .unwrap()
            .status
            .success(),
        Invocation::Storage => store.export_vm("source", &output).is_ok(),
    };
    assert_eq!(
        (success, files(&source) == before, link_target.exists()),
        (matches!(destination, Destination::AbsentTar), true, false),
        "export must refuse occupied output and preserve all source bytes"
    );
    match destination {
        Destination::DanglingTar | Destination::DanglingDirectory => {
            let current = fs::symlink_metadata(&output).unwrap();
            assert!(current.file_type().is_symlink());
            assert_eq!(Some((current.dev(), current.ino())), identity);
            assert_eq!(fs::read_link(&output).unwrap(), link_target);
        }
        Destination::ExistingTar => {
            assert_eq!(fs::read(output).unwrap(), b"preexisting caller archive");
        }
        Destination::AbsentTar => {
            let imported = VmStore::new(root.join("target"));
            imported.import_vm(&output, Some("imported")).unwrap();
            assert_eq!(
                fs::read(imported.active_disk("imported").unwrap().path).unwrap(),
                b"synthetic export source"
            );
        }
    }
}
