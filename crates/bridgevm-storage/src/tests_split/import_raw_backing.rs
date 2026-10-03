//! Raw backing content survives relocation while contradictory formats refuse.

use super::helpers::{manifest, temp_store};
use super::import_images::{chain, create_overlay, image};
use super::snapshot_names::files_under;
use crate::*;
use std::fs;
use std::io::Write;

#[test]
fn qcow2_import_retains_owned_raw_backing_bytes_and_refuses_format_mismatch() {
    for extension in ["vmbridge", "tar"] {
        let source = temp_store();
        let mut config = manifest("raw-base");
        config.storage.primary.format = "raw".into();
        config.storage.primary.path = "disks/root.raw".into();
        config.storage.primary.size = "1MiB".into();
        source.create_vm(&config).unwrap();
        let primary = source.prepare_primary_disk("raw-base").unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&primary.path)
            .unwrap()
            .write_all(b"owned raw backing sentinel")
            .unwrap();
        source
            .create_snapshot("raw-base", "captured", SnapshotKind::Disk)
            .unwrap();
        create_overlay(&source, "raw-base", "captured");
        let bundle = source.bundle_path("raw-base");
        let original = files_under(&bundle);
        let exported = source.root().join(format!("export.{extension}"));
        source.export_vm("raw-base", &exported).unwrap();
        let target = temp_store();
        let imported = fs::canonicalize(
            target
                .import_vm(&exported, Some("portable"))
                .unwrap()
                .output,
        )
        .unwrap();
        assert_eq!(files_under(&bundle), original);
        if extension == "vmbridge" {
            let record = exported.join("metadata/snapshot-disks/captured.json");
            let mut value: serde_json::Value =
                serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
            value["backing_format"] = "qcow2".into();
            fs::write(&record, serde_json::to_vec(&value).unwrap()).unwrap();
            let before = files_under(&exported);
            assert!(target.import_vm(&exported, Some("bad")).is_err());
            assert!(!target.bundle_path("bad").exists());
            assert_eq!(files_under(&exported), before);
        }
        let withdrawn = source.root().with_extension("unavailable");
        fs::rename(source.root(), &withdrawn).unwrap();
        let active = target.active_disk("portable").unwrap();
        assert!(active.path.starts_with(&imported));
        let actual = chain(&active.path);
        assert_eq!(actual[1]["format"], "raw");
        assert_eq!(actual[1]["virtual-size"], 1024 * 1024);
        let flattened = target.root().join("content.raw");
        image(&[
            "convert",
            "-f",
            "qcow2",
            "-O",
            "raw",
            active.path.to_str().unwrap(),
            flattened.to_str().unwrap(),
        ]);
        assert_eq!(
            fs::read(flattened).unwrap(),
            fs::read(imported.join("disks/root.raw")).unwrap()
        );
        assert_eq!(
            target
                .snapshot_disk_metadata("portable", "captured")
                .unwrap()
                .unwrap()
                .backing_format,
            "raw"
        );
        fs::remove_dir_all(withdrawn).unwrap();
        fs::remove_dir_all(target.root()).unwrap();
    }
}
