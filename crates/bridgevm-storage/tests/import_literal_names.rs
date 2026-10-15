#![cfg(unix)]

use bridgevm_config::{Guest, VmManifest, VmMode};
use bridgevm_storage::VmStore;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

#[path = "import_literal_names/active.rs"]
mod active;

fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, result);
            } else {
                result.insert(
                    path.strip_prefix(root).unwrap().into(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

#[derive(Clone, Copy, Debug)]
enum Names {
    Plain,
    LiteralDisk,
    ExtraLiteral,
    Collision,
}

fn roundtrip(names: Names, archive: bool) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
    let fixture = std::env::temp_dir().join(format!(
        "bridgevm-literal-name-{}-{now}-{id}",
        std::process::id()
    ));
    fs::create_dir(&fixture).unwrap();
    let source = VmStore::new(fixture.join("source"));
    let target = VmStore::new(fixture.join("target"));
    let mut manifest = VmManifest::new(
        "raw-source",
        VmMode::Fast,
        Guest {
            os: "ubuntu".into(),
            version: None,
            arch: "arm64".into(),
        },
        "4KiB",
    );
    let relative = if matches!(names, Names::LiteralDisk) {
        r"disks/disk\name.raw"
    } else {
        "disks/disk-name.raw"
    };
    manifest.storage.primary.path = relative.into();
    manifest.storage.primary.format = "raw".into();
    manifest.validate().unwrap();
    let bundle = source.create_vm(&manifest).unwrap();
    fs::write(bundle.join(relative), b"owned raw content").unwrap();
    let mut extras = Vec::new();
    if matches!(names, Names::ExtraLiteral | Names::Collision) {
        extras.push((r"notes\detail.txt", b"literal notes".as_slice()));
    }
    if matches!(names, Names::Collision) {
        extras.push(("notes/detail.txt", b"nested notes".as_slice()));
    }
    for (name, content) in &extras {
        let path = bundle.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    let source_before = files(&bundle);
    let output = fixture.join(if archive {
        "export.tar"
    } else {
        "export.vmbridge"
    });
    let exported = source.export_vm("raw-source", &output).unwrap();
    let export_before = if archive { None } else { Some(files(&output)) };
    let archive_before = if archive {
        Some(fs::read(&output).unwrap())
    } else {
        None
    };
    let imported = target.import_vm(&output, Some("raw-copy"));
    assert_eq!(files(&bundle), source_before, "source changed");
    if let Some(before) = export_before {
        assert_eq!(files(&output), before, "export changed");
    }
    if let Some(before) = archive_before {
        assert_eq!(fs::read(&output).unwrap(), before, "archive changed");
    }
    println!("fixture={}; names={names:?}; archive={archive}; export_count={}; export_names={:?}; import_result={:?}",
        fixture.display(), exported.copied_file_count, exported.copied_files, imported.as_ref().map(|v| &v.output));
    let imported = imported.expect("exported valid raw bundle must import");
    assert_eq!(
        fs::read(imported.output.join(relative)).unwrap(),
        b"owned raw content"
    );
    assert!(imported.copied_files.contains(&relative.to_string()));
    assert!(exported.copied_files.contains(&relative.to_string()));
    for (name, content) in extras {
        assert_eq!(fs::read(imported.output.join(name)).unwrap(), content);
        assert!(imported.copied_files.contains(&name.to_string()));
        assert!(exported.copied_files.contains(&name.to_string()));
    }
    for receipt in [&exported.copied_files, &imported.copied_files] {
        assert_eq!(
            receipt
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            receipt.len()
        );
    }
    active::assert_imported_disk(&target, &imported.output.join(relative));
    fs::remove_dir_all(fixture).unwrap();
}

#[test]
fn plain_raw_name_directory_roundtrip() {
    roundtrip(Names::Plain, false);
}

#[test]
fn plain_raw_name_tar_roundtrip() {
    roundtrip(Names::Plain, true);
}

#[test]
fn literal_backslash_raw_name_directory_roundtrip() {
    roundtrip(Names::LiteralDisk, false);
}

#[test]
fn literal_backslash_raw_name_tar_roundtrip() {
    roundtrip(Names::LiteralDisk, true);
}

#[test]
fn literal_extra_file_directory_roundtrip() {
    roundtrip(Names::ExtraLiteral, false);
}

#[test]
fn literal_extra_file_tar_roundtrip() {
    roundtrip(Names::ExtraLiteral, true);
}

#[test]
fn literal_and_nested_names_directory_roundtrip() {
    roundtrip(Names::Collision, false);
}

#[test]
fn literal_and_nested_names_tar_roundtrip() {
    roundtrip(Names::Collision, true);
}
