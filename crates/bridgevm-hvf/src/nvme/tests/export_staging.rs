use super::super::NvmeController;
use crate::media::{VirtBootMediaConfig, WritableMedia};
use crate::snapshot_pair::managed::runtime::acquire;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) struct Fixture(pub(super) PathBuf);
impl Fixture {
    pub(super) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "bv-nvme-export-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn export_never_truncates_source_named_like_old_staging_file() {
    let f = Fixture::new();
    let source = f.0.join(".snapshot.raw.export");
    let destination = f.0.join("snapshot.raw");
    let original = vec![0x5a; 8192];
    fs::write(&source, &original).unwrap();
    fs::write(&destination, b"previous-export").unwrap();
    let vars = f.0.join("vars.fd");
    fs::write(&vars, b"synthetic-vars").unwrap();
    let mut media = VirtBootMediaConfig::qemu_defaults();
    media.flash_vars = WritableMedia::new(vars);
    media.nvme_disk =
        Some(WritableMedia::new(source.clone()).with_snapshot_path(Some(destination.clone())));
    let _owner = acquire(&mut media).unwrap();
    let selected = &media.nvme_disk.as_ref().unwrap().path;
    let mut controller = NvmeController::with_raw_file(selected, false).unwrap();
    let result = controller.export_disk_image(&destination);
    let source_after = fs::read(&source).unwrap();
    assert_eq!(
        source_after.len(),
        original.len(),
        "read-only source must survive export; result={result:?}"
    );
    assert_eq!(source_after, original);
    assert_eq!(result.unwrap(), original.len() as u64);
    assert_eq!(fs::read(destination).unwrap(), original);
}

#[test]
fn export_does_not_take_ownership_of_unrelated_staging_name() {
    let f = Fixture::new();
    let foreign = f.0.join(".snapshot.raw.export");
    let destination = f.0.join("snapshot.raw");
    fs::write(&foreign, b"unrelated-existing-file").unwrap();
    let image = vec![0x7c; 4096];
    let mut controller = NvmeController::with_disk_image(image.clone());
    controller.export_disk_image(&destination).unwrap();
    assert_eq!(fs::read(&foreign).unwrap(), b"unrelated-existing-file");
    assert_eq!(fs::read(destination).unwrap(), image);
}
