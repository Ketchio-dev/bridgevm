use super::super::NvmeController;
use super::export_staging::Fixture;
use std::fs;
use std::os::unix::fs::{symlink, MetadataExt};

#[test]
fn old_staging_aliases_never_overwrite_unrelated_bytes() {
    for link in [false, true] {
        let f = Fixture::new();
        let sentinel = f.0.join("sentinel");
        let foreign = f.0.join(".snapshot.raw.export");
        let destination = f.0.join("snapshot.raw");
        fs::write(&sentinel, b"preserve-this").unwrap();
        if link {
            symlink(&sentinel, &foreign).unwrap();
        } else {
            fs::hard_link(&sentinel, &foreign).unwrap();
        }
        let before = fs::symlink_metadata(&foreign).unwrap();
        let image = vec![0x91; 512];
        let mut controller = NvmeController::with_disk_image(image.clone());
        controller.export_disk_image(&destination).unwrap();
        let after = fs::symlink_metadata(&foreign).unwrap();
        assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
        assert_eq!(fs::read(&sentinel).unwrap(), b"preserve-this");
        assert_eq!(fs::read(&foreign).unwrap(), b"preserve-this");
        assert_eq!(fs::read(&destination).unwrap(), image);
    }
}

#[test]
fn read_failure_removes_only_owned_staging_and_preserves_previous_output() {
    let f = Fixture::new();
    let source = f.0.join("source.raw");
    let destination = f.0.join("snapshot.raw");
    let foreign = f.0.join(".snapshot.raw.export");
    fs::write(&source, [0xab; 512]).unwrap();
    fs::write(&destination, b"old-output").unwrap();
    fs::write(&foreign, b"foreign").unwrap();
    let mut controller = NvmeController::with_raw_file(&source, false).unwrap();
    // Controlled source failure after open, not a simulated export result.
    fs::OpenOptions::new()
        .write(true)
        .open(&source)
        .unwrap()
        .set_len(0)
        .unwrap();
    assert!(controller.export_disk_image(&destination).is_err());
    assert_eq!(fs::read(&destination).unwrap(), b"old-output");
    assert_eq!(fs::read(&foreign).unwrap(), b"foreign");
    assert_eq!(fs::read_dir(&f.0).unwrap().count(), 3);
}

#[test]
fn rename_failure_retains_destination_and_cleans_private_staging() {
    let f = Fixture::new();
    let destination = f.0.join("snapshot.raw");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("keep"), b"owned-by-someone-else").unwrap();
    let mut controller = NvmeController::with_disk_image(vec![0x92; 512]);
    assert!(controller.export_disk_image(&destination).is_err());
    assert_eq!(
        fs::read(destination.join("keep")).unwrap(),
        b"owned-by-someone-else"
    );
    assert_eq!(fs::read_dir(&f.0).unwrap().count(), 1);
}
