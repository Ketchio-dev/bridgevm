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
    DanglingLink,
    ExistingDirectory,
    Absent,
}
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn files(root: &Path) -> BTreeMap<PathBuf, [u8; 32]> {
    let mut result = BTreeMap::new();
    fn visit(root: &Path, path: &Path, files: &mut BTreeMap<PathBuf, [u8; 32]>) {
        for entry in fs::read_dir(path).unwrap() {
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
    visit(root, root, &mut result);
    result
}

pub fn check(invocation: Invocation, destination: Destination) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "bridgevm-clone-preservation-{}-{now}-{}",
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
    manifest.validate().unwrap();
    let source = store.create_vm(&manifest).unwrap();
    fs::write(source.join("disks/root.raw"), b"synthetic raw source").unwrap();
    let before = files(&source);
    let output = store.bundle_path("copy");
    let link_target = root.join("never-created-target");
    let identity = match destination {
        Destination::DanglingLink => {
            symlink(&link_target, &output).unwrap();
            let m = fs::symlink_metadata(&output).unwrap();
            Some((m.dev(), m.ino()))
        }
        Destination::ExistingDirectory => {
            fs::create_dir(&output).unwrap();
            fs::write(output.join("sentinel"), b"preexisting caller content").unwrap();
            None
        }
        Destination::Absent => None,
    };
    let succeeded = match invocation {
        Invocation::Cli => {
            let result = Command::new(env!("CARGO_BIN_EXE_bridgevm"))
                .arg("--store")
                .arg(store.root())
                .args(["clone", "source", "copy"])
                .output()
                .unwrap();
            assert_eq!(
                result.status.code(),
                Some(if matches!(destination, Destination::Absent) {
                    0
                } else {
                    1
                })
            );
            result.status.success()
        }
        Invocation::Storage => store.clone_vm("source", "copy", false).is_ok(),
    };
    assert_eq!(files(&source), before, "clone changed source files");
    assert!(
        !link_target.exists(),
        "refused destination target was created"
    );
    match destination {
        Destination::DanglingLink => {
            assert!(!succeeded);
            let current = fs::symlink_metadata(&output)
                .expect("refused preexisting symbolic link must survive");
            assert!(current.file_type().is_symlink());
            assert_eq!(Some((current.dev(), current.ino())), identity);
            assert_eq!(fs::read_link(&output).unwrap(), link_target);
        }
        Destination::ExistingDirectory => {
            assert!(!succeeded);
            assert_eq!(
                fs::read(output.join("sentinel")).unwrap(),
                b"preexisting caller content"
            );
        }
        Destination::Absent => {
            assert!(succeeded);
            assert_eq!(
                fs::read(output.join("disks/root.raw")).unwrap(),
                b"synthetic raw source"
            );
            assert_eq!(store.get_vm("copy").unwrap().1.name, "copy");
        }
    }
}
