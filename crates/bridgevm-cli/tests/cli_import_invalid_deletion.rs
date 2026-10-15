#![cfg(unix)]

use bridgevm_config::{Guest, VmManifest, VmMode};
use bridgevm_storage::VmStore;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
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
    fn visit(root: &Path, current: &Path, files: &mut BTreeMap<PathBuf, String>) {
        for entry in fs::read_dir(current).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root).unwrap().into(),
                    hex::encode(Sha256::digest(fs::read(path).unwrap())),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

fn source(store: &VmStore, name: &str) -> PathBuf {
    let mut manifest = VmManifest::new(
        name,
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
    fs::write(bundle.join("disks/root.raw"), b"synthetic raw data").unwrap();
    bundle
}

fn check(use_cli: bool) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "bridgevm-invalid-deletion-{}-{now}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    let _fixture = Fixture(root.clone());
    let input = source(&VmStore::new(root.join("source")), "source");
    fs::write(input.join("metadata/deletion.json"), b"{malformed deletion").unwrap();
    let input_before = snapshot(&input);
    let target = VmStore::new(root.join("target"));
    let healthy = source(&target, "healthy");
    let healthy_before = snapshot(&healthy);
    let imported = if use_cli {
        Command::new(env!("CARGO_BIN_EXE_bridgevm"))
            .arg("--store")
            .arg(target.root())
            .arg("import")
            .arg(&input)
            .args(["--name", "copy"])
            .output()
            .unwrap()
            .status
            .success()
    } else {
        target.import_vm(&input, Some("copy")).is_ok()
    };
    let list = target.list_vms();
    let listed = list.as_ref().is_ok_and(|entries| {
        entries
            .iter()
            .any(|(_, manifest)| manifest.name == "healthy")
    });
    let cli_list = Command::new(env!("CARGO_BIN_EXE_bridgevm"))
        .arg("--store")
        .arg(target.root())
        .arg("list")
        .output()
        .unwrap();
    let cli_listed = cli_list.status.success()
        && String::from_utf8(cli_list.stdout)
            .unwrap()
            .lines()
            .any(|line| line.starts_with("healthy\t"));
    let input_preserved = snapshot(&input) == input_before;
    let healthy_preserved = snapshot(&healthy) == healthy_before;
    let output_exists = target.bundle_path("copy").exists();
    let stages_removed = fs::read_dir(target.vms_dir()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".bridgevm-import")
    });
    eprintln!(
        "invalid-deletion observation: {}",
        serde_json::json!({
            "cli":use_cli,"imported":imported,"list_ok":list.is_ok(),"healthy_listed":listed,"cli_healthy_listed":cli_listed,
            "healthy_get_ok":target.get_vm("healthy").is_ok(),"input_preserved":input_preserved,"healthy_preserved":healthy_preserved,
            "output_exists":output_exists,"stages_removed":stages_removed,"input_snapshot":input_before,"healthy_snapshot":healthy_before,
            "output_snapshot":if output_exists { Some(snapshot(&target.bundle_path("copy"))) } else { None }
        })
    );
    assert!(input_preserved && healthy_preserved && stages_removed);
    assert!(
        !imported && !output_exists && listed && cli_listed,
        "invalid deletion metadata must not publish an entry that poisons existing inventory"
    );
}

#[test]
fn public_invalid_deletion_import_preserves_existing_inventory() {
    check(true);
}

#[test]
fn storage_invalid_deletion_import_preserves_existing_inventory() {
    check(false);
}
