use bridgevm_config::{Guest, VmManifest, VmMode};
use bridgevm_storage::VmStore;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT: AtomicU64 = AtomicU64::new(0);
#[derive(Debug, Clone, Copy)]
pub enum Invocation {
    Cli,
    Storage,
}
#[derive(Debug, Clone, Copy)]
pub enum Format {
    Directory,
    Tar,
}
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn tree(root: &Path) -> BTreeMap<PathBuf, String> {
    fn visit(root: &Path, current: &Path, result: &mut BTreeMap<PathBuf, String>) {
        for entry in fs::read_dir(current).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            if entry.file_type().unwrap().is_dir() {
                result.insert(relative, "directory".into());
                visit(root, &path, result);
            } else {
                result.insert(
                    relative,
                    hex::encode(Sha256::digest(fs::read(path).unwrap())),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

fn cli(store: &VmStore, args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bridgevm"))
        .arg("--store")
        .arg(store.root())
        .args(args)
        .output()
        .unwrap()
}

fn import(store: &VmStore, input: &Path, invocation: Invocation) -> (bool, String) {
    match invocation {
        Invocation::Cli => {
            let output = cli(
                store,
                &[
                    "import".as_ref(),
                    input.as_os_str(),
                    "--name".as_ref(),
                    "copy".as_ref(),
                ],
            );
            (
                output.status.success(),
                String::from_utf8(output.stderr).unwrap(),
            )
        }
        Invocation::Storage => match store.import_vm(input, Some("copy")) {
            Ok(_) => (true, String::new()),
            Err(error) => (false, error.to_string()),
        },
    }
}

fn extraction_stages() -> BTreeSet<String> {
    fs::read_dir(std::env::temp_dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("bridgevm-import-tar-"))
        .collect()
}

pub fn check(invocation: Invocation, format: Format, deleted: bool) {
    // The temporary extraction namespace is shared by this test process.
    static CASE_LOCK: Mutex<()> = Mutex::new(());
    let _guard = CASE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "bridgevm-tombstone-{}-{now}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    let _fixture = Fixture(root.clone());
    let source_store = VmStore::new(root.join("source-store"));
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
    let source = source_store.create_vm(&manifest).unwrap();
    fs::write(
        source.join("disks/root.raw"),
        b"synthetic preserved raw bytes",
    )
    .unwrap();
    let input = match format {
        Format::Directory => source.clone(),
        Format::Tar => {
            let archive = root.join("backup.tar");
            source_store.export_vm("source", &archive).unwrap();
            archive
        }
    };
    if deleted {
        match invocation {
            Invocation::Cli => assert!(cli(
                &source_store,
                &[
                    "delete".as_ref(),
                    "source".as_ref(),
                    "--metadata-only".as_ref()
                ]
            )
            .status
            .success()),
            Invocation::Storage => {
                source_store.delete_vm_metadata_only("source").unwrap();
            }
        }
        assert!(source_store.get_vm("source").is_err());
        assert!(source_store.list_vms().unwrap().is_empty());
        if matches!(format, Format::Tar) {
            // Add the actual deletion records to the prior portable export.
            // The healthy control uses the identical export without these records.
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
    let source_before = tree(&source);
    let input_before = if input.is_file() {
        Some(Sha256::digest(fs::read(&input).unwrap()))
    } else {
        None
    };
    let target = VmStore::new(root.join("target-store"));
    target.ensure().unwrap();
    let target_before = tree(target.root());
    let stages_before = extraction_stages();
    let first = import(&target, &input, invocation);
    let get_ok = target.get_vm("copy").is_ok();
    let listed = target
        .list_vms()
        .unwrap()
        .iter()
        .any(|(_, manifest)| manifest.name == "copy");
    let status_ok = cli(&target, &["status".as_ref(), "copy".as_ref()])
        .status
        .success();
    let listing = cli(&target, &["list".as_ref()]);
    assert!(listing.status.success());
    let cli_listed = String::from_utf8(listing.stdout)
        .unwrap()
        .lines()
        .any(|line| line.starts_with("copy\t"));
    let target_after_first = tree(target.root());
    let repeat = import(&target, &input, invocation);
    let source_preserved = tree(&source) == source_before;
    let input_preserved =
        input_before.is_none_or(|before| Sha256::digest(fs::read(&input).unwrap()) == before);
    let target_preserved_on_repeat = tree(target.root()) == target_after_first;
    let stages_after = extraction_stages();
    let stages_removed = stages_after == stages_before
        && !target_after_first
            .keys()
            .any(|path| path.to_string_lossy().contains(".bridgevm-import"));
    let refused_without_output = !first.0 && target_after_first == target_before;
    eprintln!(
        "observation: {}",
        serde_json::json!({
            "invocation":format!("{invocation:?}"),"format":format!("{format:?}"),"deleted":deleted,
            "first_success":first.0,"get_ok":get_ok,"listed":listed,"cli_status_ok":status_ok,"cli_listed":cli_listed,
            "repeat_success":repeat.0,"repeat_occupied":repeat.1.contains("already exists"),
            "source_preserved":source_preserved,"input_preserved":input_preserved,"destination_preserved_on_repeat":target_preserved_on_repeat,
            "stages_removed":stages_removed,"refused_without_output":refused_without_output,
            "source_snapshot":source_before,"destination_before":target_before,"destination_snapshot":target_after_first,
            "input_archive_sha256":input_before.map(hex::encode),"extraction_stages_before":stages_before,"extraction_stages_after":stages_after
        })
    );
    assert!(source_preserved && input_preserved && target_preserved_on_repeat && stages_removed);
    assert!(!repeat.0);
    if !deleted {
        assert!(first.0, "healthy import must remain accepted");
    }
    assert!(
        (first.0 && get_ok && listed && status_ok && cli_listed) || refused_without_output,
        "import must either register a visible/readable VM or refuse without occupying its destination"
    );
}
