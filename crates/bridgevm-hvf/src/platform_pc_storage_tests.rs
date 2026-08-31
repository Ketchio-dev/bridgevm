use super::*;
use std::fs;

fn raw_disk_path() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "bridgevm-pc-storage-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_file(&path);
    path
}

#[test]
fn raw_disk_attachment_is_file_backed_and_copy_on_write() {
    let path = raw_disk_path();
    let source = vec![0x31; crate::nvme::LBA_SIZE * 2];
    fs::write(&path, &source).expect("create aligned raw disk");

    let mut platform = BridgeVmPcPlatform::new();
    platform
        .attach_nvme_raw_file(&path, false)
        .expect("attach COW raw disk");

    assert_eq!(platform.nvme.disk_len(), source.len() as u64);
    assert!(platform.nvme.disk_image_if_memory().is_none());
    platform
        .nvme
        .disk
        .write_at(0, &[0x72; crate::nvme::LBA_SIZE])
        .expect("write sparse overlay");

    let mut observed = vec![0; crate::nvme::LBA_SIZE];
    platform
        .nvme
        .disk
        .read_at_into(0, &mut observed)
        .expect("read sparse overlay");
    assert_eq!(observed, vec![0x72; crate::nvme::LBA_SIZE]);
    assert_eq!(fs::read(&path).expect("read source"), source);

    fs::remove_file(path).ok();
}
